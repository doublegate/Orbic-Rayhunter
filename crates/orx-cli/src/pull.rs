//! `orx pull-logs`: download recordings from the device's HTTP API to a host
//! directory, with a manifest and SHA-256, skipping entries already pulled
//! unchanged. Per recording it fetches the raw QMDL (`/api/qmdl/{name}`) and the
//! analysis report (`/api/analysis-report/{name}`, NDJSON, best-effort). The
//! device API is unauthenticated on `0.0.0.0:8080`; this is a read-only pull.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

/// Typed outcome of an HTTP GET, so a caller can distinguish a genuine 404 (the
/// resource is absent -- fine for a best-effort fetch) from a transport error,
/// a non-404 status, a truncated transfer, or a local I/O error (all of which
/// must fail closed rather than be treated as "absent").
#[derive(Debug)]
pub enum PullError {
    /// HTTP 404: the resource does not exist.
    NotFound,
    /// Any other non-200 status.
    Http(u16),
    /// Transport, truncation, encoding, or local I/O failure.
    Other(String),
}

impl std::fmt::Display for PullError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PullError::NotFound => write!(f, "HTTP 404 (not found)"),
            PullError::Http(s) => write!(f, "HTTP {s}"),
            PullError::Other(m) => write!(f, "{m}"),
        }
    }
}

impl From<String> for PullError {
    fn from(s: String) -> Self {
        PullError::Other(s)
    }
}

impl From<&str> for PullError {
    fn from(s: &str) -> Self {
        PullError::Other(s.to_string())
    }
}

/// POST `path` with an empty body and return the response status on a 2xx, else
/// `Err`. For the daemon's action endpoints (e.g. `/api/delete-recording/{name}`,
/// which replies `202 Accepted`). The (bounded) response body is read and
/// discarded. 404 maps to `PullError::NotFound`, like [`http_get_to`].
pub fn http_post(addr: SocketAddr, path: &str, timeout: Duration) -> Result<u16, PullError> {
    post_impl(addr, path, b"", None, timeout)
}

/// POST a JSON body and return the response status on a 2xx. Used for the
/// daemon's config write (`/api/config`). 404 maps to `PullError::NotFound`.
pub fn http_post_json(
    addr: SocketAddr,
    path: &str,
    body: &str,
    timeout: Duration,
) -> Result<u16, PullError> {
    post_impl(
        addr,
        path,
        body.as_bytes(),
        Some("application/json"),
        timeout,
    )
}

fn post_impl(
    addr: SocketAddr,
    path: &str,
    body: &[u8],
    content_type: Option<&str>,
    timeout: Duration,
) -> Result<u16, PullError> {
    const MAX_DRAIN: u64 = 64 * 1024;
    let deadline = Instant::now() + timeout;
    let mut stream =
        TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("connect: {e}"))?;
    stream.set_write_timeout(Some(timeout)).ok();
    let ct = content_type
        .map(|c| format!("Content-Type: {c}\r\n"))
        .unwrap_or_default();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n{ct}Content-Length: {}\r\n\r\n",
        body.len()
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("write: {e}"))?;
    if !body.is_empty() {
        stream
            .write_all(body)
            .map_err(|e| format!("write body: {e}"))?;
    }

    let mut head = Vec::new();
    let mut buf = [0u8; 8192];
    // Decode ONLY the header block as text: body bytes may share the same read()
    // and need not be UTF-8 (and a multi-byte char could straddle the boundary).
    let hdr_end = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("read: deadline exceeded in headers".into());
        }
        stream.set_read_timeout(Some(remaining)).ok();
        match stream.read(&mut buf) {
            Ok(0) => return Err("connection closed before headers completed".into()),
            Ok(n) => {
                head.extend_from_slice(&buf[..n]);
                if let Some(pos) = find_sub(&head, b"\r\n\r\n") {
                    break pos + 4;
                }
                if head.len() > 64 * 1024 {
                    return Err("HTTP headers too large".into());
                }
            }
            Err(e) => return Err(format!("read: {e}").into()),
        }
    };
    let head_str =
        std::str::from_utf8(&head[..hdr_end]).map_err(|_| "non-UTF-8 HTTP headers".to_string())?;
    let status = head_str
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| PullError::Other("malformed HTTP status line".to_string()))?;
    if status == 404 {
        return Err(PullError::NotFound);
    }
    if !(200..300).contains(&status) {
        return Err(PullError::Http(status));
    }
    // Drain the (small) body so the peer closes cleanly; bounded and timed.
    let mut drained = 0u64;
    while drained < MAX_DRAIN {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        stream.set_read_timeout(Some(remaining)).ok();
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => drained += n as u64,
            Err(_) => break,
        }
    }
    Ok(status)
}

/// Recording names are device-supplied; only allow a safe, path- and URL-clean
/// token so a crafted manifest cannot traverse directories or inject a request.
pub fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && name != "."
        && name != ".."
        && !name.contains("..")
}

/// GET `path` and return the body as a String, capped at `max` bytes (for small
/// JSON responses). A non-2xx status or exceeding the cap is an error.
pub fn http_get_string(
    addr: SocketAddr,
    path: &str,
    max: usize,
    timeout: Duration,
) -> Result<String, String> {
    let mut buf = Vec::new();
    let n = http_get_to(addr, path, &mut buf, max as u64, timeout).map_err(|e| e.to_string())?;
    String::from_utf8(buf[..n as usize].to_vec()).map_err(|_| "response is not UTF-8".to_string())
}

/// GET `path`, streaming the response body into `sink` (never buffering it all),
/// bounded by `max` bytes and `timeout`. Returns the number of body bytes
/// written. A non-2xx status is an error; exceeding `max` is an error.
pub fn http_get_to(
    addr: SocketAddr,
    path: &str,
    sink: &mut dyn Write,
    max: u64,
    timeout: Duration,
) -> Result<u64, PullError> {
    let deadline = Instant::now() + timeout;
    let mut stream =
        TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("connect: {e}"))?;
    stream.set_write_timeout(Some(timeout)).ok();
    // HTTP/1.1 so the daemon frames a streamed body with chunked encoding (an
    // explicit 0-chunk terminator we can use to detect truncation), rather than
    // HTTP/1.0 close-delimited, where a mid-transfer close looks like a clean EOF.
    let req =
        format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nAccept: */*\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("write: {e}"))?;

    // Read until we have the full header block, then stream the body.
    let mut head = Vec::new();
    let mut buf = [0u8; 8192];
    let body_start: Vec<u8> = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("read: deadline exceeded in headers".into());
        }
        stream.set_read_timeout(Some(remaining)).ok();
        match stream.read(&mut buf) {
            Ok(0) => return Err("connection closed before headers completed".into()),
            Ok(n) => {
                head.extend_from_slice(&buf[..n]);
                if let Some(pos) = find_sub(&head, b"\r\n\r\n") {
                    let header_end = pos + 4;
                    let body = head[header_end..].to_vec();
                    head.truncate(header_end);
                    break body;
                }
                if head.len() > 64 * 1024 {
                    return Err("HTTP headers too large".into());
                }
            }
            Err(e) => return Err(format!("read: {e}").into()),
        }
    };
    let head_str = std::str::from_utf8(&head).map_err(|_| "non-UTF-8 HTTP headers".to_string())?;
    let status = head_str
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| "malformed HTTP status line".to_string())?;
    // Require exactly 200: 204 (no content) and 206 (partial) would otherwise
    // save an empty/partial file and record it as a complete download. A genuine
    // 404 is reported as NotFound so a best-effort caller can treat it as absent.
    if status == 404 {
        return Err(PullError::NotFound);
    }
    if status != 200 {
        return Err(PullError::Http(status));
    }
    // Determine the body framing. Fail closed on an invalid/conflicting
    // Content-Length or a content-encoding we would write verbatim.
    let mut content_length: Option<u64> = None;
    let mut chunked = false;
    for line in head_str.lines().skip(1) {
        let Some((k, val)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        let val = val.trim();
        match key.as_str() {
            "transfer-encoding" if val.eq_ignore_ascii_case("chunked") => chunked = true,
            "transfer-encoding" if !val.is_empty() => {
                return Err(format!("unsupported transfer-encoding: {val:?}").into());
            }
            "content-encoding" if !val.is_empty() && !val.eq_ignore_ascii_case("identity") => {
                return Err(format!("unsupported content-encoding: {val:?}").into());
            }
            "content-length" => {
                let len = val
                    .parse::<u64>()
                    .map_err(|_| format!("invalid Content-Length: {val:?}"))?;
                if content_length.is_some_and(|prev| prev != len) {
                    return Err("conflicting Content-Length headers".into());
                }
                content_length = Some(len);
            }
            _ => {}
        }
    }

    if chunked {
        return read_chunked_body(&mut stream, body_start, sink, max, deadline)
            .map_err(PullError::Other);
    }
    // Not chunked: a Content-Length lets us verify completion; without one, an
    // HTTP/1.1 close-delimited body cannot be told apart from a truncated one, so
    // fail closed rather than record a possibly-partial file as complete.
    let Some(expected) = content_length else {
        return Err("response has neither Content-Length nor chunked encoding; \
                    cannot verify a complete transfer"
            .into());
    };
    let mut written = 0u64;
    let mut emit = |data: &[u8], written: &mut u64| -> Result<(), String> {
        *written += data.len() as u64;
        if *written > max {
            return Err(format!("response exceeded {max} bytes"));
        }
        sink.write_all(data).map_err(|e| format!("write body: {e}"))
    };
    emit(&body_start, &mut written)?;
    while written < expected {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("read: deadline exceeded in body".into());
        }
        stream.set_read_timeout(Some(remaining)).ok();
        match stream.read(&mut buf) {
            Ok(0) => return Err(format!("truncated: got {written} of {expected} bytes").into()),
            Ok(n) => emit(&buf[..n], &mut written)?,
            Err(e) => return Err(format!("read body: {e}").into()),
        }
    }
    if written != expected {
        return Err(format!("overlong: got {written}, expected {expected} bytes").into());
    }
    Ok(written)
}

/// Decode an HTTP/1.1 chunked body, streaming chunk data to `sink`. Requires the
/// terminating zero-size chunk: a connection that closes before it is a
/// truncated transfer (error), not a clean EOF.
fn read_chunked_body(
    stream: &mut TcpStream,
    initial: Vec<u8>,
    sink: &mut dyn Write,
    max: u64,
    deadline: Instant,
) -> Result<u64, String> {
    let mut buf = initial;
    let mut rbuf = [0u8; 8192];
    let mut written = 0u64;
    // Read one more socket chunk into `buf`; Err if the peer closes early.
    let mut fill = |stream: &mut TcpStream, buf: &mut Vec<u8>| -> Result<(), String> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("read: deadline exceeded in chunked body".to_string());
        }
        stream.set_read_timeout(Some(remaining)).ok();
        match stream.read(&mut rbuf) {
            Ok(0) => Err("truncated: connection closed mid-chunk".to_string()),
            Ok(n) => {
                buf.extend_from_slice(&rbuf[..n]);
                Ok(())
            }
            Err(e) => Err(format!("read chunk: {e}")),
        }
    };
    loop {
        // Parse the chunk-size line.
        let line_end = loop {
            if let Some(p) = find_sub(&buf, b"\r\n") {
                break p;
            }
            if buf.len() > 64 * 1024 {
                return Err("chunk size line too long".to_string());
            }
            fill(stream, &mut buf)?;
        };
        let size_field = &buf[..line_end];
        // A chunk-size line may carry `;ext`; take the hex before any ';'.
        let hex = size_field
            .split(|&b| b == b';')
            .next()
            .unwrap_or(size_field);
        let hex = std::str::from_utf8(hex)
            .map_err(|_| "bad chunk size".to_string())?
            .trim();
        let size = u64::from_str_radix(hex, 16).map_err(|_| format!("bad chunk size {hex:?}"))?;
        buf.drain(..line_end + 2);
        if size == 0 {
            return Ok(written); // final chunk seen -> complete
        }
        // Stream `size` data bytes, reading more as needed.
        let mut need = size;
        while need > 0 {
            if buf.is_empty() {
                fill(stream, &mut buf)?;
            }
            let take = need.min(buf.len() as u64) as usize;
            written += take as u64;
            if written > max {
                return Err(format!("response exceeded {max} bytes"));
            }
            sink.write_all(&buf[..take])
                .map_err(|e| format!("write body: {e}"))?;
            buf.drain(..take);
            need -= take as u64;
        }
        // Consume the CRLF after the chunk data.
        while buf.len() < 2 {
            fill(stream, &mut buf)?;
        }
        if &buf[..2] != b"\r\n" {
            return Err("malformed chunk terminator".to_string());
        }
        buf.drain(..2);
    }
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

/// One recording to pull, parsed from the manifest.
struct Recording {
    name: String,
    /// Manifest-reported (compressed, on-device) size; `None` if the field is
    /// absent -- then the recording is never treated as "unchanged" (no skip).
    qmdl_size: Option<u64>,
    completed: bool,
}

/// Parse `/api/qmdl-manifest` into the recordings to pull: the completed
/// `entries` plus the live `current_entry` if present.
fn parse_manifest(body: &str) -> Result<Vec<Recording>, String> {
    let v = crate::json::parse(body).map_err(|e| format!("manifest JSON: {e}"))?;
    // Fail closed: the manifest must be an object with an `entries` array. A bare
    // `{}` or an error object returned with 200 must not parse as "no recordings"
    // (which would then overwrite the local state as empty).
    let entries = v
        .get("entries")
        .and_then(|e| e.as_array())
        .ok_or_else(|| "manifest has no `entries` array (unexpected response?)".to_string())?;
    let mut out = Vec::new();
    // `completed` is determined by POSITION, not `stop_reason`: the daemon keeps
    // finished recordings in `entries` and the live one in `current_entry`, and
    // some daemon versions leave `stop_reason` null on finished entries too. A
    // recording in `entries` is final, so its `qmdl_size_bytes` is stable and the
    // skip-if-unchanged check is sound.
    let mut seen = std::collections::BTreeSet::new();
    let mut take = |entry: &crate::json::Json, completed: bool| -> Result<(), String> {
        let name = entry
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| "manifest entry missing name".to_string())?
            .to_string();
        if !safe_name(&name) {
            return Err(format!("unsafe recording name in manifest: {name:?}"));
        }
        // Duplicate names would make two recordings share one `<name>.qmdl` and
        // corrupt PULL-MANIFEST.txt / .orx-pull-state -- fail closed.
        if !seen.insert(name.clone()) {
            return Err(format!("duplicate recording name in manifest: {name:?}"));
        }
        let qmdl_size = entry.get("qmdl_size_bytes").and_then(|s| s.as_u64());
        out.push(Recording {
            name,
            qmdl_size,
            completed,
        });
        Ok(())
    };
    for e in entries {
        take(e, true)?;
    }
    if let Some(cur) = v.get("current_entry").filter(|c| !c.is_null()) {
        take(cur, false)?;
    }
    Ok(out)
}

/// Download `path` to `dest` atomically: stream to `<dest>.part`, then rename on
/// a complete transfer; remove the temp on failure (so a partial file is never
/// left as `dest`). Returns the number of bytes written.
fn download_to(
    addr: SocketAddr,
    path: &str,
    dest: &Path,
    max: u64,
    timeout: Duration,
) -> Result<u64, PullError> {
    let tmp = Path::new(&format!("{}.part", dest.display())).to_path_buf();
    let mut f =
        std::fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
    match http_get_to(addr, path, &mut f, max, timeout) {
        Ok(n) => {
            drop(f);
            std::fs::rename(&tmp, dest).map_err(|e| format!("rename {}: {e}", tmp.display()))?;
            Ok(n)
        }
        Err(e) => {
            drop(f);
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// sha256 of a local file, hashed incrementally in 64 KiB chunks (never loading
/// the whole capture into RAM).
pub fn file_sha256(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut h = crate::sha256::Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(crate::sha256::hex_of(h))
}

pub struct PullOutcome {
    pub pulled: usize,
    pub skipped: usize,
    pub bytes: u64,
}

/// Download all recordings to `dir`. `completed` recordings already present with
/// the manifest's size are skipped; the live recording is always re-pulled.
pub fn pull_logs(ip: &str, dir: &Path, timeout: Duration) -> Result<PullOutcome, String> {
    let addr = crate::probe::socket(ip, crate::probe::RAYHUNTER_PORT)?;
    pull_logs_at(addr, dir, timeout)
}

/// As [`pull_logs`] but against an explicit address (so tests can use a loopback
/// server on an arbitrary port).
pub fn pull_logs_at(
    addr: SocketAddr,
    dir: &Path,
    timeout: Duration,
) -> Result<PullOutcome, String> {
    const MAX_MANIFEST: usize = 4 * 1024 * 1024;
    const MAX_QMDL: u64 = 512 * 1024 * 1024;
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;

    let body = http_get_string(addr, "/api/qmdl-manifest", MAX_MANIFEST, timeout)
        .map_err(|e| format!("fetch manifest: {e}"))?;
    let recordings = parse_manifest(&body)?;

    // Prior pull state: recording name -> the manifest `qmdl_size_bytes` it was
    // last pulled at. The on-device size is COMPRESSED and differs from the
    // decompressed bytes we download, so we skip by "manifest size unchanged
    // since last pull", not by comparing file lengths.
    let state_path = dir.join(".orx-pull-state");
    let prior = read_pull_state(&state_path);

    let mut outcome = PullOutcome {
        pulled: 0,
        skipped: 0,
        bytes: 0,
    };
    let mut manifest_lines = Vec::new();
    let mut new_state: Vec<(String, u64)> = Vec::new();
    for r in &recordings {
        let qmdl = dir.join(format!("{}.qmdl", r.name));
        let nd = dir.join(format!("{}.ndjson", r.name));
        let nd_path = format!("/api/analysis-report/{}", r.name);
        // Skippable only when the recording is completed, its manifest size is
        // known and unchanged since the last pull, and its QMDL is on disk.
        let unchanged = r.completed
            && r.qmdl_size.is_some()
            && prior.get(r.name.as_str()).copied() == r.qmdl_size
            && qmdl.is_file();
        if unchanged {
            outcome.skipped += 1;
            // Re-fetch the analysis report only if it's missing (a prior transient
            // failure, or a live->completed transition left it absent/stale).
            if !nd.is_file() {
                // Re-fetch a missing report. A genuine 404 means it does not exist
                // yet (fine); any other failure (5xx, timeout, truncation, I/O)
                // must fail closed rather than be silently swallowed.
                match download_to(addr, &nd_path, &nd, MAX_QMDL, timeout) {
                    Ok(nb) => outcome.bytes += nb,
                    Err(PullError::NotFound) => {}
                    Err(e) => return Err(format!("fetch {}.ndjson: {e}", r.name)),
                }
            }
        } else {
            let nq = download_to(
                addr,
                &format!("/api/qmdl/{}", r.name),
                &qmdl,
                MAX_QMDL,
                timeout,
            )
            .map_err(|e| format!("download {}.qmdl: {e}", r.name))?;
            outcome.pulled += 1;
            outcome.bytes += nq;
            // Analysis report (NDJSON): a genuine 404 means it does not exist yet
            // (drop any stale local copy, not an error); any other failure must
            // fail closed so a transient error is never reported as success.
            match download_to(addr, &nd_path, &nd, MAX_QMDL, timeout) {
                Ok(nb) => outcome.bytes += nb,
                Err(PullError::NotFound) => {
                    let _ = std::fs::remove_file(&nd);
                }
                Err(e) => return Err(format!("fetch {}.ndjson: {e}", r.name)),
            }
        }
        // Record skippable state ONLY for a completed recording that is fully
        // present (QMDL + NDJSON) at a known size, so the live recording -- and
        // any whose NDJSON is still missing -- is re-pulled next run.
        if let (true, Some(sz)) = (r.completed && qmdl.is_file() && nd.is_file(), r.qmdl_size) {
            new_state.push((r.name.clone(), sz));
        }
    }
    // Build the manifest from EVERY .qmdl/.ndjson present in `dir`, not only this
    // run's recordings, so a previously pulled capture that has since been purged
    // off the device keeps its entry (its file is still here). Written atomically.
    for entry in std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let path = entry.map_err(|e| format!("read dir entry: {e}"))?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_file() && (name.ends_with(".qmdl") || name.ends_with(".ndjson")) {
            manifest_lines.push(format!("{}  {name}", file_sha256(&path)?));
        }
    }
    manifest_lines.sort();
    let manifest = format!(
        "# orx pull-logs manifest\n# {} file(s) held locally\n{}\n",
        manifest_lines.len(),
        manifest_lines.join("\n")
    );
    write_atomic(&dir.join("PULL-MANIFEST.txt"), manifest.as_bytes())?;
    let state_text: String = new_state
        .iter()
        .map(|(n, s)| format!("{n}\t{s}\n"))
        .collect();
    write_atomic(&state_path, state_text.as_bytes())?;
    Ok(outcome)
}

/// Write `data` to `path` atomically (temp in the same dir, then rename), so a
/// crash or ENOSPC never leaves a truncated manifest/state file.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    // Unique temp beside the target (pid + a process-local counter + a clock
    // nonce, so repeated/concurrent writes and a shared dir never collide),
    // created fresh with O_EXCL (never follows or clobbers a planted symlink, and
    // fails closed if the name is pre-created) and mode 0600 (the data may be
    // sensitive -- e.g. a config snapshot with a private topic URL).
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("out");
    // Cap the embedded name (on a char boundary) so the temp filename stays well
    // under NAME_MAX (255 bytes) even for a long target name.
    let short: String = name.chars().take(48).collect();
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let tmp = path.with_file_name(format!(".{short}.{}.{seq}.{nonce}.tmp", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| format!("create {}: {e}", tmp.display()))?;

    // On any failure, remove the partial temp before returning (it may hold
    // sensitive data and would otherwise block a retry with EEXIST).
    let result = (|| {
        f.write_all(data)
            .map_err(|e| format!("write {}: {e}", tmp.display()))?;
        // Propagate flush/ENOSPC errors: renaming an incompletely-written file
        // over the target would defeat the crash-safety guarantee.
        f.sync_all()
            .map_err(|e| format!("sync {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("rename {}: {e}", tmp.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Read `name<TAB>qmdl_size_bytes` lines into a map (missing/garbled -> empty).
fn read_pull_state(path: &Path) -> std::collections::BTreeMap<String, u64> {
    let mut map = std::collections::BTreeMap::new();
    if let Ok(text) = std::fs::read_to_string(path) {
        for line in text.lines() {
            let Some((name, size)) = line.split_once('\t') else {
                continue;
            };
            let Ok(n) = size.trim().parse::<u64>() else {
                continue;
            };
            map.insert(name.to_string(), n);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn safe_name_rejects_traversal_and_junk() {
        assert!(safe_name("1795442375"));
        assert!(safe_name("rec-1_v2.qmdl"));
        assert!(!safe_name(""));
        assert!(!safe_name(".."));
        assert!(!safe_name("a/b"));
        assert!(!safe_name("../etc"));
        assert!(!safe_name("a b"));
    }

    #[test]
    fn write_atomic_writes_mode_0600_and_leaves_no_temp() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmp();
        let target = dir.join("secret-snapshot.json");
        write_atomic(&target, b"topic-secret").unwrap();
        // Restrictive perms (the data may be sensitive) and exact content.
        let meta = std::fs::metadata(&target).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read(&target).unwrap(), b"topic-secret");
        // No temp file is left behind on success.
        let leftover: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftover.is_empty(), "leftover temp files: {leftover:?}");
        // A second write (unique temp name) overwrites atomically, no EEXIST.
        write_atomic(&target, b"second").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"second");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_manifest_collects_entries_and_current() {
        let body = r#"{"entries":[{"name":"1795","qmdl_size_bytes":100,"stop_reason":"UserRequested"}],"current_entry":{"name":"1796","qmdl_size_bytes":200,"stop_reason":null}}"#;
        let r = parse_manifest(body).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].name, "1795");
        assert!(r[0].completed);
        assert_eq!(r[1].name, "1796");
        assert!(!r[1].completed);
    }

    #[test]
    fn parse_manifest_rejects_unsafe_name() {
        let body =
            r#"{"entries":[{"name":"../../etc/passwd","qmdl_size_bytes":1,"stop_reason":"x"}]}"#;
        assert!(parse_manifest(body).is_err());
    }

    #[test]
    fn parse_manifest_fails_closed_without_entries() {
        // A bare object or an error object returned with 200 must NOT parse as
        // "no recordings" (which would wipe local state).
        assert!(parse_manifest("{}").is_err());
        assert!(parse_manifest(r#"{"error":"nope"}"#).is_err());
        // An empty entries array is valid (no completed recordings yet).
        assert_eq!(parse_manifest(r#"{"entries":[]}"#).unwrap().len(), 0);
    }

    /// Serve one fully-custom raw HTTP response (headers + body) for one conn.
    fn serve_raw(resp: &'static str) -> SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = l.accept() {
                let mut req = [0u8; 1024];
                let _ = s.read(&mut req);
                let _ = s.write_all(resp.as_bytes());
            }
        });
        addr
    }

    #[test]
    fn http_get_to_decodes_chunked() {
        // Two chunks + terminator -> body "abcde".
        let addr = serve_raw(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n",
        );
        let mut buf = Vec::new();
        let n = http_get_to(addr, "/x", &mut buf, 1 << 20, Duration::from_secs(2)).unwrap();
        assert_eq!(n, 5);
        assert_eq!(buf, b"abcde");
    }

    #[test]
    fn http_get_to_rejects_chunked_without_terminator() {
        // Data chunk but the connection closes before the 0-chunk -> truncated.
        let addr = serve_raw("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n");
        let mut buf = Vec::new();
        let err = http_get_to(addr, "/x", &mut buf, 1 << 20, Duration::from_secs(2))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("truncated") || err.contains("mid-chunk"),
            "got: {err}"
        );
    }

    #[test]
    fn http_get_to_rejects_unframed_body() {
        // HTTP/1.1 with neither Content-Length nor chunked -> cannot verify.
        let addr = serve_raw("HTTP/1.1 200 OK\r\n\r\nabc");
        let mut buf = Vec::new();
        assert!(http_get_to(addr, "/x", &mut buf, 1 << 20, Duration::from_secs(2)).is_err());
    }

    #[test]
    fn parse_manifest_rejects_duplicate_names() {
        let body =
            r#"{"entries":[{"name":"1","qmdl_size_bytes":1},{"name":"1","qmdl_size_bytes":2}]}"#;
        assert!(parse_manifest(body).is_err());
        // A current_entry duplicating an entry name is also rejected.
        let body2 = r#"{"entries":[{"name":"1","qmdl_size_bytes":1}],"current_entry":{"name":"1","qmdl_size_bytes":2}}"#;
        assert!(parse_manifest(body2).is_err());
    }

    #[test]
    fn http_get_to_requires_200() {
        let addr = serve_raw("HTTP/1.0 204 No Content\r\n\r\n");
        let mut buf = Vec::new();
        let err = http_get_to(addr, "/x", &mut buf, 1 << 20, Duration::from_secs(2))
            .unwrap_err()
            .to_string();
        assert!(err.contains("204"), "got: {err}");
    }

    #[test]
    fn http_get_to_rejects_truncated_content_length() {
        // Declares 100 bytes but sends 3, then closes -> must error, not accept.
        let addr = serve_raw("HTTP/1.0 200 OK\r\nContent-Length: 100\r\n\r\nabc");
        let mut buf = Vec::new();
        let err = http_get_to(addr, "/x", &mut buf, 1 << 20, Duration::from_secs(2))
            .unwrap_err()
            .to_string();
        assert!(err.contains("truncated"), "got: {err}");
    }

    /// Serve canned HTTP/1.0 responses keyed by request path, for `count` conns.
    fn serve(count: usize, routes: Vec<(&'static str, &'static str)>) -> SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for _ in 0..count {
                let (mut s, _) = match l.accept() {
                    Ok(x) => x,
                    Err(_) => break,
                };
                let mut req = [0u8; 2048];
                let n = s.read(&mut req).unwrap_or(0);
                let line = String::from_utf8_lossy(&req[..n]);
                let path = line.split_whitespace().nth(1).unwrap_or("");
                let body = routes.iter().find(|(p, _)| *p == path).map(|(_, b)| *b);
                let resp = match body {
                    Some(b) => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n{b}",
                        b.len()
                    ),
                    None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string(),
                };
                let _ = s.write_all(resp.as_bytes());
            }
        });
        addr
    }

    #[test]
    fn pull_logs_downloads_then_skips_completed() {
        let manifest = r#"{"entries":[{"name":"1795","qmdl_size_bytes":5,"stop_reason":"UserRequested"}],"current_entry":{"name":"1796","qmdl_size_bytes":4,"stop_reason":null}}"#;
        let routes = vec![
            ("/api/qmdl-manifest", manifest),
            ("/api/qmdl/1795", "AAAAA"),
            ("/api/analysis-report/1795", "{\"report_version\":2}\n"),
            ("/api/qmdl/1796", "BBBB"),
            ("/api/analysis-report/1796", "{\"report_version\":2}\n"),
        ];
        // Run 1: manifest + both recordings' qmdl+ndjson = 5 connections.
        // Run 2: manifest + only the live 1796's qmdl+ndjson (1795 is skipped) = 3.
        let addr = serve(8, routes);
        let dir = tmp();

        let o1 = pull_logs_at(addr, &dir, Duration::from_secs(3)).unwrap();
        assert_eq!(o1.pulled, 2);
        assert_eq!(o1.skipped, 0);
        assert_eq!(std::fs::read(dir.join("1795.qmdl")).unwrap(), b"AAAAA");
        assert!(dir.join("1795.ndjson").is_file());
        assert!(dir.join("PULL-MANIFEST.txt").is_file());

        let o2 = pull_logs_at(addr, &dir, Duration::from_secs(3)).unwrap();
        assert_eq!(o2.skipped, 1, "completed 1795 should be skipped");
        assert_eq!(o2.pulled, 1, "live 1796 is always re-pulled");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_get_to_errors_on_404() {
        let addr = serve(1, vec![("/present", "x")]);
        let mut e = Vec::new();
        // A genuine 404 maps to the typed NotFound (so a best-effort NDJSON fetch
        // can treat it as absent); any other status must NOT be NotFound.
        let err =
            http_get_to(addr, "/missing", &mut e, 1 << 20, Duration::from_secs(2)).unwrap_err();
        assert!(matches!(err, PullError::NotFound), "got: {err}");
    }

    #[test]
    fn http_get_to_maps_500_to_http_not_notfound() {
        let addr = serve_raw("HTTP/1.0 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n");
        let mut e = Vec::new();
        let err = http_get_to(addr, "/x", &mut e, 1 << 20, Duration::from_secs(2)).unwrap_err();
        assert!(matches!(err, PullError::Http(500)), "got: {err}");
    }

    #[test]
    fn http_get_to_enforces_cap() {
        let addr = serve(1, vec![("/big", "0123456789")]);
        let mut buf = Vec::new();
        assert!(http_get_to(addr, "/big", &mut buf, 4, Duration::from_secs(2)).is_err());
    }

    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    fn tmp() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "orx-pull-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}

//! Device transport: run a command over the unauthenticated root shell on TCP
//! 24 and transfer files with an end-to-end SHA-256 check.
//!
//! The wire protocol mirrors the proven Python tools (`scripts/orbic-shell.py`,
//! `scripts/orbic-backup.py`): the busybox `nc -ll -p 24 -e /bin/sh` listener
//! forks a fresh `/bin/sh` per connection, so each `run`/`put` is its own
//! connection. Device access is a trait (`Shell`) so every caller is unit-tested
//! against [`FakeShell`] with no hardware. The security-critical part -- that a
//! transfer is only accepted when the device's own `sha256sum` matches the bytes
//! we sent -- lives in the `send_file` default method and is tested here.

use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::{Duration, Instant, SystemTime};

/// Combined stdout+stderr and the exit status of one device command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmdResult {
    pub output: String,
    pub status: i32,
}

/// A remote path is safe to interpolate into a shell command only if it is an
/// absolute path of a conservative character set -- no whitespace, no shell
/// metacharacters. `orx` only ever transfers to fixed paths, but the check is
/// defense in depth against a path reaching the device shell unquoted.
pub fn safe_remote(path: &str) -> bool {
    path.starts_with('/')
        && (2..=256).contains(&(path.len() + 1))
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
        && !path.contains("..")
}

/// Splits a streamed command response into the payload (everything before the
/// end marker) and the trailing `" <status>."`. Payload bytes are written to a
/// sink as they are confirmed, so an arbitrarily large `tar` never buffers in
/// memory. Correct across any chunk boundary: it withholds only the last
/// `marker.len()-1` bytes (the only ones that could begin a split marker) until
/// more arrives.
struct MarkerSplitter {
    marker: Vec<u8>,
    pending: Vec<u8>,
    tail: Vec<u8>,
    found: bool,
}

impl MarkerSplitter {
    fn new(marker: &str) -> Self {
        Self {
            marker: marker.as_bytes().to_vec(),
            pending: Vec::new(),
            tail: Vec::new(),
            found: false,
        }
    }

    fn push(&mut self, chunk: &[u8], sink: &mut dyn Write) -> Result<(), String> {
        if self.found {
            self.tail.extend_from_slice(chunk);
            return Ok(());
        }
        self.pending.extend_from_slice(chunk);
        if let Some(i) = find_sub(&self.pending, &self.marker) {
            sink.write_all(&self.pending[..i])
                .map_err(|e| format!("sink: {e}"))?;
            self.tail = self.pending[i + self.marker.len()..].to_vec();
            self.pending.clear();
            self.found = true;
        } else {
            let keep = self.marker.len().saturating_sub(1);
            if self.pending.len() > keep {
                let flush = self.pending.len() - keep;
                sink.write_all(&self.pending[..flush])
                    .map_err(|e| format!("sink: {e}"))?;
                self.pending.drain(..flush);
            }
        }
        Ok(())
    }

    fn found(&self) -> bool {
        self.found
    }

    /// The exit status parsed from the `" <status>."` tail, once the marker was seen.
    fn status(&self) -> Option<i32> {
        if !self.found {
            return None;
        }
        String::from_utf8_lossy(&self.tail)
            .trim_start()
            .split('.')
            .next()
            .and_then(|d| d.trim().parse::<i32>().ok())
    }
}

/// Device shell transport. `run` and `put` are the primitives; `send_file` is a
/// verified transfer built on them and must not be overridden.
pub trait Shell {
    /// Run one command; return its combined output and exit status.
    fn run(&mut self, command: &str) -> Result<CmdResult, String>;

    /// Run one command, streaming its stdout+stderr to `sink` (never buffering
    /// the whole output), and return the exit status. For large transfers such
    /// as a `tar` of a filesystem, where `run`'s in-memory buffering would not
    /// fit. The end marker and trailing status are stripped from `sink`.
    fn run_to(&mut self, command: &str, sink: &mut dyn Write) -> Result<i32, String>;

    /// Transfer raw `bytes` to `remote` with NO verification.
    fn put(&mut self, remote: &str, bytes: &[u8]) -> Result<(), String>;

    /// Adjust the per-command timeout (e.g. a short bound for `reboot`, which
    /// drops the connection without a clean close). Default: no-op.
    fn set_timeout(&mut self, _timeout: std::time::Duration) {}

    /// Transfer `bytes` to `remote` and accept only if the device's own
    /// `sha256sum` of the written file equals the hash of what we sent.
    fn send_file(&mut self, remote: &str, bytes: &[u8]) -> Result<(), String> {
        if !safe_remote(remote) {
            return Err(format!("unsafe remote path: {remote:?}"));
        }
        self.put(remote, bytes)?;
        let want = crate::sha256::hex(bytes);
        let res = self.run(&format!("sha256sum {remote}"))?;
        if res.status != 0 {
            return Err(format!(
                "device sha256sum failed (status {}): {}",
                res.status,
                res.output.trim()
            ));
        }
        let got = res.output.split_whitespace().next().unwrap_or("");
        if got != want {
            return Err(format!(
                "hash mismatch for {remote}: device {got} != expected {want}"
            ));
        }
        Ok(())
    }
}

/// Real transport over TCP 24. Each call opens its own connection, matching the
/// per-connection `/bin/sh` the device listener forks.
pub struct TcpShell {
    pub addr: SocketAddr,
    pub timeout: Duration,
    /// Port the one-shot `put` data listener binds on the device (8081, as the
    /// upstream installer uses). Overridable for loopback tests.
    pub data_port: u16,
}

impl TcpShell {
    pub fn new(addr: SocketAddr, timeout: Duration) -> Self {
        Self {
            addr,
            timeout,
            data_port: 8081,
        }
    }

    fn connect(&self) -> Result<TcpStream, String> {
        let s = TcpStream::connect_timeout(&self.addr, self.timeout)
            .map_err(|e| format!("connect {}: {e}", self.addr))?;
        s.set_write_timeout(Some(self.timeout)).ok();
        Ok(s)
    }

    /// Read until `marker` is seen or the deadline passes.
    fn read_until(
        stream: &mut TcpStream,
        marker: &str,
        deadline: Instant,
    ) -> Result<Vec<u8>, String> {
        const MAX: usize = 64 * 1024 * 1024;
        let mut raw = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("read: deadline exceeded before marker".to_string());
            }
            stream.set_read_timeout(Some(remaining)).ok();
            match stream.read(&mut buf) {
                Ok(0) => return Ok(raw),
                Ok(n) => {
                    raw.extend_from_slice(&buf[..n]);
                    if find_sub(&raw, marker.as_bytes()).is_some() {
                        return Ok(raw);
                    }
                    if raw.len() > MAX {
                        return Err("read: output exceeded 64 MiB cap before marker".to_string());
                    }
                }
                // A read timeout means the deadline was reached without the marker.
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    return Err("read: deadline exceeded before marker".to_string());
                }
                Err(e) => return Err(format!("read: {e}")),
            }
        }
    }
}

/// A per-call marker that is extremely unlikely to collide with command output.
fn make_marker() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("__orx_{:x}_{:x}__", std::process::id(), nanos)
}

fn find_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}

impl Shell for TcpShell {
    fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    fn run(&mut self, command: &str) -> Result<CmdResult, String> {
        let deadline = Instant::now() + self.timeout;
        let mut stream = self.connect()?;
        let marker = make_marker();
        // `{ cmd ; } 2>&1; printf 'MARKER %s.' "$?"` -- same framing as orbic-shell.py.
        let line = format!("{{ {command} ; }} 2>&1; printf '{marker} %s.' \"$?\"\n");
        stream
            .write_all(line.as_bytes())
            .map_err(|e| format!("write: {e}"))?;
        let raw = Self::read_until(&mut stream, &marker, deadline)?;
        let text = String::from_utf8_lossy(&raw);
        let at = text
            .find(&marker)
            .ok_or_else(|| "no end marker in device output".to_string())?;
        let output = text[..at].trim_end_matches('\n').to_string();
        // After the marker: " <status>."
        let tail = &text[at + marker.len()..];
        let status = tail
            .trim_start()
            .split('.')
            .next()
            .and_then(|d| d.trim().parse::<i32>().ok())
            .ok_or_else(|| "could not parse device exit status".to_string())?;
        Ok(CmdResult { output, status })
    }

    fn run_to(&mut self, command: &str, sink: &mut dyn Write) -> Result<i32, String> {
        let deadline = Instant::now() + self.timeout;
        let mut stream = self.connect()?;
        let marker = make_marker();
        let line = format!("{{ {command} ; }} 2>&1; printf '{marker} %s.' \"$?\"\n");
        stream
            .write_all(line.as_bytes())
            .map_err(|e| format!("write: {e}"))?;
        let mut splitter = MarkerSplitter::new(&marker);
        let mut buf = [0u8; 8192];
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("read: deadline exceeded before marker".to_string());
            }
            stream.set_read_timeout(Some(remaining)).ok();
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    splitter.push(&buf[..n], sink)?;
                    if splitter.found() {
                        break;
                    }
                }
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    return Err("read: deadline exceeded before marker".to_string());
                }
                Err(e) => return Err(format!("read: {e}")),
            }
        }
        splitter
            .status()
            .ok_or_else(|| "no end marker / unparseable status in device output".to_string())
    }

    fn put(&mut self, remote: &str, bytes: &[u8]) -> Result<(), String> {
        if !safe_remote(remote) {
            return Err(format!("unsafe remote path: {remote:?}"));
        }
        // The control channel is `nc -ll -e /bin/sh`; streaming a bulk payload
        // through `cat > file` is LOSSY -- the device nc drops bytes when its
        // pipe to the shell backs up (verified: a 4 KiB payload arrived as
        // ~3 KiB, independent of encoding). Instead use a one-shot device-side
        // listener (upstream's installer mechanism): the data path is
        // socket -> file with no shell pipe in it, which is lossless. send_file
        // still verifies the device-side sha256 afterward, so any residual race
        // (stale listener, port in use) fails closed rather than writing garbage.
        let data_port = self.data_port;
        let data_addr = SocketAddr::new(self.addr.ip(), data_port);
        let marker = make_marker();
        let mut ctl = self.connect()?;
        // Build a self-bounding, atomic, cleaned-up listener command:
        //  - write into an internal `.ncpart`, and `mv` onto `{remote}` ONLY on a
        //    clean transfer, so a bind failure never truncates `{remote}` (which
        //    matters because Shell::put is a public primitive: a caller may pass an
        //    existing file, not a disposable temp);
        //  - a watchdog kills the listener after LISTEN_SECS so a transfer that is
        //    never completed (we fail to connect, or a peer grabs and holds it)
        //    cannot leave `nc -l` orphaned in accept() on the device;
        //  - commit `{remote}` ONLY when nc exited cleanly AND the received byte
        //    count exactly equals what we sent AND the `mv` itself succeeded;
        //    otherwise `{remote}` is left untouched. This single size gate defeats
        //    a mid-transfer failure, a premature peer close, and a zero-byte
        //    connect -- none can commit a truncated or empty file (a same-length
        //    wrong payload is still caught by send_file's device-side sha256);
        //  - report the committing step's exit status in run()'s framing.
        const LISTEN_SECS: u32 = 45;
        let part = format!("{remote}.ncpart");
        let len = bytes.len();
        ctl.write_all(
            format!(
                "nc -l -p {data_port} > {part} & p=$!; (sleep {LISTEN_SECS}; kill $p 2>/dev/null) & w=$!; \
                 wait $p; st=$?; kill $w 2>/dev/null; ok=0; \
                 if [ $st -eq 0 ] && [ \"$(wc -c < {part} 2>/dev/null)\" = \"{len}\" ]; then mv {part} {remote} && ok=1; fi; \
                 [ $ok -eq 1 ] || rm -f {part}; [ $ok -eq 1 ] && st=0 || st=1; \
                 printf '{marker} %s.' \"$st\"\n"
            )
            .as_bytes(),
        )
        .map_err(|e| format!("start device listener: {e}"))?;

        // Peek the control channel BEFORE connecting: a healthy listener is silent
        // (nc blocks in accept(), nothing is printed until the transfer ends), so
        // ANY output now is an error state -- most importantly the completion
        // marker, which means the listener exited immediately (e.g. a stale
        // listener already holds the port). Fail closed in every such case so we
        // never stream this file's bytes into a listener bound to another path.
        ctl.set_read_timeout(Some(Duration::from_millis(300))).ok();
        let mut pbuf = [0u8; 256];
        match ctl.read(&mut pbuf) {
            Ok(0) => return Err("device control channel closed before transfer".to_string()),
            Ok(_) => {
                return Err(format!(
                    "device listener on port {data_port} produced output before any transfer \
                     (it did not bind -- a stale listener may hold the port); aborted before sending"
                ));
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => return Err(format!("device control channel: {e}")),
        }

        // Connect the data channel; retry briefly while the listener binds.
        let connect_deadline = Instant::now() + Duration::from_secs(5);
        let data = loop {
            match TcpStream::connect_timeout(&data_addr, Duration::from_secs(1)) {
                Ok(s) => break Some(s),
                Err(_) if Instant::now() < connect_deadline => {
                    std::thread::sleep(Duration::from_millis(150));
                }
                Err(_) => break None,
            }
        };
        let mut data = match data {
            Some(s) => s,
            None => {
                // Do NOT poke the port: a zero-byte connect would give the listener
                // a clean EOF, and only the device-side size gate (received !=
                // {len}) stops that from committing an empty file. Leave the
                // watchdog to reap the orphaned listener after LISTEN_SECS.
                return Err(format!("could not connect to device data port {data_port}"));
            }
        };
        data.set_write_timeout(Some(self.timeout)).ok();
        data.write_all(bytes)
            .map_err(|e| format!("write data: {e}"))?;
        data.shutdown(Shutdown::Write)
            .map_err(|e| format!("data shutdown: {e}"))?;
        drop(data); // EOF -> device nc writes the file and exits

        // Read the listener's completion: wait for the full `{marker} <status>.`
        // (the trailing '.' guarantees the status is complete even if TCP split it).
        let deadline = Instant::now() + self.timeout;
        let end = format!("{marker} ");
        let mut raw: Vec<u8> = Vec::new(); // the peek consumed nothing on the healthy path
        let mut rbuf = [0u8; 256];
        loop {
            if let Some(at) = find_sub(&raw, end.as_bytes()) {
                let tail = &raw[at + end.len()..];
                if let Some(dot) = tail.iter().position(|&b| b == b'.') {
                    let status: i32 = std::str::from_utf8(&tail[..dot])
                        .ok()
                        .and_then(|d| d.trim().parse().ok())
                        .ok_or_else(|| "could not parse device listener status".to_string())?;
                    return if status == 0 {
                        Ok(())
                    } else {
                        Err(format!("device listener exited with status {status}"))
                    };
                }
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("device listener did not report completion".to_string());
            }
            ctl.set_read_timeout(Some(remaining)).ok();
            match ctl.read(&mut rbuf) {
                Ok(0) => return Err("device control channel closed before status".to_string()),
                Ok(n) => raw.extend_from_slice(&rbuf[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    return Err("device listener did not report completion".to_string());
                }
                Err(e) => return Err(format!("device control channel: {e}")),
            }
        }
    }
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::collections::HashMap;

    /// In-memory device for tests: `put` writes to a fake filesystem, `run`
    /// answers `sha256sum <path>` from it (optionally corrupted) and canned
    /// responses otherwise.
    #[derive(Default)]
    pub struct FakeShell {
        pub fs: HashMap<String, Vec<u8>>,
        pub responses: HashMap<String, CmdResult>,
        /// `sha256sum` returns a wrong hash (simulated corruption in transit).
        pub corrupt: bool,
        /// `put` silently drops the data (simulated short write / failed transfer).
        pub drop_writes: bool,
        /// Every command passed to `run`, in order (for sequence assertions).
        pub cmds: Vec<String>,
    }

    impl Shell for FakeShell {
        fn run(&mut self, command: &str) -> Result<CmdResult, String> {
            self.cmds.push(command.to_string());
            if let Some(remote) = command.strip_prefix("sha256sum ") {
                let remote = remote.trim();
                return Ok(match self.fs.get(remote) {
                    Some(bytes) => {
                        let h = if self.corrupt {
                            "0".repeat(64)
                        } else {
                            crate::sha256::hex(bytes)
                        };
                        CmdResult {
                            output: format!("{h}  {remote}"),
                            status: 0,
                        }
                    }
                    None => CmdResult {
                        output: format!(
                            "sha256sum: can't open '{remote}': No such file or directory"
                        ),
                        status: 1,
                    },
                });
            }
            Ok(self.responses.get(command).cloned().unwrap_or(CmdResult {
                output: String::new(),
                status: 0,
            }))
        }

        fn run_to(&mut self, command: &str, sink: &mut dyn Write) -> Result<i32, String> {
            let r = self.run(command)?;
            sink.write_all(r.output.as_bytes())
                .map_err(|e| format!("sink: {e}"))?;
            Ok(r.status)
        }

        fn put(&mut self, remote: &str, bytes: &[u8]) -> Result<(), String> {
            if !self.drop_writes {
                self.fs.insert(remote.to_string(), bytes.to_vec());
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeShell;
    use super::*;

    #[test]
    fn safe_remote_accepts_fixed_paths_rejects_tricks() {
        assert!(safe_remote("/data/rayhunter/rayhunter-daemon"));
        assert!(safe_remote("/etc/init.d/rayhunter_daemon"));
        assert!(!safe_remote("relative/path"));
        assert!(!safe_remote("/tmp/a b")); // space
        assert!(!safe_remote("/tmp/a;rm -rf /")); // metachars
        assert!(!safe_remote("/data/../etc/passwd")); // traversal
        assert!(!safe_remote("/tmp/$(id)")); // substitution chars
    }

    #[test]
    fn send_file_accepts_matching_hash() {
        let mut sh = FakeShell::default();
        assert!(sh.send_file("/data/rayhunter/x", b"hello world").is_ok());
        assert_eq!(sh.fs.get("/data/rayhunter/x").unwrap(), b"hello world");
    }

    #[test]
    fn send_file_rejects_corruption() {
        let mut sh = FakeShell {
            corrupt: true,
            ..Default::default()
        };
        let err = sh.send_file("/data/rayhunter/x", b"payload").unwrap_err();
        assert!(err.contains("hash mismatch"), "{err}");
    }

    #[test]
    fn send_file_rejects_failed_transfer() {
        let mut sh = FakeShell {
            drop_writes: true,
            ..Default::default()
        };
        // Nothing written -> device sha256sum reports missing file -> error.
        let err = sh.send_file("/data/rayhunter/x", b"payload").unwrap_err();
        assert!(
            err.contains("sha256sum failed") || err.contains("No such file"),
            "{err}"
        );
    }

    #[test]
    fn send_file_rejects_unsafe_path_before_touching_device() {
        let mut sh = FakeShell::default();
        assert!(
            sh.send_file("/tmp/a;reboot", b"x")
                .unwrap_err()
                .contains("unsafe")
        );
        assert!(sh.fs.is_empty()); // never written
    }

    // --- Real TcpShell against a loopback stand-in for the device listener ---

    #[test]
    fn tcpshell_run_parses_output_and_status() {
        use std::net::TcpListener;
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut req = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = s.read(&mut buf).unwrap();
                if n == 0 || req.contains(&b'\n') {
                    break;
                }
                req.extend_from_slice(&buf[..n]);
                if req.contains(&b'\n') {
                    break;
                }
            }
            // Extract the marker run() chose from `printf 'MARKER %s.' ...`.
            let line = String::from_utf8_lossy(&req);
            let marker = line
                .split("printf '")
                .nth(1)
                .and_then(|r| r.split(' ').next())
                .unwrap()
                .to_string();
            s.write_all(format!("line1\nline2\n{marker} 3.").as_bytes())
                .unwrap();
        });
        let mut sh = TcpShell::new(addr, Duration::from_secs(2));
        let r = sh.run("whatever").unwrap();
        assert_eq!(r.output, "line1\nline2");
        assert_eq!(r.status, 3);
        server.join().unwrap();
    }

    #[test]
    fn tcpshell_put_uses_device_listener_losslessly() {
        use std::net::TcpListener;
        // Control channel (stands in for :24) and a free data port (for :8081).
        let ctl = TcpListener::bind("127.0.0.1:0").unwrap();
        let ctl_addr = ctl.local_addr().unwrap();
        let data = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data.local_addr().unwrap().port();
        // A payload with the full byte range (the real bug ate bytes over the
        // shell pipe); the device-listener path must deliver it exactly.
        let payload: Vec<u8> = (0..6000u32).map(|i| (i % 256) as u8).collect();
        let expect = payload.clone();
        let server = std::thread::spawn(move || {
            let (mut c, _) = ctl.accept().unwrap();
            // Read the listener command line.
            let mut cmd = Vec::new();
            let mut b = [0u8; 1];
            while c.read(&mut b).unwrap() == 1 {
                cmd.push(b[0]);
                if b[0] == b'\n' {
                    break;
                }
            }
            let line = String::from_utf8_lossy(&cmd);
            assert!(line.contains(&format!("nc -l -p {data_port}")));
            let marker = line
                .split("printf '")
                .nth(1)
                .and_then(|r| r.split(' ').next())
                .unwrap()
                .to_string();
            // Accept the data connection and read the whole payload (no shell pipe).
            let (mut d, _) = data.accept().unwrap();
            let mut got = Vec::new();
            d.read_to_end(&mut got).unwrap();
            // Report the listener finished with status 0.
            c.write_all(format!("{marker} 0.").as_bytes()).unwrap();
            got
        });
        let mut sh = TcpShell::new(ctl_addr, Duration::from_secs(5));
        sh.data_port = data_port;
        sh.put("/data/rayhunter/x", &payload).unwrap();
        let got = server.join().unwrap();
        assert_eq!(got, expect, "device-listener put must be byte-exact");
    }

    #[test]
    fn marker_splitter_separates_payload_and_status_across_chunks() {
        let marker = "__orx_abc_def__";
        let payload: Vec<u8> = (0..5000u32).map(|i| (i % 253) as u8).collect();
        let mut full = payload.clone();
        full.extend_from_slice(format!("{marker} 7.").as_bytes());
        // The marker must never leak into the sink, at any chunk size -- including
        // ones that split the marker itself.
        for chunk in [1usize, 2, 7, 8, 15, 16, 4096, full.len()] {
            let mut sp = MarkerSplitter::new(marker);
            let mut sink = Vec::new();
            for part in full.chunks(chunk) {
                sp.push(part, &mut sink).unwrap();
            }
            assert!(sp.found(), "chunk {chunk}: marker not found");
            assert_eq!(sink, payload, "chunk {chunk}: payload mismatch");
            assert!(
                !find_sub(&sink, marker.as_bytes()).is_some(),
                "chunk {chunk}: marker leaked"
            );
            assert_eq!(sp.status(), Some(7), "chunk {chunk}: status");
        }
    }

    #[test]
    fn tcpshell_run_to_streams_payload_and_status() {
        use std::net::TcpListener;
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut req = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = s.read(&mut buf).unwrap();
                if n == 0 || req.contains(&b'\n') {
                    break;
                }
                req.extend_from_slice(&buf[..n]);
                if req.contains(&b'\n') {
                    break;
                }
            }
            let line = String::from_utf8_lossy(&req);
            let marker = line
                .split("printf '")
                .nth(1)
                .and_then(|r| r.split(' ').next())
                .unwrap()
                .to_string();
            s.write_all(format!("PAYLOAD-BYTES{marker} 0.").as_bytes())
                .unwrap();
        });
        let mut sh = TcpShell::new(addr, Duration::from_secs(2));
        let mut sink = Vec::new();
        let status = sh.run_to("tar -cf - .", &mut sink).unwrap();
        assert_eq!(status, 0);
        assert_eq!(sink, b"PAYLOAD-BYTES");
        server.join().unwrap();
    }

    #[test]
    fn tcpshell_run_times_out_when_no_marker() {
        use std::net::TcpListener;
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            std::thread::sleep(Duration::from_millis(600));
            drop(s);
        });
        let mut sh = TcpShell::new(addr, Duration::from_millis(200));
        let err = sh.run("sleep").unwrap_err();
        assert!(err.contains("deadline") || err.contains("marker"), "{err}");
        server.join().unwrap();
    }
}

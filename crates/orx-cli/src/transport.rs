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

/// Device shell transport. `run` and `put` are the primitives; `send_file` is a
/// verified transfer built on them and must not be overridden.
pub trait Shell {
    /// Run one command; return its combined output and exit status.
    fn run(&mut self, command: &str) -> Result<CmdResult, String>;

    /// Transfer raw `bytes` to `remote` with NO verification.
    fn put(&mut self, remote: &str, bytes: &[u8]) -> Result<(), String>;

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
}

impl TcpShell {
    pub fn new(addr: SocketAddr, timeout: Duration) -> Self {
        Self { addr, timeout }
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
                    if find_sub(&raw, marker.as_bytes()).is_some() || raw.len() > MAX {
                        return Ok(raw);
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

    fn put(&mut self, remote: &str, bytes: &[u8]) -> Result<(), String> {
        if !safe_remote(remote) {
            return Err(format!("unsafe remote path: {remote:?}"));
        }
        let mut stream = self.connect()?;
        // `cat > remote` then stream bytes; closing the write half is EOF to cat.
        stream
            .write_all(format!("cat > {remote}\n").as_bytes())
            .map_err(|e| format!("write cmd: {e}"))?;
        stream
            .write_all(bytes)
            .map_err(|e| format!("write data: {e}"))?;
        stream
            .shutdown(Shutdown::Write)
            .map_err(|e| format!("shutdown: {e}"))?;
        // Drain until the device closes, bounded by the timeout, so the write lands.
        let deadline = Instant::now() + self.timeout;
        let mut sink = [0u8; 4096];
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            stream.set_read_timeout(Some(remaining)).ok();
            match stream.read(&mut sink) {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        Ok(())
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
    }

    impl Shell for FakeShell {
        fn run(&mut self, command: &str) -> Result<CmdResult, String> {
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

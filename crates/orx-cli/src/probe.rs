//! Network reachability probes for the device web interfaces.

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// Default address of the Orbic on its RNDIS / Wi-Fi network.
pub const DEFAULT_DEVICE_IP: &str = "192.168.1.1";
/// Port the Rayhunter daemon serves its web UI on.
pub const RAYHUNTER_PORT: u16 = 8080;
/// Port the stock Orbic web UI serves on.
pub const STOCK_UI_PORT: u16 = 80;

/// Whether a TCP connection to `addr` succeeds within `timeout`.
pub fn tcp_open(addr: SocketAddr, timeout: Duration) -> bool {
    TcpStream::connect_timeout(&addr, timeout).is_ok()
}

/// Parse `ip` and `port` into a socket address.
pub fn socket(ip: &str, port: u16) -> Result<SocketAddr, String> {
    format!("{ip}:{port}")
        .parse()
        .map_err(|e| format!("invalid device address {ip:?}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn socket_rejects_garbage() {
        assert!(socket("not-an-ip", 80).is_err());
        assert_eq!(socket("192.168.1.1", 8080).unwrap().port(), 8080);
    }

    #[test]
    fn tcp_open_detects_listener_and_closed_port() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        assert!(tcp_open(addr, Duration::from_millis(500)));
        drop(l);
        assert!(!tcp_open(addr, Duration::from_millis(500)));
    }
}

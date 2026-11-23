use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use orx_cli::{probe, usb};

const USAGE: &str = "\
orx - Orbic-Rayhunter host helper

USAGE:
    orx doctor [--ip <addr>] [--json]   Detect the Orbic over USB and probe its web UIs
    orx --version
    orx --help

EXIT CODES:
    0   a device was found over USB, or a device web UI answered
    1   nothing found (no USB device and no reachable web UI)
    2   usage error, or the host environment could not be read";

/// Escape a string for embedding in the compact JSON `--json` output.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn opt_json(v: Option<&str>) -> String {
    v.map_or_else(|| "null".to_string(), |s| format!("\"{}\"", json_escape(s)))
}

/// A device-reported version is trusted only if it is a plain version token:
/// 1-64 chars of `[0-9A-Za-z.+-]`. Anything else (control chars, escape
/// sequences, overlong input) is dropped rather than printed.
fn sane_version(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'))
}

fn doctor(ip: &str, json: bool) -> ExitCode {
    let (stock, rh) = match (
        probe::socket(ip, probe::STOCK_UI_PORT),
        probe::socket(ip, probe::RAYHUNTER_PORT),
    ) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    let devices = match usb::scan(Path::new("/sys/bus/usb/devices")) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: cannot read /sys/bus/usb/devices: {e}");
            return ExitCode::from(2);
        }
    };

    let t = Duration::from_secs(2);
    let stock_up = probe::tcp_open(stock, t);
    let rh_up = probe::tcp_open(rh, t);
    // Read the running Rayhunter version only when the port answers; a failure
    // (version None) is informational, never an error.
    let rh_version = if rh_up {
        probe::http_get(rh, "/api/system-stats", t)
            .ok()
            .and_then(|b| probe::json_str_field(&b, "rayhunter_version"))
            // The version is attacker-influenceable (a compromised daemon or a
            // MITM on the local link), so reject anything that is not a plain
            // version token before it reaches a terminal or JSON output.
            .filter(|v| sane_version(v))
    } else {
        None
    };

    if json {
        let devs: Vec<String> = devices
            .iter()
            .map(|d| {
                format!(
                    "{{\"sysfs\":\"{}\",\"vid\":\"{:04x}\",\"pid\":\"{:04x}\",\"product\":{},\"serial\":{},\"mode\":\"{}\"}}",
                    json_escape(&d.sysfs_name),
                    d.vid,
                    d.pid,
                    opt_json(d.product.as_deref()),
                    opt_json(d.serial.as_deref()),
                    json_escape(&d.mode.describe()),
                )
            })
            .collect();
        println!(
            "{{\"usb\":[{}],\"stock_ui\":{{\"addr\":\"{stock}\",\"reachable\":{stock_up}}},\"rayhunter\":{{\"addr\":\"{rh}\",\"reachable\":{rh_up},\"version\":{}}}}}",
            devs.join(","),
            opt_json(rh_version.as_deref()),
        );
    } else {
        if devices.is_empty() {
            println!("usb:        no Qualcomm (05c6) device found");
        }
        for d in &devices {
            println!(
                "usb:        {} {:04x}:{:04x} {} serial={} mode={}",
                d.sysfs_name,
                d.vid,
                d.pid,
                // `product` is the model only; the serial is printed once, in its own field.
                d.product.as_deref().unwrap_or("?"),
                d.serial.as_deref().unwrap_or("?"),
                d.mode.describe()
            );
        }
        println!(
            "stock ui:   {stock} {}",
            if stock_up { "reachable" } else { "unreachable" }
        );
        let rh_state = match (rh_up, rh_version.as_deref()) {
            (true, Some(v)) => format!("reachable (Rayhunter {v})"),
            (true, None) => "reachable".to_string(),
            (false, _) => "not running / not installed".to_string(),
        };
        println!("rayhunter:  {rh} {rh_state}");
    }

    if devices.is_empty() && !stock_up && !rh_up {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("doctor") => {
            let mut ip = probe::DEFAULT_DEVICE_IP.to_string();
            let mut json = false;
            let mut rest = args[1..].iter();
            while let Some(arg) = rest.next() {
                match arg.as_str() {
                    "--json" => json = true,
                    "--ip" => match rest.next() {
                        Some(v) => ip = v.clone(),
                        None => {
                            eprintln!("error: --ip needs a value\n\n{USAGE}");
                            return ExitCode::from(2);
                        }
                    },
                    other => {
                        eprintln!("error: unexpected argument {other:?}\n\n{USAGE}");
                        return ExitCode::from(2);
                    }
                }
            }
            doctor(&ip, json)
        }
        Some("--version" | "-V") => {
            println!("orx {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") | None => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("error: unknown command {other:?}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

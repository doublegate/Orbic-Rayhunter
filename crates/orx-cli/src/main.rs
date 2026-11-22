use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use orx_cli::{probe, usb};

const USAGE: &str = "\
orx - Orbic-Rayhunter host helper

USAGE:
    orx doctor [--ip <addr>]    Detect the Orbic over USB and probe its web UIs
    orx --version
    orx --help";

fn doctor(ip: &str) -> ExitCode {
    let addr_ok = |port| probe::socket(ip, port);
    let (stock, rh) = match (
        addr_ok(probe::STOCK_UI_PORT),
        addr_ok(probe::RAYHUNTER_PORT),
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

    if devices.is_empty() {
        println!("usb:        no Qualcomm (05c6) device found");
    }
    for d in &devices {
        println!(
            "usb:        {} {:04x}:{:04x} {} serial={} mode={}",
            d.sysfs_name,
            d.vid,
            d.pid,
            d.product.as_deref().unwrap_or("?"),
            d.serial.as_deref().unwrap_or("?"),
            d.mode.describe()
        );
    }

    let t = Duration::from_secs(2);
    let stock_up = probe::tcp_open(stock, t);
    let rh_up = probe::tcp_open(rh, t);
    println!(
        "stock ui:   {stock} {}",
        if stock_up { "reachable" } else { "unreachable" }
    );
    println!(
        "rayhunter:  {rh} {}",
        if rh_up {
            "reachable"
        } else {
            "not running / not installed"
        }
    );

    if devices.is_empty() && !stock_up {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("doctor") => {
            let ip = match args.get(1).map(String::as_str) {
                None => probe::DEFAULT_DEVICE_IP,
                Some("--ip") => match args.get(2) {
                    Some(ip) => ip.as_str(),
                    None => {
                        eprintln!("error: --ip needs a value\n\n{USAGE}");
                        return ExitCode::from(2);
                    }
                },
                Some(other) => {
                    eprintln!("error: unexpected argument {other:?}\n\n{USAGE}");
                    return ExitCode::from(2);
                }
            };
            doctor(ip)
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

//! USB discovery through Linux sysfs (`/sys/bus/usb/devices`).

use std::fs;
use std::io;
use std::path::Path;

/// Qualcomm's USB vendor ID, used by the Orbic RC400L in every mode.
pub const QUALCOMM_VID: u16 = 0x05c6;

/// Operating mode inferred from the USB product ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrbicMode {
    /// Stock composition (RNDIS + CDC data). Measured on ORB400L_V1.2.0_BVZRT.
    Normal,
    /// Debug composition (ADB + DIAG + AT) after the vendor USB control request.
    /// Reported by community research (ref-docs/orbic-firmware-v1.2.0.md); not yet
    /// measured on this unit.
    Debug,
    /// Qualcomm Emergency Download (EDL / Sahara-Firehose) mode.
    Edl,
    /// A Qualcomm device with a product ID this tool does not recognise yet.
    UnknownQualcomm(u16),
}

impl OrbicMode {
    /// Classify a vendor/product pair; `None` for non-Qualcomm devices.
    pub fn classify(vid: u16, pid: u16) -> Option<Self> {
        if vid != QUALCOMM_VID {
            return None;
        }
        Some(match pid {
            0xf626 => Self::Normal,
            0xf601 => Self::Debug,
            0x9008 => Self::Edl,
            other => Self::UnknownQualcomm(other),
        })
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Normal => "normal (RNDIS + CDC data, stock composition)".to_string(),
            Self::Debug => "debug (ADB + DIAG + AT; root via AT+SYSCMD)".to_string(),
            Self::Edl => "EDL (Qualcomm emergency download)".to_string(),
            Self::UnknownQualcomm(pid) => {
                format!("unrecognised Qualcomm composition (PID {pid:04x})")
            }
        }
    }
}

/// A Qualcomm USB device found in sysfs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualcommDevice {
    pub sysfs_name: String,
    pub vid: u16,
    pub pid: u16,
    pub product: Option<String>,
    pub serial: Option<String>,
    pub mode: OrbicMode,
}

fn read_attr(dir: &Path, name: &str) -> Option<String> {
    fs::read_to_string(dir.join(name))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn parse_hex_u16(s: &str) -> Option<u16> {
    u16::from_str_radix(s.trim(), 16).ok()
}

/// Byte offset of the last `_SN:` marker that starts `product` or follows
/// whitespace. A marker in the middle of a word (`FOO_SN:`) is not one.
fn sn_marker(product: &str) -> Option<usize> {
    product.rmatch_indices("_SN:").map(|(i, _)| i).find(|&i| {
        i == 0
            || product[..i]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace)
    })
}

/// The serial embedded in a product string such as `MDM9207-MTP _SN:0123ABCD`.
///
/// The RC400L (V1.2.0) exposes no `serial` sysfs attribute in normal mode and
/// carries the serial here instead. Only a plain ASCII alphanumeric token of
/// 1..=64 characters, directly after a word-start `_SN:`, is accepted, so
/// arbitrary product text is never reported as a serial.
pub fn serial_from_product(product: &str) -> Option<String> {
    let tail = &product[sn_marker(product)? + "_SN:".len()..];
    (!tail.is_empty() && tail.len() <= 64 && tail.bytes().all(|b| b.is_ascii_alphanumeric()))
        .then(|| tail.to_string())
}

/// The product string without its `_SN:` serial suffix, for display.
///
/// The product string carries a device identifier on this hardware, so anything
/// that prints or logs it should print this model text and the serial
/// separately (so a redacting caller masks the one field that holds it).
pub fn product_model(product: &str) -> &str {
    match sn_marker(product) {
        Some(i) if serial_from_product(product).is_some() => product[..i].trim_end(),
        _ => product,
    }
}

/// Read one sysfs USB device directory. Returns `None` for interfaces,
/// hubs without IDs, or non-Qualcomm devices.
pub fn read_device(dir: &Path) -> Option<QualcommDevice> {
    let vid = parse_hex_u16(&read_attr(dir, "idVendor")?)?;
    let pid = parse_hex_u16(&read_attr(dir, "idProduct")?)?;
    let mode = OrbicMode::classify(vid, pid)?;
    let product = read_attr(dir, "product");
    let serial =
        read_attr(dir, "serial").or_else(|| product.as_deref().and_then(serial_from_product));
    Some(QualcommDevice {
        sysfs_name: dir.file_name()?.to_string_lossy().into_owned(),
        vid,
        pid,
        product,
        serial,
        mode,
    })
}

/// Scan a sysfs USB devices root (normally `/sys/bus/usb/devices`).
pub fn scan(root: &Path) -> io::Result<Vec<QualcommDevice>> {
    let mut found: Vec<QualcommDevice> = fs::read_dir(root)?
        .filter_map(Result::ok)
        .filter_map(|e| read_device(&e.path()))
        .collect();
    found.sort_by(|a, b| a.sysfs_name.cmp(&b.sysfs_name));
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!("orx-test-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fake_dev(root: &Path, name: &str, attrs: &[(&str, &str)]) {
        let d = root.join(name);
        fs::create_dir_all(&d).unwrap();
        for (k, v) in attrs {
            fs::write(d.join(k), format!("{v}\n")).unwrap();
        }
    }

    #[test]
    fn classify_known_and_unknown_pids() {
        assert_eq!(OrbicMode::classify(0x05c6, 0xf626), Some(OrbicMode::Normal));
        assert_eq!(OrbicMode::classify(0x05c6, 0xf601), Some(OrbicMode::Debug));
        assert_eq!(OrbicMode::classify(0x05c6, 0x9008), Some(OrbicMode::Edl));
        assert_eq!(
            OrbicMode::classify(0x05c6, 0x1234),
            Some(OrbicMode::UnknownQualcomm(0x1234))
        );
        assert_eq!(OrbicMode::classify(0x18d1, 0xf626), None);
    }

    #[test]
    fn scan_finds_only_qualcomm_devices() {
        let t = TempDir::new("scan");
        fake_dev(
            &t.0,
            "1-4",
            &[
                ("idVendor", "05c6"),
                ("idProduct", "f626"),
                ("product", "MDM9207-MTP _SN:0000TEST"),
                ("serial", "0000TEST"),
            ],
        );
        fake_dev(&t.0, "1-2", &[("idVendor", "046d"), ("idProduct", "c52b")]);
        fake_dev(&t.0, "1-4:1.0", &[("bInterfaceClass", "e0")]);

        let found = scan(&t.0).unwrap();
        assert_eq!(found.len(), 1);
        let d = &found[0];
        assert_eq!(d.sysfs_name, "1-4");
        assert_eq!(d.mode, OrbicMode::Normal);
        assert_eq!(d.serial.as_deref(), Some("0000TEST"));
    }

    #[test]
    fn serial_falls_back_to_the_product_string() {
        // The RC400L (V1.2.0) exposes no `serial` attribute in normal mode; the
        // serial is embedded in the product string instead (measured 2026-11-23).
        let t = TempDir::new("sn");
        fake_dev(
            &t.0,
            "1-1",
            &[
                ("idVendor", "05c6"),
                ("idProduct", "f626"),
                ("product", "MDM9207-MTP _SN:0000TEST"),
            ],
        );
        fake_dev(
            &t.0,
            "1-2",
            &[
                ("idVendor", "05c6"),
                ("idProduct", "f601"),
                ("product", "MDM9207-MTP"),
            ],
        );
        fake_dev(
            &t.0,
            "1-3",
            &[
                ("idVendor", "05c6"),
                ("idProduct", "9008"),
                ("product", "MDM9207-MTP _SN:not a serial!"),
            ],
        );
        let found = scan(&t.0).unwrap();
        assert_eq!(found[0].serial.as_deref(), Some("0000TEST"));
        assert_eq!(found[1].serial, None);
        assert_eq!(found[2].serial, None);
    }

    #[test]
    fn serial_from_product_takes_only_a_plain_token() {
        assert_eq!(
            serial_from_product("X _SN:F00D1234"),
            Some("F00D1234".into())
        );
        assert_eq!(serial_from_product("X _SN:"), None);
        assert_eq!(serial_from_product("X"), None);
        assert_eq!(serial_from_product("X _SN:ab/cd"), None);
        assert_eq!(
            serial_from_product(&format!("X _SN:{}", "A".repeat(65))),
            None
        );
    }

    #[test]
    fn a_real_serial_attribute_wins_over_the_product_string() {
        let t = TempDir::new("prec");
        fake_dev(
            &t.0,
            "1-1",
            &[
                ("idVendor", "05c6"),
                ("idProduct", "f626"),
                ("product", "MDM9207-MTP _SN:FROMPRODUCT"),
                ("serial", "FROMSYSFS"),
            ],
        );
        assert_eq!(scan(&t.0).unwrap()[0].serial.as_deref(), Some("FROMSYSFS"));
    }

    #[test]
    fn serial_token_bounds_and_position() {
        let max = "A".repeat(64);
        assert_eq!(serial_from_product(&format!("X _SN:{max}")), Some(max));
        assert_eq!(serial_from_product("_SN:AB12"), Some("AB12".into()));
        // `_SN:` must start the string or follow whitespace, and the token must follow it
        // directly: mid-word markers and padded tokens are not serials.
        assert_eq!(serial_from_product("FOO_SN:1234"), None);
        assert_eq!(serial_from_product("X _SN: 1234"), None);
    }

    #[test]
    fn model_strips_the_serial_from_the_product_string() {
        assert_eq!(product_model("MDM9207-MTP _SN:0000TEST"), "MDM9207-MTP");
        assert_eq!(product_model("MDM9207-MTP"), "MDM9207-MTP");
        assert_eq!(product_model("FOO_SN:1234"), "FOO_SN:1234");
    }

    #[test]
    fn malformed_ids_are_skipped() {
        let t = TempDir::new("malformed");
        fake_dev(&t.0, "2-1", &[("idVendor", "zzzz"), ("idProduct", "f626")]);
        fake_dev(&t.0, "2-2", &[("idVendor", "05c6")]);
        assert!(scan(&t.0).unwrap().is_empty());
    }
}

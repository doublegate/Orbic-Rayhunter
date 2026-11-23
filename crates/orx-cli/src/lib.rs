//! Host-side helpers for running Rayhunter on the Orbic RC400L.
//!
//! v0.1 scope: detect the device over USB (sysfs) and probe the Rayhunter
//! web UI. Backup, install and verify commands land in later phases
//! (see `to-dos/`).

pub mod probe;
pub mod sha256;
pub mod usb;

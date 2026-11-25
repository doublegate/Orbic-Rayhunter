//! Host-side helpers for running Rayhunter on the Orbic RC400L.
//!
//! v0.1 scope: detect the device over USB (sysfs) and probe the Rayhunter
//! web UI. Backup, install and verify commands land in later phases
//! (see `to-dos/`).

pub mod backup;
pub mod check;
pub mod install;
pub mod json;
pub mod probe;
pub mod pull;
pub mod sha256;
pub mod transport;
pub mod usb;
pub mod verify;

//! `opd1-core`: pure application logic for OPD1 TRCKER.
//!
//! No Tauri dependency lives here so this crate stays unit-testable on any
//! platform (including the Linux dev VPS). Later tasks add stats math,
//! party detection, and the SQLite layer.

pub mod lockfile;
pub mod presence;
pub mod shards;

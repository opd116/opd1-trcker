//! `opd1-core`: pure application logic for OPD1 TRCKER.
//!
//! No Tauri dependency lives here so this crate stays unit-testable on any
//! platform (including the Linux dev VPS). Later tasks add party
//! detection and card assembly on top of `db`/`stats`.

pub mod db;
pub mod lockfile;
pub mod presence;
pub mod shards;
pub mod stats;

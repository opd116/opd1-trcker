//! `opd1-core`: pure application logic for OPD1 TRCKER.
//!
//! No Tauri dependency lives here so this crate stays unit-testable on any
//! platform (including the Linux dev VPS). `model` assembles the cards the
//! UI shows; `names` (hidden-name rules) and `party` (party detection)
//! decide two of their fields from what `db` knows.

pub mod db;
pub mod lockfile;
pub mod model;
pub mod mystats;
pub mod names;
pub mod party;
pub mod presence;
pub mod shards;
pub mod stats;

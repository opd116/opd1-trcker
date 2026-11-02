//! SQLite storage: schema/migrations here, the typed sync repository in
//! [`repo`]. No Tauri dependency — `opd1-core` stays unit-testable on the
//! Linux dev VPS (see CLAUDE.md's "Smart data (SQLite)").

use std::path::Path;

use rusqlite::{Connection, Result};

pub mod history;
pub mod marks;
pub mod repo;

/// Ordered, append-only list of migrations. Each entry is applied exactly
/// once (tracked by `PRAGMA user_version`, which lands at `MIGRATIONS.len()`
/// once all of them have run). A committed migration is frozen: migration 1
/// (`schema.sql`) froze at `c9ad7f4` (R89) and `tests/db.rs`'s
/// `migration1_is_frozen` fails the day anyone edits it. A later task only
/// appends to this list; there is no self-heal and no in-place edit.
const MIGRATIONS: &[&str] = &[
    include_str!("schema.sql"),
    include_str!("migration2.sql"),
    include_str!("migration3.sql"),
    include_str!("migration4.sql"),
];

/// Opens (creating if needed) the SQLite database at `path`, checks its
/// schema version, sets its pragmas, and applies any migrations from
/// [`MIGRATIONS`] not yet applied — idempotent on reopen, since a connection
/// whose `user_version` already matches `MIGRATIONS.len()` runs no SQL at all.
pub fn open_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;

    let current_version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let target_version = MIGRATIONS.len() as i64;

    // R203/V03: a negative version (corrupt/hostile header) or one from a
    // newer build is refused explicitly. The old slice `[current..]` panicked
    // on a negative value, and a newer schema was silently accepted. The check
    // runs before the WAL pragma, so a refusal does not rewrite the header and
    // never runs a migration/downgrade.
    if current_version < 0 || current_version > target_version {
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!(
                "unsupported schema version {current_version}; this build supports 0..={target_version}"
            )),
        ));
    }

    // WAL + NORMAL: the standard "don't fsync on every write, but don't
    // corrupt on a crash either" pair for a single-writer desktop app.
    // (SQLite silently keeps `:memory:` databases on its own in-memory
    // journal regardless of what we ask for here — harmless for tests.)
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;

    if current_version < target_version {
        let tx = conn.unchecked_transaction()?;
        for migration in &MIGRATIONS[current_version as usize..] {
            tx.execute_batch(migration)?;
        }
        tx.pragma_update(None, "user_version", target_version)?;
        tx.commit()?;
    }

    Ok(conn)
}

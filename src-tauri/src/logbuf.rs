//! The in-memory ring-buffer logger (ruling R30): the last [`CAPACITY`]
//! log lines, returned as one string by the `copy_log` command (the UI
//! puts it on the clipboard on Ctrl+Shift+L — no clipboard plugin).
//!
//! What is logged is decided at each call site, and the rule there is
//! absolute: never a token, password, JWT, lockfile secret, or any
//! player's name (visible or hidden) — puuids are shortened to their first
//! 8 characters (see [`short`]) so a log line can still be correlated
//! without carrying a full account id around.
//!
//! Only this app's own targets (`opd1_*`) are kept at Info (Debug in
//! debug builds); every dependency (tauri, reqwest, hyper, rustls, ...)
//! only gets through at Warn or above, so a user-shared log stays short
//! and about us.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use log::{Level, LevelFilter, Log, Metadata, Record};

/// How many lines `copy_log` returns (R30).
pub const CAPACITY: usize = 500;

/// A bounded line buffer: pushing past `cap` drops the oldest line.
pub struct LineBuffer {
    cap: usize,
    lines: VecDeque<String>,
}

impl LineBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            lines: VecDeque::with_capacity(cap.min(64)),
        }
    }

    pub fn push(&mut self, line: String) {
        if self.cap == 0 {
            return;
        }
        while self.lines.len() >= self.cap {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// Oldest first, one line per `\n`.
    pub fn joined(&self) -> String {
        let mut out = String::with_capacity(self.lines.iter().map(|l| l.len() + 1).sum());
        for (i, line) in self.lines.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(line);
        }
        out
    }
}

struct RingLogger {
    own_level: Level,
    lines: Mutex<LineBuffer>,
}

impl RingLogger {
    fn accepts(&self, metadata: &Metadata) -> bool {
        if metadata.target().starts_with("opd1") {
            metadata.level() <= self.own_level
        } else {
            metadata.level() <= Level::Warn
        }
    }
}

impl Log for RingLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.accepts(metadata)
    }

    fn log(&self, record: &Record) {
        if !self.accepts(record.metadata()) {
            return;
        }
        let line = format_line(now_ms(), record.level(), record.target(), record.args());
        if cfg!(debug_assertions) {
            eprintln!("{line}");
        }
        self.lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(line);
    }

    fn flush(&self) {}
}

static LOGGER: OnceLock<RingLogger> = OnceLock::new();

/// Installs the ring-buffer logger as the process-wide `log` backend.
/// Safe to call more than once (later calls are no-ops).
pub fn init() {
    let own_level = if cfg!(debug_assertions) {
        Level::Debug
    } else {
        Level::Info
    };
    let logger = LOGGER.get_or_init(|| RingLogger {
        own_level,
        lines: Mutex::new(LineBuffer::new(CAPACITY)),
    });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(if cfg!(debug_assertions) {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        });
    }
}

/// The last [`CAPACITY`] lines, oldest first (empty before [`init`]).
pub fn snapshot() -> String {
    match LOGGER.get() {
        Some(logger) => logger
            .lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .joined(),
        None => String::new(),
    }
}

/// First 8 characters of a puuid/match id — enough to correlate log lines,
/// without writing whole account ids into a log the user shares.
pub fn short(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `HH:MM:SS.mmmZ LEVEL module: message` (UTC; the module is the last
/// `::` segment of the target, e.g. `poll`).
pub fn format_line(ms: i64, level: Level, target: &str, args: &std::fmt::Arguments) -> String {
    let day_ms = ms.rem_euclid(86_400_000);
    let (h, m, s, milli) = (
        day_ms / 3_600_000,
        (day_ms / 60_000) % 60,
        (day_ms / 1000) % 60,
        day_ms % 1000,
    );
    let module = target.rsplit("::").next().unwrap_or(target);
    format!("{h:02}:{m:02}:{s:02}.{milli:03}Z {level:<5} {module}: {args}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_keeps_only_the_last_capacity_lines() {
        let mut buf = LineBuffer::new(CAPACITY);
        for i in 0..(CAPACITY + 100) {
            buf.push(format!("line {i}"));
        }
        let joined = buf.joined();
        let lines: Vec<&str> = joined.lines().collect();
        assert_eq!(lines.len(), CAPACITY);
        assert_eq!(lines[0], "line 100");
        assert_eq!(lines[CAPACITY - 1], format!("line {}", CAPACITY + 99));
    }

    #[test]
    fn buffer_joined_is_empty_when_nothing_logged() {
        assert_eq!(LineBuffer::new(3).joined(), "");
    }

    #[test]
    fn format_line_is_utc_clock_level_module_message() {
        // 1970-01-02 01:02:03.004 UTC
        let ms = 86_400_000 + 3_723_004;
        let line = format_line(
            ms,
            Level::Warn,
            "opd1_trcker_lib::poll",
            &format_args!("hello {}", 1),
        );
        assert_eq!(line, "01:02:03.004Z WARN  poll: hello 1");
    }

    #[test]
    fn dependency_logs_below_warn_are_dropped() {
        let logger = RingLogger {
            own_level: Level::Info,
            lines: Mutex::new(LineBuffer::new(10)),
        };
        let meta =
            |target: &'static str, level| Metadata::builder().target(target).level(level).build();
        assert!(logger.accepts(&meta("opd1_trcker_lib::poll", Level::Info)));
        assert!(!logger.accepts(&meta("opd1_trcker_lib::poll", Level::Debug)));
        assert!(!logger.accepts(&meta("tauri::manager", Level::Info)));
        assert!(logger.accepts(&meta("reqwest::connect", Level::Warn)));
    }

    #[test]
    fn short_truncates_ids_and_keeps_short_strings() {
        assert_eq!(short("aaaaaaaa-0000-0000-0000-000000000001"), "aaaaaaaa");
        assert_eq!(short("abc"), "abc");
    }
}

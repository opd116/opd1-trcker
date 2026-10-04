//! The in-memory ring-buffer logger (ruling R30): the last [`CAPACITY`]
//! log lines, returned as one string (after a one-line header) by the
//! `copy_log` command (the UI puts it on the clipboard on Ctrl+Shift+L — no
//! clipboard plugin). The UI's own failures arrive through `ui_log` and are
//! written here under the `ui` module (see [`log_ui`]).
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

// ---------------------------------------------------------------------
// ui_log: the UI's own failures, in the same ring (B1)
// ---------------------------------------------------------------------

/// The target of every `ui_log` line: it starts with `opd1`, so the ring keeps
/// it at Info in a release build, and the line's module reads `ui`.
pub const UI_TARGET: &str = "opd1_trcker::ui";

/// A UI line is at most this many characters, the closing `…` included
/// (the UI caps its own lines at the same length).
pub const UI_MAX_CHARS: usize = 500;

/// How much of a message is even looked at: the rest could never show in the
/// 500 characters kept, and a huge text is not worth scanning.
const UI_MAX_INPUT_CHARS: usize = 4_096;

/// At most this many UI lines per rolling [`UI_WINDOW_MS`] (the UI sends at
/// most 20 per 10 s; this is the second fence, in case the page runs away).
pub const UI_MAX_LINES: usize = 20;
pub const UI_WINDOW_MS: i64 = 60_000;

/// `error` -> Error, `warn` -> Warn (either, in any letter case); anything
/// else is Info. Never fails: an unknown level is still a line worth keeping.
pub fn ui_level(level: &str) -> Level {
    if level.eq_ignore_ascii_case("error") {
        Level::Error
    } else if level.eq_ignore_ascii_case("warn") {
        Level::Warn
    } else {
        Level::Info
    }
}

/// The part of a message that is looked at: all of it up to
/// [`UI_MAX_INPUT_CHARS`] characters; a longer one is cut at its last
/// whitespace and gets a `…`, so a token is never split in two before the
/// redaction sees it (half a JWT no longer looks like one). A message with no
/// whitespace at all is cut at the cap: nothing past 500 characters is ever
/// shown, and the cap is far beyond that.
fn ui_head(message: &str) -> String {
    let mut head: String = message.chars().take(UI_MAX_INPUT_CHARS).collect();
    if message.chars().nth(UI_MAX_INPUT_CHARS).is_some() {
        if let Some(at) = head.rfind(char::is_whitespace) {
            head.truncate(at);
        }
        head.push('…');
    }
    head
}

/// A UI message as the log may carry it: redacted like a captured body (a
/// secret-named key or a JWT-shaped string is gone), on one line, every full
/// uuid shortened to 8 characters, at most [`UI_MAX_CHARS`] characters.
///
/// That is all Rust can do. It cannot tell a player's name from any other
/// words in a free text, so no name is removed here: the barrier is the UI
/// (`ui/src/lib/uilog.ts` sends only an Error's name and message, a string, or
/// an object's key names, never a card or roster value).
pub fn ui_text(message: &str) -> String {
    opd1_riot::raw_capture::log_excerpt(&ui_head(message), UI_MAX_CHARS)
}

/// What the limiter says about one incoming line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAdmit {
    /// Over the limit: the line is counted and not logged.
    Drop,
    /// Logged; `dropped_before` lines were dropped since the last one that was.
    Allow { dropped_before: u32 },
}

/// At most [`UI_MAX_LINES`] ring lines in any [`UI_WINDOW_MS`], the
/// `ui_log: N lines dropped` notice included (it takes a slot of its own). The
/// lines refused meanwhile are counted, and the count goes out with the next
/// line let through. Time is passed in, so a test needs no clock. Holds at
/// most [`UI_MAX_LINES`] timestamps.
#[derive(Debug, Default)]
pub struct UiLogLimiter {
    /// When the ring lines let through in the current window were logged.
    sent: VecDeque<i64>,
    dropped: u32,
}

impl UiLogLimiter {
    pub const fn new() -> Self {
        Self {
            sent: VecDeque::new(),
            dropped: 0,
        }
    }

    pub fn admit(&mut self, now_ms: i64) -> UiAdmit {
        // A line leaves the window after a minute; one stamped in the future
        // (the clock was set back) leaves at once, or the cap would hold for hours.
        self.sent
            .retain(|&t| t <= now_ms && now_ms.saturating_sub(t) < UI_WINDOW_MS);
        // With drops to report, the notice and the line are two ring lines.
        let needed = if self.dropped > 0 { 2 } else { 1 };
        if self.sent.len() + needed > UI_MAX_LINES {
            self.dropped = self.dropped.saturating_add(1);
            return UiAdmit::Drop;
        }
        for _ in 0..needed {
            self.sent.push_back(now_ms);
        }
        UiAdmit::Allow {
            dropped_before: std::mem::take(&mut self.dropped),
        }
    }
}

/// What one `ui_log` call puts in the log: nothing (dropped), the line, or,
/// when lines were dropped before it, the one `ui_log: N lines dropped` line
/// first. Pure: [`write_ui_lines`] writes them.
pub fn ui_lines(
    limiter: &mut UiLogLimiter,
    now_ms: i64,
    level: &str,
    message: &str,
) -> Vec<(Level, String)> {
    let UiAdmit::Allow { dropped_before } = limiter.admit(now_ms) else {
        return Vec::new();
    };
    let mut lines = Vec::with_capacity(2);
    if dropped_before > 0 {
        lines.push((
            Level::Warn,
            format!("ui_log: {dropped_before} lines dropped"),
        ));
    }
    lines.push((ui_level(level), ui_text(message)));
    lines
}

/// Writes [`ui_lines`]'s answer to the ring, under [`UI_TARGET`].
pub fn write_ui_lines(lines: Vec<(Level, String)>) {
    for (level, text) in lines {
        log::log!(target: UI_TARGET, level, "{text}");
    }
}

/// The process-wide fence of `ui_log`. A static, next to [`LOGGER`], so a UI
/// error that arrives before `setup()` has managed the app state is still
/// logged.
static UI_LIMITER: Mutex<UiLogLimiter> = Mutex::new(UiLogLimiter::new());

/// One UI line, now: [`ui_lines`] through the process-wide limiter, written to
/// the ring. Never fails.
pub fn log_ui(level: &str, message: &str) {
    log_ui_with(&UI_LIMITER, now_ms(), level, message);
}

/// [`log_ui`] with the limiter and the time given (what a test injects).
pub fn log_ui_with(limiter: &Mutex<UiLogLimiter>, now_ms: i64, level: &str, message: &str) {
    // The lock is released before anything is written.
    let lines = ui_lines(
        &mut limiter
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        now_ms,
        level,
        message,
    );
    write_ui_lines(lines);
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

    // -- ui_log (B1) -------------------------------------------------------

    const JWT: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dQw4w9WgXcQ";
    const PUUID: &str = "aaaaaaaa-0000-0000-0000-000000000001";

    #[test]
    fn ui_levels_map_case_insensitively_and_unknown_is_info() {
        assert_eq!(ui_level("error"), Level::Error);
        assert_eq!(ui_level("ERROR"), Level::Error);
        assert_eq!(ui_level("Warn"), Level::Warn);
        assert_eq!(ui_level("warn"), Level::Warn);
        assert_eq!(ui_level("info"), Level::Info);
        assert_eq!(ui_level("debug"), Level::Info);
        assert_eq!(ui_level("trace"), Level::Info);
        assert_eq!(ui_level(""), Level::Info);
        assert_eq!(ui_level("err or"), Level::Info);
        assert_eq!(ui_level("ärror"), Level::Info);
    }

    #[test]
    fn ui_text_redacts_shortens_and_collapses() {
        let text = format!("promise: bad {JWT}\nfor player {PUUID}\r\n\t at app.js:1:2");
        let out = ui_text(&text);
        assert!(!out.contains(JWT) && !out.contains(PUUID), "{out}");
        assert!(!out.contains('\n') && !out.contains('\r') && !out.contains('\t'));
        assert_eq!(
            out,
            "promise: bad REDACTED for player aaaaaaaa at app.js:1:2"
        );
    }

    #[test]
    fn ui_text_is_at_most_500_characters() {
        let out = ui_text(&"x".repeat(2_000));
        assert_eq!(out.chars().count(), UI_MAX_CHARS);
        assert!(out.ends_with('…'));
        let exact = "y".repeat(UI_MAX_CHARS);
        assert_eq!(ui_text(&exact), exact);
        // A very long multi-byte text is cut on a character, never inside one.
        let out = ui_text(&"é".repeat(200_000));
        assert_eq!(out.chars().count(), UI_MAX_CHARS);
    }

    #[test]
    fn a_long_message_is_cut_at_whitespace_so_a_token_is_never_split() {
        // A JWT that straddles the 4096-character cap.
        let mut text = "x ".repeat(2_040); // 4080 characters
        text.push_str(JWT);
        text.push_str(" tail");
        assert!(text.chars().count() > UI_MAX_INPUT_CHARS);
        let head = ui_head(&text);
        assert!(
            !head.contains("eyJ"),
            "no piece of the token: {}",
            &head[head.len() - 20..]
        );
        assert!(head.ends_with("x…"), "cut at the last whitespace, then …");
        assert!(head.chars().count() <= UI_MAX_INPUT_CHARS);

        // At the cap or under it nothing is cut.
        let exact = "ab ".repeat(UI_MAX_INPUT_CHARS / 3 + 1)[..UI_MAX_INPUT_CHARS].to_string();
        assert_eq!(ui_head(&exact), exact);
        assert_eq!(ui_head("short"), "short");
        // No whitespace to cut at: the head is the cap (and the `…`), and the
        // 500 characters shown are far inside it.
        let solid = "z".repeat(UI_MAX_INPUT_CHARS + 10);
        assert_eq!(ui_head(&solid).chars().count(), UI_MAX_INPUT_CHARS + 1);
        // A multi-byte tail is cut on a character.
        let wide = "é ".repeat(UI_MAX_INPUT_CHARS);
        assert!(ui_head(&wide).ends_with("é…"));
    }

    #[test]
    fn ui_text_shortens_a_puuid_glued_to_a_hex_digit() {
        // `id` ends in a hex digit; the id is still a puuid.
        assert_eq!(ui_text(&format!("player id{PUUID}")), "player idaaaaaaaa");
        assert_eq!(ui_text(&format!("\"id\"{PUUID}")), "\"id\"aaaaaaaa");
    }

    #[test]
    fn ui_text_of_nothing_is_empty() {
        assert_eq!(ui_text(""), "");
        assert_eq!(ui_text("  \n\t "), "");
    }

    #[test]
    fn the_limiter_drops_the_21st_line_within_a_minute() {
        let mut l = UiLogLimiter::new();
        for i in 0..UI_MAX_LINES as i64 {
            assert_eq!(
                l.admit(1_000 + i),
                UiAdmit::Allow { dropped_before: 0 },
                "line {i}"
            );
        }
        assert_eq!(l.admit(30_000), UiAdmit::Drop, "the 21st");
        assert_eq!(l.admit(59_000), UiAdmit::Drop, "still inside the window");
    }

    #[test]
    fn the_drop_count_is_reported_once_when_the_window_allows_again() {
        let mut l = UiLogLimiter::new();
        for _ in 0..UI_MAX_LINES {
            l.admit(0);
        }
        for _ in 0..3 {
            assert_eq!(l.admit(10_000), UiAdmit::Drop);
        }
        // A minute after the first 20: all of them left the window.
        assert_eq!(l.admit(UI_WINDOW_MS), UiAdmit::Allow { dropped_before: 3 });
        assert_eq!(
            l.admit(UI_WINDOW_MS + 1),
            UiAdmit::Allow { dropped_before: 0 },
            "the count is reported once"
        );
    }

    #[test]
    fn the_window_is_rolling_not_fixed() {
        let mut l = UiLogLimiter::new();
        // 10 lines at t=0 and 10 at t=40s: the window is full until the first
        // ten are a minute old, then only those ten slots free up.
        for _ in 0..10 {
            l.admit(0);
        }
        for _ in 0..10 {
            l.admit(40_000);
        }
        assert_eq!(l.admit(59_999), UiAdmit::Drop, "one dropped");
        // Ten slots are free at 60 s; the drop notice takes one, so the line
        // that follows it and 8 more fit, and the 10th does not.
        assert_eq!(l.admit(60_000), UiAdmit::Allow { dropped_before: 1 });
        for i in 0..8 {
            assert_eq!(
                l.admit(60_001 + i),
                UiAdmit::Allow { dropped_before: 0 },
                "slot {i}"
            );
        }
        assert_eq!(l.admit(60_100), UiAdmit::Drop, "the other ten are still in");
    }

    #[test]
    fn the_drop_notice_takes_a_slot_of_its_own() {
        let mut l = UiLogLimiter::new();
        for _ in 0..UI_MAX_LINES {
            l.admit(0);
        }
        assert_eq!(l.admit(1), UiAdmit::Drop);
        // The notice and the line: two ring lines, so 18 more fit in this
        // window, not 19.
        assert_eq!(l.admit(UI_WINDOW_MS), UiAdmit::Allow { dropped_before: 1 });
        for i in 0..(UI_MAX_LINES - 2) as i64 {
            assert_eq!(
                l.admit(UI_WINDOW_MS + 1 + i),
                UiAdmit::Allow { dropped_before: 0 },
                "line {i}"
            );
        }
        assert_eq!(l.admit(UI_WINDOW_MS + 100), UiAdmit::Drop);
    }

    #[test]
    fn with_one_slot_free_and_drops_to_report_the_line_waits_too() {
        let mut l = UiLogLimiter::new();
        l.admit(0);
        for _ in 0..19 {
            l.admit(30_000);
        }
        assert_eq!(l.admit(31_000), UiAdmit::Drop, "the 21st: counted");
        // At 60 s one slot is free (the t=0 line left), but the notice and a
        // line need two: this line is dropped as well, the notice still owed.
        assert_eq!(l.admit(60_000), UiAdmit::Drop);
        assert_eq!(l.sent.len(), 19);
        // At 90 s everything has left: the notice counts both.
        assert_eq!(
            l.admit(90_000),
            UiAdmit::Allow { dropped_before: 2 },
            "the notice is not lost"
        );
    }

    #[test]
    fn at_most_20_ring_lines_in_any_minute_whatever_the_pace() {
        let mut l = UiLogLimiter::new();
        let mut written: Vec<(i64, usize)> = Vec::new();
        // A line every 700 ms for 10 minutes: far past the limit, with the
        // window opening and closing again and again.
        let mut t = 0;
        while t < 600_000 {
            let n = ui_lines(&mut l, t, "error", "x").len();
            if n > 0 {
                written.push((t, n));
            }
            assert!(l.sent.len() <= UI_MAX_LINES, "bounded at {t}");
            t += 700;
        }
        for &(start, _) in &written {
            let in_window: usize = written
                .iter()
                .filter(|(t, _)| *t >= start && *t < start + UI_WINDOW_MS)
                .map(|(_, n)| n)
                .sum();
            assert!(in_window <= UI_MAX_LINES, "{in_window} lines from {start}");
        }
        assert!(written.len() > 20, "lines do get through again");
    }

    #[test]
    fn a_clock_set_back_does_not_hold_the_limiter_shut() {
        let mut l = UiLogLimiter::new();
        for _ in 0..UI_MAX_LINES {
            l.admit(10_000_000);
        }
        assert_eq!(l.admit(10_000_001), UiAdmit::Drop);
        // The clock jumped back an hour: lines "from the future" don't count.
        assert!(matches!(l.admit(6_400_000), UiAdmit::Allow { .. }));
    }

    #[test]
    fn ui_lines_put_the_drop_count_first_and_only_once() {
        let mut l = UiLogLimiter::new();
        for i in 0..UI_MAX_LINES {
            let lines = ui_lines(&mut l, 0, "error", &format!("line {i}"));
            assert_eq!(lines, vec![(Level::Error, format!("line {i}"))]);
        }
        assert!(ui_lines(&mut l, 1, "error", "over").is_empty());
        assert!(ui_lines(&mut l, 2, "warn", "over").is_empty());
        let lines = ui_lines(&mut l, UI_WINDOW_MS, "WARN", "back");
        assert_eq!(
            lines,
            vec![
                (Level::Warn, "ui_log: 2 lines dropped".to_string()),
                (Level::Warn, "back".to_string()),
            ]
        );
        let lines = ui_lines(&mut l, UI_WINDOW_MS + 1, "whatever", "again");
        assert_eq!(lines, vec![(Level::Info, "again".to_string())]);
    }

    #[test]
    fn a_ui_line_lands_in_the_ring_under_the_ui_module_at_its_level() {
        init();
        let limiter = Mutex::new(UiLogLimiter::new());
        let ring = |marker: &str| -> Vec<String> {
            snapshot()
                .lines()
                .filter(|l| l.contains(marker))
                .map(str::to_string)
                .collect()
        };
        log_ui_with(
            &limiter,
            0,
            "ERROR",
            &format!("logbuf-test-a: {JWT} for {PUUID}\nsecond"),
        );
        log_ui_with(&limiter, 1, "warn", "logbuf-test-b");
        log_ui_with(&limiter, 2, "shout", "logbuf-test-c");

        let a = ring("logbuf-test-a");
        assert_eq!(a.len(), 1, "{a:?}");
        assert!(
            a[0].contains(" ERROR ui: logbuf-test-a: REDACTED for aaaaaaaa second"),
            "{}",
            a[0]
        );
        assert!(!a[0].contains(JWT) && !a[0].contains(PUUID));
        assert!(ring("logbuf-test-b")[0].contains(" WARN  ui: logbuf-test-b"));
        assert!(ring("logbuf-test-c")[0].contains(" INFO  ui: logbuf-test-c"));
    }

    #[test]
    fn a_poisoned_limiter_still_logs() {
        init();
        let limiter = Mutex::new(UiLogLimiter::new());
        std::thread::scope(|s| {
            let _ = s
                .spawn(|| {
                    let _held = limiter.lock().unwrap();
                    panic!("a caller panicked while it held the limiter");
                })
                .join();
        });
        assert!(limiter.is_poisoned());
        log_ui_with(&limiter, 0, "info", "logbuf-test-poisoned");
        assert!(snapshot().contains(" ui: logbuf-test-poisoned"));
    }

    #[test]
    fn ui_lines_never_panic_on_empty_or_huge_input() {
        let mut l = UiLogLimiter::new();
        let huge = "é\u{1F600}\n".repeat(300_000);
        for (level, message) in [
            ("", ""),
            ("error", &huge),
            (&huge, "x"),
            ("warn", "\0\u{7f}"),
        ] {
            let lines = ui_lines(&mut l, 0, level, message);
            for (_, text) in &lines {
                assert!(text.chars().count() <= UI_MAX_CHARS);
                assert!(!text.contains('\n'));
            }
        }
    }

    #[test]
    fn short_truncates_ids_and_keeps_short_strings() {
        assert_eq!(short("aaaaaaaa-0000-0000-0000-000000000001"), "aaaaaaaa");
        assert_eq!(short("abc"), "abc");
    }
}

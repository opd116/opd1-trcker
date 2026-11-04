//! Window behaviour (R189, user 2026-10-22): the custom title bar's window
//! (no decorations, a fixed 700×860, R212), Glance mode, the tray,
//! close to tray, launch with Windows and the saved-player toast.
//!
//! - [`setup`] (from `lib.rs`): the tray icon, the
//!   stored settings applied once, and the `--minimized` start (the
//!   autostart entry passes it: the window stays hidden in the tray).
//! - [`on_window_event`]: the title bar's × (`getCurrentWindow().close()`)
//!   hides to the tray when the `tray` setting is on, else quits.
//! - [`apply_settings`]: `tray` (also from `set_settings`).
//! - [`set_glance`]: the 420px column and back.
//! - [`notify_saved_player`]: the Windows toast.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

use opd1_core::wire::Settings;

use crate::state::{lock, AppState};

/// The one window's label (`tauri.conf.json`).
pub const MAIN_WINDOW: &str = "main";
/// The argument the autostart entry starts the app with.
pub const MINIMIZED_ARG: &str = "--minimized";
/// The autostart entry's name (the `HKCU\…\Run` value; `nsis/hooks.nsh`
/// removes it on uninstall).
#[cfg_attr(not(windows), allow(dead_code))] // Only the Windows autostart reads it.
const AUTOSTART_NAME: &str = "OPD1 TRCKER";
const TRAY_ID: &str = "main";

/// The normal window's one fixed size (logical px), as `tauri.conf.json` sets it (R212): never resizable or maximizable.
pub const NORMAL_SIZE: (f64, f64) = (700.0, 860.0);
/// Glance mode's one fixed size (logical px).
pub const GLANCE_WIDTH: f64 = 420.0;
pub const GLANCE_HEIGHT: f64 = 420.0;

/// A window's place: outer position and inner size, physical px.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

fn to_physical(logical: f64, scale: f64) -> u32 {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    (logical * scale).round() as u32
}

/// The Glance column for a window at `current`: [`GLANCE_WIDTH`] wide, its
/// 420 logical px high like the approved prototype, its top-left kept.
pub fn glance_geometry(current: Geometry, scale: f64) -> Geometry {
    Geometry {
        width: to_physical(GLANCE_WIDTH, scale),
        height: to_physical(GLANCE_HEIGHT, scale),
        ..current
    }
}

/// The window to restore after Glance (R212): always exactly the fixed normal
/// size (never a remembered custom one), at the saved top-left.
pub fn restore_geometry(saved: Geometry, scale: f64) -> Geometry {
    Geometry {
        width: to_physical(NORMAL_SIZE.0, scale),
        height: to_physical(NORMAL_SIZE.1, scale),
        ..saved
    }
}

/// Lifts the size limits, so a new size is never refused by the old maximum.
fn lift_limits(w: &WebviewWindow) -> tauri::Result<()> {
    w.set_min_size(None::<LogicalSize<f64>>)?;
    w.set_max_size(None::<LogicalSize<f64>>)
}

/// Pins the window to exactly `size` logical px: minimum and maximum equal it,
/// and it can be neither dragged to another size nor maximized.
fn pin_limits(w: &WebviewWindow, size: (f64, f64)) -> tauri::Result<()> {
    w.set_min_size(Some(LogicalSize::new(size.0, size.1)))?;
    w.set_max_size(Some(LogicalSize::new(size.0, size.1)))?;
    w.set_resizable(false)?;
    w.set_maximizable(false)
}

/// The window's state the settings and Glance mode share (Tauri managed
/// state, set up by [`setup`]).
pub struct WindowFlags {
    /// The `tray` setting: × hides to the tray.
    close_to_tray: AtomicBool,
    /// While Glance is on: the window it replaced, and whether it was
    /// maximized.
    glance: Mutex<Option<(Geometry, bool)>>,
}

impl Default for WindowFlags {
    /// The settings' defaults (`Settings::default`).
    fn default() -> Self {
        let d = Settings::default();
        WindowFlags {
            close_to_tray: AtomicBool::new(d.tray),
            glance: Mutex::new(None),
        }
    }
}

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN_WINDOW)
}

/// Shows, restores and focuses the main window (tray click, tray "Show").
fn show_main(app: &AppHandle) {
    if let Some(w) = main_window(app) {
        let _ = w.unminimize();
        let _ = w.show();
        set_webview_visible(&w, true);
        let _ = w.set_focus();
    }
}

/// Tells WebView2 whether the page is on screen. Hiding the native window
/// (close to tray) or minimizing it does not: WebView2 keeps the page
/// "visible" until its controller hears otherwise, so the waiting screen's
/// radar, the pulsing dots and the 1 s clocks kept rendering in the tray for
/// days. Hidden, the page reads `document.hidden` and Chromium stops drawing
/// and throttles its timers; Rust (the poll loop, the toasts) is unaffected.
fn set_webview_visible(w: &WebviewWindow, visible: bool) {
    #[cfg(windows)]
    {
        let done = w.with_webview(move |wv| {
            // SAFETY: a plain COM call on the live controller WebView2 handed
            // Tauri, made on the main thread `with_webview` runs it on.
            if let Err(e) = unsafe { wv.controller().SetIsVisible(visible) } {
                log::debug!("webview visibility ({visible}): {e}");
            }
        });
        if let Err(e) = done {
            log::debug!("webview visibility ({visible}): {e}");
        }
    }
    #[cfg(not(windows))]
    let _ = (w, visible);
}

/// Startup: the tray, the stored settings applied
/// once, and the window shown — unless the app was started with
/// [`MINIMIZED_ARG`] (by the autostart entry): then it stays hidden in the
/// tray, or minimized when the `tray` setting is off.
pub fn setup(app: &tauri::App, st: &AppState) -> tauri::Result<()> {
    app.manage(WindowFlags::default());
    setup_tray(app)?;

    let settings = st.db_blocking(super::settings::load).unwrap_or_default();
    apply_settings(app.handle(), &settings);
    autostart_at_start(settings.boot);

    let minimized = std::env::args().any(|a| a == MINIMIZED_ARG);
    if let Some(w) = main_window(app.handle()) {
        // R212: one fixed size, set explicitly rather than trusting the creation size.
        let fixed = lift_limits(&w)
            .and_then(|()| w.set_size(LogicalSize::new(NORMAL_SIZE.0, NORMAL_SIZE.1)))
            .and_then(|()| pin_limits(&w, NORMAL_SIZE));
        if let Err(e) = fixed {
            log::warn!("window fixed size failed: {e}");
        }
        // The config's `center` is not applied to a window created hidden
        // (`visible: false`): on Windows it came up at x = -27 on a
        // 1920×1080 screen. Center it here, before it is first shown.
        if let Err(e) = w.center() {
            log::warn!("window center failed: {e}");
        }
        if !minimized {
            w.show()?;
        } else if !settings.tray {
            w.show()?;
            w.minimize()?;
            set_webview_visible(&w, false);
        } else {
            // Hidden in the tray until "Show" or a click on the icon.
            set_webview_visible(&w, false);
        }
    }
    Ok(())
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("OPD1 TRCKER")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    // The bundle's icon (icons/icon.ico, 32x32.png …).
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// `on_window_event`: the main window's close request (the title bar's ×)
/// hides it to the tray when the `tray` setting is on; otherwise the app
/// quits. A resize tells WebView2 whether the page is on screen (minimized
/// or restored, from the title bar, the taskbar or Win+D).
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != MAIN_WINDOW {
        return;
    }
    if let tauri::WindowEvent::Resized(_) = event {
        if let Some(w) = main_window(window.app_handle()) {
            let on_screen = !w.is_minimized().unwrap_or(false) && w.is_visible().unwrap_or(true);
            set_webview_visible(&w, on_screen);
        }
        return;
    }
    let tauri::WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    let to_tray = window
        .try_state::<WindowFlags>()
        .is_some_and(|f| f.close_to_tray.load(Ordering::Relaxed));
    if to_tray {
        api.prevent_close();
        let _ = window.hide();
        if let Some(w) = main_window(window.app_handle()) {
            set_webview_visible(&w, false);
        }
    } else {
        window.app_handle().exit(0);
    }
}

/// Applies `tray` (close to tray; the tray icon shows only with it).
/// Failures are logged. `boot` is not applied here: [`set_autostart`] runs
/// only when it changes, and [`autostart_at_start`] at startup, so a
/// switch-off in Task Manager is never undone by an unrelated save or a
/// restart.
pub fn apply_settings(app: &AppHandle, settings: &Settings) {
    if let Some(flags) = app.try_state::<WindowFlags>() {
        flags.close_to_tray.store(settings.tray, Ordering::Relaxed);
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_visible(settings.tray);
    }
}

/// Startup: with `boot` on, only the current user's `Run` value is made
/// right (written when missing, after a first install or a reinstall, or
/// when it names another exe path); Task Manager's `StartupApproved` entry
/// is never touched here. `AutoLaunch::enable` would also mark it enabled,
/// undoing the user switching the app off in Task Manager at every start.
/// With `boot` off the entry is removed.
pub fn autostart_at_start(on: bool) {
    if !on {
        set_autostart(false);
        return;
    }
    if cfg!(debug_assertions) {
        return;
    }
    if let Err(e) = refresh_run_value() {
        log::warn!("autostart refresh: {e}");
    }
}

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Writes the `Run` value (the same command line `AutoLaunch` writes) unless
/// it already holds it.
#[cfg(windows)]
fn refresh_run_value() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let (path, args) = autostart_command(&exe);
    let want = format!("{} {}", path, args.join(" "));
    let have = windows_registry::CURRENT_USER
        .open(RUN_KEY)
        .and_then(|key| key.get_string(AUTOSTART_NAME))
        .ok();
    if have.as_deref() == Some(want.as_str()) {
        return Ok(());
    }
    windows_registry::CURRENT_USER
        .create(RUN_KEY)
        .and_then(|key| key.set_string(AUTOSTART_NAME, &want))
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn refresh_run_value() -> Result<(), String> {
    Ok(())
}

/// The `boot` setting: the autostart entry written (it also refreshes the
/// exe path) or removed. A debug build leaves it alone, so `tauri dev` and
/// tests never register a dev binary.
///
/// `auto-launch` directly (the crate `tauri-plugin-autostart` wraps), for
/// two things the plugin does not expose: the entry always goes to the
/// current user's `Run` key (the plugin's default writes the machine-wide
/// one when the app runs elevated, which the per-user uninstaller cannot
/// remove), and the exe path is quoted (it contains a space). Removing
/// clears both keys, so an older machine-wide entry goes too.
pub fn set_autostart(on: bool) {
    if cfg!(debug_assertions) {
        log::debug!("autostart not changed in a debug build (boot: {on})");
        return;
    }
    let result = autostart()
        .and_then(|a| if on { a.enable() } else { a.disable() }.map_err(|e| e.to_string()));
    if let Err(e) = result {
        log::warn!("autostart {}: {e}", if on { "enable" } else { "disable" });
    }
}

/// The `Run` value's command line: the quoted exe path, then [`MINIMIZED_ARG`].
#[cfg_attr(not(windows), allow(dead_code))] // Windows only; the tests use it everywhere.
pub fn autostart_command(exe: &std::path::Path) -> (String, [&'static str; 1]) {
    (format!("\"{}\"", exe.display()), [MINIMIZED_ARG])
}

#[cfg(windows)]
fn autostart() -> Result<auto_launch::AutoLaunch, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let (path, args) = autostart_command(&exe);
    auto_launch::AutoLaunchBuilder::new()
        .set_app_name(AUTOSTART_NAME)
        .set_app_path(&path)
        .set_args(&args)
        .set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser)
        .build()
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn autostart() -> Result<NoAutostart, String> {
    Err("launch with Windows is Windows only".into())
}

/// Stands in for `auto_launch::AutoLaunch` off Windows (never built).
#[cfg(not(windows))]
struct NoAutostart;

#[cfg(not(windows))]
impl NoAutostart {
    fn enable(&self) -> Result<(), String> {
        Ok(())
    }
    fn disable(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Glance mode: a 420×420 window (on) or the normal window (off). On:
/// the window's position is remembered and the window is pinned to exactly
/// 420×420. Off: it returns to exactly 700×860 at the same place (R212). Asking for the mode it is already in
/// changes nothing.
#[tauri::command]
pub fn set_glance(on: bool, app: AppHandle) -> Result<(), String> {
    let window = main_window(&app).ok_or("no main window")?;
    let flags = app
        .try_state::<WindowFlags>()
        .ok_or("window state not ready")?;
    let mut saved = lock(&flags.glance);
    let err = |e: tauri::Error| e.to_string();
    if on {
        if saved.is_some() {
            return Ok(());
        }
        let scale = window.scale_factor().map_err(err)?;
        let pos = window.outer_position().map_err(err)?;
        let size = window.inner_size().map_err(err)?;
        let current = Geometry {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
        };
        let target = glance_geometry(current, scale);
        *saved = Some((current, false));
        lift_limits(&window).map_err(err)?;
        window
            .set_size(PhysicalSize::new(target.width, target.height))
            .map_err(err)?;
        pin_limits(&window, (GLANCE_WIDTH, GLANCE_HEIGHT)).map_err(err)?;
    } else {
        let Some((before, _)) = saved.take() else {
            return Ok(());
        };
        let scale = window.scale_factor().map_err(err)?;
        let target = restore_geometry(before, scale);
        lift_limits(&window).map_err(err)?;
        window
            .set_size(PhysicalSize::new(target.width, target.height))
            .map_err(err)?;
        window
            .set_position(PhysicalPosition::new(target.x, target.y))
            .map_err(err)?;
        pin_limits(&window, NORMAL_SIZE).map_err(err)?;
    }
    Ok(())
}

/// The toast's text: `"{name} — {note}"`, or the name alone without a note
/// (a long note is cut to 120 characters).
pub fn notification_body(name: &str, note: &str) -> String {
    let note = note.trim();
    if note.is_empty() {
        return name.trim().to_string();
    }
    let mut short: String = note.chars().take(120).collect();
    if short.len() < note.len() {
        short.push('…');
    }
    format!("{} — {short}", name.trim())
}

/// "Saved player in your match": a Windows toast, with the default sound
/// when `sound` (the setting), silent otherwise. Windows shows it only for
/// the installed app (the plugin's documented limit). Failures are logged.
///
/// Wired in `lib.rs` setup through `AppState::set_alert_hook`.
pub fn notify_saved_player(app: &AppHandle, name: &str, note: &str, sound: bool) {
    use tauri_plugin_notification::NotificationExt;
    let mut toast = app
        .notification()
        .builder()
        .title("Saved player in your match")
        .body(notification_body(name, note));
    if sound {
        toast = toast.sound("Default");
    }
    if let Err(e) = toast.show() {
        log::warn!("saved-player notification: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Geometry = Geometry {
        x: 100,
        y: 50,
        width: 1280,
        height: 800,
    };

    #[test]
    fn the_autostart_command_quotes_a_path_with_a_space() {
        let exe = std::path::Path::new(r"C:\Users\me\AppData\Local\OPD1 TRCKER\opd1-trcker.exe");
        let (path, args) = autostart_command(exe);
        assert_eq!(
            path,
            r#""C:\Users\me\AppData\Local\OPD1 TRCKER\opd1-trcker.exe""#
        );
        assert_eq!(args, ["--minimized"]);
    }

    #[test]
    fn glance_is_420_square_and_keeps_the_top_left() {
        assert_eq!(
            glance_geometry(AT, 1.0),
            Geometry {
                x: 100,
                y: 50,
                width: 420,
                height: 420
            }
        );
        // 150% scaling: physical px.
        assert_eq!(glance_geometry(AT, 1.5).width, 630);
        // Normal height never leaks into the compact Glance footprint.
        let short = Geometry { height: 500, ..AT };
        assert_eq!(glance_geometry(short, 1.0).height, 420);
        assert_eq!(glance_geometry(short, 1.25).height, 525);
        // A nonsense scale is 1.
        assert_eq!(glance_geometry(AT, 0.0).width, 420);
        assert_eq!(glance_geometry(AT, f64::NAN).width, 420);
    }

    #[test]
    fn leaving_glance_always_restores_the_fixed_normal_size() {
        let fixed = Geometry {
            x: 100,
            y: 50,
            width: 700,
            height: 860,
        };
        // Whatever the window was, the normal size is exactly 700x860 at the same top-left.
        assert_eq!(restore_geometry(AT, 1.0), fixed);
        let small = Geometry {
            width: 600,
            height: 700,
            ..AT
        };
        assert_eq!(restore_geometry(small, 1.0), fixed);
        assert_eq!(restore_geometry(small, 1.5).width, 1050);
        assert_eq!(restore_geometry(small, 1.5).height, 1290);
        // Glance never leaks into it.
        let there = glance_geometry(fixed, 1.0);
        assert_ne!(there, fixed);
        assert_eq!(restore_geometry(there, 1.0), fixed);
    }

    #[test]
    fn compact_normal_window_round_trip_returns_exactly_700x860_at_both_scales() {
        for scale in [1.0, 1.5] {
            let normal = Geometry {
                width: to_physical(700.0, scale),
                height: to_physical(860.0, scale),
                ..AT
            };
            let glance = glance_geometry(normal, scale);
            assert_eq!(glance.width, to_physical(420.0, scale));
            assert_eq!(glance.height, to_physical(420.0, scale));
            assert_eq!((glance.x, glance.y), (normal.x, normal.y));
            assert_eq!(restore_geometry(glance, scale), normal);
            assert_eq!(restore_geometry(normal, scale), normal);
        }
    }

    #[test]
    fn the_toast_names_the_player_and_the_note() {
        assert_eq!(notification_body("Shanky#OPD1", ""), "Shanky#OPD1");
        assert_eq!(notification_body("Shanky#OPD1", "  "), "Shanky#OPD1");
        assert_eq!(
            notification_body("Shanky#OPD1", " plays Jett "),
            "Shanky#OPD1 — plays Jett"
        );
        let long = "x".repeat(200);
        let body = notification_body("A", &long);
        assert_eq!(body.chars().count(), "A — ".chars().count() + 121);
        assert!(body.ends_with('…'));
    }

    #[test]
    fn the_flags_start_at_the_settings_defaults() {
        let flags = WindowFlags::default();
        let d = Settings::default();
        assert_eq!(flags.close_to_tray.load(Ordering::Relaxed), d.tray);
        assert!(lock(&flags.glance).is_none());
    }
}

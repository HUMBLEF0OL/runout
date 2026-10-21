//! System tray (TSD §2, §4.5, FR-026; DESIGN §6 `#/tray` board): status line, pause (1 h / until
//! resumed) or resume, Quick add, open, quit. The tooltip and status line follow `LoggerStatus`;
//! while paused only Resume shows (the menu is rebuilt when that flips). Closing the window hides
//! it here and the logger keeps running.
use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::commands;
use crate::logger::{self, Logger, LoggerStatus};
use crate::state::AppState;

const ID: &str = "main";
/// Windows `NOTIFYICONDATAW.szTip` holds 128 UTF-16 units including the NUL.
const TIP_MAX: usize = 127;

/// Items whose text or enabled state follows the logger, and which menu is shown.
struct Items<R: Runtime> {
    status: MenuItem<R>,
    pause: Vec<MenuItem<R>>,
    paused: bool,
}

/// Menu entries below the status line, as on the `#/tray` board: `(id, label, accelerator)`;
/// `None` = separator.
fn entries(paused: bool) -> Vec<Option<(&'static str, &'static str, Option<&'static str>)>> {
    let logging = if paused {
        vec![Some(("resume", "Resume logging", None))]
    } else {
        vec![
            Some(("pause_1h", "Pause for 1 hour", None)),
            Some(("pause", "Pause until I resume", None)),
        ]
    };
    [None]
        .into_iter()
        .chain(logging)
        .chain([
            Some(("quick_add", "Quick add", Some("CmdOrCtrl+N"))),
            Some(("open", "Open Runout", None)),
            None,
            Some(("quit", "Quit Runout", None)),
        ])
        .collect()
}

fn menu<R: Runtime>(
    app: &AppHandle<R>,
    paused: bool,
    tip: &str,
) -> tauri::Result<(Menu<R>, Items<R>)> {
    let status = MenuItem::with_id(app, "status", tip.replace('&', "&&"), false, None::<&str>)?;
    let menu = Menu::with_items(app, &[&status])?;
    let mut pause = vec![];
    for e in entries(paused) {
        match e {
            None => menu.append(&PredefinedMenuItem::separator(app)?)?,
            Some((id, text, acc)) => {
                let item = MenuItem::with_id(app, id, text, true, acc)?;
                menu.append(&item)?;
                if id.starts_with("pause") {
                    pause.push(item);
                }
            }
        }
    }
    Ok((
        menu,
        Items {
            status,
            pause,
            paused,
        },
    ))
}

/// Tooltip (and menu status line) for a logger status.
pub fn tooltip(s: &LoggerStatus) -> String {
    let text = match (s.state, &s.now_playing) {
        ("listening", Some(np)) => {
            format!("Runout: logging {} by {}", np.track_name, np.artist_name)
        }
        ("paused", _) => match s
            .paused_until
            .as_deref()
            .and_then(|u| chrono::DateTime::parse_from_rfc3339(u).ok())
        {
            Some(u) => format!(
                "Runout: logging paused until {}",
                u.with_timezone(&chrono::Local).format("%H:%M")
            ),
            None => "Runout: logging paused until you resume".into(),
        },
        ("off", _) => "Runout: logging is off".into(),
        ("error", _) => "Runout: can't see what Spotify is playing".into(),
        // idle, or listening without a track: Spotify closed or not playing (TSD §6)
        _ => "Runout: Spotify isn't playing or isn't open".into(),
    };
    let mut units = 0;
    text.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= TIP_MAX
        })
        .collect()
}

/// Emit `logger-status` and update the tray. Every `logger-status` goes through here.
pub fn emit_status<R: Runtime>(app: &AppHandle<R>, s: &LoggerStatus) {
    let _ = app.emit("logger-status", s);
    let tip = tooltip(s);
    if let Some(t) = app.tray_by_id(ID) {
        let _ = t.set_tooltip(Some(&tip));
    }
    if let Some(items) = app.try_state::<Mutex<Items<R>>>() {
        let paused = s.state == "paused";
        let mut i = items.lock().unwrap();
        if i.paused != paused
            && let (Some(t), Ok((m, next))) = (app.tray_by_id(ID), menu(app, paused, &tip))
            && t.set_menu(Some(m)).is_ok()
        {
            *i = next;
        }
        let _ = i.status.set_text(tip.replace('&', "&&")); // a menu item reads `&` as an access key
        for p in &i.pause {
            let _ = p.set_enabled(s.state != "off");
        }
    }
}

pub(crate) fn show<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Same path as the `pause_logger`/`resume_logger` commands. Runs off the main thread: the logger
/// thread holds the logger lock while it waits on the main thread to update the tray.
fn change<R: Runtime>(app: &AppHandle<R>, minutes: Option<Option<i64>>) {
    let app = app.clone();
    std::thread::spawn(move || {
        let (st, lg) = (app.state::<AppState>(), app.state::<Mutex<Logger>>());
        let _ = commands::change_logger(&app, &st, &lg, |s| match minutes {
            Some(m) => logger::pause(s, m, chrono::Utc::now()),
            None => {
                logger::resume(s);
                Ok(())
            }
        });
    });
}

/// Write the play in progress (≥ 1 s). Tray Quit, and `RunEvent::Exit` (G-36). A second call
/// finds no play and writes nothing.
fn flush<R: Runtime>(app: &AppHandle<R>) {
    let (Some(st), Some(lg)) = (
        app.try_state::<AppState>(),
        app.try_state::<Mutex<Logger>>(),
    ) else {
        return; // exit before setup managed them
    };
    let mut lg = lg.lock().unwrap(); // lock order logger → db
    let db = st.db.lock().unwrap();
    let _ = lg.close(&db, chrono::Utc::now());
}

/// `RunEvent::Exit` (lib.rs): runs on the main thread, also on Windows shutdown / sign-out (tao
/// handles `WM_ENDSESSION` by firing it, then exits). The logger thread can hold its lock while it
/// waits on the main thread (tray update), so the flush runs on a thread and is given `wait`.
// ponytail: on that race the play in progress is lost, as before G-36.
pub(crate) fn flush_on_exit<R: Runtime>(app: &AppHandle<R>, wait: std::time::Duration) {
    let (tx, rx) = std::sync::mpsc::channel();
    let app = app.clone();
    std::thread::spawn(move || {
        flush(&app);
        let _ = tx.send(());
    });
    let _ = rx.recv_timeout(wait);
}

/// Quit: write the play in progress, then exit (the `Exit` flush then finds nothing).
fn quit<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    std::thread::spawn(move || {
        flush(&app);
        app.exit(0);
    });
}

/// Quick add (G-35): show the window and ask the UI to open its Quick add dialog (API.md §6).
fn quick_add<R: Runtime>(app: &AppHandle<R>) {
    show(app);
    let _ = app.emit("open-quick-add", ());
}

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    // Starts as "logging"; the first `emit_status` switches to the paused menu if need be.
    let (menu, items) = menu(app, false, "Runout")?;
    let mut b = TrayIconBuilder::with_id(ID)
        .tooltip("Runout")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => show(app),
            "quick_add" => quick_add(app),
            "pause_1h" => change(app, Some(Some(60))),
            "pause" => change(app, Some(None)),
            "resume" => change(app, None),
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|t, e| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                show(t.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.build(app)?;
    app.manage(Mutex::new(items));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::NowPlaying;

    fn status(state: &'static str) -> LoggerStatus {
        LoggerStatus {
            state,
            now_playing: None,
            last_event_at: None,
            paused_until: None,
            autostart: false,
            include_private: false,
        }
    }

    #[test]
    fn tooltip_per_state() {
        let mut s = status("listening");
        s.now_playing = Some(NowPlaying {
            track_name: "Fixture Song".into(),
            artist_name: "Fixture Artist".into(),
            album_name: "A".into(),
            spotify_uri: None,
            ms_played: 0,
            duration_ms: 1,
            item: None,
        });
        assert_eq!(
            tooltip(&s),
            "Runout: logging Fixture Song by Fixture Artist"
        );
        assert_eq!(
            tooltip(&status("idle")),
            "Runout: Spotify isn't playing or isn't open"
        );
        assert_eq!(
            tooltip(&status("paused")),
            "Runout: logging paused until you resume"
        );
        let mut p = status("paused");
        p.paused_until = Some("2026-02-02T02:45:00Z".into());
        let t = tooltip(&p);
        assert!(t.starts_with("Runout: logging paused until ") && !t.ends_with("resume"));
        assert_eq!(tooltip(&status("off")), "Runout: logging is off");
        assert_eq!(
            tooltip(&status("error")),
            "Runout: can't see what Spotify is playing"
        );
    }

    /// G-36: the exit flush writes the play in progress once, and never hangs on a held logger lock.
    #[test]
    fn exit_flush_writes_the_play_in_progress() {
        use crate::logger::accumulator::{PlayState, Track};
        use std::time::{Duration, Instant};
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let h = app.handle();
        flush_on_exit(h, Duration::from_secs(1)); // nothing managed yet: no panic
        let d = std::path::Path::new("d");
        app.manage(AppState {
            db: Mutex::new(crate::db::open_in_memory().unwrap()),
            paths: crate::state::Paths {
                config_dir: d.into(),
                data_dir: d.into(),
                local_data_dir: d.into(),
            },
            settings: Mutex::new(crate::settings::Settings::defaults(d, d)),
        });
        app.manage(Mutex::new(Logger::default()));
        let track = Track {
            title: "Fixture Song".into(),
            artist: "Fixture Artist".into(),
            album: "Fixture Album".into(),
            duration_ms: 200_000,
            position_ms: 0,
        };
        {
            let st = app.state::<AppState>();
            let s = st.settings.lock().unwrap().clone();
            let started = chrono::Utc::now() - chrono::Duration::seconds(40);
            app.state::<Mutex<Logger>>()
                .lock()
                .unwrap()
                .observe(
                    &st.db.lock().unwrap(),
                    &s,
                    Some((track, PlayState::Playing)),
                    started,
                )
                .unwrap();
        }
        let plays = || -> Vec<(String, bool)> {
            let st = app.state::<AppState>();
            let db = st.db.lock().unwrap();
            // unfiltered: the test reads every row the flush wrote
            let mut q = db
                .prepare("SELECT track_name, counts_as_play FROM listening_event")
                .unwrap();
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        // logger lock held elsewhere (logger thread waiting on the main thread): gives up in time
        {
            let lg = app.state::<Mutex<Logger>>();
            let _held = lg.lock().unwrap();
            let t = Instant::now();
            flush_on_exit(h, Duration::from_millis(200));
            assert!(t.elapsed() < Duration::from_secs(1));
        }
        // lock freed: the stuck flush thread finishes; a later one finds nothing left
        let t = Instant::now();
        while plays().is_empty() && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        flush_on_exit(h, Duration::from_secs(5));
        assert_eq!(plays(), vec![("Fixture Song".into(), true)]);
    }

    /// G-35, DESIGN §6 `#/tray` board: pause items while logging, only Resume while paused, then
    /// Quick add (Ctrl+N), Open Runout, Quit Runout.
    #[test]
    fn menu_follows_the_tray_board() {
        let labels = |p| {
            entries(p)
                .into_iter()
                .map(|e| e.map_or("-", |(_, l, _)| l))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            labels(false),
            [
                "-",
                "Pause for 1 hour",
                "Pause until I resume",
                "Quick add",
                "Open Runout",
                "-",
                "Quit Runout"
            ]
        );
        assert_eq!(
            labels(true),
            [
                "-",
                "Resume logging",
                "Quick add",
                "Open Runout",
                "-",
                "Quit Runout"
            ]
        );
        let quick = entries(false)
            .into_iter()
            .flatten()
            .find(|e| e.0 == "quick_add");
        assert_eq!(quick.and_then(|e| e.2), Some("CmdOrCtrl+N"));
    }

    #[test]
    fn tooltip_fits_windows_limit() {
        let mut s = status("listening");
        s.now_playing = Some(NowPlaying {
            track_name: "é".repeat(200),
            artist_name: "𝄞".repeat(50), // 2 UTF-16 units each
            album_name: String::new(),
            spotify_uri: None,
            ms_played: 0,
            duration_ms: 1,
            item: None,
        });
        let t = tooltip(&s);
        assert!(t.encode_utf16().count() <= TIP_MAX);
        assert!(t.starts_with("Runout: logging éé"));
    }
}

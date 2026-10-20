//! System tray (TSD §2, §4.5, FR-026; DESIGN §6 `#/tray`): open, pause logging (1 h / until
//! resumed), resume, quit. The tooltip and a status line follow `LoggerStatus`; closing the window
//! hides it here and the logger keeps running.
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

/// Items whose text or enabled state follows the logger.
struct Items<R: Runtime> {
    status: MenuItem<R>,
    pause_1h: MenuItem<R>,
    pause: MenuItem<R>,
    resume: MenuItem<R>,
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
    if let Some(i) = app.try_state::<Items<R>>() {
        let paused = s.state == "paused";
        let _ = i.status.set_text(tip.replace('&', "&&")); // a menu item reads `&` as an access key
        let _ = i.resume.set_enabled(paused);
        let _ = i.pause_1h.set_enabled(!paused && s.state != "off");
        let _ = i.pause.set_enabled(!paused && s.state != "off");
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

/// Quit: write the play in progress (≥ 1 s), then exit.
fn quit<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    std::thread::spawn(move || {
        {
            let (st, lg) = (app.state::<AppState>(), app.state::<Mutex<Logger>>());
            let mut lg = lg.lock().unwrap(); // lock order logger → db
            let db = st.db.lock().unwrap();
            let _ = lg.close(&db, chrono::Utc::now());
        }
        app.exit(0);
    });
}

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let item = |id: &str, text: &str, on: bool| MenuItem::with_id(app, id, text, on, None::<&str>);
    let items = Items {
        status: item("status", "Runout", false)?,
        pause_1h: item("pause_1h", "Pause logging for 1 hour", true)?,
        pause: item("pause", "Pause logging until resumed", true)?,
        resume: item("resume", "Resume logging", false)?,
    };
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(
        app,
        &[
            &items.status,
            &sep()?,
            &item("open", "Open Runout", true)?,
            &items.pause_1h,
            &items.pause,
            &items.resume,
            &sep()?,
            &item("quit", "Quit Runout", true)?,
        ],
    )?;
    let mut b = TrayIconBuilder::with_id(ID)
        .tooltip("Runout")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => show(app),
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
    app.manage(items);
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

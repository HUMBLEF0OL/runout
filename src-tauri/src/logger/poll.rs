//! Logger thread (TSD §4.5): wakes on every SMTC event and at least every 5 s (the timeline
//! fallback for apps that don't raise every event), takes a snapshot and feeds the `Logger`.
//! Emits `play-logged` per written play and `logger-status` when the state or track changes.
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::session::Watcher;
use super::{Logger, LoggerStatus, play};
use crate::state::AppState;

pub const POLL: Duration = Duration::from_secs(5);

pub fn spawn<R: Runtime>(app: AppHandle<R>) {
    std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel();
        let mut watcher = None;
        let mut last: Option<LoggerStatus> = None;
        loop {
            let _ = rx.recv_timeout(POLL);
            if watcher.is_none() {
                watcher = Watcher::new(tx.clone()).ok();
            }
            let st = app.state::<AppState>();
            let apps = st.settings.lock().unwrap().logger_apps.clone();
            // SMTC calls block briefly: no lock held. A failed read skips this tick.
            let snap = watcher.as_mut().map(|w| w.snapshot(&apps));
            let lg = app.state::<Mutex<Logger>>();
            let mut lg = lg.lock().unwrap();
            let db = st.db.lock().unwrap();
            let s = st.settings.lock().unwrap().clone();
            let now = chrono::Utc::now();
            match snap {
                None => lg.error = true,
                Some(Ok(snap)) => {
                    if let Ok(Some(id)) = lg.observe(&db, &s, snap, now)
                        && let Ok(p) = play(&db, id)
                    {
                        let _ = app.emit("play-logged", p);
                    }
                }
                Some(Err(_)) => {}
            }
            if let Ok(status) = lg.status(&db, &s, now) {
                emit_if_changed(&app, &mut last, status);
            }
        }
    });
}

/// `logger-status` only when something other than the running `ms_played` changed.
fn emit_if_changed<R: Runtime>(app: &AppHandle<R>, last: &mut Option<LoggerStatus>, s: LoggerStatus) {
    let key = |s: &LoggerStatus| {
        let mut k = s.clone();
        if let Some(np) = &mut k.now_playing {
            np.ms_played = 0;
        }
        k
    };
    if last.as_ref().map(key) != Some(key(&s)) {
        let _ = app.emit("logger-status", &s);
        *last = Some(s);
    }
}

//! SMTC session watch (TSD §4.5): follows the first media session whose `SourceAppUserModelId`
//! contains an allow-listed name (`Settings::logger_apps`, default `Spotify`), and pings `tx` on
//! session, media-properties and playback changes. `snapshot` reads what that session shows now.
use std::sync::mpsc::Sender;

use super::accumulator::{PlayState, Track};

/// `None` = no allow-listed session.
pub type Snapshot = Option<(Track, PlayState)>;

/// Case-insensitive substring match against the allow-list; empty entries never match.
pub fn allowed(aumid: &str, apps: &[String]) -> bool {
    let id = aumid.to_lowercase();
    apps.iter()
        .any(|a| !a.trim().is_empty() && id.contains(&a.trim().to_lowercase()))
}

#[cfg(windows)]
pub use imp::Watcher;

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::Foundation::TypedEventHandler;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    use windows::core::Result;

    pub struct Watcher {
        mgr: Manager,
        tx: Sender<()>,
        /// The followed session: (AUMID, session, media token, playback token).
        sub: Option<(String, Session, i64, i64)>,
    }

    fn ping<S, A>(tx: &Sender<()>) -> TypedEventHandler<S, A>
    where
        S: windows::core::RuntimeType + 'static,
        A: windows::core::RuntimeType + 'static,
    {
        let tx = tx.clone();
        TypedEventHandler::new(move |_, _| {
            let _ = tx.send(());
            Ok(())
        })
    }

    impl Watcher {
        /// Fails when SMTC is unavailable (TSD §6: logger shows the error state).
        pub fn new(tx: Sender<()>) -> Result<Self> {
            let mgr = Manager::RequestAsync()?.join()?;
            mgr.SessionsChanged(&ping(&tx))?;
            Ok(Self { mgr, tx, sub: None })
        }

        fn find(&self, apps: &[String]) -> Result<Option<(String, Session)>> {
            let all = self.mgr.GetSessions()?;
            for i in 0..all.Size()? {
                let s = all.GetAt(i)?;
                let id = s.SourceAppUserModelId()?.to_string();
                if allowed(&id, apps) {
                    return Ok(Some((id, s)));
                }
            }
            Ok(None)
        }

        /// Subscribe to the session's own events when the followed session changes.
        fn follow(&mut self, found: Option<&(String, Session)>) -> Result<()> {
            if self.sub.as_ref().map(|s| &s.0) == found.map(|f| &f.0) {
                return Ok(());
            }
            if let Some((_, s, m, p)) = self.sub.take() {
                let _ = s.RemoveMediaPropertiesChanged(m);
                let _ = s.RemovePlaybackInfoChanged(p);
            }
            if let Some((id, s)) = found {
                let m = s.MediaPropertiesChanged(&ping(&self.tx))?;
                let p = s.PlaybackInfoChanged(&ping(&self.tx))?;
                self.sub = Some((id.clone(), s.clone(), m, p));
            }
            Ok(())
        }

        pub fn snapshot(&mut self, apps: &[String]) -> Result<Snapshot> {
            let found = self.find(apps)?;
            self.follow(found.as_ref())?;
            let Some((_, s)) = found else {
                return Ok(None);
            };
            let state = match s.GetPlaybackInfo()?.PlaybackStatus()? {
                Status::Closed => return Ok(None),
                Status::Playing => PlayState::Playing,
                Status::Paused => PlayState::Paused,
                _ => PlayState::Stopped, // Opened, Changing, Stopped
            };
            let p = s.TryGetMediaPropertiesAsync()?.join()?;
            let title = p.Title()?.to_string();
            if title.is_empty() {
                return Ok(None); // between tracks
            }
            let track = Track {
                title,
                artist: p.Artist()?.to_string(),
                album: p.AlbumTitle()?.to_string(),
                // TimeSpan is in 100 ns units
                duration_ms: s.GetTimelineProperties()?.EndTime()?.Duration / 10_000,
            };
            Ok(Some((track, state)))
        }
    }
}

/// Non-Windows builds compile; the logger reports `error` (TSD §6 "SMTC unavailable").
#[cfg(not(windows))]
pub struct Watcher;

#[cfg(not(windows))]
impl Watcher {
    pub fn new(_tx: Sender<()>) -> Result<Self, ()> {
        Err(())
    }
    pub fn snapshot(&mut self, _apps: &[String]) -> Result<Snapshot, ()> {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_list_is_a_case_insensitive_substring() {
        let apps = vec!["Spotify".to_string(), " ".to_string()];
        assert!(allowed(
            "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify",
            &apps
        ));
        assert!(allowed("spotify.exe", &apps));
        assert!(!allowed("Microsoft.ZuneMusic", &apps));
        assert!(!allowed("anything", &[String::new()]));
    }
}

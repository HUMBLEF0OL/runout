//! Spotify Web API, Release 3 (TSD §5.2). `Spotify` is Tauri-managed state: tokens (Credential
//! Manager, `auth.rs`), the HTTP client (`client.rs`, every request through `net.rs`) and the
//! separate `spotify_cache.db` (`cache.rs`, source A only, TSD §4.2).
//! Playlist Studio reads only `item` ratings/tags: nothing here touches listening data (D27,
//! enforced by `scripts/check.py`). Nothing in this module logs; tokens and the client ID never
//! appear in errors.
pub mod auth;
pub mod autofill;
pub mod cache;
pub mod client;
pub mod create;
pub mod discover;
pub mod inbox;
pub mod library_sync;
pub mod studio;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rusqlite::Connection;
use serde::Serialize;

use crate::error::{AppError, Code, Result};

pub use auth::Tokens;
pub use client::{Body, HttpReq, HttpResp, Transport};

/// Env var holding the developer app's client ID (never in the repo, never logged; KB spotify.md).
/// Optional: the Connect dialog saves a pasted one to `cleanup::CLIENT_ID_FILE` instead.
pub const CLIENT_ID_ENV: &str = "RUNOUT_SPOTIFY_CLIENT_ID";
/// Baked in at build time by build.rs (env var or `src-tauri/spotify_client_id.local`), if set.
const BUILTIN_CLIENT_ID: Option<&str> = option_env!("RUNOUT_BUILTIN_CLIENT_ID");
/// `spotify_auth` detail when no client ID is set: the Connect dialog then asks for one.
pub const NO_CLIENT_ID: &str = "client_id";

/// API.md §5 `get_spotify_status` (also the `spotify-status` event payload).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpotifyStatus {
    pub state: State,
    pub display_name: Option<String>,
    /// Refresh tokens expire 6 months after sign-in; refreshing doesn't extend it (KB, BRD S7).
    pub renew_by: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Disconnected,
    Connected,
    Reconnect,
}

type Emit = Box<dyn Fn(&SpotifyStatus) + Send + Sync>;

pub struct Spotify {
    /// Credential Manager target; `None` keeps tokens in memory only (tests).
    credential: Option<String>,
    /// Reads `credential` (`auth::load_tokens`; tests inject a failing one).
    read_credential: fn(&str) -> Result<Option<Tokens>>,
    /// Deletes `credential` (`cleanup::delete_credential`; tests inject a failing one).
    delete_credential: fn(&str) -> Result<()>,
    /// `cleanup::DISCONNECT_MARKER`: written before the credential is deleted, removed once the
    /// delete (or a new sign-in's store) succeeds. While it exists the stored tokens count as
    /// deleted: startup retries the delete instead of signing back in (TSD §5.2 Disconnect).
    pending_delete: PathBuf,
    /// Test override; production reads [`CLIENT_ID_ENV`], then `client_id_file`, then
    /// [`BUILTIN_CLIENT_ID`], each time.
    client_id: Option<String>,
    /// `cleanup::CLIENT_ID_FILE`: the client ID pasted in the Connect dialog.
    client_id_file: PathBuf,
    /// Outer `None` = not read from Credential Manager yet.
    tokens: Mutex<Option<Option<Tokens>>>,
    /// Set when Spotify answered `invalid_grant` (persisted as `cleanup::RECONNECT_MARKER`); cleared by
    /// connect/disconnect.
    reconnect: AtomicBool,
    reconnect_marker: PathBuf,
    /// Bumped (under the tokens lock) whenever the tokens are cleared or replaced by a sign-in: a
    /// refresh that started before must not store its answer (it would undo a disconnect or
    /// overwrite the newer grant).
    generation: AtomicU64,
    cache_path: PathBuf,
    /// Opened lazily; lock briefly, never across an `.await`.
    cache: Mutex<Option<Connection>>,
    pub(crate) transport: Transport,
    /// Max 1 in-flight write (TSD §5.2).
    pub(crate) write_lock: tauri::async_runtime::Mutex<()>,
    /// Single-flight token refresh: one rotation serves every waiting caller (auth.rs).
    pub(crate) refresh_lock: tauri::async_runtime::Mutex<()>,
    /// Held across a whole Studio apply/undo (check → snapshot → every chunk), so two of them
    /// never interleave their PUT/POST chunks (studio.rs; enforced by check.py).
    pub(crate) studio_lock: tauri::async_runtime::Mutex<()>,
    /// One Inbox check or create at a time (timer, Check now, connect; inbox.rs).
    pub(crate) inbox_lock: tauri::async_runtime::Mutex<()>,
    emit: Mutex<Option<Emit>>,
}

impl Spotify {
    /// Production: `Runout/spotify` credential (G-15), cache in `%LOCALAPPDATA%`, real network.
    pub fn new(local_data_dir: &Path) -> Self {
        Self::with(
            Some(crate::cleanup::CREDENTIAL.into()),
            None,
            local_data_dir.join(crate::cleanup::CACHE_DB),
            client::net_transport(),
        )
    }

    pub fn with(
        credential: Option<String>,
        client_id: Option<String>,
        cache_path: PathBuf,
        transport: Transport,
    ) -> Self {
        let reconnect_marker = cache_path.with_file_name(crate::cleanup::RECONNECT_MARKER);
        Self {
            credential,
            read_credential: auth::load_tokens,
            delete_credential: crate::cleanup::delete_credential,
            pending_delete: cache_path.with_file_name(crate::cleanup::DISCONNECT_MARKER),
            client_id,
            client_id_file: cache_path.with_file_name(crate::cleanup::CLIENT_ID_FILE),
            tokens: Mutex::new(None),
            reconnect: AtomicBool::new(reconnect_marker.exists()),
            reconnect_marker,
            generation: AtomicU64::new(0),
            cache_path,
            cache: Mutex::new(None),
            transport,
            write_lock: tauri::async_runtime::Mutex::new(()),
            refresh_lock: tauri::async_runtime::Mutex::new(()),
            studio_lock: tauri::async_runtime::Mutex::new(()),
            inbox_lock: tauri::async_runtime::Mutex::new(()),
            emit: Mutex::new(None),
        }
    }

    /// lib.rs wires this to `app.emit("spotify-status", …)`.
    pub fn on_status(&self, f: impl Fn(&SpotifyStatus) + Send + Sync + 'static) {
        *self.emit.lock().unwrap() = Some(Box::new(f));
    }

    /// A failed Credential Manager read emits nothing: the UI keeps the last state it was sent.
    fn emit_status(&self) {
        let Ok(s) = self.status_now() else { return };
        if let Some(f) = &*self.emit.lock().unwrap() {
            f(&s);
        }
    }

    /// The client ID from the test override, [`CLIENT_ID_ENV`], the saved file or the build; missing →
    /// `spotify_auth` with detail [`NO_CLIENT_ID`].
    pub(crate) fn client_id(&self) -> Result<String> {
        let id = self
            .client_id
            .clone()
            .or_else(|| std::env::var(CLIENT_ID_ENV).ok())
            .or_else(|| std::fs::read_to_string(&self.client_id_file).ok())
            .or_else(|| BUILTIN_CLIENT_ID.map(str::to_string))
            .unwrap_or_default();
        check_client_id(&id).map_err(|_| {
            AppError::new(
                Code::SpotifyAuth,
                "Spotify isn't set up yet: paste your Spotify developer app's Client ID.",
            )
            .with_detail(NO_CLIENT_ID)
        })
    }

    /// Save a client ID pasted in the Connect dialog (validated; used from the next sign-in on).
    pub fn save_client_id(&self, id: &str) -> Result<()> {
        let id = check_client_id(id)?;
        std::fs::write(&self.client_id_file, id)?;
        Ok(())
    }

    /// Current tokens; a failed Credential Manager read reads as `None` for this call only.
    /// Anything that deletes or refuses on "signed out" uses [`Self::load_tokens`] instead.
    pub(crate) fn tokens(&self) -> Option<Tokens> {
        self.load_tokens().unwrap_or(None)
    }

    /// Tokens, reading Credential Manager once. `Err` = the read failed, which is not "signed
    /// out": nothing is cached (the next call reads again) and nothing may be deleted on it.
    fn load_tokens(&self) -> Result<Option<Tokens>> {
        let mut t = self.tokens.lock().unwrap();
        if t.is_none() {
            *t = Some(match &self.credential {
                // A disconnect whose delete failed: still signed out; retry the delete.
                Some(target) if self.pending_delete.exists() => {
                    if (self.delete_credential)(target).is_ok() {
                        let _ = std::fs::remove_file(&self.pending_delete);
                    }
                    None
                }
                Some(target) => (self.read_credential)(target)?,
                None => None,
            });
        }
        Ok(t.clone().flatten())
    }

    /// Replace the tokens in memory and in Credential Manager (`None` deletes them). Every call
    /// bumps the generation, so a refresh already in flight drops its answer.
    pub(crate) fn set_tokens(&self, tokens: Option<Tokens>) -> Result<()> {
        let mut slot = self.tokens.lock().unwrap();
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.write_tokens(&mut slot, tokens)
    }

    /// A refresh's answer, stored only if no disconnect/sign-out/sign-in ran since `generation` was read
    /// (checked and written under one lock). `false` = dropped.
    pub(crate) fn set_refreshed(&self, generation: u64, tokens: Tokens) -> Result<bool> {
        let mut slot = self.tokens.lock().unwrap();
        if self.generation.load(Ordering::SeqCst) != generation {
            return Ok(false);
        }
        self.write_tokens(&mut slot, Some(tokens)).map(|_| true)
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Memory always follows, even when Credential Manager fails: a failed delete must not leave
    /// the app signed in, and a rotated token is the only valid one. The store error is returned.
    /// A delete persists `pending_delete` first, so a failed one stays signed out after a restart.
    fn write_tokens(
        &self,
        slot: &mut Option<Option<Tokens>>,
        tokens: Option<Tokens>,
    ) -> Result<()> {
        let stored = match (&self.credential, &tokens) {
            (Some(target), Some(t)) => auth::store_tokens(target, t).inspect(|_| {
                let _ = std::fs::remove_file(&self.pending_delete);
            }),
            (Some(target), None) => {
                let _ = std::fs::write(&self.pending_delete, b"");
                (self.delete_credential)(target).inspect(|_| {
                    let _ = std::fs::remove_file(&self.pending_delete);
                })
            }
            (None, _) => Ok(()),
        };
        *slot = Some(tokens);
        stored
    }

    /// Every R3 command except status/connect/disconnect starts here: disconnected → `spotify_auth`.
    /// A failed Credential Manager read is returned as is (`io`), not as "connect first".
    pub fn require_connected(&self) -> Result<()> {
        if self.load_tokens()?.is_some() {
            return Ok(());
        }
        let msg = if self.reconnect.load(Ordering::SeqCst) {
            "Spotify signed you out. Reconnect Spotify in Settings."
        } else {
            "Connect Spotify in Settings first."
        };
        Err(AppError::new(Code::SpotifyAuth, msg))
    }

    /// Status without network: display name from the cache (`me` entry) when present. A failed
    /// Credential Manager read is returned (`io`), never reported as `disconnected` (TSD §5.2).
    pub fn status_now(&self) -> Result<SpotifyStatus> {
        Ok(match self.load_tokens()? {
            Some(t) => SpotifyStatus {
                state: State::Connected,
                display_name: self
                    .with_cache(|c| cache::get(c, cache::ME, chrono::Utc::now()))
                    .ok()
                    .flatten()
                    .and_then(|v| v["display_name"].as_str().map(str::to_string)),
                renew_by: Some(t.renew_by),
            },
            None if self.reconnect.load(Ordering::SeqCst) => SpotifyStatus {
                state: State::Reconnect,
                display_name: None,
                renew_by: None,
            },
            None => SpotifyStatus {
                state: State::Disconnected,
                display_name: None,
                renew_by: None,
            },
        })
    }

    /// `get_spotify_status`: when connected but the cached name expired, re-reads `GET /me`
    /// (best effort; offline still answers `connected`).
    pub async fn status(&self) -> Result<SpotifyStatus> {
        let s = self.status_now()?;
        if s.state == State::Connected && s.display_name.is_none() {
            let _ = auth::fetch_me(self).await;
            return self.status_now();
        }
        Ok(s)
    }

    /// `invalid_grant` for refresh token `dead`: only if it is still the stored one (checked and
    /// cleared under the tokens lock, so a sign-in landing meanwhile is kept), tokens and
    /// `spotify_cache.db` go (a revoked grant counts as a disconnect for the 5-day rule, TSD §5.2),
    /// state `reconnect` is persisted and `spotify-status` emitted.
    pub(crate) fn mark_reconnect_if(&self, dead: &str) {
        {
            let mut slot = self.tokens.lock().unwrap();
            if !matches!(&*slot, Some(Some(t)) if t.refresh == dead) {
                return;
            }
            self.generation.fetch_add(1, Ordering::SeqCst);
            let _ = self.write_tokens(&mut slot, None);
            let _ = std::fs::write(&self.reconnect_marker, b"");
            self.reconnect.store(true, Ordering::SeqCst);
        }
        // ponytail: the cache lock is taken after the tokens lock is released (with_cache locks
        // cache, then tokens); a sign-in landing in this gap loses only rebuildable cache entries.
        let _ = self.remove_cache();
        self.emit_status();
    }

    /// Run `f` on `spotify_cache.db`, opening (and creating) it on first use. Refuses to open it
    /// without a sign-in, so a request still in flight at disconnect can't re-create the file.
    pub fn with_cache<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let mut g = self.cache.lock().unwrap();
        if g.is_none() {
            self.require_connected()?;
            *g = Some(cache::open(&self.cache_path)?);
        }
        f(g.as_ref().unwrap())
    }

    /// Startup + daily sweeper (TSD §5.2): only when the file exists, so a never-connected
    /// install never gets one. A cache without a sign-in (crash mid-disconnect) is deleted; a
    /// failed Credential Manager read deletes nothing (the error is returned, the next sweep retries).
    pub fn sweep(&self) -> Result<usize> {
        if self.cache.lock().unwrap().is_none() && !self.cache_path.exists() {
            return Ok(0);
        }
        if self.load_tokens()?.is_none() {
            return self.remove_cache().map(|_| 0);
        }
        self.with_cache(|c| cache::sweep(c, chrono::Utc::now()))
    }

    /// Tokens first (so `with_cache` refuses from then on), then the cache file. Both run; the
    /// first error is returned.
    fn clear(&self) -> Result<()> {
        let tokens = self.set_tokens(None);
        let cache = self.remove_cache();
        tokens.and(cache)
    }

    /// Close and delete `spotify_cache.db` (+ journal files).
    fn remove_cache(&self) -> Result<()> {
        let mut first = None;
        let mut g = self.cache.lock().unwrap();
        drop(g.take()); // closes the connection before the file goes
        for side in ["", "-journal", "-wal", "-shm"] {
            let p = PathBuf::from(format!("{}{side}", self.cache_path.display()));
            if let Err(e) = std::fs::remove_file(&p)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                first.get_or_insert(AppError::from(e));
            }
        }
        first.map_or(Ok(()), Err)
    }

    fn clear_reconnect(&self) -> Result<()> {
        self.reconnect.store(false, Ordering::SeqCst);
        match std::fs::remove_file(&self.reconnect_marker) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    /// `disconnect_spotify`: delete the tokens and `spotify_cache.db` (+ journal files) at once
    /// (5-day rule, TSD §5.2). Waits for a running Studio apply/undo (seconds) so it never leaves
    /// a playlist cut short with its undo snapshot deleted. Every step runs; the first error is
    /// returned.
    pub async fn disconnect(&self) -> Result<()> {
        let _studio = self.studio_lock.lock().await;
        let cleared = self.clear();
        let marker = self.clear_reconnect();
        self.emit_status();
        cleared.and(marker)
    }

    /// `connect_spotify` (auth.rs): browser sign-in, then status. `follow`: also ask for
    /// `user-follow-read` (Inbox followed-artists option, TSD §5.3).
    pub async fn connect(&self, follow: bool) -> Result<SpotifyStatus> {
        auth::connect(self, follow).await?;
        let _ = self.clear_reconnect();
        let _ = auth::fetch_me(self).await;
        self.emit_status();
        self.status_now()
    }
}

/// A client ID is up to 64 letters/digits (Spotify's are 32 hex); trimmed.
fn check_client_id(id: &str) -> Result<String> {
    let id = id.trim();
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::validation(
            "That isn't a Client ID. Copy it from your app's page in the Spotify developer dashboard.",
        ));
    }
    Ok(id.to_string())
}

/// Validate a URI at the command boundary: `spotify:{kind}:{22 base62}` with `kind` in `kinds`.
pub fn check_uri(uri: &str, kinds: &[&str]) -> Result<String> {
    let uri = uri.trim();
    let ok = uri.split(':').collect::<Vec<_>>();
    let valid = matches!(ok.as_slice(), ["spotify", k, id]
        if kinds.contains(k) && id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric()));
    if valid {
        Ok(uri.to_string())
    } else {
        Err(AppError::new(
            Code::InvalidLink,
            "That isn't a Spotify link Runout can use.",
        ))
    }
}

/// The 22-char id of a URI already checked by [`check_uri`].
pub fn uri_id(uri: &str) -> &str {
    uri.rsplit(':').next().unwrap_or_default()
}

/// Test helpers for every spotify/ module: a stub transport, a connected `Spotify` with memory
/// tokens and a temp cache, never the real API or the real credential.
#[cfg(test)]
pub mod testing {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Arc;

    /// Scripted responses, popped in order; every request is recorded.
    #[derive(Clone, Default)]
    pub struct Stub {
        pub responses: Arc<Mutex<VecDeque<HttpResp>>>,
        pub requests: Arc<Mutex<Vec<HttpReq>>>,
        /// Each answer arrives this late (lets concurrent callers overlap in flight).
        pub delay_ms: u64,
    }

    impl Stub {
        pub fn new(responses: &[(u16, &str)]) -> Self {
            let s = Self::default();
            for (status, body) in responses {
                s.push(*status, body);
            }
            s
        }
        pub fn push(&self, status: u16, body: &str) {
            self.responses.lock().unwrap().push_back(HttpResp {
                status,
                retry_after: None,
                body: body.to_string(),
            });
        }
        pub fn push_resp(&self, r: HttpResp) {
            self.responses.lock().unwrap().push_back(r);
        }
        pub fn transport(&self) -> Transport {
            let s = self.clone();
            Arc::new(move |req: HttpReq| {
                let s = s.clone();
                Box::pin(async move {
                    crate::net::check(&req.url)?; // stubs still obey the allow-list
                    if s.delay_ms > 0 {
                        let ms = s.delay_ms;
                        let _ = tauri::async_runtime::spawn_blocking(move || {
                            std::thread::sleep(std::time::Duration::from_millis(ms))
                        })
                        .await;
                    }
                    s.requests.lock().unwrap().push(req);
                    s.responses
                        .lock()
                        .unwrap()
                        .pop_front()
                        .ok_or_else(|| AppError::internal("stub: no response left"))
                })
            })
        }
        /// `(method, url)` of every request so far.
        pub fn calls(&self) -> Vec<(String, String)> {
            self.requests
                .lock()
                .unwrap()
                .iter()
                .map(|r| (r.method.to_string(), r.url.clone()))
                .collect()
        }
    }

    pub fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "runout-sp-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Disconnected `Spotify` on `stub`, memory tokens, cache in `dir`.
    pub fn spotify(dir: &Path, stub: &Stub) -> Spotify {
        Spotify::with(
            None,
            Some("TestClientId0000".into()),
            dir.join(crate::cleanup::CACHE_DB),
            stub.transport(),
        )
    }

    /// Connected with a fresh synthetic access token (valid 1 h).
    pub fn connected(dir: &Path, stub: &Stub) -> Spotify {
        let sp = spotify(dir, stub);
        sp.set_tokens(Some(Tokens::synthetic(3600))).unwrap();
        sp
    }

    pub fn block<F: std::future::Future>(f: F) -> F::Output {
        tauri::async_runtime::block_on(f)
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use std::sync::Arc;

    /// A request in flight when Disconnect runs must not bring back the file or the tokens.
    #[test]
    fn disconnect_during_a_request_leaves_no_file_and_no_tokens() {
        let d = temp_dir("mod-inflight");
        let mut stub = Stub::new(&[(200, r#"{"display_name":"Synthetic"}"#)]);
        stub.delay_ms = 300;
        let sp = Arc::new(connected(&d, &stub));
        let sp2 = sp.clone();
        let req = tauri::async_runtime::spawn(async move { auth::fetch_me(&sp2).await });
        std::thread::sleep(std::time::Duration::from_millis(100));
        block(sp.disconnect()).unwrap();
        let r = block(req).unwrap();
        assert_eq!(r.unwrap_err().code, Code::SpotifyAuth);
        assert!(
            !d.join(crate::cleanup::CACHE_DB).exists(),
            "cache re-created"
        );
        assert!(sp.tokens().is_none());
        assert_eq!(sp.status_now().unwrap().state, State::Disconnected);
    }

    /// An expired token being refreshed at Disconnect: the answer is dropped, not stored.
    #[test]
    fn refresh_answer_after_disconnect_is_dropped() {
        let d = temp_dir("mod-refresh-gone");
        let mut stub = Stub::new(&[(
            200,
            r#"{"access_token":"a2","expires_in":3600,"refresh_token":"r2"}"#,
        )]);
        stub.delay_ms = 300;
        let sp = Arc::new(spotify(&d, &stub));
        sp.set_tokens(Some(Tokens::synthetic(-10))).unwrap();
        let sp2 = sp.clone();
        let req = tauri::async_runtime::spawn(async move { auth::access_token(&sp2, None).await });
        std::thread::sleep(std::time::Duration::from_millis(100));
        block(sp.disconnect()).unwrap();
        assert!(block(req).unwrap().is_err());
        assert!(
            sp.tokens().is_none(),
            "refresh re-stored tokens after disconnect"
        );
    }

    #[test]
    fn reconnect_survives_restart_and_purges_the_cache() {
        let d = temp_dir("mod-reconnect");
        let stub = Stub::new(&[]);
        let sp = connected(&d, &stub);
        sp.with_cache(|c| {
            cache::put(
                c,
                cache::ME,
                "me",
                &serde_json::json!({}),
                chrono::Utc::now(),
            )
        })
        .unwrap();
        sp.mark_reconnect_if(&sp.tokens().unwrap().refresh);
        assert!(
            !d.join(crate::cleanup::CACHE_DB).exists(),
            "revoked grant keeps the cache"
        );
        drop(sp);
        let again = spotify(&d, &stub);
        assert_eq!(again.status_now().unwrap().state, State::Reconnect);
        block(again.disconnect()).unwrap();
        assert_eq!(
            spotify(&d, &stub).status_now().unwrap().state,
            State::Disconnected
        );
    }

    /// A Credential Manager read error is not "signed out": the sweeper keeps the cache (and its
    /// undo snapshots), nothing is cached, and the next read sees the sign-in.
    #[test]
    fn credential_read_error_keeps_the_cache_and_is_retried() {
        use std::sync::atomic::AtomicBool;
        static FAIL: AtomicBool = AtomicBool::new(true);
        fn flaky(_: &str) -> Result<Option<Tokens>> {
            if FAIL.load(Ordering::SeqCst) {
                Err(AppError::new(Code::Io, "vault locked"))
            } else {
                Ok(Some(Tokens::synthetic(3600)))
            }
        }
        let d = temp_dir("mod-cred-error");
        let stub = Stub::new(&[]);
        connected(&d, &stub)
            .with_cache(|c| {
                cache::put_snapshot(
                    c,
                    "spotify:playlist:PPPPPPPPPPPPPPPPPPPPPP",
                    "s1",
                    &serde_json::json!([]),
                    chrono::Utc::now(),
                )
            })
            .unwrap();
        let mut sp = spotify(&d, &stub);
        sp.credential = Some("never-written".into());
        sp.read_credential = flaky;
        assert_eq!(sp.sweep().unwrap_err().code, Code::Io);
        assert!(
            d.join(crate::cleanup::CACHE_DB).exists(),
            "read error deleted the cache"
        );
        assert_eq!(sp.require_connected().unwrap_err().code, Code::Io);
        // status says "couldn't read", never "disconnected" (Settings would offer Connect)
        assert_eq!(sp.status_now().unwrap_err().code, Code::Io);
        let sent = Arc::new(Mutex::new(0));
        let s2 = sent.clone();
        sp.on_status(move |_| *s2.lock().unwrap() += 1);
        sp.emit_status();
        assert_eq!(*sent.lock().unwrap(), 0, "a guessed state was emitted");
        FAIL.store(false, Ordering::SeqCst);
        assert_eq!(
            sp.status_now().unwrap().state,
            State::Connected,
            "read error was cached"
        );
        assert_eq!(sp.sweep().unwrap(), 0);
        assert!(d.join(crate::cleanup::CACHE_DB).exists());
    }

    /// Disconnect whose Credential Manager delete fails: the app says `disconnected` and must stay
    /// so after a restart (the stored refresh token must not sign it back in and re-create the
    /// cache); each start retries the delete until it succeeds.
    #[test]
    fn failed_credential_delete_stays_signed_out_after_restart() {
        use std::sync::atomic::AtomicBool;
        static FAIL: AtomicBool = AtomicBool::new(true);
        fn still_stored(_: &str) -> Result<Option<Tokens>> {
            Ok(Some(Tokens::synthetic(3600)))
        }
        fn flaky_delete(_: &str) -> Result<()> {
            if FAIL.load(Ordering::SeqCst) {
                Err(AppError::new(Code::Io, "vault locked"))
            } else {
                Ok(())
            }
        }
        let restart = |d: &Path, stub: &Stub| {
            let mut sp = spotify(d, stub);
            sp.credential = Some("never-written".into());
            sp.read_credential = still_stored;
            sp.delete_credential = flaky_delete;
            sp
        };
        let d = temp_dir("mod-delete-fails");
        let stub = Stub::new(&[]);
        let mut sp = connected(&d, &stub);
        sp.credential = Some("never-written".into());
        sp.delete_credential = flaky_delete;
        assert_eq!(block(sp.disconnect()).unwrap_err().code, Code::Io);
        assert_eq!(sp.status_now().unwrap().state, State::Disconnected);
        let marker = d.join(crate::cleanup::DISCONNECT_MARKER);
        assert!(marker.exists(), "pending delete not persisted");

        let sp = restart(&d, &stub);
        assert_eq!(
            sp.status_now().unwrap().state,
            State::Disconnected,
            "undeleted token signed the app back in"
        );
        assert!(sp.with_cache(|_| Ok(())).is_err());
        assert!(!d.join(crate::cleanup::CACHE_DB).exists());
        assert!(
            marker.exists(),
            "marker dropped before the delete succeeded"
        );

        FAIL.store(false, Ordering::SeqCst);
        let sp = restart(&d, &stub);
        assert_eq!(sp.status_now().unwrap().state, State::Disconnected);
        assert!(!marker.exists(), "retried delete didn't clear the marker");
    }

    /// A sign-in while a refresh is out: the refresh's answer (old grant) is dropped, and an
    /// `invalid_grant` for the old refresh token doesn't wipe the new sign-in.
    #[test]
    fn sign_in_beats_a_stale_refresh_or_invalid_grant() {
        let d = temp_dir("mod-signin-race");
        let sp = connected(&d, &Stub::new(&[]));
        let old = sp.tokens().unwrap();
        let g = sp.generation();
        let fresh = Tokens {
            refresh: "new-grant".into(),
            ..Tokens::synthetic(3600)
        };
        sp.set_tokens(Some(fresh.clone())).unwrap(); // exchange stores the new sign-in
        assert!(
            !sp.set_refreshed(g, old.clone()).unwrap(),
            "stale refresh stored"
        );
        sp.mark_reconnect_if(&old.refresh);
        assert!(sp.tokens().unwrap() == fresh);
        assert_eq!(sp.status_now().unwrap().state, State::Connected);
    }

    #[test]
    fn orphan_cache_without_sign_in_is_swept_away() {
        let d = temp_dir("mod-orphan");
        let stub = Stub::new(&[]);
        let sp = connected(&d, &stub);
        sp.with_cache(|c| {
            cache::put(
                c,
                cache::ME,
                "me",
                &serde_json::json!({}),
                chrono::Utc::now(),
            )
        })
        .unwrap();
        drop(sp); // memory tokens gone, file left (a crash mid-disconnect)
        let sp = spotify(&d, &stub);
        assert_eq!(sp.sweep().unwrap(), 0);
        assert!(!d.join(crate::cleanup::CACHE_DB).exists());
    }
}

//! The only outbound HTTP in Runout (TSD §7): one shared client, host allow-list checked before every
//! request and on every redirect hop, `online()` cached 30 s with an `online-changed` hook (TSD §4.3).
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use reqwest::{Client, Method, RequestBuilder, Response, Url, redirect};

use crate::error::{AppError, Result};

/// TSD §7. Keep in sync with `ALLOWED_HOSTS` in `scripts/check.py`.
pub const ALLOWED_HOSTS: [&str; 4] = [
    "open.spotify.com",
    "spotify.link",
    "api.spotify.com",
    "accounts.spotify.com",
];
pub const MAX_REDIRECTS: usize = 5;
pub const TIMEOUT: Duration = Duration::from_secs(5);
const ONLINE_TTL: Duration = Duration::from_secs(30);
const ONLINE_PROBE: &str = "https://open.spotify.com/";

fn host_allowed(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|h| ALLOWED_HOSTS.contains(&h))
}

/// Parse `url` and refuse anything outside the allow-list (https, default port, listed host only).
pub fn check(url: &str) -> Result<Url> {
    let parsed = Url::parse(url).map_err(|_| AppError::validation("Not a web address."))?;
    if host_allowed(&parsed) {
        Ok(parsed)
    } else {
        Err(AppError::validation("Runout doesn't contact that address."))
    }
}

static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    let policy = redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            attempt.error("too many redirects")
        } else if !host_allowed(attempt.url()) {
            attempt.error("redirect off the allow-list")
        } else {
            attempt.follow()
        }
    });
    Client::builder()
        .redirect(policy)
        .connect_timeout(TIMEOUT)
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("Runout/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("reqwest client")
});

/// Send a request after the allow-list check. `build` adds headers/body/timeout (R3 adds auth here).
/// Any HTTP status is returned as-is; callers map 401/403/429 themselves.
pub async fn send(
    method: Method,
    url: &str,
    build: impl FnOnce(RequestBuilder) -> RequestBuilder,
) -> Result<Response> {
    let url = check(url)?;
    Ok(build(CLIENT.request(method, url)).send().await?)
}

/// GET with the 5 s timeout used for links and oEmbed.
pub async fn get(url: &str) -> Result<Response> {
    send(Method::GET, url, |r| r.timeout(TIMEOUT)).await
}

type OnChange = Box<dyn Fn(bool) + Send + Sync>;
static ON_CHANGE: Mutex<Option<OnChange>> = Mutex::new(None);
static ONLINE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

/// Register the `online-changed` emitter (lib.rs wires it to the AppHandle).
pub fn on_online_change(f: impl Fn(bool) + Send + Sync + 'static) {
    *ON_CHANGE.lock().unwrap() = Some(Box::new(f));
}

/// HEAD open.spotify.com, cached 30 s. Any HTTP answer counts as online.
// ponytail: no OS network-change hook yet; lib.rs re-probes every 30 s instead.
pub async fn online() -> bool {
    if let Some((at, v)) = *ONLINE.lock().unwrap()
        && at.elapsed() < ONLINE_TTL
    {
        return v;
    }
    let now = send(Method::HEAD, ONLINE_PROBE, |r| r.timeout(TIMEOUT))
        .await
        .is_ok();
    record_online(now);
    now
}

/// Store a probe result; fires the hook when it differs from the last known value.
pub fn record_online(now: bool) {
    let prev = ONLINE
        .lock()
        .unwrap()
        .replace((Instant::now(), now))
        .map(|(_, v)| v);
    if prev.is_some_and(|p| p != now)
        && let Some(f) = &*ON_CHANGE.lock().unwrap()
    {
        f(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Code;

    #[test]
    fn allow_list() {
        for ok in [
            "https://open.spotify.com/oembed?url=x",
            "https://spotify.link/abc",
            "https://api.spotify.com/v1/me",
            "https://accounts.spotify.com/api/token",
        ] {
            assert!(check(ok).is_ok(), "{ok}");
        }
        let h = "https:/";
        for bad in [
            format!("{h}/blocked.invalid/"),
            format!("{h}/open.spotify.com.blocked.invalid/"),
            format!("{h}/open.spotify.com@blocked.invalid/"),
            format!("{h}/open.spotify.com:8443/"),
            format!("{h}/127.0.0.1/"),
            "http://open.spotify.com/".to_string(),
            "ftp://spotify.link/x".to_string(),
            "open.spotify.com/album".to_string(),
        ] {
            assert_eq!(check(&bad).unwrap_err().code, Code::Validation, "{bad}");
        }
    }

    #[test]
    fn non_listed_host_is_refused_without_a_network_call() {
        // `.invalid` never resolves: had a request gone out, the error would be `offline`, not `validation`.
        let url = format!("{}//blocked.invalid/", "https:");
        let e = tauri::async_runtime::block_on(get(&url)).unwrap_err();
        assert_eq!(e.code, Code::Validation);
    }

    #[test]
    fn online_change_fires_only_on_flip() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static FIRED: AtomicUsize = AtomicUsize::new(0);
        on_online_change(|_| {
            FIRED.fetch_add(1, Ordering::SeqCst);
        });
        record_online(true);
        record_online(true);
        record_online(false);
        record_online(false);
        record_online(true);
        assert_eq!(FIRED.load(Ordering::SeqCst), 2);
    }
}

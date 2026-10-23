//! Web API client (TSD §5.2): bearer header, refresh on expiry/401, 429 `Retry-After` backoff, max
//! 1 in-flight write, pagination. Requests leave through a [`Transport`]; production's is
//! `net::send` (allow-list, TSD §7), tests inject `spotify::testing::Stub`.
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::Spotify;
use crate::error::{AppError, Code, Result};

pub const API: &str = "https://api.spotify.com/v1";
/// Wait out a 429 in Rust up to this long; longer → `spotify_busy` with `retry_after_s`.
const MAX_WAIT_S: u64 = 10;
const MAX_TRIES: usize = 3;

pub enum Body {
    Json(Value),
    /// `application/x-www-form-urlencoded`, already encoded ([`form`]).
    Form(String),
}

/// One request. Not `Debug`: it carries the bearer token.
pub struct HttpReq {
    pub method: Method,
    pub url: String,
    pub bearer: Option<String>,
    pub body: Option<Body>,
}

pub struct HttpResp {
    pub status: u16,
    pub retry_after: Option<u64>,
    pub body: String,
}

pub type Transport =
    Arc<dyn Fn(HttpReq) -> Pin<Box<dyn Future<Output = Result<HttpResp>> + Send>> + Send + Sync>;

/// Production transport: `net::send` (allow-list checked there).
pub fn net_transport() -> Transport {
    Arc::new(|r: HttpReq| {
        Box::pin(async move {
            let HttpReq {
                method,
                url,
                bearer,
                body,
            } = r;
            let write = method != Method::GET;
            let resp = crate::net::send(method, &url, |b| apply(b, bearer.as_deref(), body, write))
                .await?;
            let retry_after = resp
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse().ok());
            Ok(HttpResp {
                status: resp.status().as_u16(),
                retry_after,
                body: resp.text().await?,
            })
        })
    })
}

/// Bearer, body and headers onto a request (split out so the headers are unit-tested: every Rust
/// test runs on the stub transport, which never builds a real request).
fn apply(
    mut b: reqwest::RequestBuilder,
    bearer: Option<&str>,
    body: Option<Body>,
    write: bool,
) -> reqwest::RequestBuilder {
    if let Some(t) = bearer {
        b = b.bearer_auth(t);
    }
    match body {
        Some(Body::Json(v)) => b
            .header("Content-Type", "application/json")
            .body(v.to_string()),
        Some(Body::Form(s)) => b
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(s),
        // Spotify answers 411 to a body-less PUT/DELETE (`/me/library` save, M7 live run 21 Oct
        // 2026; KB spotify.md). hyper omits the header for an empty body, so it is set explicitly
        // (a bare `.body("")` still got 411).
        None if !write => b,
        None => b.header(reqwest::header::CONTENT_LENGTH, "0").body(""),
    }
}

/// Percent-encode for query strings and form bodies (RFC 3986 unreserved kept).
pub fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// `k=v&k=v`, encoded.
pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| [enc(k), enc(v)].join("="))
        .collect::<Vec<_>>()
        .join("&")
}

/// `path` (`/me/playlists?limit=50`) → full API URL; a full `https://` URL (a `next` link) as is.
fn url(path: &str) -> String {
    if path.starts_with("https://") {
        path.to_string()
    } else {
        [API, path].concat()
    }
}

fn sleep(secs: u64) -> impl Future<Output = ()> {
    let h =
        tauri::async_runtime::spawn_blocking(move || std::thread::sleep(Duration::from_secs(secs)));
    async move {
        let _ = h.await;
    }
}

/// HTTP status → API.md error (TSD §6). 2xx never reaches here.
fn status_error(r: &HttpResp) -> AppError {
    match r.status {
        401 => AppError::new(
            Code::SpotifyAuth,
            "Spotify sign-in expired. Reconnect Spotify.",
        ),
        403 => AppError::new(
            Code::SpotifyForbidden,
            "Spotify refused this (Premium account, or not on the app's allow-list?).",
        ),
        404 => AppError::not_found("Spotify couldn't find that."),
        429 => AppError {
            retry_after_s: r.retry_after,
            ..AppError::new(Code::SpotifyBusy, "Spotify is busy. Try again shortly.")
        },
        s => AppError::internal("Spotify answered with an error.").with_detail(format!("HTTP {s}")),
    }
}

/// A write that failed with this error definitely didn't land: Spotify refused it with a 4xx (incl.
/// 429). Anything else (offline/timeout/reset, 5xx, unreadable 2xx) may still have landed (TSD §5.2).
pub fn refused(e: &AppError) -> bool {
    matches!(
        e.code,
        Code::SpotifyAuth | Code::SpotifyForbidden | Code::NotFound | Code::SpotifyBusy
    ) || e.detail.as_deref().is_some_and(|d| d.starts_with("HTTP 4"))
}

impl Spotify {
    /// Send one authorised request; returns the JSON body (`Null` when empty).
    /// Writes (anything but GET) hold the write lock for the whole exchange.
    pub async fn request(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let _w = if method == Method::GET {
            None
        } else {
            Some(self.write_lock.lock().await)
        };
        let mut refreshed = false;
        let mut tries = 0;
        loop {
            let access = super::auth::access_token(self, None).await?;
            let resp = (self.transport)(HttpReq {
                method: method.clone(),
                url: url(path),
                bearer: Some(access.clone()),
                body: body.clone().map(Body::Json),
            })
            .await?;
            match resp.status {
                200..=299 if resp.body.trim().is_empty() => return Ok(Value::Null),
                200..=299 => return Ok(serde_json::from_str(&resp.body)?),
                401 if !refreshed => {
                    refreshed = true;
                    super::auth::access_token(self, Some(&access)).await?;
                }
                429 if tries + 1 < MAX_TRIES && resp.retry_after.unwrap_or(1) <= MAX_WAIT_S => {
                    tries += 1;
                    sleep(resp.retry_after.unwrap_or(1)).await;
                }
                _ => return Err(status_error(&resp)),
            }
        }
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        Ok(serde_json::from_value(
            self.request(Method::GET, path, None).await?,
        )?)
    }

    /// POST/PUT/DELETE with a JSON body (or none).
    pub async fn send_json(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        self.request(method, path, body).await
    }

    /// Follow a paging object's `next` links from `first` and collect every `items` entry, up to
    /// `max` (Spotify pages are ≤ 50 / ≤ 100).
    pub async fn paginate<T: DeserializeOwned>(&self, first: &str, max: usize) -> Result<Vec<T>> {
        let mut out = Vec::new();
        let mut next = Some(first.to_string());
        while let Some(path) = next.take() {
            let page = self.request(Method::GET, &path, None).await?;
            let items = page["items"].as_array().map_or(&[][..], Vec::as_slice);
            if items.is_empty() {
                break; // a `next` past an empty page would loop forever on a malformed answer
            }
            for it in items {
                out.push(serde_json::from_value(it.clone())?);
                if out.len() >= max {
                    return Ok(out);
                }
            }
            next = page["next"].as_str().map(str::to_string);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn bodyless_writes_send_content_length_zero_and_gets_dont() {
        let c = reqwest::Client::new();
        let len = |m: Method| {
            let write = m != Method::GET;
            let r = super::apply(
                c.request(m, "https://api.spotify.com/v1/me/library"),
                Some("t"),
                None,
                write,
            )
            .build()
            .unwrap();
            r.headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .map(|v| v.to_str().unwrap().to_owned())
        };
        assert_eq!(len(Method::PUT).as_deref(), Some("0"));
        assert_eq!(len(Method::DELETE).as_deref(), Some("0"));
        assert_eq!(len(Method::GET), None);
    }

    use super::*;
    use crate::spotify::testing::*;

    #[test]
    fn encodes_like_rfc3986() {
        assert_eq!(enc("a b/c:d~_.-"), "a%20b%2Fc%3Ad~_.-");
        assert_eq!(form(&[("x", "1&2"), ("y", "é")]), "x=1%262&y=%C3%A9");
    }

    #[test]
    fn disconnected_is_spotify_auth_without_a_request() {
        let d = temp_dir("client-auth");
        let stub = Stub::new(&[]);
        let sp = spotify(&d, &stub);
        let e = block(sp.get_json::<Value>("/me")).unwrap_err();
        assert_eq!(e.code, Code::SpotifyAuth);
        assert!(stub.calls().is_empty());
    }

    #[test]
    fn retries_429_then_succeeds_and_long_wait_is_busy() {
        let d = temp_dir("client-429");
        let stub = Stub::new(&[]);
        let busy = |s| HttpResp {
            status: 429,
            retry_after: Some(s),
            body: String::new(),
        };
        stub.push_resp(busy(0));
        stub.push(200, r#"{"ok":true}"#);
        let sp = connected(&d, &stub);
        let v: Value = block(sp.get_json("/me")).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(stub.calls().len(), 2);
        stub.push_resp(busy(120));
        let e = block(sp.get_json::<Value>("/me")).unwrap_err();
        assert_eq!((e.code, e.retry_after_s), (Code::SpotifyBusy, Some(120)));
    }

    #[test]
    fn paginates_next_links() {
        let d = temp_dir("client-page");
        let next = format!("{API}/me/playlists?offset=2");
        let stub = Stub::new(&[
            (200, &format!(r#"{{"items":[1,2],"next":"{next}"}}"#)),
            (200, r#"{"items":[3],"next":null}"#),
        ]);
        let sp = connected(&d, &stub);
        let all: Vec<i64> = block(sp.paginate("/me/playlists?limit=2", 100)).unwrap();
        assert_eq!(all, [1, 2, 3]);
        assert_eq!(stub.calls()[1].1, next);
    }

    #[test]
    fn writes_are_serialised() {
        // the write lock is held for the whole exchange: a second write waits for it
        let d = temp_dir("client-write");
        let stub = Stub::new(&[(201, "")]);
        let sp = connected(&d, &stub);
        let held = block(sp.write_lock.lock());
        assert!(sp.write_lock.try_lock().is_err());
        drop(held);
        assert_eq!(
            block(sp.send_json(Method::POST, "/me/playlists", None)).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn forbidden_maps_to_spotify_forbidden() {
        let d = temp_dir("client-403");
        let stub = Stub::new(&[(403, "{}")]);
        let sp = connected(&d, &stub);
        let e = block(sp.get_json::<Value>("/me/top/tracks")).unwrap_err();
        assert_eq!(e.code, Code::SpotifyForbidden);
    }

    /// Every failure shape ends as a typed error, and only 4xx counts as "write refused".
    #[test]
    fn http_failures_map_to_typed_errors() {
        let d = temp_dir("client-errors");
        let get = |resps: &[(u16, &str)]| {
            let stub = Stub::new(resps);
            block(connected(&d, &stub).get_json::<Value>("/me")).unwrap_err()
        };
        // 401 twice: one refresh, then sign-in expired
        let e = get(&[
            (401, "{}"),
            (200, r#"{"access_token":"a2","expires_in":3600}"#),
            (401, "{}"),
        ]);
        assert_eq!(e.code, Code::SpotifyAuth);
        let e = get(&[(503, "<html>")]);
        assert_eq!(
            (e.code, e.detail.as_deref()),
            (Code::Internal, Some("HTTP 503"))
        );
        assert!(!refused(&e), "a 5xx write may have landed");
        assert!(refused(&get(&[(400, "{}")])));
        // malformed 2xx bodies
        assert_eq!(get(&[(200, "{\"items\": [")]).code, Code::Internal);
        let stub = Stub::new(&[(200, r#"{"id": 5}"#)]);
        let e = block(connected(&d, &stub).get_json::<Vec<bool>>("/x")).unwrap_err();
        assert_eq!(e.code, Code::Internal);
    }

    /// A page with no items ends paging even if `next` points back at it.
    #[test]
    fn paginate_stops_on_an_empty_page() {
        let d = temp_dir("client-loop");
        let stub = Stub::new(&[]);
        for _ in 0..5 {
            stub.push(
                200,
                &format!(r#"{{"items": [], "next": "{API}/me/playlists"}}"#),
            );
        }
        let all: Vec<Value> = block(connected(&d, &stub).paginate("/me/playlists", 100)).unwrap();
        assert!(all.is_empty());
        assert_eq!(stub.calls().len(), 1);
    }
}

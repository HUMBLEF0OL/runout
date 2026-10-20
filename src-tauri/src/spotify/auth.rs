//! Authorization Code + PKCE (TSD §5.2): S256 challenge, free loopback port, browser sign-in,
//! `/callback` with a state check, code exchange; tokens in Credential Manager under exactly
//! `Runout/spotify` (G-15, same target `cleanup.rs` deletes). Refresh on expiry/401 and store the
//! rotated refresh token; `invalid_grant` → tokens deleted, state `reconnect`.
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use reqwest::Method;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::Spotify;
use super::client::{Body, HttpReq, form};
use crate::error::{AppError, Code, Result};

pub const AUTHORIZE: &str = "https://accounts.spotify.com/authorize";
pub const TOKEN: &str = "https://accounts.spotify.com/api/token";
pub const SCOPES: &str = "playlist-read-private playlist-read-collaborative playlist-modify-private playlist-modify-public user-top-read user-read-recently-played user-library-read user-library-modify";
/// How long the browser sign-in may take.
const LOGIN_WAIT: Duration = Duration::from_secs(300);
/// Refresh this long before `expires_at`.
const EARLY_S: i64 = 60;

/// What Credential Manager holds. Deliberately not `Debug`: nothing may print it.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    /// Unix seconds.
    pub expires_at: i64,
    /// ISO-8601 UTC: sign-in + 6 months (refreshing doesn't extend it).
    pub renew_by: String,
}

impl Tokens {
    #[cfg(test)]
    pub fn synthetic(valid_s: i64) -> Self {
        Self {
            access: "synthetic-access".into(),
            refresh: "synthetic-refresh".into(),
            expires_at: chrono::Utc::now().timestamp() + valid_s,
            renew_by: "2027-04-19T00:00:00Z".into(),
        }
    }
}

/* ------------------------------------------------------------------ PKCE */

fn b64url(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = c
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..=c.len() {
            out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

/// RFC 7636 §4.2 S256: `BASE64URL(SHA256(verifier))`, no padding.
pub fn challenge(verifier: &str) -> String {
    b64url(&Sha256::digest(verifier.as_bytes()))
}

/// `n` random unreserved characters (verifier: 64, within RFC 7636's 43–128).
fn random(n: usize) -> String {
    pick(
        n,
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~",
    )
}

/// OAuth `state`: letters and digits only. It comes back in the callback query, and a `~` can
/// arrive as `%7E` (live sign-in failed this way, 20 Oct 2026), which a raw compare rejects.
fn random_state() -> String {
    pick(
        32,
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
    )
}

fn pick(n: usize, alphabet: &[u8]) -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    (0..n)
        .map(|_| alphabet[rng.random_range(0..alphabet.len())] as char)
        .collect()
}

/* ------------------------------------------------------------------ Credential Manager */

#[cfg(windows)]
fn cred_err(e: windows::core::Error) -> AppError {
    AppError::new(Code::Io, "Couldn't use the saved Spotify sign-in.").with_detail(e.message())
}

/// Write the tokens as a generic credential named exactly `target`.
#[cfg(windows)]
pub fn store_tokens(target: &str, t: &Tokens) -> Result<()> {
    use windows::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredWriteW,
    };
    use windows::core::PWSTR;
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (mut name, mut user) = (wide(target), wide("Runout"));
    let mut blob = serde_json::to_vec(t)?;
    let cred = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(user.as_mut_ptr()),
        ..Default::default()
    };
    unsafe { CredWriteW(&cred, 0) }.map_err(cred_err)
}

#[cfg(windows)]
pub fn load_tokens(target: &str) -> Result<Option<Tokens>> {
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{
        CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW,
    };
    use windows::core::HSTRING;
    let mut p: *mut CREDENTIALW = std::ptr::null_mut();
    match unsafe { CredReadW(&HSTRING::from(target), CRED_TYPE_GENERIC, None, &mut p) } {
        Err(e) if e.code() == ERROR_NOT_FOUND.to_hresult() => Ok(None),
        Err(e) => Err(cred_err(e)),
        Ok(()) => {
            let bytes = unsafe {
                let c = &*p;
                let b = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize)
                    .to_vec();
                CredFree(p as *const std::ffi::c_void);
                b
            };
            // an unreadable blob counts as signed out (connect again overwrites it)
            Ok(serde_json::from_slice(&bytes).ok())
        }
    }
}

#[cfg(not(windows))]
pub fn store_tokens(_target: &str, _t: &Tokens) -> Result<()> {
    Err(AppError::internal("Credential Manager is Windows-only."))
}

#[cfg(not(windows))]
pub fn load_tokens(_target: &str) -> Result<Option<Tokens>> {
    Ok(None)
}

/* ------------------------------------------------------------------ token endpoint */

#[derive(Deserialize)]
struct TokenResp {
    access_token: String,
    expires_in: i64,
    refresh_token: Option<String>,
}

/// Outcome of a token-endpoint call. Error bodies contribute only their `error` code.
enum Grant {
    Ok(TokenResp),
    InvalidGrant,
}

async fn token_call(sp: &Spotify, pairs: &[(&str, &str)]) -> Result<Grant> {
    let r = (sp.transport)(HttpReq {
        method: Method::POST,
        url: TOKEN.into(),
        bearer: None,
        body: Some(Body::Form(form(pairs))),
    })
    .await?;
    if (200..300).contains(&r.status) {
        return Ok(Grant::Ok(serde_json::from_str(&r.body).map_err(|_| {
            AppError::new(
                Code::SpotifyAuth,
                "Spotify's sign-in answer wasn't readable.",
            )
        })?));
    }
    let err = serde_json::from_str::<serde_json::Value>(&r.body)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_string))
        .unwrap_or_default();
    if err == "invalid_grant" {
        return Ok(Grant::InvalidGrant);
    }
    // TSD §6: a busy or failing accounts service is "Spotify busy", not a sign-in problem
    if r.status == 429 || r.status >= 500 {
        return Err(AppError {
            retry_after_s: r.retry_after,
            ..AppError::new(Code::SpotifyBusy, "Spotify is busy. Try again shortly.")
        });
    }
    Err(AppError::new(Code::SpotifyAuth, "Spotify sign-in failed.")
        .with_detail(format!("HTTP {} {err}", r.status)))
}

fn expires_at(expires_in: i64) -> i64 {
    chrono::Utc::now().timestamp() + expires_in
}

/// A valid access token: refreshes when the stored one is `rejected` (it just got a 401) or within
/// [`EARLY_S`] of expiry. The rotated refresh token is stored whenever Spotify sends one.
/// Single-flight: callers queue on `refresh_lock` and re-check, so one rotation serves them all and
/// nobody sends an already-rotated refresh token; a refresh answered after a disconnect is dropped.
pub async fn access_token(sp: &Spotify, rejected: Option<&str>) -> Result<String> {
    let stale = |t: &Tokens| {
        rejected == Some(t.access.as_str())
            || t.expires_at - EARLY_S <= chrono::Utc::now().timestamp()
    };
    let current = || -> Result<Tokens> {
        sp.require_connected()?;
        sp.tokens()
            .ok_or_else(|| AppError::new(Code::SpotifyAuth, "Connect Spotify first."))
    };
    let t = current()?;
    if !stale(&t) {
        return Ok(t.access);
    }
    let _one = sp.refresh_lock.lock().await;
    let t = current()?;
    if !stale(&t) {
        return Ok(t.access); // another caller refreshed while we waited
    }
    let generation = sp.generation();
    let client_id = sp.client_id()?;
    let pairs = [
        ("grant_type", "refresh_token"),
        ("refresh_token", t.refresh.as_str()),
        ("client_id", client_id.as_str()),
    ];
    match token_call(sp, &pairs).await? {
        Grant::Ok(r) => {
            let next = Tokens {
                access: r.access_token,
                refresh: r.refresh_token.unwrap_or(t.refresh),
                expires_at: expires_at(r.expires_in),
                renew_by: t.renew_by,
            };
            if !sp.set_refreshed(generation, next.clone())? {
                return Err(AppError::new(
                    Code::SpotifyAuth,
                    "Connect Spotify in Settings first.",
                ));
            }
            Ok(next.access)
        }
        Grant::InvalidGrant => {
            // only a refresh token that is still the stored one is a dead grant
            sp.mark_reconnect_if(&t.refresh);
            Err(AppError::new(
                Code::SpotifyAuth,
                "Spotify signed you out. Reconnect Spotify in Settings.",
            ))
        }
    }
}

/* ------------------------------------------------------------------ loopback sign-in */

/// One HTTP request line from the browser. `Ok(None)` = not the callback (favicon etc.).
/// `Ok(Some(code))` only when `state` matches; Spotify's `error=` → `spotify_auth`.
pub fn parse_callback(request_line: &str, state: &str) -> Result<Option<String>> {
    let target = request_line.split_whitespace().nth(1).unwrap_or_default();
    let Some(query) = target.strip_prefix("/callback?") else {
        return Ok(None);
    };
    let get = |k: &str| {
        query
            .split('&')
            .find_map(|kv| kv.strip_prefix(k).and_then(|v| v.strip_prefix('=')))
    };
    if get("state") != Some(state) {
        return Err(AppError::new(
            Code::SpotifyAuth,
            "Spotify sign-in didn't match. Try again.",
        ));
    }
    if get("error").is_some() {
        return Err(AppError::new(
            Code::SpotifyAuth,
            "Spotify sign-in was cancelled.",
        ));
    }
    // codes are URL-safe; anything else is refused rather than decoded
    match get("code") {
        Some(c)
            if !c.is_empty()
                && c.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.~".contains(&b)) =>
        {
            Ok(Some(c.to_string()))
        }
        _ => Err(AppError::new(
            Code::SpotifyAuth,
            "Spotify sign-in came back without a code.",
        )),
    }
}

const DONE_PAGE: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!doctype html><title>Runout</title><p>Runout is connected to Spotify. You can close this tab.</p>";
const FAIL_PAGE: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!doctype html><title>Runout</title><p>Runout couldn't connect to Spotify. Go back to Runout and try again.</p>";
const NOT_FOUND: &str = "HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n";
/// A browser request line is far shorter; anything longer is refused.
const MAX_LINE: usize = 8192;
/// One connection gets this long to send its request line, in total (not per read).
const LINE_WAIT: Duration = Duration::from_secs(5);

/// The request line, read within `deadline` and [`MAX_LINE`]; `None` = slow, oversized or closed.
fn read_request_line(s: &std::net::TcpStream, deadline: Instant) -> Option<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 512];
    while buf.len() < MAX_LINE {
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())?;
        s.set_read_timeout(Some(left)).ok()?;
        let n = std::io::Read::read(&mut &*s, &mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.iter().position(|b| *b == b'\n') {
            return String::from_utf8(buf[..i].to_vec()).ok();
        }
    }
    None
}

/// Wait (blocking) for `/callback` on `listener` until [`LOGIN_WAIT`]. Returns the code and the
/// browser's connection, still unanswered: success is only shown once the exchange worked
/// ([`answer`]). A failed callback is answered with the failure page here.
fn wait_for_code(listener: TcpListener, state: String) -> Result<(String, TcpStream)> {
    wait_until(listener, state, Instant::now() + LOGIN_WAIT)
}

/// Tell the browser tab how the sign-in ended.
fn answer(mut s: TcpStream, ok: bool) {
    let _ = s.write_all(if ok { DONE_PAGE } else { FAIL_PAGE }.as_bytes());
}

fn wait_until(
    listener: TcpListener,
    state: String,
    deadline: Instant,
) -> Result<(String, TcpStream)> {
    listener.set_nonblocking(true)?;
    while Instant::now() < deadline {
        let (mut s, _) = match listener.accept() {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        s.set_nonblocking(false)?;
        let Some(line) = read_request_line(&s, deadline.min(Instant::now() + LINE_WAIT)) else {
            continue;
        };
        match parse_callback(&line, &state) {
            Ok(None) => {
                let _ = s.write_all(NOT_FOUND.as_bytes());
            }
            Ok(Some(code)) => return Ok((code, s)),
            Err(e) => {
                answer(s, false);
                return Err(e);
            }
        }
    }
    Err(AppError::new(
        Code::SpotifyAuth,
        "Spotify sign-in timed out. Try again.",
    ))
}

/// Open the system browser on an allow-listed URL (the browser fetches it, not Rust).
fn open_browser(url: &str) -> Result<()> {
    crate::net::check(url)?;
    #[cfg(windows)]
    {
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()?;
    }
    Ok(())
}

/// Full sign-in: PKCE, loopback port, browser, callback, exchange, store tokens.
pub async fn connect(sp: &Spotify) -> Result<()> {
    let client_id = sp.client_id()?;
    let verifier = random(64);
    let state = random_state();
    // T3: any 127.0.0.1:<port>/callback is accepted (KB spotify.md); `localhost` is not.
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let redirect = [
        "http://127.0.0.1:",
        &listener.local_addr()?.port().to_string(),
        "/callback",
    ]
    .concat();
    let query = form(&[
        ("client_id", &client_id),
        ("response_type", "code"),
        ("redirect_uri", &redirect),
        ("code_challenge_method", "S256"),
        ("code_challenge", &challenge(&verifier)),
        ("scope", SCOPES),
        ("state", &state),
    ]);
    open_browser(&[AUTHORIZE, "?", &query].concat())?;
    let (code, browser) =
        tauri::async_runtime::spawn_blocking(move || wait_for_code(listener, state))
            .await
            .map_err(|_| AppError::internal("Sign-in stopped."))??;
    let r = exchange(sp, &client_id, &code, &redirect, &verifier).await;
    answer(browser, r.is_ok());
    r
}

/// `authorization_code` grant → tokens stored; `renew_by` = now + 6 months.
pub async fn exchange(
    sp: &Spotify,
    client_id: &str,
    code: &str,
    redirect: &str,
    verifier: &str,
) -> Result<()> {
    let pairs = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect),
        ("client_id", client_id),
        ("code_verifier", verifier),
    ];
    let Grant::Ok(r) = token_call(sp, &pairs).await? else {
        return Err(AppError::new(
            Code::SpotifyAuth,
            "Spotify sign-in expired. Try again.",
        ));
    };
    let refresh = r.refresh_token.ok_or_else(|| {
        AppError::new(
            Code::SpotifyAuth,
            "Spotify didn't grant offline access. Try again.",
        )
    })?;
    let now = chrono::Utc::now();
    let renew = now
        .checked_add_months(chrono::Months::new(6))
        .unwrap_or(now);
    sp.set_tokens(Some(Tokens {
        access: r.access_token,
        refresh,
        expires_at: expires_at(r.expires_in),
        renew_by: renew.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    }))
}

/// `GET /me` → `me` cache entry (display name only; source A stays in the cache, TSD §4.2).
pub async fn fetch_me(sp: &Spotify) -> Result<()> {
    let me: serde_json::Value = sp.get_json("/me").await?;
    let v = serde_json::json!({ "display_name": me["display_name"] });
    sp.with_cache(|c| super::cache::put(c, super::cache::ME, "me", &v, chrono::Utc::now()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spotify::State;
    use crate::spotify::testing::*;

    #[test]
    fn pkce_challenge_matches_rfc7636_appendix_b() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let v = random(64);
        assert_eq!(v.len(), 64);
        assert!(
            v.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
        );
        assert_ne!(v, random(64));
        // state must survive the round trip unencoded
        for _ in 0..200 {
            assert!(random_state().bytes().all(|b| b.is_ascii_alphanumeric()));
        }
    }

    #[test]
    fn callback_needs_matching_state_and_a_code() {
        let ok = parse_callback("GET /callback?code=AbC-12_x&state=S1 HTTP/1.1", "S1").unwrap();
        assert_eq!(ok.as_deref(), Some("AbC-12_x"));
        assert_eq!(
            parse_callback("GET /favicon.ico HTTP/1.1", "S1").unwrap(),
            None
        );
        for bad in [
            "GET /callback?code=AbC&state=S2 HTTP/1.1",
            "GET /callback?code=AbC HTTP/1.1",
            "GET /callback?error=access_denied&state=S1 HTTP/1.1",
            "GET /callback?state=S1 HTTP/1.1",
            "GET /callback?code=a%3Cb&state=S1 HTTP/1.1",
        ] {
            assert_eq!(
                parse_callback(bad, "S1").unwrap_err().code,
                Code::SpotifyAuth,
                "{bad}"
            );
        }
    }

    #[test]
    fn loopback_listener_answers_and_returns_the_code() {
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        let t = std::thread::spawn(move || wait_for_code(l, "st".into()));
        let open = |line: &str| {
            let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.write_all(format!("{line}\r\nHost: x\r\n\r\n").as_bytes())
                .unwrap();
            s
        };
        let read = |mut s: std::net::TcpStream| {
            let mut out = String::new();
            let _ = std::io::Read::read_to_string(&mut s, &mut out);
            out
        };
        assert!(read(open("GET /favicon.ico HTTP/1.1")).starts_with("HTTP/1.1 404"));
        let tab = open("GET /callback?code=c0de&state=st HTTP/1.1");
        let (code, browser) = t.join().unwrap().unwrap();
        assert_eq!(code, "c0de");
        // the page waits for the exchange: a failed one never says "connected"
        answer(browser, false);
        let out = read(tab);
        assert!(out.contains("couldn't connect") && !out.contains("connected to Spotify"));
    }

    #[test]
    fn token_store_round_trip_with_a_test_target() {
        let target = format!("Runout/test-token-store-{}", std::process::id());
        assert_ne!(target, crate::cleanup::CREDENTIAL);
        let t = Tokens::synthetic(3600);
        store_tokens(&target, &t).unwrap();
        assert!(load_tokens(&target).unwrap() == Some(t.clone()));
        let mut t2 = t.clone();
        t2.refresh = "rotated".into();
        store_tokens(&target, &t2).unwrap(); // overwrite in place
        assert!(load_tokens(&target).unwrap() == Some(t2));
        crate::cleanup::delete_credential(&target).unwrap();
        assert!(load_tokens(&target).unwrap().is_none());
    }

    #[test]
    fn exchange_stores_tokens_with_renew_by() {
        let d = temp_dir("auth-exchange");
        let stub = Stub::new(&[(
            200,
            r#"{"access_token":"a1","token_type":"Bearer","expires_in":3600,"refresh_token":"r1"}"#,
        )]);
        let sp = spotify(&d, &stub);
        block(exchange(
            &sp,
            "TestClientId0000",
            "c0de",
            "http://127.0.0.1:5/callback",
            "v",
        ))
        .unwrap();
        let t = sp.tokens().unwrap();
        assert_eq!((t.access.as_str(), t.refresh.as_str()), ("a1", "r1"));
        assert!(t.renew_by > crate::db::now());
        let req = &stub.requests.lock().unwrap()[0];
        assert_eq!(req.url, TOKEN);
        let Some(Body::Form(f)) = &req.body else {
            panic!("form body")
        };
        assert!(f.contains("grant_type=authorization_code") && f.contains("code_verifier=v"));
    }

    #[test]
    fn expired_token_refreshes_and_stores_the_rotated_refresh_token() {
        let d = temp_dir("auth-rotate");
        let stub = Stub::new(&[
            (
                200,
                r#"{"access_token":"a2","expires_in":3600,"refresh_token":"r2"}"#,
            ),
            (200, r#"{"id":"me"}"#),
        ]);
        let sp = spotify(&d, &stub);
        sp.set_tokens(Some(Tokens::synthetic(-10))).unwrap();
        let v: serde_json::Value = block(sp.get_json("/me")).unwrap();
        assert_eq!(v["id"], "me");
        let t = sp.tokens().unwrap();
        assert_eq!(
            (t.access.as_str(), t.refresh.as_str()),
            ("a2", "r2"),
            "rotated token kept"
        );
        assert_eq!(
            t.renew_by,
            Tokens::synthetic(0).renew_by,
            "refresh doesn't extend renew_by"
        );
        // no refresh_token in the answer → the old one stays
        stub.push(200, r#"{"access_token":"a3","expires_in":3600}"#);
        stub.push(200, "{}");
        sp.set_tokens(Some(Tokens {
            expires_at: 0,
            ..sp.tokens().unwrap()
        }))
        .unwrap();
        block(sp.get_json::<serde_json::Value>("/me")).unwrap();
        assert_eq!(sp.tokens().unwrap().refresh, "r2");
    }

    #[test]
    fn a_401_refreshes_once_then_retries() {
        let d = temp_dir("auth-401");
        let stub = Stub::new(&[
            (401, "{}"),
            (200, r#"{"access_token":"a2","expires_in":3600}"#),
            (200, r#"{"ok":1}"#),
        ]);
        let sp = connected(&d, &stub);
        let v: serde_json::Value = block(sp.get_json("/me")).unwrap();
        assert_eq!(v["ok"], 1);
        let reqs = stub.requests.lock().unwrap();
        assert_eq!(reqs[2].bearer.as_deref(), Some("a2"));
    }

    #[test]
    fn invalid_grant_deletes_tokens_and_asks_to_reconnect() {
        let d = temp_dir("auth-invalid");
        let stub = Stub::new(&[(
            400,
            r#"{"error":"invalid_grant","error_description":"Refresh token revoked"}"#,
        )]);
        let sp = spotify(&d, &stub);
        sp.set_tokens(Some(Tokens::synthetic(-10))).unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let s2 = seen.clone();
        sp.on_status(move |s| s2.lock().unwrap().push(s.state));
        let e = block(sp.get_json::<serde_json::Value>("/me")).unwrap_err();
        assert_eq!(e.code, Code::SpotifyAuth);
        assert!(sp.tokens().is_none());
        assert_eq!(sp.status_now().unwrap().state, State::Reconnect);
        assert_eq!(
            *seen.lock().unwrap(),
            [State::Reconnect],
            "spotify-status emitted"
        );
        assert_eq!(sp.require_connected().unwrap_err().code, Code::SpotifyAuth);
        assert!(
            !d.join(crate::cleanup::CACHE_DB).exists(),
            "a revoked grant leaves no cache"
        );
    }

    /// Two callers with an expired token: one refresh, both get the rotated token.
    #[test]
    fn concurrent_callers_share_one_refresh() {
        let d = temp_dir("auth-single-flight");
        let mut stub = Stub::new(&[
            (
                200,
                r#"{"access_token":"a2","expires_in":3600,"refresh_token":"r2"}"#,
            ),
            (200, r#"{"n":1}"#),
            (200, r#"{"n":2}"#),
        ]);
        stub.delay_ms = 150;
        let sp = std::sync::Arc::new(spotify(&d, &stub));
        sp.set_tokens(Some(Tokens::synthetic(-10))).unwrap();
        let run = |sp: std::sync::Arc<Spotify>| {
            tauri::async_runtime::spawn(
                async move { sp.get_json::<serde_json::Value>("/me").await },
            )
        };
        let (a, b) = (run(sp.clone()), run(sp.clone()));
        block(a).unwrap().unwrap();
        block(b).unwrap().unwrap();
        let reqs = stub.requests.lock().unwrap();
        assert_eq!(reqs.iter().filter(|r| r.url == TOKEN).count(), 1);
        assert!(
            reqs.iter()
                .filter(|r| r.url != TOKEN)
                .all(|r| r.bearer.as_deref() == Some("a2"))
        );
        assert_eq!(sp.tokens().unwrap().refresh, "r2");
    }

    /// `invalid_grant` for a refresh token that was rotated meanwhile must not sign the user out.
    #[test]
    fn stale_invalid_grant_keeps_newer_tokens() {
        let d = temp_dir("auth-stale-grant");
        let me: std::sync::Arc<std::sync::OnceLock<std::sync::Arc<Spotify>>> = Default::default();
        let m2 = me.clone();
        let transport: crate::spotify::Transport = std::sync::Arc::new(move |_req: HttpReq| {
            let m2 = m2.clone();
            Box::pin(async move {
                // someone else stored a rotated token while this refresh was out
                let sp = m2.get().unwrap();
                sp.set_tokens(Some(Tokens {
                    refresh: "rotated".into(),
                    ..Tokens::synthetic(3600)
                }))?;
                Ok(crate::spotify::HttpResp {
                    status: 400,
                    retry_after: None,
                    body: r#"{"error":"invalid_grant"}"#.into(),
                })
            })
        });
        let sp = std::sync::Arc::new(Spotify::with(
            None,
            Some("TestClientId0000".into()),
            d.join(crate::cleanup::CACHE_DB),
            transport,
        ));
        let _ = me.set(sp.clone());
        sp.set_tokens(Some(Tokens::synthetic(-10))).unwrap();
        assert!(block(access_token(&sp, None)).is_err());
        assert_eq!(sp.tokens().unwrap().refresh, "rotated");
        assert_eq!(sp.status_now().unwrap().state, State::Connected);
    }

    #[test]
    fn token_endpoint_busy_is_spotify_busy() {
        let d = temp_dir("auth-busy");
        let stub = Stub::new(&[]);
        stub.push_resp(crate::spotify::HttpResp {
            status: 429,
            retry_after: Some(30),
            body: String::new(),
        });
        stub.push(503, "");
        let sp = spotify(&d, &stub);
        sp.set_tokens(Some(Tokens::synthetic(-10))).unwrap();
        let e = block(access_token(&sp, None)).unwrap_err();
        assert_eq!((e.code, e.retry_after_s), (Code::SpotifyBusy, Some(30)));
        let e = block(access_token(&sp, None)).unwrap_err();
        assert_eq!(e.code, Code::SpotifyBusy);
        assert!(sp.tokens().is_some(), "busy is not a sign-out");
    }

    #[test]
    fn failed_callback_shows_a_failure_page_and_slow_clients_cant_hold_the_wait() {
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        let t = std::thread::spawn(move || wait_for_code(l, "st".into()));
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(b"GET /callback?error=access_denied&state=st HTTP/1.1\r\n\r\n")
            .unwrap();
        let mut out = String::new();
        let _ = std::io::Read::read_to_string(&mut s, &mut out);
        assert!(out.contains("couldn't connect") && !out.contains("close this tab"));
        assert_eq!(t.join().unwrap().unwrap_err().code, Code::SpotifyAuth);

        // a client dripping bytes with no newline: the overall deadline still ends the wait
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        let start = Instant::now();
        let t = std::thread::spawn(move || {
            wait_until(l, "st".into(), Instant::now() + Duration::from_secs(1))
        });
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        for _ in 0..8 {
            let _ = s.write_all(b"G");
            std::thread::sleep(Duration::from_millis(300));
        }
        assert!(t.join().unwrap().is_err());
        assert!(start.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn missing_client_id_is_spotify_auth() {
        let d = temp_dir("auth-noid");
        let stub = Stub::new(&[]);
        let sp = Spotify::with(None, Some(String::new()), d.join("c.db"), stub.transport());
        let e = sp.client_id().unwrap_err();
        assert_eq!(e.code, Code::SpotifyAuth);
        assert!(e.message.contains(crate::spotify::CLIENT_ID_ENV));
    }

    /// G-15: production writes exactly the target `--uninstall-cleanup` deletes.
    #[test]
    fn production_credential_target_is_the_one_cleanup_deletes() {
        let d = temp_dir("auth-target");
        let sp = Spotify::new(&d);
        assert_eq!(sp.credential.as_deref(), Some("Runout/spotify"));
        assert_eq!(crate::cleanup::CREDENTIAL, "Runout/spotify");
        assert_eq!(sp.cache_path, d.join("spotify_cache.db"));
    }
}

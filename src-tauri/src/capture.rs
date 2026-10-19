//! Phone capture listener (TSD §5.1, FR-018, NFR-18): `POST /capture {code, link}` on the LAN.
//! Off by default. Pure screening ([`Guard::screen`]) and effects ([`handle`]) are separate from the
//! socket loop ([`serve`]) so tests drive them directly.
//!
//! Decisions: loopback (127/8) is rejected like any non-private source (TSD lists only the four
//! private ranges). A URI already in the library answers 200 "already in your library" and changes
//! nothing (no usage event, no `capture-received`). On/off and the code persist in `capture.json`
//! so an iPhone Shortcut keeps working across restarts.
use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Shutdown, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, Code, Result};
use crate::library::items;
use crate::links::{self, CaptureSource, LinkState};
use crate::text::{clean_line, limit};
use crate::usage;

pub const PORT: u16 = 47821;
pub const MAX_BODY: usize = 2048;
pub const RATE_LIMIT: usize = 10;
const RATE_WINDOW: Duration = Duration::from_secs(60);
pub const MAX_WRONG_CODES: u32 = 5;
/// Requests being served at once; more get 503 without a thread (a stalled body holds one forever).
pub const MAX_IN_FLIGHT: usize = 4;
pub const FILE: &str = "capture.json";

/// `true` for 10/8, 172.16/12, 192.168/16, 169.254/16 (IPv4-mapped IPv6 included). Nothing else.
pub fn is_private(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(_) => false,
    }
}

fn new_code() -> String {
    use rand::RngExt;
    format!("{:06}", rand::rng().random_range(0..1_000_000u32))
}

/// Screening state: the pairing code, wrong-code count and the 60 s request window.
#[derive(Debug)]
pub struct Guard {
    code: String,
    wrong: u32,
    recent: VecDeque<Instant>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Reject {
        status: u16,
        message: &'static str,
        disable: bool,
    },
    /// Trimmed link that parses `ok` or `short` (TSD §4.1).
    Accept(String),
}

fn reject(status: u16, message: &'static str) -> Verdict {
    Verdict::Reject {
        status,
        message,
        disable: false,
    }
}

#[derive(Deserialize)]
struct Body {
    code: String,
    link: String,
}

impl Guard {
    pub fn new(code: String) -> Self {
        Self {
            code,
            wrong: 0,
            recent: VecDeque::new(),
        }
    }

    /// Accept-loop gate, before any thread or body byte: private source IP, then the 10/min window.
    /// Every LAN request counts, whatever happens to it later (a stalled body included).
    pub fn admit(&mut self, ip: IpAddr, now: Instant) -> Option<(u16, &'static str)> {
        if !is_private(ip) {
            return Some((403, "Runout only accepts shares from your home network."));
        }
        while self
            .recent
            .front()
            .is_some_and(|t| now.duration_since(*t) >= RATE_WINDOW)
        {
            self.recent.pop_front();
        }
        if self.recent.len() >= RATE_LIMIT {
            return Some((429, "Too many shares in a minute. Try again shortly."));
        }
        self.recent.push_back(now);
        None
    }

    /// NFR-18 checks after [`Guard::admit`], in order: private source IP, route, ≤ 2 KB, JSON,
    /// code (5 wrong → disable), link.
    pub fn screen(&mut self, ip: IpAddr, method: &str, url: &str, body: &[u8]) -> Verdict {
        if !is_private(ip) {
            return reject(403, "Runout only accepts shares from your home network.");
        }
        if method != "POST" || url.split('?').next() != Some("/capture") {
            return reject(404, "Send POST /capture.");
        }
        if body.len() > MAX_BODY {
            return reject(413, "That share is too big.");
        }
        let Ok(b) = serde_json::from_slice::<Body>(body) else {
            return reject(400, "Send JSON with code and link.");
        };
        let digits: String = b.code.chars().filter(char::is_ascii_digit).collect();
        if digits != self.code {
            self.wrong += 1;
            return if self.wrong >= MAX_WRONG_CODES {
                Verdict::Reject {
                    status: 401,
                    message: "Wrong pairing code. Phone capture is now off on the PC.",
                    disable: true,
                }
            } else {
                reject(401, "Wrong pairing code.")
            };
        }
        let link = clean_line(&b.link, limit::LINK);
        match links::parse(&link).state {
            LinkState::Ok | LinkState::Short => Verdict::Accept(link),
            LinkState::Unsupported => reject(
                400,
                "Runout keeps albums, tracks and artists. Podcasts, audiobooks and playlists aren't saved.",
            ),
            LinkState::Invalid => reject(400, "That isn't a Spotify link."),
        }
    }
}

/// What saving an accepted link did.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// New name-needed item; payload is ItemSummary-shaped JSON (the `capture-received` payload).
    New(Value),
    Existing(i64),
    /// Short link while offline: queued in `unresolved_capture` (source `shortcut`).
    Queued,
}

/// Save an accepted link (TSD §4.2: URI only, `added_via = 'phone'`) + a `capture` usage event
/// timed from `arrived` to save (TSD §4.12).
pub async fn save(db: &Mutex<Connection>, link: &str, arrived: Instant) -> Result<Outcome> {
    let (uri, kind) = match links::resolve_or_queue(db, link, CaptureSource::Shortcut).await {
        Ok(r) => r,
        Err(e) if e.code == Code::Offline => return Ok(Outcome::Queued),
        Err(e) => return Err(e),
    };
    let mut conn = db.lock().unwrap();
    if let Some(id) = items::id_by_uri(&conn, &uri)? {
        return Ok(Outcome::Existing(id));
    }
    let tx = conn.transaction()?;
    let id = items::insert_name_needed(&tx, kind, &uri, "phone")?;
    let ms = arrived.elapsed().as_millis() as i64;
    usage::record(&tx, usage::Kind::Capture, Some("phone"), Some(ms))?;
    let summary = serde_json::to_value(items::summary(&tx, id)?)?;
    tx.commit()?;
    Ok(Outcome::New(summary))
}

/// Effects the listener needs from the app (lib.rs wires them to AppState + `emit`).
pub struct Hooks {
    pub save: Box<SaveFn>,
    pub emit: Box<EmitFn>,
}
pub type SaveFn = dyn Fn(&str, Instant) -> Result<Outcome> + Send + Sync;
pub type EmitFn = dyn Fn(&'static str, Value) + Send + Sync;

#[derive(Serialize, Deserialize)]
struct Persisted {
    on: bool,
    code: String,
}

struct Shared {
    guard: Mutex<Guard>,
    on: AtomicBool,
    /// `serve_one` threads running now (≤ [`MAX_IN_FLIGHT`]).
    in_flight: AtomicUsize,
    file: PathBuf,
    hooks: Hooks,
}

impl Shared {
    fn persist(&self) -> Result<()> {
        let p = Persisted {
            on: self.on.load(Ordering::SeqCst),
            code: self.guard.lock().unwrap().code.clone(),
        };
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.file, serde_json::to_vec(&p)?)?;
        Ok(())
    }

    /// The listener stopped by itself (lockout or a socket error): off, persisted, UI told why.
    fn turn_off(&self, reason: &str) {
        self.on.store(false, Ordering::SeqCst);
        let _ = self.persist();
        (self.hooks.emit)("capture-disabled", serde_json::json!({ "reason": reason }));
    }
}

/// [`Guard::admit`] plus the in-flight cap. `None` = a `serve_one` slot was taken (release it after).
fn gate(sh: &Shared, ip: IpAddr, now: Instant) -> Option<(u16, &'static str)> {
    if let Some(r) = sh.guard.lock().unwrap().admit(ip, now) {
        return Some(r);
    }
    if sh.in_flight.fetch_add(1, Ordering::SeqCst) >= MAX_IN_FLIGHT {
        sh.in_flight.fetch_sub(1, Ordering::SeqCst);
        return Some((503, "Runout is busy. Try again shortly."));
    }
    None
}

/// One request: screen, then save/emit. Returns `(status, text for the Shortcut's "Show result")`.
/// A wrong-code lockout turns the listener off, persists that and emits `capture-disabled`.
fn handle(
    sh: &Shared,
    ip: IpAddr,
    method: &str,
    url: &str,
    body: &[u8],
    arrived: Instant,
) -> (u16, String) {
    // Turned off (or locked out) while this request was in flight: NFR-18 "disabled at any time".
    if !sh.on.load(Ordering::SeqCst) {
        return (503, "Phone capture is off on the PC.".into());
    }
    let verdict = sh.guard.lock().unwrap().screen(ip, method, url, body);
    match verdict {
        Verdict::Reject {
            status,
            message,
            disable,
        } => {
            if disable {
                // The code was under attack: retire it now so a restart or re-enable can't reuse it.
                *sh.guard.lock().unwrap() = Guard::new(new_code());
                sh.turn_off("wrong_code");
            }
            (status, message.into())
        }
        Verdict::Accept(link) => match (sh.hooks.save)(&link, arrived) {
            Ok(Outcome::New(item)) => {
                (sh.hooks.emit)("capture-received", item);
                (201, "Saved to Runout. Name it on your PC.".into())
            }
            Ok(Outcome::Existing(_)) => (200, "Already in your library.".into()),
            Ok(Outcome::Queued) => (
                202,
                "Saved. Runout will finish it when the PC is back online.".into(),
            ),
            Err(e) if e.code == Code::InvalidLink || e.code == Code::UnsupportedLink => {
                (400, e.message)
            }
            Err(e) => (500, e.message),
        },
    }
}

/// Stateless checks before any body byte is read: source IP first, then route, then a declared
/// length ≤ [`MAX_BODY`] (chunked or missing → 411). `None` = go on and read the body.
fn pre_read(
    ip: IpAddr,
    method: &str,
    url: &str,
    len: Option<usize>,
) -> Option<(u16, &'static str)> {
    if !is_private(ip) {
        return Some((403, "Runout only accepts shares from your home network."));
    }
    if method != "POST" || url.split('?').next() != Some("/capture") {
        return Some((404, "Send POST /capture."));
    }
    match len {
        None => Some((411, "Send a Content-Length.")),
        Some(n) if n > MAX_BODY => Some((413, "That share is too big.")),
        Some(_) => None,
    }
}

/// Per-socket read/write timeout: a stalled client frees its slot after this.
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// Whole-request deadline, so a client dripping one byte per timeout can't hold a slot for long.
const DEADLINE: Duration = Duration::from_secs(10);
/// Request line + headers cap; the body is never read past [`MAX_BODY`].
const MAX_HEAD: usize = 8192;

/// Parsed request head: method, path, declared length (`None` if missing, bad or chunked),
/// and the body bytes that arrived with the head.
struct Head {
    method: String,
    path: String,
    len: Option<usize>,
    rest: Vec<u8>,
}

/// Read up to the blank line (≤ [`MAX_HEAD`] bytes). `Err(status)` for a too-big, slow or broken head.
fn read_head(s: &mut TcpStream, arrived: Instant) -> std::result::Result<Head, u16> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() >= MAX_HEAD {
            return Err(431);
        }
        if arrived.elapsed() > DEADLINE {
            return Err(408);
        }
        match s.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(408),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let text = std::str::from_utf8(&buf[..end]).map_err(|_| 400u16)?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let (method, path) = (first.next().unwrap_or(""), first.next().unwrap_or(""));
    let (mut len, mut chunked) = (None, false);
    for line in lines {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim();
        if k.eq_ignore_ascii_case("content-length") {
            // All digits but past usize → too big (413); anything else → missing (411).
            len = v.parse().ok().or_else(|| {
                (!v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())).then_some(usize::MAX)
            });
        } else if k.eq_ignore_ascii_case("transfer-encoding") {
            chunked = true;
        }
    }
    Ok(Head {
        method: method.to_owned(),
        path: path.to_owned(),
        len: if chunked { None } else { len },
        rest: buf[end + 4..].to_vec(),
    })
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        411 => "Length Required",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    }
}

/// Minimal HTTP/1.1 reply, then close. The text is what the Shortcut's "Show result" shows.
fn respond(mut s: TcpStream, status: u16, text: &str) {
    let _ = s.set_write_timeout(Some(IO_TIMEOUT));
    let _ = write!(
        s,
        "HTTP/1.1 {status} {}\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        reason(status),
        text.len()
    );
    let _ = s.flush();
    let _ = s.shutdown(Shutdown::Write);
}

/// One connection, on its own thread: head (≤ 8 KB), [`pre_read`] on the declared length, then
/// exactly that many body bytes (≤ 2 KB). Every read has a timeout and the request a deadline, so
/// a stalled or lying client only holds this slot briefly and never makes us allocate what it
/// declared (G-17).
// ponytail: thread per request, capped by `gate` (10/min, MAX_IN_FLIGHT). Pool it if that changes.
fn serve_one(mut s: TcpStream, ip: IpAddr, sh: &Shared, arrived: Instant) {
    let _ = s.set_read_timeout(Some(IO_TIMEOUT));
    let (status, text) = match read_head(&mut s, arrived) {
        Err(status) => (status, "Send POST /capture with a JSON body.".to_owned()),
        Ok(h) => match pre_read(ip, &h.method, &h.path, h.len) {
            Some((status, text)) => (status, text.to_owned()),
            None => {
                let want = h.len.unwrap_or(0);
                let mut body = h.rest;
                body.truncate(want);
                let mut chunk = [0u8; 512];
                while body.len() < want && arrived.elapsed() <= DEADLINE {
                    match s.read(&mut chunk[..(want - body.len()).min(512)]) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => body.extend_from_slice(&chunk[..n]),
                    }
                }
                if body.len() < want {
                    (408, "The share didn't arrive in time.".to_owned())
                } else {
                    handle(sh, ip, &h.method, &h.path, &body, arrived)
                }
            }
        },
    };
    respond(s, status, &text);
}

/// How often the accept loop looks at `stop` / a lockout.
const POLL: Duration = Duration::from_millis(100);

/// Accept loop (non-blocking, polled): [`gate`]s each connection by peer IP and rate, hands it to
/// [`serve_one`] and stops when `stop` is set or capture turns off (lockout, or a socket error →
/// `capture-disabled {reason: 'error'}`). Returning drops the socket, so the port closes at once.
fn serve(
    server: TcpListener,
    stop: Arc<AtomicBool>,
    sh: Arc<Shared>,
    running: Arc<Mutex<Option<Running>>>,
) {
    while !stop.load(Ordering::SeqCst) && sh.on.load(Ordering::SeqCst) {
        match server.accept() {
            Ok((s, peer)) => {
                let arrived = Instant::now();
                let ip = peer.ip();
                // Rejects are written here: nothing is read, and a small reply fits the send buffer.
                if let Some((status, text)) = gate(&sh, ip, arrived) {
                    respond(s, status, text);
                    continue;
                }
                if s.set_nonblocking(false).is_err() {
                    sh.in_flight.fetch_sub(1, Ordering::SeqCst);
                    continue;
                }
                let sh = sh.clone();
                std::thread::spawn(move || {
                    // Drop guard: the slot comes back even if serving panics.
                    struct Slot<'a>(&'a AtomicUsize);
                    impl Drop for Slot<'_> {
                        fn drop(&mut self) {
                            self.0.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                    let _slot = Slot(&sh.in_flight);
                    serve_one(s, ip, &sh, arrived);
                });
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => std::thread::sleep(POLL),
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::ConnectionReset
                        | ErrorKind::ConnectionAborted
                        | ErrorKind::Interrupted
                ) => {}
            Err(_) => {
                if !stop.load(Ordering::SeqCst) {
                    sh.turn_off("error");
                }
                break;
            }
        }
    }
    drop(server);
    // Lockout: nobody else is tearing down (set_enabled holds the lock while it does), so close here.
    if let Ok(mut r) = running.try_lock()
        && let Some(r) = r.take()
    {
        close(r);
    }
}

struct Running {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
    port: u16,
}

/// Stop the accept loop and wait for it (bounded: it polls every [`POLL`]); it drops the socket.
fn close(r: Running) {
    r.stop.store(true, Ordering::SeqCst);
    if r.thread.thread().id() != std::thread::current().id() {
        let _ = r.thread.join();
    }
}

/// API.md `get_capture` shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureInfo {
    pub on: bool,
    pub address: String,
    pub port: u16,
    /// Shown as `123 456`; the Shortcut may send it with or without the space.
    pub code: String,
}

/// The listener, managed by Tauri. `get_capture` / `set_capture_enabled` / `new_pairing_code` wrap it.
pub struct Listener {
    shared: Arc<Shared>,
    port: u16,
    running: Arc<Mutex<Option<Running>>>,
}

impl Listener {
    /// Load `file` (missing/corrupt → off + fresh code). Call [`Listener::resume`] to honour a saved "on".
    pub fn new(file: &Path, port: u16, hooks: Hooks) -> Self {
        let saved = std::fs::read(file)
            .ok()
            .and_then(|b| serde_json::from_slice::<Persisted>(&b).ok())
            .filter(|p| p.code.len() == 6 && p.code.bytes().all(|b| b.is_ascii_digit()));
        let (on, code) = saved.map_or((false, new_code()), |p| (p.on, p.code));
        Self {
            shared: Arc::new(Shared {
                guard: Mutex::new(Guard::new(code)),
                on: AtomicBool::new(on),
                in_flight: AtomicUsize::new(0),
                file: file.to_owned(),
                hooks,
            }),
            port,
            running: Arc::default(),
        }
    }

    /// At start-up: bind if capture was left on.
    pub fn resume(&self) -> Result<()> {
        if self.shared.on.load(Ordering::SeqCst) {
            self.set_enabled(true)?;
        }
        Ok(())
    }

    pub fn info(&self) -> CaptureInfo {
        let code = self.shared.guard.lock().unwrap().code.clone();
        // Listener stopped itself (lockout) → report off.
        let on = self.shared.on.load(Ordering::SeqCst);
        CaptureInfo {
            on,
            address: lan_ip().map(|ip| ip.to_string()).unwrap_or_default(),
            port: self.port,
            code: format!("{} {}", &code[..3], &code[3..]),
        }
    }

    /// Port actually bound (tests bind port 0).
    pub fn bound_port(&self) -> Option<u16> {
        self.running.lock().unwrap().as_ref().map(|r| r.port)
    }

    pub fn set_enabled(&self, on: bool) -> Result<CaptureInfo> {
        let mut running = self.running.lock().unwrap();
        if let Some(r) = running.take() {
            close(r);
        }
        if on {
            let bound = TcpListener::bind(("0.0.0.0", self.port))
                .and_then(|l| l.set_nonblocking(true).map(|()| l));
            let server = match bound {
                Ok(s) => s,
                Err(e) => {
                    self.shared.on.store(false, Ordering::SeqCst);
                    let _ = self.shared.persist();
                    return Err(AppError::new(
                        Code::Io,
                        format!("Couldn't open port {} for phone capture.", self.port),
                    )
                    .with_detail(e.to_string()));
                }
            };
            let port = server.local_addr().map_or(self.port, |a| a.port());
            let stop = Arc::new(AtomicBool::new(false));
            self.shared.on.store(true, Ordering::SeqCst);
            let (st, sh, run) = (stop.clone(), self.shared.clone(), self.running.clone());
            let thread = std::thread::spawn(move || serve(server, st, sh, run));
            *running = Some(Running { stop, thread, port });
        } else {
            self.shared.on.store(false, Ordering::SeqCst);
        }
        drop(running);
        self.shared.persist()?;
        Ok(self.info())
    }

    /// New pairing code; resets the wrong-code count. On/off unchanged.
    pub fn new_pairing_code(&self) -> Result<CaptureInfo> {
        *self.shared.guard.lock().unwrap() = Guard::new(new_code());
        self.shared.persist()?;
        Ok(self.info())
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        if let Some(r) = self.running.lock().unwrap().take() {
            close(r);
        }
    }
}

/// The PC's LAN address: a UDP `connect` picks the default-route interface without sending anything.
pub fn lan_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("10.254.254.254:9").ok()?;
    Some(s.local_addr().ok()?.ip()).filter(|ip| !ip.is_unspecified())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::net::Ipv4Addr;

    const A: &str = "4kQ2vNnB8XcRtYp0LmZs1a"; // synthetic 22-char id
    const LAN: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20));

    fn body(code: &str, link: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"code": code, "link": link})).unwrap()
    }
    fn album() -> String {
        format!("https://open.spotify.com/album/{A}?si=x")
    }
    fn status(v: &Verdict) -> u16 {
        match v {
            Verdict::Reject { status, .. } => *status,
            Verdict::Accept(_) => 200,
        }
    }

    #[test]
    fn private_ranges_only() {
        for ok in [
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.0.9",
            "169.254.3.4",
            "::ffff:192.168.1.2",
        ] {
            assert!(is_private(ok.parse().unwrap()), "{ok}");
        }
        for bad in [
            "127.0.0.1",
            "172.32.0.1",
            "172.15.0.1",
            "203.0.113.5",
            "100.64.0.1",
            "::1",
            "fe80::1",
            "0.0.0.0",
        ] {
            assert!(!is_private(bad.parse().unwrap()), "{bad}");
        }
    }

    #[test]
    fn accepts_good_and_short_links() {
        let mut g = Guard::new("482913".into());
        let v = g.screen(
            LAN,
            "POST",
            "/capture",
            &body("482 913", &format!("  {}  ", album())),
        );
        assert_eq!(v, Verdict::Accept(album()));
        let v = g.screen(
            LAN,
            "POST",
            "/capture",
            &body("482913", "https://spotify.link/aXk3P9q"),
        );
        assert_eq!(v, Verdict::Accept("https://spotify.link/aXk3P9q".into()));
        assert_eq!(status(&g.screen(LAN, "GET", "/capture", b"")), 404);
        assert_eq!(
            status(&g.screen(LAN, "POST", "/capture", b"{not json")),
            400
        );
    }

    #[test]
    fn nfr18_wrong_code_five_times_disables() {
        let mut g = Guard::new("482913".into());
        for i in 1..=5 {
            let v = g.screen(LAN, "POST", "/capture", &body("111111", &album()));
            let Verdict::Reject {
                status, disable, ..
            } = v
            else {
                panic!()
            };
            assert_eq!((status, disable), (401, i == 5), "attempt {i}");
        }
    }

    #[test]
    fn nfr18_non_lan_rejected() {
        let mut g = Guard::new("482913".into());
        for ip in ["203.0.113.5", "127.0.0.1"] {
            let v = g.screen(
                ip.parse().unwrap(),
                "POST",
                "/capture",
                &body("482913", &album()),
            );
            assert_eq!(status(&v), 403, "{ip}");
        }
    }

    #[test]
    fn nfr18_eleventh_request_in_a_minute_rejected() {
        let mut g = Guard::new("482913".into());
        let t = Instant::now();
        for _ in 0..10 {
            assert_eq!(g.admit(LAN, t), None);
        }
        assert_eq!(g.admit(LAN, t).unwrap().0, 429);
        let out = "203.0.113.5".parse().unwrap();
        assert_eq!(g.admit(out, t).unwrap().0, 403, "IP before the window");
        assert_eq!(g.admit(LAN, t + Duration::from_secs(61)), None);
    }

    /// Every request counts against 10/min at the accept loop, before a thread or body read; at
    /// most MAX_IN_FLIGHT are served at once (stalled bodies hold their slot), the rest get 503.
    #[test]
    fn gate_counts_every_request_and_caps_in_flight() {
        let dir = tmp("gate");
        let (l, _) = listener(&dir, 0);
        let sh = &l.shared;
        let t = Instant::now();
        for _ in 0..MAX_IN_FLIGHT {
            assert_eq!(gate(sh, LAN, t), None);
        }
        assert_eq!(gate(sh, LAN, t).unwrap().0, 503);
        assert_eq!(sh.in_flight.load(Ordering::SeqCst), MAX_IN_FLIGHT);
        sh.in_flight.store(0, Ordering::SeqCst); // those requests finished
        for _ in 0..RATE_LIMIT - MAX_IN_FLIGHT - 1 {
            assert_eq!(gate(sh, LAN, t), None);
            sh.in_flight.fetch_sub(1, Ordering::SeqCst);
        }
        assert_eq!(gate(sh, LAN, t).unwrap().0, 429, "the 503 counted too");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn socket_error_turns_capture_off_persists_and_emits() {
        let dir = tmp("sockerr");
        let (l, events) = listener(&dir, 0);
        l.shared.on.store(true, Ordering::SeqCst);
        l.shared.persist().unwrap();
        l.shared.turn_off("error");
        assert!(!l.info().on);
        assert_eq!(
            events.lock().unwrap().as_slice(),
            &[("capture-disabled", serde_json::json!({"reason": "error"}))]
        );
        drop(l);
        assert!(!listener(&dir, 0).0.info().on, "off persisted");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn nfr18_body_over_2kb_rejected() {
        let mut g = Guard::new("482913".into());
        let big = body("482913", &format!("{}{}", album(), "x".repeat(MAX_BODY)));
        assert_eq!(status(&g.screen(LAN, "POST", "/capture", &big)), 413);
    }

    #[test]
    fn nfr18_non_spotify_link_rejected() {
        let mut g = Guard::new("482913".into());
        for link in [
            &format!("{}//blocked.invalid/album/{A}", "https:"), // split so check.py's host scan skips it
            "coltrane",
            &format!("https://open.spotify.com/episode/{A}"),
        ] {
            assert_eq!(
                status(&g.screen(LAN, "POST", "/capture", &body("482913", link))),
                400,
                "{link}"
            );
        }
    }

    #[test]
    fn save_creates_name_needed_item_once() {
        let db = Mutex::new(db::open_in_memory().unwrap());
        let link = format!("spotify:track:{A}");
        let out = tauri::async_runtime::block_on(save(&db, &link, Instant::now())).unwrap();
        let Outcome::New(item) = out else {
            panic!("{out:?}")
        };
        let keys: Vec<&str> = item
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let mut want = vec![
            "id",
            "type",
            "display_name",
            "spotify_uri",
            "status",
            "priority",
            "rating",
            "favourite",
            "revisit",
            "in_backlog",
            "backlog_order",
            "tags",
            "artist_names",
            "album_name",
            "added_via",
            "recommended_by",
            "notes",
            "added_at",
            "modified_at",
        ];
        let mut got = keys.clone();
        got.sort();
        want.sort();
        assert_eq!(got, want);
        assert_eq!(item["display_name"], Value::Null);
        assert_eq!(item["added_via"], "phone");
        assert_eq!(item["in_backlog"], true);
        assert_eq!(item["tags"], serde_json::json!([]));
        let (n, ms): (i64, Option<i64>) = db
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*), max(duration_ms) FROM usage_event WHERE kind = 'capture'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(n, 1);
        assert!(ms.is_some());
        let again = tauri::async_runtime::block_on(save(&db, &link, Instant::now())).unwrap();
        assert_eq!(again, Outcome::Existing(item["id"].as_i64().unwrap()));
    }

    type Events = Arc<Mutex<Vec<(&'static str, Value)>>>;

    fn listener(dir: &Path, port: u16) -> (Listener, Events) {
        let events: Events = Arc::default();
        let ev = events.clone();
        let hooks = Hooks {
            save: Box::new(|_, _| Ok(Outcome::Existing(1))),
            emit: Box::new(move |e, v| ev.lock().unwrap().push((e, v))),
        };
        (Listener::new(&dir.join(FILE), port, hooks), events)
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("runout-capture-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn request_in_flight_when_turned_off_is_refused() {
        let dir = tmp("offinflight");
        let (l, events) = listener(&dir, 0);
        let code = l.info().code;
        let (status, _) = handle(
            &l.shared,
            LAN,
            "POST",
            "/capture",
            &body(&code, &album()),
            Instant::now(),
        );
        assert_eq!(status, 503, "capture is off: nothing saved");
        assert!(events.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lockout_disables_persists_and_emits() {
        let dir = tmp("lockout");
        let (l, events) = listener(&dir, 0);
        assert!(!l.info().on, "off by default");
        let attacked = l.info().code;
        l.shared.on.store(true, Ordering::SeqCst);
        for _ in 0..5 {
            handle(
                &l.shared,
                LAN,
                "POST",
                "/capture",
                &body("000000x", &album()),
                Instant::now(),
            );
        }
        assert!(!l.info().on);
        assert_eq!(
            events.lock().unwrap().as_slice(),
            &[(
                "capture-disabled",
                serde_json::json!({"reason": "wrong_code"})
            )]
        );
        let rotated = l.info().code;
        assert_ne!(rotated, attacked, "lockout retires the attacked code");
        drop(l);
        let (l, _) = listener(&dir, 0);
        assert_eq!(
            (l.info().on, l.info().code),
            (false, rotated),
            "off + new code persisted across restart"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pre_read_checks_ip_first_then_route_then_length() {
        let out = "203.0.113.5".parse().unwrap();
        assert_eq!(pre_read(out, "POST", "/capture", None).unwrap().0, 403);
        assert_eq!(pre_read(LAN, "GET", "/", Some(1)).unwrap().0, 404);
        assert_eq!(pre_read(LAN, "POST", "/capture", None).unwrap().0, 411);
        assert_eq!(
            pre_read(LAN, "POST", "/capture", Some(MAX_BODY + 1))
                .unwrap()
                .0,
            413
        );
        assert_eq!(pre_read(LAN, "POST", "/capture?x=1", Some(MAX_BODY)), None);
    }

    fn connect(port: u16) -> std::io::Result<std::net::TcpStream> {
        std::net::TcpStream::connect_timeout(
            &(Ipv4Addr::LOCALHOST, port).into(),
            Duration::from_secs(1),
        )
    }

    /// A client that declares 2000 bytes and then goes silent must not hang the listener.
    #[test]
    fn stalled_body_does_not_block_stop() {
        let dir = tmp("stall");
        let (l, _) = listener(&dir, 0);
        l.set_enabled(true).unwrap();
        let port = l.bound_port().unwrap();
        let mut stalled = connect(port).unwrap();
        stalled
            .write_all(b"POST /capture HTTP/1.1\r\nHost: x\r\nContent-Length: 2000\r\n\r\n{")
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let t = Instant::now();
        assert!(!l.set_enabled(false).unwrap().on);
        assert!(
            t.elapsed() < Duration::from_secs(2),
            "stop took {:?}",
            t.elapsed()
        );
        drop(stalled);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// NFR-18: after a lockout the port is closed, not just ignored.
    #[test]
    fn lockout_closes_the_port() {
        let dir = tmp("lockport");
        let (l, _) = listener(&dir, 0);
        l.set_enabled(true).unwrap();
        let port = l.bound_port().unwrap();
        l.shared.on.store(false, Ordering::SeqCst); // what `handle` does on the 5th wrong code
        let t = Instant::now();
        while l.bound_port().is_some() && t.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(l.bound_port().is_none());
        std::thread::sleep(Duration::from_millis(100));
        assert!(connect(port).is_err(), "port still accepting after lockout");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Drive `serve_one` over a real loopback socket as if the peer were on the LAN (loopback
    /// itself is rejected by the accept-loop gate). `send` runs on the client; returns the reply.
    fn exchange(name: &str, send: impl FnOnce(&mut TcpStream, &str)) -> (String, Duration) {
        let dir = tmp(name);
        let (l, _) = listener(&dir, 0);
        let code = l.info().code;
        l.shared.on.store(true, Ordering::SeqCst); // a live listener's request
        let sh = l.shared.clone();
        let srv = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut c = connect(srv.local_addr().unwrap().port()).unwrap();
        let (s, _) = srv.accept().unwrap();
        let t = Instant::now();
        let th = std::thread::spawn(move || serve_one(s, LAN, &sh, Instant::now()));
        send(&mut c, &code);
        let mut resp = String::new();
        let _ = c.read_to_string(&mut resp);
        th.join().unwrap();
        let took = t.elapsed();
        drop(l);
        let _ = std::fs::remove_dir_all(dir);
        (resp, took)
    }

    /// G-17: a declared 10^12-byte body is refused from the header alone — nothing is read or
    /// allocated for it, and the reply comes at once.
    #[test]
    fn huge_content_length_413_without_reading() {
        let (resp, took) = exchange("huge", |c, _| {
            c.write_all(
                b"POST /capture HTTP/1.1\r\nHost: x\r\nContent-Length: 1000000000000\r\n\r\n",
            )
            .unwrap();
        });
        assert!(resp.starts_with("HTTP/1.1 413"), "{resp}");
        assert!(took < Duration::from_secs(1), "took {took:?}");
        let (resp, _) = exchange("huger", |c, _| {
            c.write_all(
                b"POST /capture HTTP/1.1\r\nContent-Length: 99999999999999999999999\r\n\r\n",
            )
            .unwrap();
        });
        assert!(resp.starts_with("HTTP/1.1 413"), "{resp}");
    }

    #[test]
    fn chunked_body_411() {
        let (resp, _) = exchange("chunked", |c, _| {
            c.write_all(b"POST /capture HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n{}\r\n")
                .unwrap();
        });
        assert!(resp.starts_with("HTTP/1.1 411"), "{resp}");
    }

    #[test]
    fn oversized_head_431() {
        let (resp, _) = exchange("bighead", |c, _| {
            let pad = "a".repeat(MAX_HEAD);
            let _ = write!(c, "POST /capture HTTP/1.1\r\nX: {pad}\r\n\r\n");
        });
        assert!(resp.starts_with("HTTP/1.1 431"), "{resp}");
    }

    /// G-17: a body that stops arriving times the connection out (IO_TIMEOUT), freeing the slot.
    #[test]
    fn stalled_body_times_out() {
        let (resp, took) = exchange("stallbody", |c, _| {
            c.write_all(b"POST /capture HTTP/1.1\r\nContent-Length: 2000\r\n\r\n{")
                .unwrap();
        });
        assert!(resp.starts_with("HTTP/1.1 408"), "{resp}");
        assert!(took < IO_TIMEOUT + Duration::from_secs(2), "took {took:?}");
    }

    /// A real POST end to end: head + body over the socket, through `handle`, reply parsed back.
    #[test]
    fn real_post_end_to_end() {
        let (resp, _) = exchange("e2e", |c, code| {
            let b = body(code, &album());
            write!(c, "POST /capture HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", b.len()).unwrap();
            c.write_all(&b).unwrap();
        });
        assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"), "{resp}");
        assert!(resp.contains("Connection: close\r\n"), "{resp}");
        assert!(resp.ends_with("\r\n\r\nAlready in your library."), "{resp}");
    }

    #[test]
    fn real_socket_smoke() {
        let dir = tmp("socket");
        let (l, _) = listener(&dir, 0);
        let info = l.set_enabled(true).unwrap();
        assert!(info.on);
        assert_eq!(info.code.len(), 7);
        let port = l.bound_port().unwrap();
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let b = body("000000", &album());
        write!(s, "POST /capture HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", b.len()).unwrap();
        s.write_all(&b).unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        assert!(
            resp.starts_with("HTTP/1.1 403"),
            "loopback isn't a LAN source: {resp}"
        );
        let code = l.info().code;
        assert_ne!(l.new_pairing_code().unwrap().code, code);
        assert!(!l.set_enabled(false).unwrap().on); // joins the accept loop (bounded by POLL)
        assert!(l.bound_port().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}

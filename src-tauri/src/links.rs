//! Spotify link parsing (TSD §4.1, FR-002), short-link resolution via `net`, and the offline
//! `unresolved_capture` queue. `parse` mirrors `src/lib/link.ts`; both pass the same case table.
use std::sync::Mutex;

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::error::{AppError, Code, Result};
use crate::library::{ItemType, items};
use crate::net;
use crate::text::limit;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    Ok,
    Short,
    Unsupported,
    Invalid,
}

/// `parse_link` result (API.md §3). `existing_id` is filled by [`with_existing`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Parsed {
    pub state: LinkState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<ItemType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_id: Option<i64>,
}

impl Parsed {
    fn state(state: LinkState) -> Self {
        Self {
            state,
            uri: None,
            kind: None,
            existing_id: None,
        }
    }
}

/// Kinds that are real Spotify links but never library items (D23; playlists only via Playlist features).
const UNSUPPORTED: [&str; 4] = ["episode", "show", "audiobook", "playlist"];

fn is_id(s: &str) -> bool {
    s.len() == 22 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn is_letters(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphabetic())
}

/// Case-insensitive ASCII prefix strip.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    (s.len() >= prefix.len()
        && s.is_char_boundary(prefix.len())
        && s[..prefix.len()].eq_ignore_ascii_case(prefix))
    .then(|| &s[prefix.len()..])
}

/// `intl-xx` or `intl-xx-yy`.
fn is_locale(seg: &str) -> bool {
    strip_prefix_ci(seg, "intl-").is_some_and(|rest| {
        let mut parts = rest.split('-');
        let ok = |p: Option<&str>| p.is_some_and(|p| p.len() == 2 && is_letters(p));
        ok(parts.next()) && parts.next().is_none_or(|p| ok(Some(p))) && parts.next().is_none()
    })
}

fn classify(kind: &str, id: &str) -> Parsed {
    let kind = kind.to_ascii_lowercase();
    match ItemType::parse(&kind) {
        Some(t) => Parsed {
            state: LinkState::Ok,
            uri: Some(format!("spotify:{kind}:{id}")),
            kind: Some(t),
            existing_id: None,
        },
        None if UNSUPPORTED.contains(&kind.as_str()) => Parsed::state(LinkState::Unsupported),
        None => Parsed::state(LinkState::Invalid),
    }
}

/// Pure parse of a pasted/shared link (TSD §4.1). Never touches the network.
pub fn parse(raw: &str) -> Parsed {
    let s = raw.trim();
    if s.len() > limit::LINK {
        return Parsed::state(LinkState::Invalid);
    }
    // spotify:{kind}:{id}
    if let Some(rest) = strip_prefix_ci(s, "spotify:") {
        let mut p = rest.split(':');
        return match (p.next(), p.next(), p.next()) {
            (Some(k), Some(id), None) if is_letters(k) && is_id(id) => classify(k, id),
            _ => Parsed::state(LinkState::Invalid),
        };
    }
    let rest = strip_prefix_ci(s, "https://")
        .or_else(|| strip_prefix_ci(s, "http://"))
        .unwrap_or(s);
    // spotify.link/{code}[/][?…]
    if let Some(code) = strip_prefix_ci(rest, "spotify.link/") {
        let code = code.split('?').next().unwrap_or("");
        let code = code.strip_suffix('/').unwrap_or(code);
        let ok = !code.is_empty() && code.bytes().all(|b| b.is_ascii_alphanumeric());
        return Parsed::state(if ok {
            LinkState::Short
        } else {
            LinkState::Invalid
        });
    }
    // open.spotify.com/[intl-xx/]{kind}/{id}[/?#…]
    let Some(path) = strip_prefix_ci(rest, "open.spotify.com/") else {
        return Parsed::state(LinkState::Invalid);
    };
    let mut segs = path.splitn(2, '/');
    let first = segs.next().unwrap_or("");
    let path = if is_locale(first) {
        segs.next().unwrap_or("")
    } else {
        path
    };
    let (kind, after) = path.split_once('/').unwrap_or((path, ""));
    let end = after.find(['/', '?', '#']).unwrap_or(after.len());
    let id = &after[..end];
    if is_letters(kind) && is_id(id) {
        classify(kind, id)
    } else {
        Parsed::state(LinkState::Invalid)
    }
}

/// `parse_link`: [`parse`] + the id of an item already holding that URI.
pub fn with_existing(conn: &Connection, mut p: Parsed) -> Result<Parsed> {
    if let Some(uri) = &p.uri {
        p.existing_id = items::id_by_uri(conn, uri)?;
    }
    Ok(p)
}

/// Turn a parse that isn't `ok` into the matching error.
fn ok_or_error(p: Parsed) -> Result<(String, ItemType)> {
    match (p.state, p.uri, p.kind) {
        (LinkState::Ok, Some(uri), Some(kind)) => Ok((uri, kind)),
        (LinkState::Unsupported, ..) => Err(AppError::new(
            Code::UnsupportedLink,
            "Runout keeps albums, tracks and artists. Podcasts, audiobooks and playlists aren't library items.",
        )),
        _ => Err(AppError::new(
            Code::InvalidLink,
            "That isn't a Spotify album, track or artist link.",
        )),
    }
}

/// First open.spotify.com item link in an HTML page (spotify.link may answer 200 with the target in the page).
fn find_open_link(body: &str) -> Option<Parsed> {
    let needle = "https://open.spotify.com/";
    body.match_indices(needle)
        .map(|(i, _)| {
            let tail = &body[i..];
            let end = tail
                .find(|c: char| c.is_whitespace() || "\"'<>\\&".contains(c))
                .unwrap_or(tail.len());
            parse(&tail[..end])
        })
        .find(|p| p.state == LinkState::Ok)
}

/// Resolve a link to `(uri, type)`. Short links follow allow-listed redirects (max 5, 5 s);
/// connectivity failures return `offline` (the caller queues them, see [`resolve_or_queue`]).
pub async fn resolve_short(raw: &str) -> Result<(String, ItemType)> {
    let p = parse(raw);
    if p.state != LinkState::Short {
        return ok_or_error(p);
    }
    let s = raw.trim();
    let rest = strip_prefix_ci(s, "https://")
        .or_else(|| strip_prefix_ci(s, "http://"))
        .unwrap_or(s);
    let resp = net::get(&format!("https://{rest}")).await?;
    let landed = parse(resp.url().as_str());
    if landed.state != LinkState::Invalid {
        return ok_or_error(landed);
    }
    // ponytail: whole body read (pages are ~7 KB); cap it if spotify.link ever serves large pages.
    let body = resp.text().await?;
    find_open_link(&body).map_or_else(
        || {
            Err(AppError::new(
                Code::InvalidLink,
                "That short link doesn't lead to a Spotify album, track or artist.",
            ))
        },
        ok_or_error,
    )
}

/// `prefill_title`: oEmbed title for the add dialog (TSD §4.3). The response is read and dropped here;
/// only a title the user then confirms is ever stored (TSD §4.2). Any failure but offline → `None`.
pub async fn prefill_title(uri: &str) -> Result<Option<String>> {
    let (uri, _) = ok_or_error(parse(uri))?;
    let mut parts = uri.split(':').skip(1);
    let (kind, id) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let mut url = reqwest::Url::parse("https://open.spotify.com/oembed").expect("static url");
    url.query_pairs_mut()
        .append_pair("url", &format!("https://open.spotify.com/{kind}/{id}"));
    let resp = match net::get(url.as_str()).await {
        Ok(r) if r.status().is_success() => r,
        Ok(_) => return Ok(None),
        Err(e) if e.code == Code::Offline => return Err(e),
        Err(_) => return Ok(None),
    };
    let Ok(body) = resp.bytes().await else {
        return Ok(None);
    };
    Ok(title_from_oembed(&body))
}

fn title_from_oembed(body: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    crate::text::line_or_none(v.get("title")?.as_str(), limit::NAME)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSource {
    Paste,
    Shortcut,
}

impl CaptureSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Paste => "paste",
            Self::Shortcut => "shortcut",
        }
    }
}

/// API.md `Unresolved`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unresolved {
    pub id: i64,
    pub raw_link: String,
    pub received_at: String,
    pub source: String,
    pub state: String,
    pub error: Option<String>,
}

pub fn queue_unresolved(conn: &Connection, raw: &str, source: CaptureSource) -> Result<i64> {
    conn.execute(
        "INSERT INTO unresolved_capture(raw_link, source) VALUES (?1, ?2)",
        params![raw.trim(), source.as_str()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_unresolved(conn: &Connection) -> Result<Vec<Unresolved>> {
    let mut st = conn.prepare(
        "SELECT id, raw_link, received_at, source, state, error FROM unresolved_capture ORDER BY received_at, id",
    )?;
    let rows = st.query_map([], |r| {
        Ok(Unresolved {
            id: r.get(0)?,
            raw_link: r.get(1)?,
            received_at: r.get(2)?,
            source: r.get(3)?,
            state: r.get(4)?,
            error: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn discard_unresolved(conn: &Connection, id: i64) -> Result<()> {
    if conn.execute("DELETE FROM unresolved_capture WHERE id = ?1", [id])? == 0 {
        return Err(AppError::not_found("That link is no longer waiting."));
    }
    Ok(())
}

/// Apply one resolve attempt to a queued row. Ok → name-needed item + row removed (`true`);
/// offline → left as is; any other error → `failed` with its message.
pub fn apply_resolution(
    conn: &mut Connection,
    id: i64,
    source: &str,
    outcome: Result<(String, ItemType)>,
) -> Result<bool> {
    match outcome {
        Ok((uri, kind)) => {
            let via = if source == "shortcut" { "phone" } else { "pc" };
            let tx = conn.transaction()?;
            items::insert_name_needed(&tx, kind, &uri, via)?;
            tx.execute("DELETE FROM unresolved_capture WHERE id = ?1", [id])?;
            tx.commit()?;
            Ok(true)
        }
        Err(e) if e.code == Code::Offline => Ok(false),
        Err(e) => {
            conn.execute(
                "UPDATE unresolved_capture SET state = 'failed', error = ?2 WHERE id = ?1",
                params![id, e.message],
            )?;
            Ok(false)
        }
    }
}

/// `resolve_link`: resolve now, or queue in `unresolved_capture` when offline (error still returned).
/// Takes the mutex so no lock is held across the network wait.
pub async fn resolve_or_queue(
    db: &Mutex<Connection>,
    raw: &str,
    source: CaptureSource,
) -> Result<(String, ItemType)> {
    let out = resolve_short(raw).await;
    if matches!(&out, Err(e) if e.code == Code::Offline) {
        queue_unresolved(&db.lock().unwrap(), raw, source)?;
    }
    out
}

/// `retry_unresolved` (and on `online-changed` → true): returns how many resolved. Stops at the first
/// offline answer so a flaky connection doesn't mark rows failed.
pub async fn retry_unresolved(db: &Mutex<Connection>) -> Result<usize> {
    let pending = list_unresolved(&db.lock().unwrap())?;
    let mut resolved = 0;
    for u in pending {
        let out = resolve_short(&u.raw_link).await;
        let offline = matches!(&out, Err(e) if e.code == Code::Offline);
        if apply_resolution(&mut db.lock().unwrap(), u.id, &u.source, out)? {
            resolved += 1;
        }
        if offline {
            break;
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const A: &str = "3QJzdZJYIAcoET1GcfpNGi"; // synthetic 22-char id

    fn ok(kind: &str) -> Parsed {
        classify(kind, A)
    }
    fn st(s: LinkState) -> Parsed {
        Parsed::state(s)
    }

    /// Rows 1–16 are `src/lib/link.test.ts`'s TABLE verbatim; the rest are Rust-side extras.
    #[test]
    fn parse_table() {
        use LinkState::*;
        let long = format!("https://open.spotify.com/album/{A}?si={}", "x".repeat(600));
        let cases: Vec<(String, Parsed)> = vec![
            (format!("https://open.spotify.com/album/{A}"), ok("album")),
            (
                format!("https://open.spotify.com/album/{A}?si=abc123"),
                ok("album"),
            ),
            (
                format!("https://open.spotify.com/intl-de/track/{A}"),
                ok("track"),
            ),
            (
                format!("https://open.spotify.com/intl-pt-br/artist/{A}?si=x"),
                ok("artist"),
            ),
            (format!("  open.spotify.com/track/{A}  "), ok("track")),
            (format!("spotify:artist:{A}"), ok("artist")),
            ("https://spotify.link/aXk3P9q".into(), st(Short)),
            ("spotify.link/NBdJ4D2".into(), st(Short)),
            (
                format!("https://open.spotify.com/episode/{A}"),
                st(Unsupported),
            ),
            (
                format!("https://open.spotify.com/show/{A}"),
                st(Unsupported),
            ),
            (
                format!("https://open.spotify.com/audiobook/{A}"),
                st(Unsupported),
            ),
            (
                format!("https://open.spotify.com/playlist/{A}"),
                st(Unsupported),
            ),
            (
                format!("https://open.spotify.com/album/{}", &A[1..]),
                st(Invalid),
            ),
            (format!("evil.example.com/album/{A}"), st(Invalid)),
            ("coltrane".into(), st(Invalid)),
            ("".into(), st(Invalid)),
            // extras
            (format!("http://open.spotify.com/album/{A}"), ok("album")),
            (format!("HTTPS://OPEN.SPOTIFY.COM/ALBUM/{A}"), ok("album")),
            (format!("SPOTIFY:TRACK:{A}"), ok("track")),
            (format!("https://open.spotify.com/track/{A}/"), ok("track")),
            (
                format!("https://open.spotify.com/track/{A}#t=30"),
                ok("track"),
            ),
            (
                format!("\t\nhttps://open.spotify.com/intl-ja/album/{A}?si=q&nd=1\n"),
                ok("album"),
            ),
            (format!("https://open.spotify.com/album/{A}X"), st(Invalid)),
            (
                format!("https://open.spotify.com/album/{}-", &A[1..]),
                st(Invalid),
            ),
            (format!("spotify:playlist:{A}"), st(Unsupported)),
            (format!("spotify:episode:{A}"), st(Unsupported)),
            (format!("spotify:album:{A}:extra"), st(Invalid)),
            (format!("https://open.spotify.com/user/{A}"), st(Invalid)),
            (
                format!("https://open.spotify.com/intl-deu/album/{A}"),
                st(Invalid),
            ),
            (
                format!("https://open.spotify.com/embed/album/{A}"),
                st(Invalid),
            ),
            (
                format!(
                    "{}open.spotify.com.evil.test/album/{A}",
                    "https:/".to_owned() + "/"
                ),
                st(Invalid),
            ),
            ("https://spotify.link/".into(), st(Invalid)),
            ("https://spotify.link/aXk3P9q/?si=1".into(), st(Short)),
            ("https://spotify.link/a-b".into(), st(Invalid)),
            (long, st(Invalid)),
        ];
        for (raw, want) in cases {
            assert_eq!(parse(&raw), want, "{raw:?}");
        }
    }

    #[test]
    fn oembed_title_only() {
        let body = br#"{"title":" Night\u0007 Songs ","thumbnail_url":"x","html":"<iframe>"}"#;
        assert_eq!(title_from_oembed(body).as_deref(), Some("Night Songs"));
        assert_eq!(title_from_oembed(b"{}"), None);
        assert_eq!(title_from_oembed(b"not json"), None);
        let e = tauri::async_runtime::block_on(prefill_title("spotify:show:x")).unwrap_err();
        assert_eq!(e.code, Code::InvalidLink);
    }

    #[test]
    fn serializes_like_api_md() {
        let v = serde_json::to_value(parse(&format!("spotify:album:{A}"))).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"state": "ok", "uri": format!("spotify:album:{A}"), "type": "album"})
        );
        let v = serde_json::to_value(parse("x")).unwrap();
        assert_eq!(v, serde_json::json!({"state": "invalid"}));
    }

    #[test]
    fn finds_target_in_short_link_page() {
        let html = format!(
            "<a href=\"https://open.spotify.com/\">x</a><meta content=\"https://open.spotify.com/track/{A}?si=z&amp;b\">"
        );
        assert_eq!(find_open_link(&html), Some(ok("track")));
        assert_eq!(find_open_link("<html>nothing</html>"), None);
    }

    #[test]
    fn existing_id_and_errors() {
        let c = db::open_in_memory().unwrap();
        let uri = format!("spotify:album:{A}");
        let id = items::insert_name_needed(&c, ItemType::Album, &uri, "pc").unwrap();
        let p = with_existing(&c, parse(&format!("https://open.spotify.com/album/{A}"))).unwrap();
        assert_eq!(p.existing_id, Some(id));
        let e = ok_or_error(parse(&format!("spotify:show:{A}"))).unwrap_err();
        assert_eq!(e.code, Code::UnsupportedLink);
        assert_eq!(ok_or_error(parse("x")).unwrap_err().code, Code::InvalidLink);
        // a non-short link resolves without any network
        let r = tauri::async_runtime::block_on(resolve_short(&uri)).unwrap();
        assert_eq!(r, (uri, ItemType::Album));
    }

    #[test]
    fn unresolved_queue_lifecycle() {
        let mut c = db::open_in_memory().unwrap();
        let a = queue_unresolved(
            &c,
            " https://spotify.link/aXk3P9q ",
            CaptureSource::Shortcut,
        )
        .unwrap();
        let b = queue_unresolved(&c, "https://spotify.link/NBdJ4D2", CaptureSource::Paste).unwrap();
        let d = queue_unresolved(&c, "https://spotify.link/Qq1", CaptureSource::Paste).unwrap();
        let rows = list_unresolved(&c).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(
            (rows[0].raw_link.as_str(), rows[0].state.as_str()),
            ("https://spotify.link/aXk3P9q", "unresolved")
        );

        // offline: untouched
        assert!(!apply_resolution(&mut c, a, "shortcut", Err(AppError::offline())).unwrap());
        assert_eq!(list_unresolved(&c).unwrap().len(), 3);
        // resolved: name-needed item from the phone, row gone
        let uri = format!("spotify:track:{A}");
        assert!(
            apply_resolution(&mut c, a, "shortcut", Ok((uri.clone(), ItemType::Track))).unwrap()
        );
        let (name, via, prov): (Option<String>, String, String) = c
            .query_row(
                "SELECT display_name, added_via, (SELECT source FROM field_provenance WHERE item_id = item.id AND field = 'spotify_uri') FROM item WHERE spotify_uri = ?1",
                [&uri],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (name, via.as_str(), prov.as_str()),
            (None, "phone", "promoted")
        );
        // same URI again: no duplicate item
        assert!(apply_resolution(&mut c, b, "paste", Ok((uri, ItemType::Track))).unwrap());
        let n: i64 = c
            .query_row("SELECT count(*) FROM item", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        // other error: marked failed
        let bad = AppError::new(Code::InvalidLink, "nope");
        assert!(!apply_resolution(&mut c, d, "paste", Err(bad)).unwrap());
        let rows = list_unresolved(&c).unwrap();
        assert_eq!(
            (rows.len(), rows[0].state.as_str(), rows[0].error.as_deref()),
            (1, "failed", Some("nope"))
        );
        discard_unresolved(&c, d).unwrap();
        assert_eq!(discard_unresolved(&c, d).unwrap_err().code, Code::NotFound);
    }
}

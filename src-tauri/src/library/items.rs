//! Item CRUD (FR-001–004): create/get/update/delete, duplicate → `duplicate{existing_id}`.
use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, Row, ToSql, params};
use serde::{Deserialize, Deserializer, Serialize};

use super::links::{self, LinkEntry};
use super::status::{self, HistoryEntry};
use super::{ItemType, LinkKind, NOW, Priority, Prov, Status, backlog, provenance, tags};
use crate::error::{AppError, Code, Result};
use crate::links::{LinkState, parse};
use crate::text::{self, limit};

/// API.md §2 `ItemSummary`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemSummary {
    pub id: i64,
    pub r#type: ItemType,
    pub display_name: Option<String>,
    pub spotify_uri: Option<String>,
    pub status: Status,
    pub priority: Option<Priority>,
    pub rating: Option<f64>,
    pub favourite: bool,
    pub revisit: bool,
    pub in_backlog: bool,
    pub backlog_order: Option<i64>,
    pub tags: Vec<String>,
    pub artist_names: Vec<String>,
    pub album_name: Option<String>,
    pub added_via: String,
    pub recommended_by: Option<String>,
    pub notes: Option<String>,
    pub added_at: String,
    pub modified_at: String,
}

/// API.md §2 `ItemDetail` (= `ItemSummary` + these fields).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemDetail {
    #[serde(flatten)]
    pub summary: ItemSummary,
    pub review: Option<String>,
    pub explored_at: Option<String>,
    pub reviewed_at: Option<String>,
    /// Albums only: tracks linked `on_album`. `position` = the order the tracks entered the library
    /// (track item id); MVP stores no Spotify track numbers. `null` when no track is linked.
    pub track_list: Option<Vec<TrackEntry>>,
    pub provenance: BTreeMap<String, Prov>,
    pub links: Vec<LinkEntry>,
    pub history: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrackEntry {
    pub position: i64,
    pub name: String,
    pub item_id: Option<i64>,
    /// Counted plays of the track (G-1); reconciled logger duplicates excluded (TSD §4.7).
    pub plays: i64,
}

/// API.md §2 `NewItem`.
#[derive(Debug, Clone, Deserialize)]
pub struct NewItem {
    pub r#type: ItemType,
    pub display_name: Option<String>,
    pub spotify_uri: Option<String>,
    #[serde(default)]
    pub status: Option<Status>,
    #[serde(default)]
    pub priority: Option<Priority>,
    #[serde(default)]
    pub in_backlog: Option<bool>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub recommended_by: Option<String>,
    pub provenance: Prov,
    #[serde(default)]
    pub promoted_fields: Vec<String>,
    #[serde(default)]
    pub capture_ms: Option<i64>,
    #[serde(default)]
    pub artist_name: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub album_id: Option<i64>,
}

/// Present-or-null: `Some(None)` = sent as `null`, `None` = key absent.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// API.md §2 `ItemPatch`. Nullable fields are `Option<Option<_>>` so `null` (clear) ≠ absent (keep).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ItemPatch {
    #[serde(default, deserialize_with = "present")]
    pub display_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub priority: Option<Option<Priority>>,
    #[serde(default, deserialize_with = "present")]
    pub rating: Option<Option<f64>>,
    #[serde(default, deserialize_with = "present")]
    pub review: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub notes: Option<Option<String>>,
    #[serde(default)]
    pub favourite: Option<bool>,
    #[serde(default)]
    pub revisit: Option<bool>,
    #[serde(default)]
    pub in_backlog: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    pub recommended_by: Option<Option<String>>,
    #[serde(default)]
    pub r#type: Option<ItemType>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub promoted_fields: Vec<String>,
    #[serde(default, deserialize_with = "present")]
    pub artist_name: Option<Option<String>>,
}

/// Summary columns over `item i`; [`summary_row`] reads them in this order.
pub(crate) const SUMMARY_COLS: &str = "i.id, i.type, i.display_name, i.spotify_uri, i.status, i.priority,
  i.rating, i.favourite, i.revisit, i.in_backlog, i.backlog_order,
  (SELECT json_group_array(t.name ORDER BY t.name COLLATE NOCASE)
     FROM item_tag it JOIN tag t ON t.id = it.tag_id WHERE it.item_id = i.id),
  (SELECT json_group_array(a.display_name ORDER BY a.id) FROM item_link l JOIN item a ON a.id = l.to_id
     WHERE l.from_id = i.id AND l.kind = 'by_artist' AND a.display_name IS NOT NULL),
  CASE WHEN i.type = 'track' THEN (SELECT a.display_name FROM item_link l JOIN item a ON a.id = l.to_id
     WHERE l.from_id = i.id AND l.kind = 'on_album' LIMIT 1) END,
  i.added_via, i.recommended_by, i.notes, i.added_at, i.modified_at";

fn summary_row(r: &Row) -> rusqlite::Result<ItemSummary> {
    let list = |i: usize| -> rusqlite::Result<Vec<String>> {
        Ok(serde_json::from_str(&r.get::<_, String>(i)?).unwrap_or_default())
    };
    Ok(ItemSummary {
        id: r.get(0)?,
        r#type: r.get(1)?,
        display_name: r.get(2)?,
        spotify_uri: r.get(3)?,
        status: r.get(4)?,
        priority: r.get(5)?,
        rating: r.get(6)?,
        favourite: r.get(7)?,
        revisit: r.get(8)?,
        in_backlog: r.get(9)?,
        backlog_order: r.get(10)?,
        tags: list(11)?,
        artist_names: list(12)?,
        album_name: r.get(13)?,
        added_via: r.get(14)?,
        recommended_by: r.get(15)?,
        notes: r.get(16)?,
        added_at: r.get(17)?,
        modified_at: r.get(18)?,
    })
}

/// `SELECT <summary> FROM item i <tail>` — `tail` is fixed SQL (WHERE/ORDER/LIMIT) with bound params only.
pub(crate) fn summaries(
    conn: &Connection,
    tail: &str,
    p: impl rusqlite::Params,
) -> Result<Vec<ItemSummary>> {
    let mut st = conn.prepare(&format!("SELECT {SUMMARY_COLS} FROM item i {tail}"))?;
    let rows = st.query_map(p, summary_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn summary(conn: &Connection, id: i64) -> Result<ItemSummary> {
    summaries(conn, "WHERE i.id = ?1", [id])?
        .pop()
        .ok_or_else(gone)
}

fn gone() -> AppError {
    AppError::not_found("That record no longer exists.")
}

/// Id of the item holding `uri`, if any (FR-004 duplicate check, `parse_link.existing_id`).
pub fn id_by_uri(conn: &Connection, uri: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row("SELECT id FROM item WHERE spotify_uri = ?1", [uri], |r| {
            r.get(0)
        })
        .optional()?)
}

/// "Name needed" item from a captured/resolved link (TSD §4.2): URI only, provenance `promoted`,
/// end of the backlog. Returns the existing id if the URI is already in the library.
pub fn insert_name_needed(
    conn: &Connection,
    kind: ItemType,
    uri: &str,
    added_via: &str,
) -> Result<i64> {
    if let Some(id) = id_by_uri(conn, uri)? {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO item(type, spotify_uri, added_via, backlog_order)
         VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(backlog_order), 0) + 1 FROM item WHERE in_backlog = 1))",
        params![kind.as_str(), uri, added_via],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO field_provenance(item_id, field, source) VALUES (?1, 'spotify_uri', 'promoted')",
        [id],
    )?;
    conn.execute(
        "INSERT INTO status_history(item_id, from_status, to_status) VALUES (?1, NULL, 'planned')",
        [id],
    )?;
    Ok(id)
}

pub fn get(conn: &Connection, id: i64) -> Result<ItemDetail> {
    let summary = summary(conn, id)?;
    let (review, explored_at, reviewed_at) = conn.query_row(
        "SELECT review, explored_at, reviewed_at FROM item WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let track_list = match summary.r#type {
        ItemType::Album => Some(track_list(conn, id)?).filter(|t| !t.is_empty()),
        _ => None,
    };
    Ok(ItemDetail {
        summary,
        review,
        explored_at,
        reviewed_at,
        track_list,
        provenance: provenance::map(conn, id)?,
        links: links::of(conn, id)?,
        history: status::history(conn, id)?,
    })
}

fn track_list(conn: &Connection, album: i64) -> Result<Vec<TrackEntry>> {
    let mut st = conn.prepare(
        "SELECT t.id, COALESCE(t.display_name, t.spotify_uri, ''),
           (SELECT COUNT(*) FROM listening_event e
             WHERE e.item_id = t.id AND e.counts_as_play AND e.reconciled_with IS NULL)
         FROM item_link l
         JOIN item t ON t.id = l.from_id WHERE l.to_id = ?1 AND l.kind = 'on_album' ORDER BY t.id",
    )?;
    let rows = st.query_map([album], |r| Ok((r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?)))?;
    let mut out = vec![];
    for (n, row) in rows.enumerate() {
        let (id, name, plays) = row?;
        out.push(TrackEntry {
            position: n as i64 + 1,
            name,
            item_id: Some(id),
            plays,
        });
    }
    Ok(out)
}

/// Canonical library URI + its type from a pasted value (re-validated here, TSD §7).
fn valid_uri(raw: &str) -> Result<(String, ItemType)> {
    let p = parse(raw);
    match (p.state, p.uri, p.kind) {
        (LinkState::Ok, Some(uri), Some(kind)) => Ok((uri, kind)),
        (LinkState::Unsupported, ..) => Err(AppError::new(
            Code::UnsupportedLink,
            "Runout keeps albums, tracks and artists only.",
        )),
        _ => Err(AppError::new(
            Code::InvalidLink,
            "That isn't a Spotify album, track or artist link.",
        )),
    }
}

fn clean_text(s: Option<&str>) -> Option<String> {
    s.map(|s| text::clean_text(s, limit::TEXT))
        .filter(|s| !s.is_empty())
}

fn check_rating(r: Option<f64>) -> Result<()> {
    match r {
        Some(r) if !((0.5..=5.0).contains(&r) && (r * 2.0).fract() == 0.0) => Err(
            AppError::validation("Ratings go from ½ to 5 stars in half steps."),
        ),
        _ => Ok(()),
    }
}

fn an(t: ItemType) -> &'static str {
    if t == ItemType::Album || t == ItemType::Artist {
        "n"
    } else {
        ""
    }
}

pub fn create(conn: &mut Connection, input: NewItem) -> Result<ItemDetail> {
    let uri = match input.spotify_uri.as_deref().map(str::trim) {
        Some(raw) if !raw.is_empty() => {
            let (uri, kind) = valid_uri(raw)?;
            if kind != input.r#type {
                return Err(AppError::validation(format!(
                    "That link is a{} {}, not a{} {}.",
                    an(kind),
                    kind.as_str(),
                    an(input.r#type),
                    input.r#type.as_str()
                )));
            }
            Some(uri)
        }
        _ => None,
    };
    if let Some(u) = &uri
        && let Some(id) = id_by_uri(conn, u)?
    {
        return Err(AppError::duplicate(id));
    }
    let name = text::line_or_none(input.display_name.as_deref(), limit::NAME);
    if uri.is_none() && name.is_none() {
        return Err(AppError::validation("Give it a name or paste a link."));
    }
    let notes = clean_text(input.notes.as_deref());
    let rec = text::line_or_none(input.recommended_by.as_deref(), limit::PERSON);
    let artist = text::line_or_none(input.artist_name.as_deref(), limit::NAME)
        .filter(|_| input.r#type != ItemType::Artist);
    let status = input.status.unwrap_or(Status::Planned);

    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO item(type, display_name, spotify_uri, status, priority, notes, recommended_by, in_backlog)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
        params![input.r#type, name, uri, status, input.priority, notes, rec],
    )?;
    let id = tx.last_insert_rowid();
    backlog::set_in_backlog(&tx, id, input.in_backlog.unwrap_or(true))?;
    tags::set_item_tags(&tx, id, &input.tags)?;
    let artist_src = provenance::source_for("artist", &input.promoted_fields, input.provenance);
    if let Some(a) = &artist {
        set_artist(&tx, id, Some(a), artist_src)?;
    }
    if let Some(album) = input.album_id {
        links::link(&tx, id, album, LinkKind::OnAlbum)?;
    }
    let present: Vec<&str> = [
        ("spotify_uri", uri.is_some()),
        ("display_name", name.is_some()),
        ("artist", artist.is_some()),
        ("notes", notes.is_some()),
        ("recommended_by", rec.is_some()),
        ("priority", input.priority.is_some()),
    ]
    .into_iter()
    .filter_map(|(f, on)| on.then_some(f))
    .collect();
    provenance::record(&tx, id, &present, input.provenance, &input.promoted_fields)?;
    if uri.is_some() {
        // TSD §4.2: a pasted/shared link's URI is always promoted, whatever the create default.
        provenance::record(&tx, id, &["spotify_uri"], Prov::Promoted, &[])?;
    }
    status::insert(&tx, id, None, status, None)?;
    status::apply_side_effects(&tx, id, None, status)?;
    if let Some(ms) = input.capture_ms {
        crate::usage::record(&tx, crate::usage::Kind::Capture, None, Some(ms))?;
    }
    tx.commit()?;
    get(conn, id)
}

/// Replace the item's `by_artist` link with one to the artist called `name` (case-insensitive),
/// creating that artist item if needed. `None` unlinks.
fn set_artist(conn: &Connection, item: i64, name: Option<&str>, src: Prov) -> Result<()> {
    conn.execute(
        "DELETE FROM item_link WHERE from_id = ?1 AND kind = 'by_artist'",
        [item],
    )?;
    let Some(name) = name else { return Ok(()) };
    let found: Option<i64> = conn
        .query_row(
            "SELECT id FROM item WHERE type = 'artist' AND display_name = ?1 COLLATE NOCASE
             ORDER BY id LIMIT 1",
            [name],
            |r| r.get(0),
        )
        .optional()?;
    let artist = match found {
        Some(id) => id,
        None => {
            conn.execute(
                "INSERT INTO item(type, display_name, in_backlog) VALUES ('artist', ?1, 0)",
                [name],
            )?;
            let id = conn.last_insert_rowid();
            provenance::record(conn, id, &["display_name"], src, &[])?;
            status::insert(conn, id, None, Status::Planned, None)?;
            id
        }
    };
    links::link(conn, item, artist, LinkKind::ByArtist)
}

fn set_col(conn: &Connection, id: i64, col: &str, v: &dyn ToSql) -> Result<()> {
    // `col` is always a literal from `update` below, never user input.
    conn.execute(
        &format!("UPDATE item SET {col} = ?2 WHERE id = ?1"),
        params![id, v],
    )?;
    Ok(())
}

pub fn update(conn: &mut Connection, id: i64, patch: ItemPatch) -> Result<ItemDetail> {
    let (cur, linked): (ItemType, bool) = conn
        .query_row(
            "SELECT type, spotify_uri IS NOT NULL FROM item WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(gone)?;
    let tx = conn.transaction()?;
    let mut touched: Vec<&str> = vec![];
    if let Some(t) = patch.r#type
        && t != cur
    {
        if linked {
            return Err(AppError::validation(
                "A record with a Spotify link keeps the link's type.",
            ));
        }
        set_col(&tx, id, "type", &t)?;
        links::drop_misfits(&tx, id)?;
        touched.push("type");
    }
    if let Some(v) = &patch.display_name {
        set_col(
            &tx,
            id,
            "display_name",
            &text::line_or_none(v.as_deref(), limit::NAME),
        )?;
        touched.push("display_name");
    }
    if let Some(v) = &patch.priority {
        set_col(&tx, id, "priority", v)?;
        touched.push("priority");
    }
    if let Some(v) = patch.rating {
        check_rating(v)?;
        set_col(&tx, id, "rating", &v)?;
        touched.push("rating");
    }
    if let Some(v) = &patch.review {
        let v = clean_text(v.as_deref());
        tx.execute(
            &format!(
                "UPDATE item SET review = ?2, reviewed_at = CASE WHEN ?2 IS NULL THEN NULL ELSE {NOW} END
                 WHERE id = ?1"
            ),
            params![id, v],
        )?;
        touched.push("review");
    }
    if let Some(v) = &patch.notes {
        set_col(&tx, id, "notes", &clean_text(v.as_deref()))?;
        touched.push("notes");
    }
    if let Some(v) = &patch.recommended_by {
        let v = text::line_or_none(v.as_deref(), limit::PERSON);
        set_col(&tx, id, "recommended_by", &v)?;
        touched.push("recommended_by");
    }
    for (col, v) in [("favourite", patch.favourite), ("revisit", patch.revisit)] {
        if let Some(v) = v {
            set_col(&tx, id, col, &v)?;
            touched.push(col);
        }
    }
    if let Some(on) = patch.in_backlog {
        backlog::set_in_backlog(&tx, id, on)?;
        touched.push("in_backlog");
    }
    if let Some(t) = &patch.tags {
        tags::set_item_tags(&tx, id, t)?;
        touched.push("tags");
    }
    if let Some(v) = &patch.artist_name {
        let is_artist = patch.r#type.unwrap_or(cur) == ItemType::Artist;
        let name = text::line_or_none(v.as_deref(), limit::NAME).filter(|_| !is_artist);
        let src = provenance::source_for("artist", &patch.promoted_fields, Prov::User);
        set_artist(&tx, id, name.as_deref(), src)?;
        touched.push("artist");
    }
    let nameless: bool = tx.query_row(
        "SELECT display_name IS NULL AND spotify_uri IS NULL FROM item WHERE id = ?1",
        [id],
        |r| r.get(0),
    )?;
    if nameless {
        return Err(AppError::validation(
            "A record without a link needs a name.",
        ));
    }
    provenance::record(&tx, id, &touched, Prov::User, &patch.promoted_fields)?;
    touch(&tx, id)?;
    tx.commit()?;
    get(conn, id)
}

/// Bump `modified_at`.
pub(crate) fn touch(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        &format!("UPDATE item SET modified_at = {NOW} WHERE id = ?1"),
        [id],
    )?;
    Ok(())
}

/// FR-022. Links, tags, provenance and history go with it (ON DELETE CASCADE).
pub fn delete_item(conn: &Connection, id: i64) -> Result<()> {
    match conn.execute("DELETE FROM item WHERE id = ?1", [id])? {
        0 => Err(gone()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::library::testutil::{add, new};

    const URI: &str = "spotify:album:AAAAAAAAAAAAAAAAAAAAAA";

    #[test]
    fn duplicate_uri_is_blocked_with_existing_id() {
        let mut c = open_in_memory().unwrap();
        let mut n = new(ItemType::Album, "One");
        n.spotify_uri =
            Some("https://open.spotify.com/intl-de/album/AAAAAAAAAAAAAAAAAAAAAA?si=x".into());
        let first = create(&mut c, n.clone()).unwrap();
        assert_eq!(first.summary.spotify_uri.as_deref(), Some(URI));
        n.spotify_uri = Some(URI.into());
        let e = create(&mut c, n).unwrap_err();
        assert_eq!(
            (e.code, e.existing_id),
            (Code::Duplicate, Some(first.summary.id))
        );
    }

    #[test]
    fn create_rejects_bad_links_and_nameless_items() {
        let mut c = open_in_memory().unwrap();
        let mut n = new(ItemType::Album, "x");
        n.spotify_uri = Some("https://open.spotify.com/episode/AAAAAAAAAAAAAAAAAAAAAA".into());
        assert_eq!(
            create(&mut c, n.clone()).unwrap_err().code,
            Code::UnsupportedLink
        );
        n.spotify_uri = Some("not a link".into());
        assert_eq!(
            create(&mut c, n.clone()).unwrap_err().code,
            Code::InvalidLink
        );
        n.spotify_uri = None;
        n.display_name = Some(" \u{200B} ".into());
        assert_eq!(create(&mut c, n).unwrap_err().code, Code::Validation);
    }

    #[test]
    fn type_must_match_the_uri() {
        let mut c = open_in_memory().unwrap();
        let mut n = new(ItemType::Track, "x");
        n.spotify_uri = Some(URI.into()); // an album URI
        assert_eq!(
            create(&mut c, n.clone()).unwrap_err().code,
            Code::Validation
        );
        n.r#type = ItemType::Album;
        let linked = create(&mut c, n).unwrap().summary.id;
        // a record with a Spotify URI can't change type; a manual one can
        let to_track = || ItemPatch {
            r#type: Some(ItemType::Track),
            ..Default::default()
        };
        assert_eq!(
            update(&mut c, linked, to_track()).unwrap_err().code,
            Code::Validation
        );
        assert_eq!(get(&c, linked).unwrap().summary.r#type, ItemType::Album);
        let same = ItemPatch {
            r#type: Some(ItemType::Album),
            ..Default::default()
        };
        assert!(
            update(&mut c, linked, same).is_ok(),
            "same type is no change"
        );
        let manual = add(&mut c, ItemType::Album, "m");
        assert_eq!(
            update(&mut c, manual, to_track()).unwrap().summary.r#type,
            ItemType::Track
        );
    }

    #[test]
    fn create_cleans_text_records_history_and_capture() {
        let mut c = open_in_memory().unwrap();
        let mut n = new(ItemType::Album, "  Night\u{202E}  Songs ");
        n.notes = Some("a\r\n\r\n\r\nb".into());
        n.capture_ms = Some(4200);
        n.tags = vec!["Jazz".into(), "jazz".into(), " ".into()];
        let d = create(&mut c, n).unwrap();
        assert_eq!(d.summary.display_name.as_deref(), Some("Night Songs"));
        assert_eq!(d.summary.notes.as_deref(), Some("a\n\nb"));
        assert_eq!(d.summary.tags, vec!["Jazz"]);
        assert_eq!(d.summary.added_via, "pc");
        assert_eq!((d.history.len(), d.history[0].to), (1, Status::Planned));
        assert_eq!(d.summary.backlog_order, Some(1));
        let ms: i64 = c
            .query_row(
                "SELECT duration_ms FROM usage_event WHERE kind = 'capture'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ms, 4200);
        let mut huge = new(ItemType::Album, "Other");
        huge.capture_ms = Some(i64::MAX);
        create(&mut c, huge).unwrap();
        let max: i64 = c
            .query_row("SELECT max(duration_ms) FROM usage_event", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            max, 3_600_000,
            "TSD §7: clamped, so the median can't overflow"
        );
    }

    #[test]
    fn artist_name_finds_or_creates_and_relinks() {
        let mut c = open_in_memory().unwrap();
        let existing = add(&mut c, ItemType::Artist, "Khruangbin");
        let mut n = new(ItemType::Album, "Mordechai");
        n.artist_name = Some("khruangbin".into());
        let d = create(&mut c, n).unwrap();
        assert_eq!(d.summary.artist_names, vec!["Khruangbin"]);
        assert_eq!(d.links[0].item.id, existing);

        let p = ItemPatch {
            artist_name: Some(Some("Arooj Aftab".into())),
            ..Default::default()
        };
        let d = update(&mut c, d.summary.id, p).unwrap();
        assert_eq!(d.summary.artist_names, vec!["Arooj Aftab"]);
        let artists: i64 = c
            .query_row("SELECT count(*) FROM item WHERE type = 'artist'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(artists, 2, "a new artist item was created");
        let artist = get(&c, d.links[0].item.id).unwrap();
        assert!(!artist.summary.in_backlog);
        assert_eq!(artist.links[0].direction, links::Direction::To);

        let p = ItemPatch {
            artist_name: Some(None),
            ..Default::default()
        };
        let d = update(&mut c, d.summary.id, p).unwrap();
        assert!(d.summary.artist_names.is_empty() && d.links.is_empty());
    }

    #[test]
    fn album_id_links_track_on_album_and_fills_track_list() {
        let mut c = open_in_memory().unwrap();
        let album = add(&mut c, ItemType::Album, "Vulture Prince");
        let mut ids = vec![];
        for name in ["Baghon Main", "Mohabbat"] {
            let mut n = new(ItemType::Track, name);
            n.album_id = Some(album);
            let d = create(&mut c, n).unwrap();
            assert_eq!(d.summary.album_name.as_deref(), Some("Vulture Prince"));
            ids.push(d.summary.id);
        }
        let a = get(&c, album).unwrap();
        let tl = a.track_list.unwrap();
        assert_eq!(
            tl.iter()
                .map(|t| (t.position, t.name.as_str(), t.item_id))
                .collect::<Vec<_>>(),
            vec![
                (1, "Baghon Main", Some(ids[0])),
                (2, "Mohabbat", Some(ids[1]))
            ]
        );
        let empty = add(&mut c, ItemType::Album, "Empty");
        assert_eq!(get(&c, empty).unwrap().track_list, None);
        // album_id on a non-track is refused
        let mut n = new(ItemType::Album, "Bad");
        n.album_id = Some(album);
        assert_eq!(create(&mut c, n).unwrap_err().code, Code::Validation);
    }

    #[test]
    fn patch_distinguishes_null_from_absent_and_sets_reviewed_at() {
        let mut c = open_in_memory().unwrap();
        let id = add(&mut c, ItemType::Album, "A");
        let p: ItemPatch =
            serde_json::from_value(serde_json::json!({"rating": 4.5, "review": "great"})).unwrap();
        let d = update(&mut c, id, p).unwrap();
        assert_eq!(d.summary.rating, Some(4.5));
        assert!(d.reviewed_at.is_some());
        let p: ItemPatch = serde_json::from_value(serde_json::json!({"review": null})).unwrap();
        let d = update(&mut c, id, p).unwrap();
        assert_eq!(
            (d.review, d.reviewed_at, d.summary.rating),
            (None, None, Some(4.5))
        );
        let p: ItemPatch = serde_json::from_value(serde_json::json!({"rating": 0.7})).unwrap();
        assert_eq!(update(&mut c, id, p).unwrap_err().code, Code::Validation);
        let p: ItemPatch =
            serde_json::from_value(serde_json::json!({"display_name": null})).unwrap();
        assert_eq!(update(&mut c, id, p).unwrap_err().code, Code::Validation);
        assert_eq!(
            get(&c, id).unwrap().summary.display_name.as_deref(),
            Some("A")
        );
    }

    #[test]
    fn type_change_drops_links_that_no_longer_fit() {
        let mut c = open_in_memory().unwrap();
        let album = add(&mut c, ItemType::Album, "Al");
        let other_track = add(&mut c, ItemType::Track, "Orig");
        let mut n = new(ItemType::Track, "T");
        n.album_id = Some(album);
        n.artist_name = Some("Ar".into());
        let t = create(&mut c, n).unwrap().summary.id;
        links::link(&c, t, other_track, LinkKind::VersionOf).unwrap();

        // track → album: on_album and version_of (other is a track) go; by_artist stays.
        let p = ItemPatch {
            r#type: Some(ItemType::Album),
            ..Default::default()
        };
        let d = update(&mut c, t, p).unwrap();
        assert_eq!(
            d.links.iter().map(|l| l.kind).collect::<Vec<_>>(),
            vec![LinkKind::ByArtist]
        );
        // album → artist: by_artist (an artist can't be by an artist) goes too.
        let p = ItemPatch {
            r#type: Some(ItemType::Artist),
            ..Default::default()
        };
        assert!(update(&mut c, t, p).unwrap().links.is_empty());
    }

    #[test]
    fn delete_cascades_and_missing_is_not_found() {
        let mut c = open_in_memory().unwrap();
        let mut n = new(ItemType::Album, "A");
        n.tags = vec!["x".into()];
        let id = create(&mut c, n).unwrap().summary.id;
        delete_item(&c, id).unwrap();
        assert_eq!(get(&c, id).unwrap_err().code, Code::NotFound);
        assert_eq!(delete_item(&c, id).unwrap_err().code, Code::NotFound);
        let n: i64 = c
            .query_row("SELECT count(*) FROM item_tag", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn detail_serializes_api_md_shape() {
        let mut c = open_in_memory().unwrap();
        let id = add(&mut c, ItemType::Album, "A");
        let v = serde_json::to_value(get(&c, id).unwrap()).unwrap();
        for k in [
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
            "review",
            "explored_at",
            "reviewed_at",
            "track_list",
            "provenance",
            "links",
            "history",
        ] {
            assert!(v.get(k).is_some(), "missing {k}");
        }
        assert!(v["spotify_uri"].is_null() && v["track_list"].is_null());
        assert_eq!(v["provenance"]["display_name"], "user");
        assert_eq!(v["history"][0]["from"], serde_json::Value::Null);
    }
}

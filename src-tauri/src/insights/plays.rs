//! Plays (SC-08, SC-05): `list_plays`, `list_unmatched` + `link_play` (FR-030), `get_item_listening`.
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Range, SRC, Sourced, base, last_months, rollup, sourced};
use crate::error::{AppError, Result};
use crate::importer::normalize::norm;
use crate::library::ItemType;
use crate::library::items::{ItemSummary, summaries, summary};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tab {
    All,
    Unrated,
    Missing,
}

/// `list_plays` arg `q`.
#[derive(Debug, Clone, Deserialize)]
pub struct PlayQuery {
    pub tab: Tab,
    pub range: Range,
    pub offset: i64,
    pub limit: i64,
    /// Local `YYYY-MM-DD` (M10: Trends calendar day → Recent plays): that day only, `range` ignored.
    #[serde(default)]
    pub date: Option<String>,
}

/// API.md §4 `Play`. `item` is the track record; `album_rating` the rating of its album record
/// (or of the album the play was linked to by hand), G-7.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Play {
    pub id: i64,
    pub ended_at_local: String,
    pub ms_played: i64,
    pub counts_as_play: bool,
    pub skipped: bool,
    pub track_name: String,
    pub artist_name: String,
    pub album_name: String,
    pub source: String,
    pub spotify_uri: Option<String>,
    pub item: Option<ItemSummary>,
    pub album_rating: Option<f64>,
}

/// API.md §4 `Unmatched`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unmatched {
    pub norm_key: String,
    pub track_name: String,
    pub artist_name: String,
    pub plays: i64,
    pub detail: String,
    pub suggestion: String,
    pub event_id: i64,
    pub candidates: Vec<ItemSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MonthPlays {
    pub month: String,
    pub plays: i64,
}

/// API.md §4 `get_item_listening` payload.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemListening {
    pub plays: i64,
    pub ms_total: i64,
    pub last_played_local: Option<String>,
    pub by_month: Vec<MonthPlays>,
}

const MAX_PAGE: i64 = 200;

/// Plays in `q.range`, newest first. `missing` = no track record; `unrated` = a counted play of
/// an unrated track record.
pub fn list_plays(
    conn: &Connection,
    q: &PlayQuery,
    include_private: bool,
    now: DateTime<Utc>,
) -> Result<crate::library::Page<Play>> {
    let day = match q.date.as_deref() {
        None => None,
        Some(d) => Some(
            super::v3::strict_day(d)
                .ok_or_else(|| AppError::validation("That date isn't valid."))?,
        ),
    };
    // A day: its local date, padded on the UTC time index (local is at most a day off). The date
    // is a validated `NaiveDate`, so formatting it into the SQL is safe; `?1` stays bound (unused).
    let when = match day {
        Some(d) => format!(
            "substr(e.local, 1, 10) = '{0}' AND e.ended_at_utc >= date('{0}', '-1 day')
             AND e.ended_at_utc < date('{0}', '+2 day') AND ?1 = ?1",
            d.format("%Y-%m-%d")
        ),
        None => "e.ended_at_utc >= ?1".to_owned(),
    };
    let start = if day.is_some() {
        String::new()
    } else {
        q.range.start(now)
    };
    let tab = match q.tab {
        Tab::All => "",
        Tab::Unrated => " AND e.counts_as_play AND t.id IS NOT NULL AND t.rating IS NULL",
        Tab::Missing => " AND t.id IS NULL",
    };
    let track = "LEFT JOIN item t ON t.id = e.item_id AND t.type = 'track'";
    // Range `all` counts from the rollup (NFR-04, G-17); the page walks the time index newest first.
    let total: i64 = if day.is_none() && super::use_rollup(&start) {
        let n = if q.tab == Tab::Unrated {
            "e.counts_as_play"
        } else {
            "e.events"
        };
        conn.query_row(
            &format!(
                "{} SELECT COALESCE(SUM({n}), 0) FROM base e {track} WHERE 1{tab}",
                rollup(include_private)
            ),
            [],
            |r| r.get(0),
        )?
    } else {
        conn.query_row(
            &format!(
                "{} SELECT COUNT(*) FROM base e {track} WHERE {when}{tab}",
                base(include_private)
            ),
            [&start],
            |r| r.get(0),
        )?
    };
    let from = format!(
        "{} SELECT {{cols}} FROM base e {track}{{album}} WHERE {when}{tab}",
        base(include_private)
    );
    let from = from.replace(
        "{album}",
        "
           LEFT JOIN item a ON a.id = COALESCE(
             (SELECT l.to_id FROM item_link l WHERE l.from_id = t.id AND l.kind = 'on_album' LIMIT 1),
             (SELECT x.id FROM item x WHERE x.id = e.item_id AND x.type = 'album'))",
    );
    let offset = q.offset.max(0);
    let mut st = conn.prepare(&format!(
        "{} ORDER BY e.ended_at_utc DESC, e.id DESC LIMIT ?2 OFFSET ?3",
        from.replace(
            "{cols}",
            "e.id, e.local, e.ms_played, e.counts_as_play, COALESCE(e.skipped, 0),
             COALESCE(e.track_name, ''), COALESCE(e.artist_name, ''), COALESCE(e.album_name, ''),
             e.source, e.spotify_track_uri, t.id, a.rating"
        )
    ))?;
    let rows = st
        .query_map(params![start, q.limit.clamp(1, MAX_PAGE), offset], |r| {
            Ok((
                Play {
                    id: r.get(0)?,
                    ended_at_local: r.get(1)?,
                    ms_played: r.get(2)?,
                    counts_as_play: r.get(3)?,
                    skipped: r.get(4)?,
                    track_name: r.get(5)?,
                    artist_name: r.get(6)?,
                    album_name: r.get(7)?,
                    source: r.get(8)?,
                    spotify_uri: r.get(9)?,
                    item: None,
                    album_rating: r.get(11)?,
                },
                r.get::<_, Option<i64>>(10)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let rows = rows
        .into_iter()
        .map(|(mut p, track)| {
            p.item = track.map(|id| summary(conn, id)).transpose()?;
            Ok(p)
        })
        .collect::<Result<_>>()?;
    Ok(crate::library::Page {
        rows,
        total,
        offset,
    })
}

/// Plays with no record, grouped by `norm_key`, that some record in the library could be:
/// tracks with the same normalized name first, then albums named like the play's album.
/// Groups with no candidate aren't listed (nothing to link them to).
pub fn list_unmatched(conn: &Connection, include_private: bool) -> Result<Vec<Unmatched>> {
    let mut by_name: HashMap<(&str, String), Vec<i64>> = HashMap::new();
    let mut st = conn.prepare(
        "SELECT id, type, display_name FROM item
         WHERE type IN ('track', 'album') AND display_name IS NOT NULL ORDER BY id",
    )?;
    for r in st.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, ItemType>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, kind, name) = r?;
        by_name
            .entry((kind.as_str(), norm(&name)))
            .or_default()
            .push(id);
    }
    let mut st = conn.prepare(&format!(
        "{} SELECT norm_key, MAX(track_name), MAX(artist_name), MAX(album_name),
                   SUM(counts_as_play), 0
            FROM base WHERE item_id IS NULL AND norm_key IS NOT NULL
            GROUP BY norm_key ORDER BY SUM(counts_as_play) DESC, norm_key",
        if super::use_rollup("") {
            rollup(include_private)
        } else {
            base(include_private)
        }
    ))?;
    // The rollup has no event ids: the newest play of each listed group, by the norm_key index.
    let mut newest = conn.prepare(&format!(
        "{} SELECT MAX(id) FROM base WHERE norm_key = ?1 AND item_id IS NULL",
        base(include_private)
    ))?;
    let groups = st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = vec![];
    for (norm_key, track_name, artist_name, album, plays, _) in groups {
        // norm_key = norm(artist) | norm(track)
        let track = norm_key.split_once('|').map_or("", |(_, t)| t).to_owned();
        let none = vec![];
        let tracks = by_name.get(&("track", track)).unwrap_or(&none);
        let albums = by_name.get(&("album", norm(&album))).unwrap_or(&none);
        if tracks.is_empty() && albums.is_empty() {
            continue;
        }
        let detail = match (tracks.len(), albums.is_empty()) {
            (0, _) => format!("You have the album {album}"),
            (1, false) => format!("You have the track and the album {album}"),
            (1, true) => "You have a track with this name".to_owned(),
            (n, _) => format!("{n} tracks in your library have this name"),
        };
        let suggestion = if tracks.is_empty() {
            "Link to the album"
        } else {
            "Link to the track"
        };
        let ids: Vec<i64> = tracks.iter().chain(albums).copied().collect();
        let mut candidates = summaries(
            conn,
            &format!(
                "WHERE i.id IN ({})",
                ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
            ),
            [],
        )?;
        candidates.sort_by_key(|c| ids.iter().position(|id| *id == c.id));
        let event_id = newest.query_row([&norm_key], |r| r.get(0))?;
        out.push(Unmatched {
            norm_key,
            track_name,
            artist_name,
            plays,
            detail,
            suggestion: suggestion.to_owned(),
            event_id,
            candidates,
        });
    }
    Ok(out)
}

/// FR-030 manual link: every unlinked play with the same `norm_key` as `event_id` now counts for
/// `item_id` (a track, or an album when the track isn't in the library). Returns rows changed.
pub fn link_play(conn: &Connection, event_id: i64, item_id: i64) -> Result<usize> {
    let key: Option<String> = conn
        // unfiltered: looks up the one event the user picked, by id
        .query_row(
            "SELECT norm_key FROM listening_event WHERE id = ?1",
            [event_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That play no longer exists."))?;
    let kind: ItemType = conn
        .query_row("SELECT type FROM item WHERE id = ?1", [item_id], |r| {
            r.get(0)
        })
        .optional()?
        .ok_or_else(|| AppError::not_found("That record no longer exists."))?;
    if kind == ItemType::Artist {
        return Err(AppError::validation("Link a play to a track or an album."));
    }
    Ok(conn.execute(
        "UPDATE listening_event SET item_id = ?1
         WHERE (id = ?2 OR (norm_key = ?3 AND item_id IS NULL))",
        params![item_id, event_id, key],
    )?)
}

/// SC-05 listening panel: plays of the record (an album or artist counts its tracks, `SRC`).
pub fn item_listening(
    conn: &Connection,
    id: i64,
    include_private: bool,
    now: DateTime<Utc>,
) -> Result<Sourced<ItemListening>> {
    summary(conn, id)?; // not_found for a gone record
    let mine = format!(
        "{}{SRC}, mine AS (SELECT * FROM base WHERE item_id IN (SELECT s FROM src WHERE t = ?1))",
        base(include_private)
    );
    let (plays, ms_total, last_played_local) = conn.query_row(
        &format!(
            "{mine} SELECT COALESCE(SUM(counts_as_play), 0), COALESCE(SUM(ms_played), 0),
                           MAX(CASE WHEN counts_as_play THEN local END) FROM mine"
        ),
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let mut st = conn.prepare(&format!(
        "{mine} SELECT COUNT(*) FROM mine WHERE counts_as_play AND substr(local, 1, 7) = ?2"
    ))?;
    let by_month = last_months(now, 6)
        .into_iter()
        .map(|month| {
            Ok(MonthPlays {
                plays: st.query_row(params![id, month], |r| r.get(0))?,
                month,
            })
        })
        .collect::<Result<_>>()?;
    sourced(
        conn,
        include_private,
        ItemListening {
            plays,
            ms_total,
            last_played_local,
            by_month,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::super::fixture::{self, ALBUM, ARTIST, T1, T2, TWIN_A, TWIN_B};
    use super::*;
    use crate::error::Code;

    fn q(tab: Tab, range: Range) -> PlayQuery {
        PlayQuery {
            tab,
            range,
            offset: 0,
            limit: 50,
            date: None,
        }
    }

    /// M10: `date` keeps that local day's plays only and replaces `range`; a bad date is refused.
    #[test]
    fn list_plays_for_one_day() {
        let (c, ids) = fixture::build();
        let now = fixture::now();
        let day = |d: &str| PlayQuery {
            date: Some(d.into()),
            ..q(Tab::All, Range::Week)
        };
        let p = list_plays(&c, &day("2026-10-17"), false, now).unwrap();
        assert_eq!(
            (p.total, p.rows.iter().map(|r| r.id).collect::<Vec<_>>()),
            (1, vec![ids[1]])
        );
        // e6 is in 2025: outside the 7-day range, inside its day
        let p = list_plays(&c, &day("2025-01-10"), false, now).unwrap();
        assert_eq!(
            (p.total, p.rows.iter().map(|r| r.id).collect::<Vec<_>>()),
            (1, vec![ids[5]])
        );
        assert_eq!(
            list_plays(&c, &day("2026-10-01"), false, now)
                .unwrap()
                .total,
            0
        );
        for bad in ["2026-13-01", "yesterday", "2026-10-1"] {
            assert_eq!(
                list_plays(&c, &day(bad), false, now).unwrap_err().code,
                Code::Validation
            );
        }
    }

    #[test]
    fn list_plays_tabs_and_album_rating() {
        let (c, ids) = fixture::build();
        let now = fixture::now();
        let p = list_plays(&c, &q(Tab::All, Range::Week), false, now).unwrap();
        // e1, e2, e3 (e4 reconciled, e5 private)
        assert_eq!(p.total, 3);
        assert_eq!(
            p.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![ids[0], ids[1], ids[2]]
        );
        let e1 = &p.rows[0];
        assert_eq!(e1.item.as_ref().map(|i| i.id), Some(T1));
        assert_eq!(e1.album_rating, Some(4.0)); // Album One, G-7
        assert!(e1.counts_as_play && !e1.skipped && e1.source == "export");
        assert_eq!(e1.ended_at_local, "2026-10-18T10:00:00");
        assert!(p.rows[1].skipped);
        let missing = list_plays(&c, &q(Tab::Missing, Range::Week), false, now).unwrap();
        assert_eq!(
            missing.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![ids[2]]
        );
        let unrated = list_plays(&c, &q(Tab::Unrated, Range::Week), false, now).unwrap();
        assert_eq!(unrated.total, 2);
        // private shows with the setting; paging
        let all = list_plays(&c, &q(Tab::All, Range::All), true, now).unwrap();
        assert_eq!(all.total, 8);
        let page = list_plays(
            &c,
            &PlayQuery {
                offset: 7,
                ..q(Tab::All, Range::All)
            },
            true,
            now,
        )
        .unwrap();
        assert_eq!((page.rows.len(), page.offset, page.total), (1, 7, 8));
        assert_eq!(page.rows[0].id, ids[5]); // oldest: e6 (2025)
    }

    #[test]
    fn play_linked_to_album_has_no_item_but_album_rating() {
        let (c, ids) = fixture::build();
        link_play(&c, ids[2], ALBUM).unwrap();
        let p = list_plays(&c, &q(Tab::Missing, Range::Week), false, fixture::now()).unwrap();
        assert_eq!(p.rows[0].id, ids[2]);
        assert_eq!(
            (p.rows[0].item.is_none(), p.rows[0].album_rating),
            (true, Some(4.0))
        );
    }

    #[test]
    fn unmatched_groups_and_link() {
        let (c, ids) = fixture::build();
        let u = list_unmatched(&c, false).unwrap();
        // "Other Song" / "Secret" have no candidate record; "Twin" has two tracks and Album One
        assert_eq!(u.len(), 1);
        let t = &u[0];
        assert_eq!(
            (t.norm_key.as_str(), t.plays, t.event_id),
            ("nobody|twin", 1, ids[8])
        );
        assert_eq!(
            t.candidates.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![TWIN_A, TWIN_B, ALBUM]
        );
        assert_eq!(t.detail, "2 tracks in your library have this name");
        assert_eq!(t.suggestion, "Link to the track");

        assert_eq!(link_play(&c, t.event_id, TWIN_B).unwrap(), 2); // e8 and e9
        assert!(list_unmatched(&c, false).unwrap().is_empty());
        assert_eq!(
            item_listening(&c, TWIN_B, false, fixture::now())
                .unwrap()
                .data
                .plays,
            1
        );
        assert_eq!(
            link_play(&c, 9999, TWIN_A).unwrap_err().code,
            Code::NotFound
        );
        assert_eq!(
            link_play(&c, ids[0], 9999).unwrap_err().code,
            Code::NotFound
        );
        assert_eq!(
            link_play(&c, ids[0], ARTIST).unwrap_err().code,
            Code::Validation
        );
    }

    #[test]
    fn album_track_list_carries_track_plays() {
        // G-1: Song One e1, e2, e7 (e4 reconciled); Song Two e6; Song Three none
        let (c, _) = fixture::build();
        let plays = |p| {
            let tl = crate::library::items::detail(&c, ALBUM, p)
                .unwrap()
                .track_list
                .unwrap();
            tl.iter().map(|t| t.plays).collect::<Vec<i64>>()
        };
        assert_eq!(plays(false), vec![3, 1, 0]);
        // Song Two's play becomes private: hidden unless the setting is on (D24, TSD §4.8)
        c.execute(
            "UPDATE listening_event SET private_session = 1 WHERE item_id = ?1",
            [T2],
        )
        .unwrap();
        assert_eq!(plays(false), vec![3, 0, 0]);
        assert_eq!(plays(true), vec![3, 1, 0]);
    }

    #[test]
    fn item_listening_counts_album_and_artist_tracks() {
        let (c, _) = fixture::build();
        let now = fixture::now();
        let t1 = item_listening(&c, T1, false, now).unwrap();
        assert_eq!(t1.data.plays, 3); // e1, e2, e7; e4 reconciled
        assert_eq!(t1.data.ms_total, 390_000);
        assert_eq!(
            t1.data.last_played_local.as_deref(),
            Some("2026-10-18T10:00:00")
        );
        assert_eq!(t1.data.by_month.last().unwrap().plays, 3);
        assert_eq!(item_listening(&c, ALBUM, false, now).unwrap().data.plays, 4);
        assert_eq!(
            item_listening(&c, ARTIST, false, now).unwrap().data.plays,
            4
        );
        assert_eq!(item_listening(&c, T2, false, now).unwrap().data.plays, 1);
        assert_eq!(
            item_listening(&c, 9999, false, now).unwrap_err().code,
            Code::NotFound
        );
    }
}

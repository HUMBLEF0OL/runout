//! `get_genres_decades` (API.md §4.1, design M10 §7): the Genres & decades tab. Listening time from
//! sources S and E (the v3 `ev` view, private sessions per the setting) labelled with source M answers
//! from the 0012 `enrich_*` tables. Definitions:
//! - an artist's genre = its most-voted genre (ties: by name); a genre's share = its artists' time ÷
//!   the period's time. `genre_count` = distinct genres of the period's artists (any rank).
//! - an album's year = its release group's first release; `unknown_year_pct` = time with no year.
//! - average age = the period's last year − the time-weighted mean release year.
//! - released this year = time on albums first released in the period's last year ÷ time with a year.
//! - country shares are of the time whose artist has a country; coverage shares are of all the time.
//!
//! "same name" artists carry their top hit's labels and count in the shares; `coverage.same_name_pct`
//! is that part of the time, which the UI shows as "probably" (kb T6 Reading).
//! Range `all` reads the 0005 `listening_rollup` (NFR-04, TSD §4.13); the others the `ev` view.
use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, TimeZone};
use serde::Serialize;

use crate::error::Result;
use crate::insights::v3::{Ctx, Grain, PeriodQuery, between, resolve, view_coverage, view_model};
use crate::insights::v3::{Src, month};
use crate::insights::{Covered, coverage, pct};

pub const TOP_GENRES: usize = 10;
pub const MONTH_GENRES: usize = 6;
pub const MONTHS: u32 = 24;
pub const TOP_COUNTRIES: usize = 10;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Share {
    pub name: String,
    pub share_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Decade {
    pub decade: i32,
    pub share_pct: f64,
    pub top_album: Option<AlbumRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AlbumRef {
    pub name: String,
    pub by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Age {
    pub years: i32,
    pub year: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GenreRow {
    pub genre: String,
    pub shares: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GenreMonths {
    pub months: Vec<String>,
    pub rows: Vec<GenreRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EnrichCoverage {
    pub artists_genre_pct: f64,
    pub albums_year_pct: f64,
    pub artists_country_pct: f64,
    pub same_name_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lookups {
    pub on: bool,
    pub contact_set: bool,
    pub pending: usize,
    pub last_lookup: Option<String>,
}

/// API.md §4.1 `GenresDecades`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GenresDecades {
    pub lookups: Lookups,
    pub attribution: &'static str,
    pub genre_count: usize,
    pub top_genres: Vec<Share>,
    pub genre_months: GenreMonths,
    pub decades: Vec<Decade>,
    pub unknown_year_pct: f64,
    pub average_age: Option<Age>,
    pub released_this_year_pct: Option<f64>,
    pub countries: Vec<Share>,
    pub coverage: EnrichCoverage,
}

struct Artist {
    same_name: bool,
    country: Option<String>,
    /// Most voted first.
    genres: Vec<String>,
}

fn artists(conn: &rusqlite::Connection) -> Result<HashMap<String, Artist>> {
    let mut out: HashMap<String, Artist> = conn
        .prepare("SELECT artist, match, country FROM enrich_artist")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Artist {
                    same_name: r.get::<_, String>(1)? == "same_name",
                    country: r.get(2)?,
                    genres: vec![],
                },
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut st = conn.prepare(
        "SELECT artist, genre FROM enrich_artist_genre ORDER BY artist, votes DESC, genre",
    )?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (a, g) = row?;
        if let Some(x) = out.get_mut(&a) {
            x.genres.push(g);
        }
    }
    Ok(out)
}

fn years(conn: &rusqlite::Connection) -> Result<HashMap<(String, String), i32>> {
    Ok(conn
        .prepare("SELECT artist, album, first_year FROM enrich_album WHERE first_year IS NOT NULL")?
        .query_map([], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

/// Largest shares first (ties by name), at most `n`.
fn top(map: HashMap<String, i64>, whole: i64, n: usize) -> Vec<Share> {
    let mut v: Vec<(String, i64)> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.into_iter()
        .take(n)
        .map(|(name, ms)| Share {
            name,
            share_pct: pct(ms, whole),
        })
        .collect()
}

pub fn lookups(conn: &rusqlite::Connection) -> Result<Lookups> {
    Ok(Lookups {
        on: super::is_on(),
        contact_set: super::CONTACT.is_some(),
        pending: super::pending(conn)?.len(),
        last_lookup: conn.query_row(
            "SELECT MAX(t) FROM (SELECT MAX(looked_up_at) AS t FROM enrich_artist
                                 UNION ALL SELECT MAX(looked_up_at) FROM enrich_album)",
            [],
            |r| r.get(0),
        )?,
    })
}

pub fn genres_decades<Tz: TimeZone>(
    cx: &Ctx<Tz>,
    q: &PeriodQuery,
) -> Result<Covered<GenresDecades>> {
    let today = cx.today();
    let first = coverage::first_day(cx.conn)?;
    let r = resolve(q, today, first)?;
    let span = r.cur;
    let model = view_model(cx, span.0, span.1)?;
    let cov = view_coverage(&model, span, today);
    let ref_year = span.1.min(today).year();

    let arts = artists(cx.conn)?;
    let yrs = years(cx.conn)?;

    // The period's time per (artist, album), with the names as played for "most played album".
    // All time: the rollup holds the same rows (not reconciled, private apart) already summed.
    let sql = if q.grain == Grain::All {
        format!(
            "SELECT lower(artist_name), lower(album_name), MIN(artist_name), MIN(album_name),
                    SUM(ms_played)
             FROM listening_rollup WHERE events > 0{} GROUP BY 1, 2",
            cx.private_filter("private_session")
        )
    } else {
        format!(
            "{} SELECT lower(IFNULL(artist_name, '')), lower(IFNULL(album_name, '')),
                      MIN(IFNULL(artist_name, '')), MIN(IFNULL(album_name, '')), SUM(ms_played)
             FROM ev WHERE {} GROUP BY 1, 2",
            Src::All.cte(cx.include_private),
            between(span)
        )
    };
    let mut st = cx.conn.prepare(&sql)?;
    let rows = st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let total: i64 = rows.iter().map(|x| x.4).sum();
    let (mut by_genre, mut by_country) = (HashMap::new(), HashMap::new());
    let mut all_genres = HashSet::new();
    let (mut genre_ms, mut country_ms, mut same_ms, mut year_ms, mut this_year_ms) =
        (0, 0, 0, 0, 0);
    let mut year_sum = 0f64;
    // decade → (ms, best album (ms, name, by))
    let mut decades: BTreeMap<i32, (i64, (i64, String, String))> = BTreeMap::new();
    let mut albums: HashMap<(String, String), (i64, i32, String, String)> = HashMap::new();

    for (artist, album, artist_name, album_name, ms) in &rows {
        if let Some(a) = arts.get(artist) {
            same_ms += if a.same_name { *ms } else { 0 };
            if let Some(g) = a.genres.first() {
                *by_genre.entry(g.clone()).or_insert(0) += ms;
                genre_ms += ms;
                all_genres.extend(a.genres.iter().cloned());
            }
            if let Some(c) = &a.country {
                *by_country.entry(c.clone()).or_insert(0) += ms;
                country_ms += ms;
            }
        }
        if let Some(&y) = yrs.get(&(artist.clone(), album.clone())) {
            year_ms += ms;
            year_sum += y as f64 * *ms as f64;
            this_year_ms += if y == ref_year { *ms } else { 0 };
            let e = albums.entry((artist.clone(), album.clone())).or_insert((
                0,
                y,
                album_name.clone(),
                artist_name.clone(),
            ));
            e.0 += ms;
        }
    }
    for (ms, y, name, by) in albums.into_values() {
        let d = decades
            .entry(y.div_euclid(10) * 10)
            .or_insert((0, (0, String::new(), String::new())));
        d.0 += ms;
        if (ms, &name) > (d.1.0, &d.1.1) {
            d.1 = (ms, name, by);
        }
    }
    let mean = (year_ms > 0).then(|| (year_sum / year_ms as f64).round() as i32);

    let data = GenresDecades {
        lookups: lookups(cx.conn)?,
        attribution: super::ATTRIBUTION,
        genre_count: all_genres.len(),
        top_genres: top(by_genre, total, TOP_GENRES),
        genre_months: genre_months(cx, &arts)?,
        decades: decades
            .into_iter()
            .map(|(decade, (ms, (_, name, by)))| Decade {
                decade,
                share_pct: pct(ms, total),
                top_album: Some(AlbumRef { name, by }),
            })
            .collect(),
        unknown_year_pct: pct(total - year_ms, total),
        average_age: mean.map(|year| Age {
            years: ref_year - year,
            year,
        }),
        released_this_year_pct: (year_ms > 0).then(|| pct(this_year_ms, year_ms)),
        countries: top(by_country, country_ms, TOP_COUNTRIES),
        coverage: EnrichCoverage {
            artists_genre_pct: pct(genre_ms, total),
            albums_year_pct: pct(year_ms, total),
            artists_country_pct: pct(country_ms, total),
            same_name_pct: pct(same_ms, total),
        },
    };
    let labelled = genre_ms + year_ms + country_ms > 0;
    let mut out = crate::insights::v3::covered(cx, data, cov, false)?;
    if labelled {
        out.sourced.sources.push("M");
    }
    Ok(out)
}

/// Genres over the last [`MONTHS`] months to today: the [`MONTH_GENRES`] genres with the most time,
/// each as % of that month's time (0008 `listening_month_artist`).
fn genre_months<Tz: TimeZone>(cx: &Ctx<Tz>, arts: &HashMap<String, Artist>) -> Result<GenreMonths> {
    let months: Vec<String> = (0..MONTHS)
        .rev()
        .filter_map(|k| {
            cx.today()
                .with_day(1)
                .and_then(|d| d.checked_sub_months(chrono::Months::new(k)))
        })
        .map(month)
        .collect();
    let idx: HashMap<&str, usize> = months
        .iter()
        .enumerate()
        .map(|(i, m)| (m.as_str(), i))
        .collect();
    let mut st = cx.conn.prepare(&format!(
        "SELECT month, artist, SUM(ms_played) FROM listening_month_artist
         WHERE month >= ?1 AND events > 0{} GROUP BY 1, 2",
        cx.private_filter("private_session")
    ))?;
    let rows = st.query_map([&months[0]], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    let mut month_ms = vec![0i64; months.len()];
    let mut per: HashMap<String, Vec<i64>> = HashMap::new();
    for row in rows {
        let (m, artist, ms) = row?;
        let Some(&i) = idx.get(m.as_str()) else {
            continue;
        };
        month_ms[i] += ms;
        if let Some(g) = arts.get(&artist).and_then(|a| a.genres.first()) {
            per.entry(g.clone())
                .or_insert_with(|| vec![0; months.len()])[i] += ms;
        }
    }
    let mut g: Vec<(String, Vec<i64>)> = per.into_iter().collect();
    g.sort_by(|a, b| {
        let s = |x: &Vec<i64>| x.iter().sum::<i64>();
        s(&b.1).cmp(&s(&a.1)).then_with(|| a.0.cmp(&b.0))
    });
    let rows = g
        .into_iter()
        .take(MONTH_GENRES)
        .map(|(genre, ms)| GenreRow {
            genre,
            shares: ms.iter().zip(&month_ms).map(|(m, w)| pct(*m, *w)).collect(),
        })
        .collect();
    Ok(GenreMonths { months, rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::events::Source;
    use crate::insights::fixture::{Ev, put};
    use crate::insights::terms::Compare;
    use chrono::Utc;

    fn ev<'a>(utc: &'a str, ms: i64, artist: &'a str, album: &'a str) -> Ev<'a> {
        Ev {
            source: Source::Export,
            utc,
            ms,
            track: "t",
            artist,
            album,
            item: None,
            platform: "windows",
            skipped: false,
            private: false,
        }
    }

    #[test]
    fn hand_computed_genres_decades() {
        let c = crate::db::open_in_memory().unwrap();
        // October 2026, total 100 min: A 40 (rock, GB, 1995 album), B 30 (jazz, US, 2026 album),
        // T 20 (same name: top hit's folk / SE, "probably"), N 10 (no answer yet). One September play outside the period.
        for (utc, ms, ar, al) in [
            ("2026-10-02T10:00:00Z", 40, "Band A", "Old One"),
            ("2026-10-03T10:00:00Z", 30, "Band B", "New One"),
            ("2026-10-04T10:00:00Z", 20, "Twin", "Twin LP"),
            ("2026-10-05T10:00:00Z", 10, "Nobody", "Unknown"),
            ("2026-09-05T10:00:00Z", 500, "Band B", "New One"),
        ] {
            put(&c, ev(utc, ms * 60_000, ar, al));
        }
        c.execute_batch(
            "INSERT INTO enrich_artist VALUES ('band a', 'matched', 'musicbrainz', 'GB', 'x'),
               ('band b', 'matched', 'wikidata', 'US', 'x'), ('twin', 'same_name', 'musicbrainz', 'SE', 'x');
             INSERT INTO enrich_artist_genre VALUES ('band a', 'rock', 5), ('band a', 'britpop', 2),
               ('band b', 'jazz', 1), ('band b', 'soul', 1), ('twin', 'folk', 3);
             INSERT INTO enrich_album VALUES ('band a', 'old one', 'matched', 1995, 'x'),
               ('band b', 'new one', 'matched', 2026, 'y');",
        )
        .unwrap();
        let cx = Ctx {
            conn: &c,
            include_private: false,
            tz: &Utc,
            now: crate::insights::fixture::now(), // 19 Oct 2026
        };
        let q = PeriodQuery {
            grain: Grain::Month,
            vs: Compare::Prev,
            at: None,
            from: None,
            to: None,
        };
        let out = genres_decades(&cx, &q).unwrap();
        let d = &out.sourced.data;
        assert!(out.sourced.sources.contains(&"M") && !out.sourced.sources.contains(&"U"));
        assert_eq!(d.attribution, "Genres from MusicBrainz, CC BY-NC-SA 3.0");
        // primary genres: rock (A, 40), jazz (B 30; jazz < soul by name at equal votes), folk (T 20)
        let s = |n: &str, p: f64| Share {
            name: n.into(),
            share_pct: p,
        };
        assert_eq!(
            d.top_genres,
            [s("rock", 40.0), s("jazz", 30.0), s("folk", 20.0)]
        );
        assert_eq!(d.genre_count, 5);
        // decades: 1990s 40 %, 2020s 30 %, unknown 30 %
        assert_eq!(
            d.decades
                .iter()
                .map(|x| (x.decade, x.share_pct))
                .collect::<Vec<_>>(),
            [(1990, 40.0), (2020, 30.0)]
        );
        assert_eq!(
            d.decades[0].top_album,
            Some(AlbumRef {
                name: "Old One".into(),
                by: "Band A".into()
            })
        );
        assert_eq!(d.unknown_year_pct, 30.0);
        // mean year (1995·40 + 2026·30) / 70 = 2008.29 → 2008; age 2026 − 2008
        assert_eq!(
            d.average_age,
            Some(Age {
                years: 18,
                year: 2008
            })
        );
        assert_eq!(d.released_this_year_pct, Some(42.9)); // 30 / 70
        // countries: of the 90 min with a country
        assert_eq!(d.countries, [s("GB", 44.4), s("US", 33.3), s("SE", 22.2)]);
        assert_eq!(
            d.coverage,
            EnrichCoverage {
                artists_genre_pct: 90.0,
                albums_year_pct: 70.0,
                artists_country_pct: 90.0,
                same_name_pct: 20.0
            }
        );
        // genres over time: September is all jazz (B), October 40 % rock, 30 % jazz
        let gm = &d.genre_months;
        assert_eq!(gm.months.len(), 24);
        assert_eq!(gm.months.last().unwrap(), "2026-10");
        let row = |g: &str| &gm.rows.iter().find(|r| r.genre == g).unwrap().shares;
        assert_eq!(row("jazz")[22..], [100.0, 30.0]);
        assert_eq!(row("rock")[22..], [0.0, 40.0]);
        assert_eq!(gm.rows[0].genre, "jazz"); // most time over the window
        // Nobody, Twin LP and Unknown are still to look up; Twin is done (same name)
        assert_eq!(d.lookups.pending, 1 + 2);
        assert_eq!(d.lookups.last_lookup.as_deref(), Some("y"));

        // all time reads the rollup: September's 500 min of B join in (600 in all)
        let all = genres_decades(
            &cx,
            &PeriodQuery {
                grain: Grain::All,
                ..q
            },
        )
        .unwrap()
        .sourced
        .data;
        assert_eq!(
            all.top_genres,
            [s("jazz", 88.3), s("rock", 6.7), s("folk", 3.3)]
        );
        assert_eq!(all.coverage.same_name_pct, 3.3);
        assert_eq!(all.unknown_year_pct, 5.0);
    }

    #[test]
    fn nothing_looked_up_has_no_source_m() {
        let c = crate::db::open_in_memory().unwrap();
        put(&c, ev("2026-10-02T10:00:00Z", 1000, "Band A", "Old One"));
        let cx = Ctx {
            conn: &c,
            include_private: false,
            tz: &Utc,
            now: crate::insights::fixture::now(),
        };
        let q = PeriodQuery {
            grain: Grain::All,
            vs: Compare::Prev,
            at: None,
            from: None,
            to: None,
        };
        let out = genres_decades(&cx, &q).unwrap();
        assert!(!out.sourced.sources.contains(&"M"));
        let d = &out.sourced.data;
        assert!(d.top_genres.is_empty() && d.decades.is_empty() && d.average_age.is_none());
        assert_eq!(d.unknown_year_pct, 100.0);
    }
}

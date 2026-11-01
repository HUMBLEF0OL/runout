//! Names-only lookups (TSD §5.4, kb/open-music-db.md): MusicBrainz first, Wikidata when MusicBrainz has
//! nothing confident. Each function takes an [`Http`] so tests run on stub answers; the real one
//! (`super::Net`) goes through `net.rs` at ≤ 1 request/s. What goes out is the name and nothing else.
use reqwest::Url;
use serde_json::Value;

use crate::importer::normalize::norm;

/// What a lookup could not finish: offline / timeout, or the host is busy or failing (any 5xx, 429,
/// Wikidata maxlag). Nothing is stored; the worker retries later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Halt {
    Offline,
    Busy,
}

/// One GET to an allowed host: `(status, body)` or a [`Halt`].
pub trait Http {
    fn get(&mut self, url: &str) -> Result<(u16, String), Halt>;
}

/// The 0012 `match` column; same_name is kept as its own class (kb gotcha: name-only search can't pick between
/// artists who share a name): it keeps the top hit's labels, shown as "probably" (kb T6 Reading).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match {
    Matched,
    SameName,
    None,
}

impl Match {
    pub fn as_str(self) -> &'static str {
        match self {
            Match::Matched => "matched",
            Match::SameName => "same_name",
            Match::None => "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtistInfo {
    pub matched: Match,
    pub via: Option<&'static str>,
    pub country: Option<String>,
    /// `(genre, votes)`, most voted first.
    pub genres: Vec<(String, i64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumInfo {
    pub matched: Match,
    pub first_year: Option<i32>,
}

/// kb "confident": the top hit's score is at least this.
pub const MIN_SCORE: i64 = 90;
/// Genres kept per artist (most voted).
pub const MAX_GENRES: usize = 5;
const MB: &str = "https://musicbrainz.org/ws/2/";
const WD: &str = "https://www.wikidata.org/w/api.php";

/// Name folded for comparison: case, accents, version suffixes (`norm`) and punctuation.
pub fn fold(s: &str) -> String {
    norm(s)
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A Lucene phrase: the name in quotes with `\` and `"` escaped (other specials are literal inside quotes).
fn phrase(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn url(base: &str, params: &[(&str, &str)]) -> String {
    Url::parse_with_params(base, params)
        .expect("constant base URL")
        .to_string()
}

pub fn artist_search_url(name: &str) -> String {
    url(
        &format!("{MB}artist"),
        &[
            ("query", &format!("artist:{}", phrase(name))),
            ("limit", "5"),
            ("fmt", "json"),
        ],
    )
}

pub fn album_search_url(album: &str, artist: &str) -> String {
    url(
        &format!("{MB}release-group"),
        &[
            (
                "query",
                &format!(
                    "releasegroup:{} AND artist:{}",
                    phrase(album),
                    phrase(artist)
                ),
            ),
            ("limit", "5"),
            ("fmt", "json"),
        ],
    )
}

/// The MBID is from a MusicBrainz answer; only hex digits and dashes are kept, so nothing else rides along.
fn artist_genres_url(mbid: &str) -> String {
    let id: String = mbid
        .chars()
        .filter(|c| c.is_ascii_hexdigit() || *c == '-')
        .collect();
    format!("{MB}artist/{id}?inc=genres&fmt=json")
}

fn wd(params: &[(&str, &str)]) -> String {
    let mut p = params.to_vec();
    p.extend([("format", "json"), ("maxlag", "5")]);
    url(WD, &p)
}

/// GET + JSON. Any 5xx or 429 → `Busy` (MusicBrainz refuses everything above 1 request/s with 503; a
/// gateway error is brief too), so nothing is stored; 4xx → `Ok(None)` (a bad query for this name,
/// stored as a miss); Wikidata's maxlag error → `Busy`.
fn json(http: &mut impl Http, url: &str) -> Result<Option<Value>, Halt> {
    let (status, body) = http.get(url)?;
    if status >= 500 || status == 429 {
        return Err(Halt::Busy);
    }
    if !(200..300).contains(&status) {
        return Ok(None);
    }
    let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    if v["error"]["code"] == "maxlag" {
        return Err(Halt::Busy);
    }
    Ok(Some(v))
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// kb rule on a MusicBrainz artist search: the top hit is an exact name or alias match with score ≥ 90
/// → matched, unless another hit has the same name → same_name (still the top hit, labelled "probably").
/// Else none (`None` hit).
pub fn classify_artists<'a>(name: &str, res: &'a Value) -> (Match, Option<&'a Value>) {
    let want = fold(name);
    let hits = res["artists"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let named = |a: &Value| fold(s(&a["name"])) == want;
    let exact = |a: &Value| {
        named(a)
            || a["aliases"]
                .as_array()
                .is_some_and(|al| al.iter().any(|x| fold(s(&x["name"])) == want))
    };
    match hits.first() {
        Some(top) if top["score"].as_i64().unwrap_or(0) >= MIN_SCORE && exact(top) => {
            let m = if hits[1..].iter().any(named) {
                Match::SameName
            } else {
                Match::Matched
            };
            (m, Some(top))
        }
        _ => (Match::None, None),
    }
}

/// `(genre, votes)` from `inc=genres`, most voted first (ties by name), at most [`MAX_GENRES`].
pub fn genres(v: &Value) -> Vec<(String, i64)> {
    let mut g: Vec<(String, i64)> = v["genres"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|g| !s(&g["name"]).is_empty())
        .map(|g| (s(&g["name"]).to_string(), g["count"].as_i64().unwrap_or(0)))
        .collect();
    g.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    g.truncate(MAX_GENRES);
    g
}

/// An ISO 3166-1 alpha-2 code, or nothing.
fn iso2(c: &str) -> Option<String> {
    (c.len() == 2 && c.bytes().all(|b| b.is_ascii_uppercase())).then(|| c.to_string())
}

/// Artist: MusicBrainz search, then its genres; Wikidata when MusicBrainz has no confident hit.
pub fn artist(http: &mut impl Http, name: &str) -> Result<ArtistInfo, Halt> {
    let Some(res) = json(http, &artist_search_url(name))? else {
        return Ok(miss());
    };
    match classify_artists(name, &res) {
        (m @ (Match::Matched | Match::SameName), Some(top)) => {
            let country = iso2(s(&top["country"]));
            let genres = match json(http, &artist_genres_url(s(&top["id"])))? {
                Some(v) => genres(&v),
                None => vec![],
            };
            Ok(ArtistInfo {
                matched: m,
                via: Some("musicbrainz"),
                country,
                genres,
            })
        }
        _ => wikidata_artist(http, name),
    }
}

fn miss() -> ArtistInfo {
    ArtistInfo {
        matched: Match::None,
        via: None,
        country: None,
        genres: vec![],
    }
}

/// QIDs of a claim's values (`entity` datavalues).
fn qids(claims: &Value, prop: &str) -> Vec<String> {
    claims[prop]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["mainsnak"]["datavalue"]["value"]["id"].as_str())
        .map(String::from)
        .collect()
}

/// Wikidata fallback (kb): candidates whose label is the name and that look like music (a genre P136 or
/// a MusicBrainz id P434). One → matched (genres P136, country P495 else P27 as ISO code P297); more
/// → same_name with the first (search-ranked) one's labels; none → none.
pub fn wikidata_artist(http: &mut impl Http, name: &str) -> Result<ArtistInfo, Halt> {
    let want = fold(name);
    let Some(found) = json(
        http,
        &wd(&[
            ("action", "wbsearchentities"),
            ("search", name),
            ("language", "en"),
            ("type", "item"),
            ("limit", "5"),
        ]),
    )?
    else {
        return Ok(miss());
    };
    let ids: Vec<&str> = found["search"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| fold(s(&c["label"])) == want)
        .filter_map(|c| c["id"].as_str())
        .filter(|id| id.starts_with('Q') && id[1..].bytes().all(|b| b.is_ascii_digit()))
        .collect();
    if ids.is_empty() {
        return Ok(miss());
    }
    let Some(ents) = json(
        http,
        &wd(&[
            ("action", "wbgetentities"),
            ("ids", &ids.join("|")),
            ("props", "claims"),
        ]),
    )?
    else {
        return Ok(miss());
    };
    let music: Vec<&Value> = ids
        .iter()
        .map(|id| &ents["entities"][id]["claims"])
        .filter(|c| c.get("P136").is_some() || c.get("P434").is_some())
        .collect();
    let (matched, claims) = match music.as_slice() {
        [] => return Ok(miss()),
        [one] => (Match::Matched, *one),
        [first, ..] => (Match::SameName, *first),
    };
    let genre_ids: Vec<String> = qids(claims, "P136").into_iter().take(MAX_GENRES).collect();
    let mut genres = vec![];
    if !genre_ids.is_empty()
        && let Some(v) = json(
            http,
            &wd(&[
                ("action", "wbgetentities"),
                ("ids", &genre_ids.join("|")),
                ("props", "labels"),
                ("languages", "en"),
            ]),
        )?
    {
        // Wikidata has no votes: listed order, first = most.
        let n = genre_ids.len() as i64;
        for (i, id) in genre_ids.iter().enumerate() {
            let l = s(&v["entities"][id]["labels"]["en"]["value"]);
            if !l.is_empty() {
                genres.push((l.to_string(), n - i as i64));
            }
        }
    }
    let place = qids(claims, "P495")
        .into_iter()
        .next()
        .or_else(|| qids(claims, "P27").into_iter().next());
    let mut country = None;
    if let Some(q) = place
        && let Some(v) = json(
            http,
            &wd(&[
                ("action", "wbgetclaims"),
                ("entity", &q),
                ("property", "P297"),
            ]),
        )?
    {
        country = iso2(s(&v["claims"]["P297"][0]["mainsnak"]["datavalue"]["value"]));
    }
    Ok(ArtistInfo {
        matched,
        via: Some("wikidata"),
        country,
        genres,
    })
}

/// Album: MusicBrainz release-group search by album and artist name. Confident = top hit score ≥ 90,
/// same title and an artist credit with the artist's name. Other exact hits with a different first
/// year → same_name (can't tell which; keeps the top hit's year, shown as "probably"). The year is the release group's first release, so remasters
/// count by their original year.
/// MusicBrainz has nothing confident → [`wikidata_album`].
pub fn album(http: &mut impl Http, album: &str, artist: &str) -> Result<AlbumInfo, Halt> {
    if let Some(res) = json(http, &album_search_url(album, artist))? {
        let got = classify_album(album, artist, &res);
        if got.matched != Match::None {
            return Ok(got);
        }
    }
    wikidata_album(http, album, artist)
}

/// Wikidata album fallback (design §7): candidates labelled with the album name that have a
/// publication date P577 and a performer P175 whose English label is the artist. One → matched;
/// several with different years → same_name (first search-ranked one's year); none → none.
pub fn wikidata_album(http: &mut impl Http, album: &str, artist: &str) -> Result<AlbumInfo, Halt> {
    let none = AlbumInfo {
        matched: Match::None,
        first_year: None,
    };
    let want = fold(album);
    let Some(found) = json(
        http,
        &wd(&[
            ("action", "wbsearchentities"),
            ("search", album),
            ("language", "en"),
            ("type", "item"),
            ("limit", "5"),
        ]),
    )?
    else {
        return Ok(none);
    };
    let ids: Vec<&str> = found["search"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| fold(s(&c["label"])) == want)
        .filter_map(|c| c["id"].as_str())
        .filter(|id| id.starts_with('Q') && id[1..].bytes().all(|b| b.is_ascii_digit()))
        .collect();
    if ids.is_empty() {
        return Ok(none);
    }
    let Some(ents) = json(
        http,
        &wd(&[
            ("action", "wbgetentities"),
            ("ids", &ids.join("|")),
            ("props", "claims"),
        ]),
    )?
    else {
        return Ok(none);
    };
    // (year, performer QIDs) of candidates with a date; "+1997-05-21T00:00:00Z" → 1997
    let cands: Vec<(Option<i32>, Vec<String>)> = ids
        .iter()
        .map(|id| &ents["entities"][id]["claims"])
        .filter(|c| c.get("P577").is_some())
        .map(|c| {
            let t = s(&c["P577"][0]["mainsnak"]["datavalue"]["value"]["time"]);
            let y = t.get(1..5).and_then(|y| y.parse::<i32>().ok());
            (y, qids(c, "P175"))
        })
        .filter(|(_, p)| !p.is_empty())
        .collect();
    let mut people: Vec<&str> = cands
        .iter()
        .flat_map(|(_, p)| p)
        .map(String::as_str)
        .collect();
    people.sort_unstable();
    people.dedup();
    people.truncate(50);
    if people.is_empty() {
        return Ok(none);
    }
    let Some(labels) = json(
        http,
        &wd(&[
            ("action", "wbgetentities"),
            ("ids", &people.join("|")),
            ("props", "labels"),
            ("languages", "en"),
        ]),
    )?
    else {
        return Ok(none);
    };
    let by = fold(artist);
    let years: Vec<Option<i32>> = cands
        .iter()
        .filter(|(_, p)| {
            p.iter()
                .any(|q| fold(s(&labels["entities"][q]["labels"]["en"]["value"])) == by)
        })
        .map(|(y, _)| *y)
        .collect();
    Ok(match years.as_slice() {
        [] => none,
        [first, rest @ ..] => AlbumInfo {
            matched: if rest.iter().any(|y| y != first) {
                Match::SameName
            } else {
                Match::Matched
            },
            first_year: *first,
        },
    })
}

pub fn classify_album(album: &str, artist: &str, res: &Value) -> AlbumInfo {
    let (want, by) = (fold(album), fold(artist));
    let year = |g: &Value| {
        s(&g["first-release-date"])
            .get(..4)
            .and_then(|y| y.parse::<i32>().ok())
    };
    let exact = |g: &Value| {
        fold(s(&g["title"])) == want
            && g["artist-credit"].as_array().is_some_and(|ac| {
                ac.iter()
                    .any(|c| fold(s(&c["name"])) == by || fold(s(&c["artist"]["name"])) == by)
            })
    };
    let hits = res["release-groups"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let none = AlbumInfo {
        matched: Match::None,
        first_year: None,
    };
    let Some(top) = hits.first() else {
        return none;
    };
    if top["score"].as_i64().unwrap_or(0) < MIN_SCORE || !exact(top) {
        return none;
    }
    let y = year(top);
    if hits[1..].iter().any(|g| exact(g) && year(g) != y) {
        return AlbumInfo {
            matched: Match::SameName,
            first_year: y,
        };
    }
    AlbumInfo {
        matched: Match::Matched,
        first_year: y,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::VecDeque;

    /// Stub HTTP: answers in order, records every URL asked.
    #[derive(Default)]
    pub struct Stub {
        pub answers: VecDeque<Result<(u16, String), Halt>>,
        pub asked: Vec<String>,
    }

    impl Stub {
        pub fn new(answers: Vec<Result<(u16, Value), Halt>>) -> Self {
            Self {
                answers: answers
                    .into_iter()
                    .map(|a| a.map(|(c, v)| (c, v.to_string())))
                    .collect(),
                asked: vec![],
            }
        }
    }

    impl Http for Stub {
        fn get(&mut self, url: &str) -> Result<(u16, String), Halt> {
            self.asked.push(url.to_string());
            self.answers.pop_front().expect("an answer per request")
        }
    }

    fn a(id: &str, name: &str, score: i64) -> Value {
        json!({"id": id, "name": name, "score": score, "country": "GB", "aliases": []})
    }

    #[test]
    fn artist_search_classes() {
        let r = json!({"artists": [a("1", "Synthetic Band", 100), a("2", "Other", 60)]});
        assert_eq!(classify_artists("synthetic band", &r).0, Match::Matched);
        // folding: accents and punctuation
        let r = json!({"artists": [a("1", "Sýnthetic-Band!", 95)]});
        assert_eq!(classify_artists("Synthetic Band", &r).0, Match::Matched);
        // alias match
        let r = json!({"artists": [{"id": "1", "name": "Real Name", "score": 100, "aliases": [{"name": "Alias Act"}]}]});
        assert_eq!(classify_artists("Alias Act", &r).0, Match::Matched);
        // two artists share the name: its own class, labelled from the top hit as "probably"
        let r = json!({"artists": [a("1", "Twin", 100), a("2", "Twin", 100)]});
        let (m, top) = classify_artists("Twin", &r);
        assert_eq!((m, top.map(|t| s(&t["id"]))), (Match::SameName, Some("1")));
        // low score, or top hit not the name
        let r = json!({"artists": [a("1", "Synthetic Band", 89)]});
        assert_eq!(classify_artists("Synthetic Band", &r).0, Match::None);
        let r =
            json!({"artists": [a("1", "Synthetic Bandits", 100), a("2", "Synthetic Band", 100)]});
        assert_eq!(classify_artists("Synthetic Band", &r).0, Match::None);
        assert_eq!(classify_artists("x", &json!({})).0, Match::None);
    }

    #[test]
    fn genres_most_voted_first_and_capped() {
        let v = json!({"genres": [{"name": "b", "count": 2}, {"name": "a", "count": 2}, {"name": "c", "count": 9},
            {"name": "d", "count": 1}, {"name": "e", "count": 1}, {"name": "f", "count": 0}, {"name": "", "count": 50}]});
        let g: Vec<_> = genres(&v).into_iter().map(|x| x.0).collect();
        assert_eq!(g, ["c", "a", "b", "d", "e"]);
    }

    #[test]
    fn only_the_name_goes_out_and_only_to_allowed_hosts() {
        let u = artist_search_url("A \"B\" & C?");
        assert!(crate::net::check(&u).is_ok());
        let q = Url::parse(&u).unwrap();
        let pairs: Vec<_> = q
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("query".into(), "artist:\"A \\\"B\\\" & C?\"".into()),
                ("limit".into(), "5".into()),
                ("fmt".into(), "json".into())
            ]
        );
        assert!(crate::net::check(&album_search_url("x", "y")).is_ok());
        assert!(crate::net::check(&wd(&[("search", "x")])).is_ok());
        // an MBID can't smuggle a path or query
        assert_eq!(
            artist_genres_url("ab-12/../x?y"),
            format!("{MB}artist/ab-12?inc=genres&fmt=json")
        );
    }

    #[test]
    fn matched_artist_reads_genres_and_country() {
        let mut h = Stub::new(vec![
            Ok((200, json!({"artists": [a("abc-1", "Synthetic Band", 100)]}))),
            Ok((200, json!({"genres": [{"name": "synthpop", "count": 3}]}))),
        ]);
        let got = artist(&mut h, "Synthetic Band").unwrap();
        assert_eq!(
            got,
            ArtistInfo {
                matched: Match::Matched,
                via: Some("musicbrainz"),
                country: Some("GB".into()),
                genres: vec![("synthpop".into(), 3)]
            }
        );
        assert!(h.asked[1].contains("/artist/abc-1?inc=genres"));
    }

    #[test]
    fn same_name_keeps_the_top_hits_labels() {
        let mut h = Stub::new(vec![
            Ok((
                200,
                json!({"artists": [a("abc-1", "Twin", 100), a("abc-2", "Twin", 95)]}),
            )),
            Ok((200, json!({"genres": [{"name": "folk", "count": 2}]}))),
        ]);
        assert_eq!(
            artist(&mut h, "Twin").unwrap(),
            ArtistInfo {
                matched: Match::SameName,
                via: Some("musicbrainz"),
                country: Some("GB".into()),
                genres: vec![("folk".into(), 2)]
            }
        );
        assert!(h.asked[1].contains("/artist/abc-1?inc=genres"));
    }

    #[test]
    fn busy_and_offline_halt_without_a_result() {
        let mut h = Stub::new(vec![Ok((503, json!({})))]);
        assert_eq!(artist(&mut h, "x"), Err(Halt::Busy));
        // any 5xx (a gateway error too) is brief: back off, nothing stored, ask again later
        let mut h = Stub::new(vec![Ok((502, json!({})))]);
        assert_eq!(artist(&mut h, "x"), Err(Halt::Busy));
        let mut h = Stub::new(vec![
            Ok((200, json!({"artists": [a("abc-1", "x", 100)]}))),
            Ok((500, json!({}))),
        ]);
        assert_eq!(artist(&mut h, "x"), Err(Halt::Busy));
        let mut h = Stub::new(vec![Err(Halt::Offline)]);
        assert_eq!(artist(&mut h, "x"), Err(Halt::Offline));
        let mut h = Stub::new(vec![Ok((200, json!({"error": {"code": "maxlag"}})))]);
        assert_eq!(wikidata_artist(&mut h, "x"), Err(Halt::Busy));
        // a bad request for this name is a miss, not a retry loop
        let mut h = Stub::new(vec![Ok((400, json!({})))]);
        assert_eq!(artist(&mut h, "x").unwrap().matched, Match::None);
    }

    #[test]
    fn wikidata_fallback_when_musicbrainz_has_nothing() {
        let mut h = Stub::new(vec![
            Ok((200, json!({"artists": []}))),
            Ok((
                200,
                json!({"search": [{"id": "Q1", "label": "Synthetic Band"}, {"id": "Q2", "label": "Synthetic Band"}, {"id": "Q3", "label": "Else"}]}),
            )),
            // Q2 is a namesake with no music claims
            Ok((
                200,
                json!({"entities": {
                "Q1": {"claims": {"P136": [{"mainsnak": {"datavalue": {"value": {"id": "Q10"}}}}],
                                  "P495": [{"mainsnak": {"datavalue": {"value": {"id": "Q20"}}}}]}},
                "Q2": {"claims": {}}}}),
            )),
            Ok((
                200,
                json!({"entities": {"Q10": {"labels": {"en": {"value": "synth-pop"}}}}}),
            )),
            Ok((
                200,
                json!({"claims": {"P297": [{"mainsnak": {"datavalue": {"value": "SE"}}}]}}),
            )),
        ]);
        let got = artist(&mut h, "Synthetic Band").unwrap();
        assert_eq!(
            got,
            ArtistInfo {
                matched: Match::Matched,
                via: Some("wikidata"),
                country: Some("SE".into()),
                genres: vec![("synth-pop".into(), 1)]
            }
        );
        assert!(h.asked[2].contains("ids=Q1%7CQ2") && !h.asked[2].contains("Q3"));
        assert!(h.asked.iter().all(|u| crate::net::check(u).is_ok()));
    }

    #[test]
    fn wikidata_two_musical_namesakes_are_same_name() {
        let music = json!({"P434": [{}]});
        let mut h = Stub::new(vec![
            Ok((
                200,
                json!({"search": [{"id": "Q1", "label": "Twin"}, {"id": "Q2", "label": "twin"}]}),
            )),
            Ok((
                200,
                json!({"entities": {"Q1": {"claims": music}, "Q2": {"claims": music}}}),
            )),
        ]);
        assert_eq!(
            wikidata_artist(&mut h, "Twin").unwrap().matched,
            Match::SameName
        );
    }

    #[test]
    fn wikidata_album_fallback_checks_the_performer() {
        let cl = |date: &str, by: &str| {
            json!({"claims": {"P577": [{"mainsnak": {"datavalue": {"value": {"time": date}}}}],
                              "P175": [{"mainsnak": {"datavalue": {"value": {"id": by}}}}]}})
        };
        let mut h = Stub::new(vec![
            Ok((200, json!({"release-groups": []}))),
            Ok((
                200,
                json!({"search": [{"id": "Q1", "label": "Synthetic Album"}, {"id": "Q2", "label": "Synthetic Album"}]}),
            )),
            // Q2: same title by someone else
            Ok((
                200,
                json!({"entities": {"Q1": cl("+1997-05-21T00:00:00Z", "Q9"), "Q2": cl("+2004-01-01T00:00:00Z", "Q8")}}),
            )),
            Ok((
                200,
                json!({"entities": {"Q9": {"labels": {"en": {"value": "Synthetic Band"}}},
                                    "Q8": {"labels": {"en": {"value": "Someone Else"}}}}}),
            )),
        ]);
        assert_eq!(
            album(&mut h, "Synthetic Album", "Synthetic Band").unwrap(),
            AlbumInfo {
                matched: Match::Matched,
                first_year: Some(1997)
            }
        );
        assert!(h.asked[1].contains("www.wikidata.org") && h.asked[3].contains("Q8%7CQ9"));
        assert!(h.asked.iter().all(|u| crate::net::check(u).is_ok()));
        // no performer with the artist's name → none
        let mut h = Stub::new(vec![
            Ok((
                200,
                json!({"search": [{"id": "Q1", "label": "Synthetic Album"}]}),
            )),
            Ok((
                200,
                json!({"entities": {"Q1": cl("+1997-01-01T00:00:00Z", "Q8")}}),
            )),
            Ok((
                200,
                json!({"entities": {"Q8": {"labels": {"en": {"value": "Someone Else"}}}}}),
            )),
        ]);
        assert_eq!(
            wikidata_album(&mut h, "Synthetic Album", "Synthetic Band")
                .unwrap()
                .matched,
            Match::None
        );
    }

    #[test]
    fn album_first_year_and_classes() {
        let rg = |title: &str, by: &str, score: i64, date: &str| {
            json!({"title": title, "score": score, "first-release-date": date,
                   "artist-credit": [{"name": by, "artist": {"name": by}}]})
        };
        let r = json!({"release-groups": [rg("Synthetic Album", "Synthetic Band", 100, "1997-05-21"),
                                          rg("Synthetic Album", "Synthetic Band", 80, "1997")]});
        assert_eq!(
            classify_album("Synthetic Album - Remastered 2011", "Synthetic Band", &r),
            AlbumInfo {
                matched: Match::Matched,
                first_year: Some(1997)
            }
        );
        let r = json!({"release-groups": [rg("Same", "Band", 100, "1990"), rg("Same", "Band", 100, "2004")]});
        assert_eq!(
            classify_album("Same", "Band", &r),
            AlbumInfo {
                matched: Match::SameName,
                first_year: Some(1990)
            }
        );
        let r = json!({"release-groups": [rg("Synthetic Album", "Someone Else", 100, "1990")]});
        assert_eq!(
            classify_album("Synthetic Album", "Synthetic Band", &r).matched,
            Match::None
        );
        let r = json!({"release-groups": [rg("X", "Band", 100, "")]});
        assert_eq!(
            classify_album("X", "Band", &r),
            AlbumInfo {
                matched: Match::Matched,
                first_year: None
            }
        );
    }
}

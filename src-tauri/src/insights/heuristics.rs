//! Insights v3 sessions and heuristics (design/M10-insights-v3.md §4.2, "estimates, labelled"):
//! sessions, deep dives, on repeat, eras, arcs, "took time from", album start to finish. One
//! constant per threshold, so WI 10 can tune each on the real history.
use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{NaiveDate, NaiveDateTime, TimeDelta};
use serde::Serialize;

use super::terms::{Ev, artist_key, is_play};

/// Session: a gap from one play's end to the next play's estimated start above this splits.
pub const SESSION_GAP_MS: i64 = 30 * 60_000;
/// Deep dive: at least this much consecutive time …
pub const DIVE_MIN_MS: i64 = 60 * 60_000;
/// … of which at least this share (%) is one artist or album.
pub const DIVE_SHARE_PCT: i64 = 80;
/// On repeat: a track played this many times in one local day.
pub const REPEAT_PLAYS: usize = 5;
/// Era: an artist holds ≥ this share of the month (%) …
pub const ERA_SHARE: f64 = 0.15;
/// … and ≥ this many times its lifetime share; the era is named after up to this many …
pub const ERA_LIFT: f64 = 1.5;
pub const ERA_ARTISTS: usize = 2;
/// … and runs shorter than this many months merge into a neighbour.
pub const ERA_MIN_MONTHS: usize = 3;
/// Arc: months looked at.
pub const ARC_MONTHS: usize = 36;
/// Arc windows: the last 3 months against the 6 before.
pub const ARC_RECENT: usize = 3;
pub const ARC_BEFORE: usize = 6;
/// Rising: recent share ≥ 2× before. Fading: ≤ 0.5×, with a before share ≥ 3%.
pub const RISING: f64 = 2.0;
pub const FADING: f64 = 0.5;
pub const FADING_PAST: f64 = 0.03;
/// Back again: ≥ 18 months under 0.5%, then ≥ 2% in a recent month.
pub const BACK_QUIET_MONTHS: usize = 18;
pub const BACK_QUIET: f64 = 0.005;
pub const BACK_AGAIN: f64 = 0.02;
/// Constant: in the top 20 in ≥ 75% of years.
pub const CONSTANT_TOP: usize = 20;
pub const CONSTANT_YEARS_PCT: usize = 75;
/// "May have taken time from": how many artists are named.
pub const TOOK_TIME_TOP: usize = 3;

fn est_start(e: &Ev) -> NaiveDateTime {
    e.local - TimeDelta::milliseconds(e.ms)
}

/// A session (estimate): plays in end order, split where the gap from one play's end to the next
/// play's estimated start (end − ms played) is over 30 min.
#[derive(Debug, Clone, PartialEq)]
pub struct Session<'a> {
    pub plays: Vec<&'a Ev>,
    /// Estimated start (local): the first play's end − its ms played.
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    /// Listening time in the session.
    pub ms: i64,
}

impl Session<'_> {
    /// Wall-clock length, estimated start to last end.
    pub fn span_ms(&self) -> i64 {
        (self.end - self.start).num_milliseconds()
    }
}

/// Sessions over `evs` (any order; only plays are used).
pub fn sessions(evs: &[Ev]) -> Vec<Session<'_>> {
    let mut plays: Vec<&Ev> = evs.iter().filter(|e| is_play(e)).collect();
    plays.sort_by_key(|e| (e.end, e.id));
    let mut out: Vec<Session> = vec![];
    for p in plays {
        let gap = out.last().map(|s| {
            let prev = s.plays.last().unwrap();
            (p.end - TimeDelta::milliseconds(p.ms) - prev.end).num_milliseconds()
        });
        match out.last_mut() {
            Some(s) if gap.is_some_and(|g| g <= SESSION_GAP_MS) => {
                s.plays.push(p);
                s.end = p.local;
                s.ms += p.ms;
            }
            _ => out.push(Session {
                plays: vec![p],
                start: est_start(p),
                end: p.local,
                ms: p.ms,
            }),
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiveKind {
    Artist,
    Album,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dive {
    pub kind: DiveKind,
    pub name: String,
    /// The album's artist; `""` for an artist dive.
    pub by: String,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub tracks: usize,
    pub ms: i64,
}

/// Deep dives (estimate) in one session: the longest stretches of consecutive plays, each
/// starting and ending on the key's plays, of ≥ 60 min where ≥ 80% of the time is one artist
/// (or one album). Stretches of one key don't overlap.
pub fn deep_dives(s: &Session) -> Vec<Dive> {
    let p = &s.plays;
    let mut pre = vec![0i64];
    for e in p {
        pre.push(pre.last().unwrap() + e.ms);
    }
    let mut out = vec![];
    for kind in [DiveKind::Artist, DiveKind::Album] {
        let key = |e: &Ev| match kind {
            _ if e.artist.is_empty() => None,
            DiveKind::Artist => Some(artist_key(&e.artist)),
            DiveKind::Album if e.album.is_empty() => None,
            DiveKind::Album => Some(format!(
                "{}\u{1f}{}",
                e.album.to_ascii_lowercase(),
                artist_key(&e.artist)
            )),
        };
        let mut at: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, e) in p.iter().enumerate() {
            if let Some(k) = key(e) {
                at.entry(k).or_default().push(i);
            }
        }
        for idx in at.values() {
            // a dive needs ≥ 80% of 60 min on this key: skip keys that can't reach it
            let total: i64 = idx.iter().map(|&i| p[i].ms).sum();
            if total * 100 < DIVE_MIN_MS * DIVE_SHARE_PCT {
                continue;
            }
            let mut kp = vec![0i64];
            for &i in idx {
                kp.push(kp.last().unwrap() + p[i].ms);
            }
            let mut i = 0;
            while i < idx.len() {
                let hit = (i..idx.len()).rev().find(|&j| {
                    let win = pre[idx[j] + 1] - pre[idx[i]];
                    win >= DIVE_MIN_MS && (kp[j + 1] - kp[i]) * 100 >= win * DIVE_SHARE_PCT
                });
                let Some(j) = hit else {
                    i += 1;
                    continue;
                };
                let w = &p[idx[i]..=idx[j]];
                let first = p[idx[i]];
                out.push(Dive {
                    kind,
                    name: match kind {
                        DiveKind::Artist => first.artist.clone(),
                        DiveKind::Album => first.album.clone(),
                    },
                    by: match kind {
                        DiveKind::Artist => String::new(),
                        DiveKind::Album => first.artist.clone(),
                    },
                    start: est_start(first),
                    end: p[idx[j]].local,
                    tracks: w
                        .iter()
                        .map(|e| e.norm_key.as_str())
                        .collect::<HashSet<_>>()
                        .len(),
                    ms: pre[idx[j] + 1] - pre[idx[i]],
                });
                i = j + 1;
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Repeat {
    pub date: NaiveDate,
    pub norm_key: String,
    pub track: String,
    pub artist: String,
    pub times: usize,
}

/// On repeat: a track with ≥ 5 plays in one local day. By date, then most played.
pub fn on_repeat(evs: &[Ev]) -> Vec<Repeat> {
    let mut by: BTreeMap<(NaiveDate, &str), Repeat> = BTreeMap::new();
    for e in evs.iter().filter(|e| is_play(e) && !e.norm_key.is_empty()) {
        by.entry((e.date(), &e.norm_key))
            .or_insert_with(|| Repeat {
                date: e.date(),
                norm_key: e.norm_key.clone(),
                track: e.track.clone(),
                artist: e.artist.clone(),
                times: 0,
            })
            .times += 1;
    }
    let mut out: Vec<Repeat> = by
        .into_values()
        .filter(|r| r.times >= REPEAT_PLAYS)
        .collect();
    out.sort_by(|a, b| a.date.cmp(&b.date).then(b.times.cmp(&a.times)));
    out
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Era {
    pub from: String,
    pub to: String,
    pub artists: Vec<String>,
}

/// Eras (estimate). `months` = every month in order, each with its `(artist, ms)` rows. A month's
/// label is the (≤ 2) artists holding ≥ 15% of its time and ≥ 1.5× their lifetime share. Runs of
/// one label shorter than 3 months merge into a neighbour: first a run between two runs of one
/// label (it joins them), else the longer neighbour (the earlier on a tie). Unlabelled runs are
/// kept as gaps between eras.
pub fn eras(months: &[(String, Vec<(String, i64)>)]) -> Vec<Era> {
    let mut life: HashMap<&str, i64> = HashMap::new();
    for (_, rows) in months {
        for (a, ms) in rows {
            *life.entry(a).or_default() += ms;
        }
    }
    let all: i64 = life.values().sum();
    let label = |rows: &[(String, i64)]| -> Vec<String> {
        let total: i64 = rows.iter().map(|r| r.1).sum();
        let mut lead: Vec<&(String, i64)> = rows
            .iter()
            .filter(|(a, ms)| {
                let share = *ms as f64 / total as f64;
                let lifetime = life[a.as_str()] as f64 / all as f64;
                !a.is_empty() && share >= ERA_SHARE && share >= ERA_LIFT * lifetime
            })
            .collect();
        lead.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut l: Vec<String> = lead
            .into_iter()
            .take(ERA_ARTISTS)
            .map(|r| r.0.clone())
            .collect();
        l.sort();
        l
    };
    // (label, first month index, last month index)
    let mut runs: Vec<(Vec<String>, usize, usize)> = vec![];
    for (i, (_, rows)) in months.iter().enumerate() {
        runs.push((label(rows), i, i));
    }
    let len = |r: &(Vec<String>, usize, usize)| r.2 - r.1 + 1;
    loop {
        // join neighbours with one label
        let mut k = 1;
        while k < runs.len() {
            if runs[k].0 == runs[k - 1].0 {
                runs[k - 1].2 = runs[k].2;
                runs.remove(k);
            } else {
                k += 1;
            }
        }
        if runs.len() < 2 {
            break;
        }
        let short = |i: &usize| len(&runs[*i]) < ERA_MIN_MONTHS;
        let sandwich = (1..runs.len() - 1)
            .filter(short)
            .find(|&i| runs[i - 1].0 == runs[i + 1].0);
        let Some(i) =
            sandwich.or_else(|| (0..runs.len()).filter(short).min_by_key(|&i| len(&runs[i])))
        else {
            break;
        };
        let into_prev = i > 0
            && (sandwich.is_some()
                || i + 1 == runs.len()
                || len(&runs[i - 1]) >= len(&runs[i + 1]));
        let r = runs.remove(i);
        if into_prev {
            runs[i - 1].2 = r.2;
        } else {
            runs[i].1 = r.1;
        }
    }
    runs.into_iter()
        .filter(|r| !r.0.is_empty())
        .map(|(artists, a, b)| Era {
            from: months[a].0.clone(),
            to: months[b].0.clone(),
            artists,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArcKind {
    Rising,
    Fading,
    BackAgain,
    Constant,
}

/// An artist's share of the time in a run of `(artist ms, all ms)` months.
pub fn window_share(m: &[(i64, i64)]) -> f64 {
    let (a, t) = m.iter().fold((0, 0), |(a, t), x| (a + x.0, t + x.1));
    if t == 0 { 0.0 } else { a as f64 / t as f64 }
}

/// Constant: in the top 20 in ≥ 75% of the years.
pub fn is_constant(years_in_top: usize, years: usize) -> bool {
    years > 0 && years_in_top * 100 >= years * CONSTANT_YEARS_PCT
}

/// An artist's arc (estimate) over `months` = `(artist ms, all ms)`, oldest first, ending this
/// month (36 of them). First match wins: back again, rising, fading, constant.
/// - back again: a run of ≥ 18 months under 0.5% ending within the last 3 months, a month ≥ 2%
///   after it, and some listening before it;
/// - rising: last-3-month share > 0 and ≥ 2× the 6 months before;
/// - fading: the 6 months before ≥ 3%, the last 3 ≤ 0.5× that.
pub fn arc(months: &[(i64, i64)], constant: bool) -> Option<ArcKind> {
    let n = months.len();
    if back_again_at(months).is_some() {
        return Some(ArcKind::BackAgain);
    }
    if n >= ARC_RECENT + ARC_BEFORE {
        let recent = window_share(&months[n - ARC_RECENT..]);
        let before = window_share(&months[n - ARC_RECENT - ARC_BEFORE..n - ARC_RECENT]);
        if recent > 0.0 && recent >= RISING * before {
            return Some(ArcKind::Rising);
        }
        if before >= FADING_PAST && recent <= FADING * before {
            return Some(ArcKind::Fading);
        }
    }
    constant.then_some(ArcKind::Constant)
}

/// Back again's months: `(first quiet month, comeback month)` as indexes into `months`, or `None`.
pub fn back_again_at(months: &[(i64, i64)]) -> Option<(usize, usize)> {
    let n = months.len();
    let share = |i: usize| window_share(&months[i..=i]);
    let recent_from = n.saturating_sub(ARC_RECENT);
    let back = (recent_from..n).find(|&i| share(i) >= BACK_AGAIN)?;
    // step over recent non-quiet months before the comeback, then count the quiet run
    let mut end = back;
    while end > recent_from && share(end - 1) >= BACK_QUIET {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && share(start - 1) < BACK_QUIET {
        start -= 1;
    }
    (end - start >= BACK_QUIET_MONTHS && months[..start].iter().any(|m| m.0 > 0))
        .then_some((start, back))
}

/// "May have taken time from" (estimate): for a fading artist, the others whose share rose most
/// over the same months (last 3 against the 6 before), up to 3, biggest rise first.
pub fn took_time_from(fading: &str, artists: &HashMap<String, Vec<(i64, i64)>>) -> Vec<String> {
    moved_most(fading, artists, true)
}

/// The others whose share moved most over the arc windows (last 3 months against the 6 before),
/// up to 3: rose most (`up`: for a fading artist) or fell most (for a rising one, "may have taken
/// time from").
pub fn moved_most(
    artist: &str,
    artists: &HashMap<String, Vec<(i64, i64)>>,
    up: bool,
) -> Vec<String> {
    let sign = if up { 1.0 } else { -1.0 };
    let mut moves: Vec<(f64, &str)> = artists
        .iter()
        .filter(|(a, m)| a.as_str() != artist && m.len() >= ARC_RECENT + ARC_BEFORE)
        .map(|(a, m)| {
            let n = m.len();
            let rise = window_share(&m[n - ARC_RECENT..])
                - window_share(&m[n - ARC_RECENT - ARC_BEFORE..n - ARC_RECENT]);
            (sign * rise, a.as_str())
        })
        .filter(|(r, _)| *r > 0.0)
        .collect();
    moves.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    moves
        .into_iter()
        .take(TOOK_TIME_TOP)
        .map(|(_, a)| a.to_owned())
        .collect()
}

/// Album start to finish (estimate): sessions in which every track of the album's track list
/// (library tracks linked `on_album`, TSD §4.9) was played. An album without a list: 0.
pub fn start_to_finish(tracklist: &HashSet<i64>, sessions: &[Session]) -> usize {
    if tracklist.is_empty() {
        return 0;
    }
    sessions
        .iter()
        .filter(|s| {
            let heard: HashSet<i64> = s.plays.iter().filter_map(|e| e.item_id).collect();
            tracklist.is_subset(&heard)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::terms::ev;

    fn t(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").unwrap()
    }
    const MIN: i64 = 60_000;

    #[test]
    fn sessions_split_on_gaps_over_30_min() {
        let evs = vec![
            ev("2026-05-01T10:05:00", 5 * MIN, "A", "One"), // starts 10:00
            ev("2026-05-01T10:10:00", 5 * MIN, "A", "Two"), // gap 0
            ev("2026-05-01T11:20:00", 20_000, "A", "Skip"), // not a play: ignored
            ev("2026-05-01T11:10:00", 30 * MIN, "B", "Long"), // starts 10:40: gap exactly 30 min
            ev("2026-05-01T11:45:00", 4 * MIN, "B", "Late"), // starts 11:41: gap 31 min → new
        ];
        let s = sessions(&evs);
        assert_eq!(s.len(), 2);
        assert_eq!(
            s[0].plays
                .iter()
                .map(|e| e.track.as_str())
                .collect::<Vec<_>>(),
            vec!["One", "Two", "Long"]
        );
        assert_eq!(
            (s[0].start, s[0].end, s[0].ms, s[0].span_ms()),
            (
                t("2026-05-01T10:00:00"),
                t("2026-05-01T11:10:00"),
                40 * MIN,
                70 * MIN
            )
        );
        assert_eq!((s[1].start, s[1].ms), (t("2026-05-01T11:41:00"), 4 * MIN));
        assert!(sessions(&[]).is_empty());
    }

    /// Back-to-back plays from `at` (local), each `mins` long.
    fn run(at: &str, specs: &[(&str, &str, i64)]) -> Vec<Ev> {
        let mut now = t(at);
        specs
            .iter()
            .map(|(artist, track, mins)| {
                now += TimeDelta::minutes(*mins);
                ev(
                    &now.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    mins * MIN,
                    artist,
                    track,
                )
            })
            .collect()
    }

    #[test]
    fn deep_dives_need_an_hour_at_80_percent() {
        // A 50 min (10 tracks), B 5 min, A 15 min: 70 min, A = 65/70 = 93%
        let mut spec: Vec<(&str, &str, i64)> = (0..10).map(|_| ("A", "a", 5)).collect();
        spec.push(("B", "b", 5));
        spec.extend([("A", "c", 5), ("A", "d", 5), ("A", "e", 5)]);
        let evs = run("2026-05-01T20:00:00", &spec);
        let s = sessions(&evs);
        assert_eq!(s.len(), 1);
        let d = deep_dives(&s[0]);
        let want = |kind, name: &str, by: &str| Dive {
            kind,
            name: name.into(),
            by: by.into(),
            start: t("2026-05-01T20:00:00"),
            end: t("2026-05-01T21:10:00"),
            tracks: 5, // a, b, c, d, e
            ms: 70 * MIN,
        };
        assert_eq!(
            d,
            vec![
                want(DiveKind::Artist, "A", ""),
                want(DiveKind::Album, "A LP", "A")
            ]
        );
        // A 50, B 20, A 10: 80 min at 75%, and no 60-min stretch of A reaches 80%
        let evs = run(
            "2026-05-01T20:00:00",
            &[("A", "a", 50), ("B", "b", 20), ("A", "c", 10)],
        );
        assert!(deep_dives(&sessions(&evs)[0]).is_empty());
        // A 40, B 30: A can't reach 48 min
        let evs = run("2026-05-01T20:00:00", &[("A", "a", 40), ("B", "b", 30)]);
        assert!(deep_dives(&sessions(&evs)[0]).is_empty());
    }

    #[test]
    fn on_repeat_is_five_plays_in_a_day() {
        let mut evs: Vec<Ev> = (0..5)
            .map(|i| ev(&format!("2026-05-01T1{i}:00:00"), 60_000, "A", "One"))
            .collect();
        evs.push(ev("2026-05-01T16:00:00", 29_000, "A", "One")); // not a play
        evs.extend((0..4).map(|i| ev(&format!("2026-05-02T1{i}:00:00"), 60_000, "A", "One")));
        let r = on_repeat(&evs);
        assert_eq!(r.len(), 1);
        assert_eq!(
            (r[0].date.to_string(), r[0].norm_key.as_str(), r[0].times),
            ("2026-05-01".into(), "a|one", 5)
        );
    }

    fn month(m: &str, rows: &[(&str, i64)]) -> (String, Vec<(String, i64)>) {
        (
            m.into(),
            rows.iter().map(|(a, ms)| (a.to_string(), *ms)).collect(),
        )
    }

    #[test]
    fn eras_group_dominant_months_and_merge_short_runs() {
        let fill = [("f1", 10), ("f2", 10), ("f3", 10), ("f4", 10)];
        let with = |lead: &'static str| -> Vec<(&'static str, i64)> {
            let mut v = vec![(lead, 60)];
            v.extend(fill);
            v
        };
        // lifetime: x 30%, y 22.5%, w 7.5%, each fill 10%. Months: x x x x y w y y
        let months: Vec<_> = ["x", "x", "x", "x", "y", "w", "y", "y"]
            .iter()
            .enumerate()
            .map(|(i, a)| month(&format!("2026-{:02}", i + 1), &with(a)))
            .collect();
        // w (1 month) sits between y runs: it joins them, so y runs May–Aug
        assert_eq!(
            eras(&months),
            vec![
                Era {
                    from: "2026-01".into(),
                    to: "2026-04".into(),
                    artists: vec!["x".into()]
                },
                Era {
                    from: "2026-05".into(),
                    to: "2026-08".into(),
                    artists: vec!["y".into()]
                },
            ]
        );
        // two leaders share a month: named after both; a 2-month run joins its longer neighbour
        let months = vec![
            month("2026-01", &[("p", 50), ("q", 50)]),
            month("2026-02", &[("p", 50), ("q", 50)]),
            month("2026-03", &[("r", 100)]),
            month("2026-04", &[("r", 100)]),
            month("2026-05", &[("r", 100)]),
        ];
        // lifetime p 20%, q 20%, r 60%: Jan–Feb [p, q] (50% ≥ 1.5 × 20%), Mar–May [r] (100% ≥ 90%)
        let e = eras(&months);
        assert_eq!(
            e,
            vec![Era {
                from: "2026-01".into(),
                to: "2026-05".into(),
                artists: vec!["r".into()]
            }]
        );
        assert!(eras(&[]).is_empty());
    }

    fn months(f: impl Fn(usize) -> i64) -> Vec<(i64, i64)> {
        (0..ARC_MONTHS).map(|i| (f(i), 1000)).collect()
    }

    #[test]
    fn arcs() {
        use ArcKind::*;
        // 1% in the 6 months before, 3% in the last 3
        let rising = months(|i| match i {
            27..=32 => 10,
            33.. => 30,
            _ => 0,
        });
        assert_eq!(arc(&rising, false), Some(Rising));
        // 5% before, 2% now (≤ 2.5%)
        let fading = months(|i| if i >= 33 { 20 } else { 50 });
        assert_eq!(arc(&fading, true), Some(Fading));
        // 2% before is under the 3% floor: not fading
        assert_eq!(arc(&months(|i| if i >= 33 { 5 } else { 20 }), false), None);
        // heard in months 0–4, quiet 5–33 (29 months), 3% in month 34
        let back = months(|i| match i {
            0..=4 => 10,
            34 => 30,
            _ => 0,
        });
        assert_eq!(arc(&back, false), Some(BackAgain));
        assert_eq!(back_again_at(&back), Some((5, 34))); // quiet from month 5, back in 34
        // the same with nothing before the quiet run is new, not back: rising
        assert_eq!(
            arc(&months(|i| if i == 34 { 30 } else { 0 }), false),
            Some(Rising)
        );
        // only 17 quiet months: not back again
        let short = months(|i| match i {
            0..=17 => 10,
            35 => 30,
            _ => 0,
        });
        assert_eq!(arc(&short, false), Some(Rising));
        // flat 5%: constant only when the years say so
        let flat = months(|_| 50);
        assert_eq!(arc(&flat, true), Some(Constant));
        assert_eq!(arc(&flat, false), None);
        assert!(is_constant(3, 4) && !is_constant(2, 4) && !is_constant(0, 0));

        let artists: HashMap<String, Vec<(i64, i64)>> = [
            ("fader", fading.clone()),
            ("big", months(|i| if i >= 33 { 100 } else { 0 })), // +10 points
            ("small", months(|i| if i >= 33 { 30 } else { 10 })), // +2 points
            ("down", months(|i| if i >= 33 { 10 } else { 30 })), // falls
        ]
        .into_iter()
        .map(|(a, m)| (a.to_owned(), m))
        .collect();
        assert_eq!(took_time_from("fader", &artists), vec!["big", "small"]);
        // the other way round: who fell most (for a rising artist), the fader first
        assert_eq!(moved_most("big", &artists, false)[0], "fader");
    }

    #[test]
    fn start_to_finish_needs_every_listed_track_in_one_session() {
        let mut evs = run(
            "2026-05-01T20:00:00",
            &[
                ("A", "one", 4),
                ("A", "two", 4),
                ("B", "x", 4),
                ("A", "three", 4),
            ],
        );
        evs.extend(run(
            "2026-05-02T20:00:00",
            &[("A", "one", 4), ("A", "two", 4)],
        ));
        for e in &mut evs {
            e.item_id = match e.track.as_str() {
                "one" => Some(1),
                "two" => Some(2),
                "three" => Some(3),
                _ => None,
            };
        }
        let s = sessions(&evs);
        assert_eq!(s.len(), 2);
        let list: HashSet<i64> = [1, 2, 3].into();
        assert_eq!(start_to_finish(&list, &s), 1);
        assert_eq!(start_to_finish(&HashSet::new(), &s), 0);
    }
}

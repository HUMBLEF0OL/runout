//! `Streaming_History_Audio_*.json` → `Event`s (TSD §4.8). The `Row` struct IS the field
//! allow-list: serde ignores every other key, so `ip_addr*`, `user_agent*`, `username`,
//! `conn_country`, `offline_timestamp` and `episode_*`/`audiobook_*` are never even held in memory.
use std::collections::HashMap;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;

use super::events::{Event, Source};
use super::source::{File, unrecognized};
use crate::error::Result;
use crate::text::{clean_line, limit};

/// The only fields read from an export row. ⚑ T1: names per TSD §4.8 until U-2 (WI 4 edits only this).
#[derive(Deserialize)]
struct Row {
    ts: String,
    ms_played: u32,
    platform: Option<String>,
    master_metadata_track_name: Option<String>,
    master_metadata_album_artist_name: Option<String>,
    master_metadata_album_album_name: Option<String>,
    spotify_track_uri: Option<String>,
    reason_start: Option<String>,
    reason_end: Option<String>,
    shuffle: Option<bool>,
    skipped: Option<bool>,
    offline: Option<bool>,
    incognito_mode: Option<bool>,
}

#[derive(Default)]
pub struct Parsed {
    /// Music rows, in file order (not yet de-duplicated).
    pub events: Vec<Event>,
    /// Rows without a track uri: podcasts, audiobooks, video.
    pub skipped: usize,
}

/// `spotify:track:` + 22 base-62 characters (TSD §4.1).
fn track_uri(s: &str) -> bool {
    s.strip_prefix("spotify:track:")
        .is_some_and(|id| id.len() == 22 && id.bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn short(s: Option<String>) -> Option<String> {
    s.map(|s| clean_line(&s, 100)).filter(|s| !s.is_empty())
}

pub fn parse(files: &[File], tz: &mut LocalTime) -> Result<Parsed> {
    let mut out = Parsed::default();
    for f in files {
        let bad = |what: String| {
            unrecognized("This file isn't Spotify streaming history.")
                .with_detail(format!("{}: {what}", f.name))
        };
        let rows: Vec<Row> = serde_json::from_slice(&f.bytes).map_err(|e| bad(e.to_string()))?;
        for r in rows {
            let Some(uri) = r.spotify_track_uri.filter(|u| track_uri(u)) else {
                out.skipped += 1;
                continue;
            };
            let utc = DateTime::parse_from_rfc3339(&r.ts)
                .map_err(|_| bad("field `ts` is not a timestamp".into()))?
                .with_timezone(&Utc);
            let name = |s: Option<String>| clean_line(&s.unwrap_or_default(), limit::NAME);
            out.events.push(Event {
                source: Source::Export,
                ended_at_utc: utc.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                ended_at_local: Some(tz.local(utc).format("%Y-%m-%dT%H:%M:%S").to_string()),
                ms_played: r.ms_played.into(),
                track_name: name(r.master_metadata_track_name),
                artist_name: name(r.master_metadata_album_artist_name),
                album_name: name(r.master_metadata_album_album_name),
                spotify_track_uri: Some(uri),
                platform: short(r.platform),
                reason_start: short(r.reason_start),
                reason_end: short(r.reason_end),
                shuffle: r.shuffle,
                skipped: r.skipped,
                offline: r.offline,
                private_session: r.incognito_mode.unwrap_or(false),
                item_id: None,
                import_batch_id: None,
            });
        }
    }
    Ok(out)
}

/// UTC → local wall time with the zone's Windows rules for that year
/// (`SystemTimeToTzSpecificLocalTimeEx` + dynamic DST), so a 2006 play uses 2006's DST dates.
/// Offsets are cached per 15-minute UTC slot: every zone offset and transition is a multiple of 15 min.
pub struct LocalTime {
    #[cfg(windows)]
    tz: windows::Win32::System::Time::DYNAMIC_TIME_ZONE_INFORMATION,
    cache: HashMap<i64, i64>,
}

const SLOT: i64 = 15 * 60;

impl LocalTime {
    /// The PC's current zone.
    pub fn system() -> Self {
        #[cfg(windows)]
        {
            let mut tz = Default::default();
            unsafe { windows::Win32::System::Time::GetDynamicTimeZoneInformation(&mut tz) };
            Self {
                tz,
                cache: HashMap::new(),
            }
        }
        #[cfg(not(windows))]
        Self {
            cache: HashMap::new(),
        }
    }

    /// A zone by its Windows key name (`"W. Europe Standard Time"`), for tests.
    #[cfg(all(test, windows))]
    pub fn zone(key: &str) -> Self {
        use windows::Win32::System::Time::EnumDynamicTimeZoneInformation;
        for i in 0.. {
            let mut tz = Default::default();
            assert_eq!(
                unsafe { EnumDynamicTimeZoneInformation(i, &mut tz) },
                0,
                "zone {key}"
            );
            let end = tz
                .TimeZoneKeyName
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(128);
            if String::from_utf16_lossy(&tz.TimeZoneKeyName[..end]) == key {
                return Self {
                    tz,
                    cache: HashMap::new(),
                };
            }
        }
        unreachable!()
    }

    pub fn local(&mut self, utc: DateTime<Utc>) -> NaiveDateTime {
        let slot = utc.timestamp().div_euclid(SLOT) * SLOT;
        #[cfg(windows)]
        let tz = &self.tz;
        let offset = *self.cache.entry(slot).or_insert_with(|| {
            let at = DateTime::from_timestamp(slot, 0).unwrap_or_default();
            #[cfg(windows)]
            return offset_at(tz, at);
            #[cfg(not(windows))]
            {
                use chrono::{Offset, TimeZone};
                chrono::Local
                    .offset_from_utc_datetime(&at.naive_utc())
                    .fix()
                    .local_minus_utc()
                    .into()
            }
        });
        utc.naive_utc() + chrono::Duration::seconds(offset)
    }
}

/// Seconds to add to `at` (UTC) for local wall time in `tz`; 0 if Windows refuses the date.
#[cfg(windows)]
fn offset_at(
    tz: &windows::Win32::System::Time::DYNAMIC_TIME_ZONE_INFORMATION,
    at: DateTime<Utc>,
) -> i64 {
    use chrono::{Datelike, Timelike};
    use windows::Win32::Foundation::SYSTEMTIME;
    use windows::Win32::System::Time::SystemTimeToTzSpecificLocalTimeEx;
    let u = SYSTEMTIME {
        wYear: at.year() as u16,
        wMonth: at.month() as u16,
        wDay: at.day() as u16,
        wHour: at.hour() as u16,
        wMinute: at.minute() as u16,
        ..Default::default()
    };
    let mut l = SYSTEMTIME::default();
    if unsafe { SystemTimeToTzSpecificLocalTimeEx(Some(tz), &u, &mut l) }.is_err() {
        return 0;
    }
    let local = chrono::NaiveDate::from_ymd_opt(l.wYear.into(), l.wMonth.into(), l.wDay.into())
        .and_then(|d| d.and_hms_opt(l.wHour.into(), l.wMinute.into(), l.wSecond.into()));
    local.map_or(0, |t| (t - at.naive_utc()).num_seconds())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::source::{self, fixture};

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[cfg(windows)]
    #[test]
    fn dst_boundaries_use_that_years_rules() {
        let mut cet = LocalTime::zone("W. Europe Standard Time");
        for (u, want) in [
            ("2026-03-29T00:59:59Z", "2026-03-29T01:59:59"), // last second of CET
            ("2026-03-29T01:00:00Z", "2026-03-29T03:00:00"), // CEST starts; 02:xx never exists
            ("2026-10-25T00:59:59Z", "2026-10-25T02:59:59"), // last second of CEST
            ("2026-10-25T01:00:00Z", "2026-10-25T02:00:00"), // 02:xx happens twice
            ("2026-01-15T12:00:00Z", "2026-01-15T13:00:00"),
        ] {
            let got = cet.local(utc(u)).format("%Y-%m-%dT%H:%M:%S").to_string();
            assert_eq!(got, want, "{u}");
        }
        // US DST began the first Sunday of April in 2006, the second Sunday of March from 2007:
        // mid-March 2006 is still PST (UTC-8), mid-March 2007 already PDT (UTC-7).
        let mut pt = LocalTime::zone("Pacific Standard Time");
        let mut h = |u| pt.local(utc(u)).format("%H:%M").to_string();
        assert_eq!(h("2006-03-15T20:00:00Z"), "12:00");
        assert_eq!(h("2007-03-15T20:00:00Z"), "13:00");
    }

    #[test]
    fn parses_only_allow_listed_music_rows() {
        let e = source::read(&fixture()).unwrap();
        let p = parse(&e.streaming, &mut LocalTime::system()).unwrap();
        assert_eq!(
            (p.events.len(), p.skipped),
            (16, 2),
            "music; podcast + audiobook"
        );
        assert_eq!(p.events.iter().filter(|e| e.private_session).count(), 1);
        let old = p
            .events
            .iter()
            .find(|e| e.ended_at_utc == "2026-03-02T09:00:00Z");
        let old = old.unwrap();
        assert_eq!(old.track_name, "Quiet Engine");
        assert_eq!(
            old.platform.as_deref(),
            Some("Windows 10 (10.0.19041; x64)")
        );
        assert!(
            p.events
                .iter()
                .all(|e| e.source == Source::Export && e.ended_at_local.is_some())
        );

        let uri = "spotify:track:AAAAAAAAAAAAAAAAAAAAAA";
        for body in [
            "{}".to_owned(),
            r#"[{"ms_played": 1}]"#.to_owned(),
            format!(r#"[{{"ts": "yesterday", "ms_played": 1, "spotify_track_uri": "{uri}"}}]"#),
            format!(
                r#"[{{"ts": "2026-01-01T00:00:00Z", "ms_played": -5, "spotify_track_uri": "{uri}"}}]"#
            ),
        ] {
            let f = File {
                name: "Streaming_History_Audio_x.json".into(),
                bytes: body.clone().into(),
            };
            let e = parse(&[f], &mut LocalTime::system()).err().unwrap();
            assert_eq!(e.code, crate::error::Code::ImportUnrecognized, "{body}");
            assert!(
                e.detail
                    .unwrap()
                    .starts_with("Streaming_History_Audio_x.json: ")
            );
        }
    }
}

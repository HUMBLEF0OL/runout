//! Pure play accumulation (TSD §4.5): feed it what the media session shows, get back each finished
//! play. `ms_played` = time spent in `Playing` (pauses excluded; a seek is just another `Playing`
//! snapshot, so it adds nothing). A play closes on a track change, a replay of the same track
//! (timeline back to the start) or session end; under 1 s is noise.
use chrono::{DateTime, Utc};

/// Plays shorter than this are dropped (TSD §4.5 noise filter).
pub const MIN_MS: i64 = 1000;

/// A timeline position this close to the start, after being further in, is a replay (repeat-one,
/// "previous" restart or seek to 0): Spotify logs each as a new play, so does the logger.
// ponytail: position-based; a seek to 0:01 also counts as a replay, which matches Spotify's own rows.
const RESTART_MS: i64 = 3000;

/// Snapshots arrive at least every `poll::POLL` (5 s). A longer silence (PC asleep, hibernated) or
/// the wall clock moving backwards ends the play at the last snapshot seen: the gap is never play time.
// ponytail: generous so a db lock held by a long import doesn't split a play; sleeps under 5 min still count.
const MAX_GAP_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Playing,
    Paused,
    Stopped,
}

/// What the session shows for the current track (SMTC has no Spotify URI).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    /// Timeline position when the snapshot was taken (0 when the app doesn't report it).
    pub position_ms: i64,
}

impl Track {
    fn same(&self, o: &Track) -> bool {
        (&self.title, &self.artist, &self.album) == (&o.title, &o.artist, &o.album)
    }
}

/// A finished play, ready for `importer::events::insert`.
#[derive(Debug, Clone, PartialEq)]
pub struct Closed {
    pub track: Track,
    pub ms_played: i64,
    /// The moment playback of this track last stopped (or the close time if it was still playing).
    pub ended_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct Accumulator {
    track: Option<Track>,
    /// Time in `Playing` before `playing_since`.
    ms: i64,
    playing_since: Option<DateTime<Utc>>,
    last_played: Option<DateTime<Utc>>,
    /// Furthest timeline position seen for this play (replay detection).
    pos_ms: i64,
    /// Time of the previous snapshot (sleep / clock-jump detection).
    last_seen: Option<DateTime<Utc>>,
}

impl Accumulator {
    /// One snapshot at `at`: `None` = no session (closed). Returns the play this snapshot finished.
    pub fn observe(
        &mut self,
        now: Option<(Track, PlayState)>,
        at: DateTime<Utc>,
    ) -> Option<Closed> {
        let gap = self.clamp(at) != at;
        let gap_closed = if gap { self.close(at) } else { None };
        let changed = match (&self.track, &now) {
            (Some(cur), Some((t, _))) => {
                !cur.same(t) || (t.position_ms < RESTART_MS && self.pos_ms >= RESTART_MS)
            }
            (None, None) => false,
            _ => true,
        };
        let closed = if changed { self.close(at) } else { None }.or(gap_closed);
        self.last_seen = Some(at);
        let Some((track, state)) = now else {
            return closed;
        };
        self.pos_ms = self.pos_ms.max(track.position_ms);
        if changed {
            self.track = Some(track);
        } else if let Some(cur) = &mut self.track {
            cur.duration_ms = track.duration_ms; // can arrive after the title
        }
        match (state, self.playing_since) {
            (PlayState::Playing, None) => self.playing_since = Some(at),
            (PlayState::Paused | PlayState::Stopped, Some(since)) => {
                self.ms += (at - since).num_milliseconds().max(0);
                self.playing_since = None;
                self.last_played = Some(at);
            }
            _ => {}
        }
        closed
    }

    /// Time played so far on the current track.
    pub fn ms_played(&self, at: DateTime<Utc>) -> i64 {
        self.ms
            + self
                .playing_since
                .map_or(0, |s| (at - s).num_milliseconds().max(0))
    }

    /// `at`, or the last snapshot time when `at` is after a sleep-sized gap or before it.
    fn clamp(&self, at: DateTime<Utc>) -> DateTime<Utc> {
        match self.last_seen {
            Some(l) if at < l || (at - l).num_milliseconds() > MAX_GAP_MS => l,
            _ => at,
        }
    }

    pub fn track(&self) -> Option<&Track> {
        self.track.as_ref()
    }

    /// Finish the current track at `at` (track change, session end, logger paused or turned off).
    pub fn close(&mut self, at: DateTime<Utc>) -> Option<Closed> {
        let at = self.clamp(at);
        let ms = self.ms_played(at);
        let ended_at = if self.playing_since.is_some() {
            Some(at)
        } else {
            self.last_played
        };
        let track = std::mem::take(self).track?;
        (ms >= MIN_MS).then(|| Closed {
            track,
            ms_played: ms,
            ended_at: ended_at.unwrap_or(at),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PlayState::*;

    fn t(s: f64) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(1_770_000_000_000 + (s * 1000.0) as i64).unwrap()
    }

    fn song(name: &str) -> Track {
        Track {
            title: name.into(),
            artist: "Fixture Artist".into(),
            album: "Fixture Album".into(),
            duration_ms: 200_000,
            position_ms: 0,
        }
    }

    /// Feed (second, snapshot) steps; collect (title, ms_played, ended_at second) of every close.
    fn run(steps: &[(f64, Option<(&str, PlayState)>)]) -> Vec<(String, i64, f64)> {
        let mut a = Accumulator::default();
        let mut out = vec![];
        for (s, snap) in steps {
            if let Some(c) = a.observe(snap.map(|(n, st)| (song(n), st)), t(*s)) {
                let end = (c.ended_at - t(0.0)).num_milliseconds() as f64 / 1000.0;
                out.push((c.track.title, c.ms_played, end));
            }
        }
        out
    }

    #[test]
    fn accumulation_table() {
        let a = |n: &str, ms: i64, end: f64| (n.to_string(), ms, end);
        for (name, steps, want) in [
            (
                "play, pause, play, track change",
                vec![
                    (0.0, Some(("A", Playing))),
                    (10.0, Some(("A", Paused))),
                    (20.0, Some(("A", Playing))),
                    (45.0, Some(("B", Playing))),
                ],
                vec![a("A", 35_000, 45.0)],
            ),
            (
                "seeks (repeated Playing snapshots, poll ticks) add nothing",
                vec![
                    (0.0, Some(("A", Playing))),
                    (5.0, Some(("A", Playing))),
                    (10.0, Some(("A", Playing))),
                    (30.0, None),
                ],
                vec![a("A", 30_000, 30.0)],
            ),
            (
                "track change while paused ends at the pause, next starts paused",
                vec![
                    (0.0, Some(("A", Playing))),
                    (20.0, Some(("A", Paused))),
                    (50.0, Some(("B", Paused))),
                    (60.0, Some(("B", Playing))),
                    (65.0, None),
                ],
                vec![a("A", 20_000, 20.0), a("B", 5_000, 65.0)],
            ),
            (
                "stopped counts like paused",
                vec![
                    (0.0, Some(("A", Playing))),
                    (12.0, Some(("A", Stopped))),
                    (99.0, None),
                ],
                vec![a("A", 12_000, 12.0)],
            ),
            (
                "under 1 s is noise",
                vec![
                    (0.0, Some(("A", Playing))),
                    (0.999, Some(("B", Playing))),
                    (1.999, Some(("C", Playing))),
                ],
                vec![a("B", 1_000, 1.999)],
            ),
            (
                "never played → nothing",
                vec![(0.0, Some(("A", Paused))), (100.0, None)],
                vec![],
            ),
        ] {
            assert_eq!(run(&steps), want, "{name}");
        }
    }

    #[test]
    fn same_track_replayed_back_to_back_is_two_plays() {
        // TSD §4.7: a track repeated back-to-back; the timeline returning to the start splits it
        let at = |pos_s: i64| {
            Some((
                Track {
                    position_ms: pos_s * 1000,
                    ..song("A")
                },
                Playing,
            ))
        };
        let mut a = Accumulator::default();
        assert_eq!(a.observe(at(0), t(0.0)), None);
        assert_eq!(a.observe(at(150), t(150.0)), None);
        let first = a.observe(at(1), t(200.0)).unwrap();
        assert_eq!(first.ms_played, 200_000);
        assert_eq!(a.observe(at(100), t(300.0)), None); // later polls don't split again
        let second = a.observe(None, t(400.0)).unwrap();
        assert_eq!(
            (second.track.title.as_str(), second.ms_played),
            ("A", 200_000)
        );
        // a pause near the start then resume is still one play
        let mut b = Accumulator::default();
        b.observe(at(1), t(0.0));
        b.observe(at(2), t(2.0));
        assert_eq!(b.observe(at(2), t(5.0)), None);
    }

    #[test]
    fn sleep_or_clock_jump_never_counts_as_play_time() {
        // PC sleeps 8 h while "Playing": the play ends at the last tick before the gap.
        assert_eq!(
            run(&[
                (0.0, Some(("A", Playing))),
                (30.0, Some(("A", Playing))),
                (30.0 + 8.0 * 3600.0, Some(("A", Paused))),
                (30.0 + 8.0 * 3600.0 + 5.0, None),
            ]),
            vec![("A".to_string(), 30_000, 30.0)]
        );
        // after the gap the same track playing on is a fresh play
        let r = run(&[
            (0.0, Some(("A", Playing))),
            (10.0, Some(("A", Playing))),
            (10_000.0, Some(("A", Playing))),
            (10_020.0, None),
        ]);
        assert_eq!(r[0], ("A".to_string(), 10_000, 10.0));
        assert_eq!(r[1].1, 20_000);
        // wall clock set back an hour mid-play: time before the jump is kept, none is invented
        let r = run(&[
            (3_600.0, Some(("A", Playing))),
            (3_620.0, Some(("A", Playing))),
            (25.0, Some(("A", Playing))),
            (40.0, None),
        ]);
        assert_eq!(r[0], ("A".to_string(), 20_000, 3_620.0));
        assert_eq!(r[1].1, 15_000);
        // exit flush right after resume (no tick in between) also stops at the last tick
        let mut a = Accumulator::default();
        a.observe(Some((song("A"), Playing)), t(0.0));
        a.observe(Some((song("A"), Playing)), t(60.0));
        assert_eq!(a.close(t(9_000.0)).unwrap().ms_played, 60_000);
    }

    #[test]
    fn close_and_live_ms() {
        let mut a = Accumulator::default();
        assert_eq!(a.observe(Some((song("A"), Playing)), t(0.0)), None);
        assert_eq!(a.ms_played(t(7.5)), 7_500);
        assert_eq!(a.track().map(|t| t.title.as_str()), Some("A"));
        let c = a.close(t(29.9)).unwrap();
        assert_eq!(c.ms_played, 29_900);
        assert!(a.track().is_none() && a.close(t(40.0)).is_none());
    }
}

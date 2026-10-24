# M10 design: Insights v3, deep listening analytics

Status: **final design, milestone ready**, 24 Oct 2026 (user goal: "finalize the design and put it in M10"). Brainstormed with the user the same day. Decision **D35**. Plan: [plans/M10-deep-insights.md](../plans/M10-deep-insights.md). Milestone: [MILESTONES M10](../MILESTONES.md#m10--deep-listening-insights-insights-v3).

Mockup: [m10-insights-mockup.html](m10-insights-mockup.html). Open it with Vite on :1420; it loads the app's own stylesheet from there. Screens: [screens/m10-insights/](screens/m10-insights/) (one PNG per tab, with feasibility notes shown). All mock numbers are invented; only the structure is the design.

## 1. Intent

The user wants insight into their own listening as deep as Last.fm's, built into Runout as its own engine. There's no Last.fm account, scrobbling or Last.fm data (D34 stands). They use Insights in two ways, both equally:
- a regular check-in: "how was my week, month or year?";
- digging into one question: "when did I stop listening to this artist, and what replaced them?".

Last.fm is the reference for depth only. A survey of Last.fm's reports on 24 Oct 2026 (weekly/yearly reports, library, Pro) gave the feature list this design starts from. A feasibility audit of Runout's data the same day decided what survives.

What makes Runout's version different:
- **Exact listening time.** Runout has `ms_played` per play, where a Last.fm scrobble is "half the track or 4 minutes".
- **Behaviour Last.fm never sees:** skips, why plays ended, shuffle, device and offline (export fields).
- **The user's own opinions:** ratings, statuses, backlog and tags, set against what they play.

## 2. Goals, non-goals, anti-goals

**Goals**
1. Period reports (week, month, year, all time, custom) with honest comparisons.
2. Long-run trends over the whole history (2015 onwards for this user).
3. Discovery, listening behaviour, records and a free explorer.
4. A clear view of what data Runout has. Re-importing newer exports is a routine step.
5. Everything local, from sources U, S and E (BRD §5), within NFR-04 on 500k events.

**Non-goals (this milestone)**
- BPM, energy, mood, danceability. There's no permitted source: Spotify removed audio features (S6), the audio can't be analysed (protected playback; client files are off limits, S11), and AcousticBrainz is frozen since 2022.
- "How mainstream" scores, global averages and comparisons with other people. These need other listeners' data.
- Where a discovery came from (playlist, radio or search). The export records *how* a track started, not *from where*.
- A free-text "ask anything" box. It needs a language model: no network beyond the allow-list, no remote transmission (NFR-07), no paid tools (NFR-03), and a local model is out of proportion.
- Podcasts and audiobooks (BRD §4.3), and the user's location (`conn_country` stays dropped).

**Anti-goals (a failure even if it works)**
- **Comparing unlike coverage.** For example, "you listened 60% less" because the last export ended and only desktop plays exist since. Every comparison must be coverage-aware (§4.1).
- **Presenting a heuristic as fact.** Eras, "took time from", sessions and start-to-finish albums are estimates and must say so.
- **Any insight touching source A** (Web API, oEmbed, the Spotify cache). That includes durations, years or genres from autofill cache rows.
- **Shareable "profile" cards.** Fingerprint scores stay on screen, never exported or shared, which keeps the accepted risk in BRD §11.1 narrow.
- **Making lifetime views slow** (§6).

## 3. Constraints
- **Sources U, S, E only** (FR-031). The insights module keeps no handle on the cache DB (TSD §4.9, `check.py` "insights no cache").
- **Every card shows its sources and freshness** (FR-032): the "From: …" line, plus the page freshness line.
- **Private-session plays are excluded by default,** as today.
- **Design system:** DESIGN §3 tokens, §5 components, M9 motion recipes. Line tabs with sliding panels (`useSlide`), `Stat` with Now/Before bars, rank rows, rings, `CountUp`, the `.hm-plot` month bars. No new colours, no pills, no uppercase labels.
- **Performance (NFR-04):** screen loads < 500 ms and filter results < 200 ms, on 10k records and 500k events.
- **Contract first:** new or changed commands go into API.md, then `api.ts`, the mocks and Rust, in one change (API.md §7).

## 4. Data and definitions

### 4.1 What is stored (audit, 24 Oct 2026)
All plays are in `listening_event`.

| Fields | Export plays (E) | Desktop plays (S) |
|---|---|---|
| end time, ms played, track / artist / album names, `norm_key`, `counts_as_play` (≥ 30 s) | yes | yes |
| `reason_start`, `reason_end`, `shuffle`, `skipped`, `offline`, `platform`, track URI | yes | NULL |

- **Matching:** desktop plays that an export later covers are linked to their export twin (`reconcile`, ±120 s) and counted once.
- **Identity:** artist and album identity is the **name**; there are no IDs. Collaborations count for the listed artist only.
- **Coverage:** the export covers every device up to its last date. After that, only desktop plays exist until the next export.

**Coverage rule (FR-048, new).** A comparison between two periods uses only sources that cover *both* periods completely.
- If the current period runs past the last export's end, both sides are restricted to desktop plays, and a banner says so ("From 3 Oct, only plays on this PC are counted …").
- If even that is impossible, because the logger wasn't running in the earlier period, the delta is hidden with the reason.
- Totals that aren't comparisons (for example, "41 h this month") show everything there is, with the freshness line.

### 4.2 Definitions (one constant each, in TSD §4.13 style)

**Basic counts**

| Term | Definition |
|---|---|
| Play | `counts_as_play` (≥ 30 s, D25) |
| Listening time | Σ `ms_played` of all events (as today) |
| Period | Local calendar week (Mon–Sun), month or year; "All time"; or a custom date range |
| Previous / same last year / usual | The period before; the same period a year earlier; the median of the last 12 periods of that length |
| Days listened | Local dates with ≥ 1 play |
| Streak | Consecutive covered local dates with ≥ 1 play. A "no data" date (§4.4) breaks nothing and counts nothing: it is skipped. An all-device or this-PC date with no plays breaks the streak |
| Music ratio | Distinct tracks (`norm_key`), albums (album + artist name) and artists (artist name) with ≥ 1 play in the period; plus tracks per artist and albums per artist |

**Discovery**

| Term | Definition |
|---|---|
| New artist | An artist whose first play in all stored data falls in the period |
| First heard | That first play's time and track |
| Stick rate | Share of new artists with ≥ 3 plays in the 90 days that start 30 days after their first play (estimate; labelled) |

**Behaviour (export plays only)**

| Term | Definition |
|---|---|
| Finished | `reason_end = 'trackdone'`, export plays only |
| Skip rate | `skipped = 1` ÷ events **with `skipped` not null**. This fixes G-45: today's denominator includes desktop plays, which have no skip flag |
| Quick bail | An export event with `ms_played < 10000` and `reason_end = 'fwdbtn'`; tracks ranked by count (≥ 3) |
| Started by you | `reason_start ∈ {clickrow, playbtn}`. "Followed on" = `trackdone`. "Back" = `backbtn`. The rest is "other". Export only; labelled as *how* a track started, not *where* |
| Device | `platform` classified by prefix: `android`/`ios` → Phone; `windows`/`osx`/`linux` → Computer; `web`/`webplayer` → Web player; else Other. Desktop-log plays → "This PC". Rules live in one Rust function with tests over the KB's seen values |

**Sessions and heuristics (estimates, labelled)**

| Term | Definition |
|---|---|
| Session | Plays in end-time order, split where the gap from one play's end to the next play's estimated start (end − ms_played) is > 30 min. Estimated starts are labelled |
| Deep dive | ≥ 60 min of consecutive plays within one session where ≥ 80% of the time is one artist (or one album) |
| Era (heuristic) | Months grouped while the same one or two artists hold ≥ 15% of time and ≥ 1.5× their lifetime share. Runs shorter than 3 months merge into a neighbour. Named after the artist(s) |
| Arc | An artist's share of monthly time over 36 months. **Rising:** last-3-month share ≥ 2× the previous 6. **Fading:** ≤ 0.5×, with a past share ≥ 3%. **Back again:** ≥ 18 months under 0.5%, then ≥ 2% in a recent month. **Constant:** in the top 20 in ≥ 75% of years |
| Took time from (heuristic) | For a fading artist, the artists whose share rose most over the same months, listed as "may have taken time from" |
| Album start to finish | Only for library albums with a track list (TSD §4.9). Every linked track was played (≥ 30 s) in one session. Other albums show nothing |
| On repeat | A track with ≥ 5 plays in one local day |

**Fingerprint (0–100 each; compared only with the user's own 12-month average; never shared)**

| Measure | Definition |
|---|---|
| Regularity | Days listened ÷ days in period (coverage gaps excluded) |
| Discovery | Time on new artists ÷ time |
| Focus | Time on the period's top 10 artists ÷ time |
| Variety | Normalised Shannon entropy of time by artist |
| Repeat | Plays of tracks already played before the period ÷ plays |

**Records and tags**

| Term | Definition |
|---|---|
| Records | First play in the data; most-played track, artist and album ever; longest streak; biggest day (time); biggest year; most artists in a month; most loyal (top 20 in the most years); forgotten favourites (top 10 in some year, no play in ≥ 730 days) |
| Next milestone | Next multiple of 5,000 plays; ETA at the last-90-day pace |
| Top tags (U) | For plays linked to a library record (`item_id`, or via `by_artist` / `on_album` links), the record's tags, weighted by listening time. Coverage shown: tagged time ÷ total time |

### 4.3 Changes to stored data
Each change is a migration plus a logger change, with tests.
1. **Store track duration from the desktop logger:** `listening_event.duration_ms`, nullable. The logger already reads it (`logger/session.rs`) and drops it. It's source S, so allowed. It enables "how far into each track you got" for desktop plays. A per-`norm_key` duration (max seen) lets export plays of tracks also heard on the desktop use it. Coverage is shown; there's no guessing for tracks never heard on the desktop.
2. **Store shuffle from the desktop logger:** `listening_event.shuffle`, which SMTC exposes (kb/spotify.md, verified 18 Oct 2026). Desktop plays then join the shuffle card. Repeat and track number aren't needed.
3. **Logger running intervals:** a `logger_run(start_utc, end_utc, reason)` table written on start, stop, pause and resume. It lets the coverage view tell "no data" apart from "logger was off", and keeps streaks honest.
4. **Rollups for speed,** kept by triggers like 0005:
   - `listening_day` (local date × private flag → ms, events, plays);
   - `listening_month_artist` (month × artist name × private flag → ms, plays);
   - `artist_first_play` (artist name → first event id, time).

   Rebuilt in one pass after an import, like today.

### 4.4 Coverage model
- Per source: the earliest and latest event; the export batches and their date ranges; logger intervals (from 4.3.3).

**Every local date has one coverage class:**

| Class | Which dates | A date with no plays means |
|---|---|---|
| **All devices** | From the first export play's date to the last export play's date | You didn't listen. It counts in "days listened", breaks streaks and lowers Regularity |
| **This PC only** | After the last export date, where a logger interval touches the date. Before the `logger_run` migration, the logger counts as running on any date with a desktop play | You didn't listen *on this PC* |
| **No data** | Every other date: before the first play, or after the export with the logger off | Unknown. Hatched in charts, skipped by streaks and Regularity, never shown as silence |

**Which class each figure uses:**
- **All-time figures** (records, longest streak, Regularity by year, calendar shading) use **all-device dates only**. Importing a newer export extends them over the newly covered dates. Already-covered dates never change meaning, so past records stay stable.
- **Figures for a period that reaches past the last export date** use the same rule as comparisons (§4.1). "Days listened", "current streak" and the fingerprint are computed on this-PC data for the whole period and labelled "on this PC". That covers the earlier part of the period too, so both halves count the same way. Without that label, a figure counts all devices.
- **Scope of the comparison rule.** FR-048 applies to **every** comparison in the app, not only Insights. Home's period strip ("than the period before", from `get_listening_insights`) uses the same coverage function and shows the same short note. `get_listening_insights` keeps its `range` parameter for Home and gains the `coverage` block (WI 1). The `Range` type stays for Home, the Collection tab and Recent plays; the new grain and period header is for the listening tabs of Insights only.

## 5. Screens (SC-09, Insights)

**Page header**
- Title, then the grain segment: Week · Month · Year · All time · Custom… (a custom range picker).
- ‹ period › stepper.
- "Compare with": the previous period · the same period last year · your usual.
- These replace today's 30 days / 12 months / All time range control on the listening tabs. The Collection tab keeps its own range control. Deep links: `#/insights/<tab>?grain=month&at=2026-09&vs=prev`.

**Tabs** (line tabs, sliding panel). Existing Collection, Your take and Year in review keep their content. Year in review gains the year-grain report cards.

| Tab | Cards (mockup PNG) | Data |
|---|---|---|
| **Report** (default) ([1](screens/m10-insights/1-report.png)) | Coverage banner when needed; 6 stat tiles (listening time, plays, artists, new artists, days listened + streak, tracks finished); every day of the period vs the previous period (ghost line); "the period in a few lines" (rule-based sentences, §5.1); when you listened (24-hour clock vs 6-month baseline, plus weekdays); top new artist (lead card with first-heard line and Save to backlog); music ratio; fingerprint (radar vs your own average); top tags (U, with coverage); most played (artists / albums / tracks: lead + ranks 2–10, rank movement, NEW, 12-month sparkline) | U, S, E |
| **Trends** ([2](screens/m10-insights/2-trends.png)) | Lifetime months (hours / plays / artists; brush a range to set the Report's custom period; gaps hatched); eras strip; artist arcs (Rising / Fading / Back again / Constants, with "may have taken time from"); fingerprint by year (5 small multiples); your tags month by month; calendar per year (minutes per day; a day opens Recent plays for that date); biggest day, longest streak, days without music | S, E (+U tags) |
| **Discovery** ([3](screens/m10-insights/3-discovery.png)) | Stats (new artists this year, stick rate, time on new artists, artists known); new artists per month split stuck/dropped; how you started tracks (export only); discovery log (artist, first played, with which track, plays since, now: in rotation / dropped / back again) | S, E |
| **How you listen** ([4](screens/m10-insights/4-habits.png)) | Banner naming the export-only coverage; how plays ended; skips by hour; quick bails; shuffle over time (desktop joins after 4.3.2); where you listen (device classes, offline share); finished tracks (+ "how far in" after 4.3.1, with its coverage); sessions (count, typical, longest, spread); deep dives | E (+S after 4.3) |
| **Records** ([5](screens/m10-insights/5-records.png)) | All-time records (8 cards); on this day (earlier years); forgotten favourites with "Play again" (opens Spotify like other "Open Spotify" actions) | S, E |
| **Genres & decades** ([6](screens/m10-insights/6-genres.png)) | **Only after U-11 is approved** (§7). Hidden until then; no placeholder tab | + open database |
| **Explore** ([7](screens/m10-insights/7-explore.png)) | Builder: up to 3 artists / albums / tracks, from–to, grain (week / month / year), measure (hours / share / plays); filters: hours of day, weekdays, and (export only) device and shuffle; a line chart, a table, "Save this view" (stored like saved filters), "Copy as CSV". Ready-made questions fill the builder; there's no free text | U, S, E |
| **Your data** ([8](screens/m10-insights/8-data.png)) | Coverage strip per source with gaps; import history (date, range covered, new plays, already had); "Import a newer export" (the existing 3-step import); monthly reminder to request an export (off by default; a local notification on app open, never network); private-session exclusion switch (moves from where it lives today, same setting) | metadata |

Other rules:
- Every tab has loading (skeleton), empty ("Import your Spotify history to see this", plus the action), offline (all local, so unaffected) and error states (DESIGN §6).
- Feasibility tags in the mockup are annotations only. The product shows the caveats as the plain notes written in the mockup's card text.

### 5.1 "The period in a few lines"
- Rule-based templates over computed numbers, at most 4 lines, in a fixed priority. Each rule needs a threshold to fire:
  1. change in listening time ≥ 10%;
  2. a new artist entering the top 5;
  3. an artist leaving the top 10 after ≥ 6 months in it;
  4. busiest hour share ≥ 30%;
  5. a streak ≥ 14 days;
  6. a record broken.
- No AI model. Wording follows DESIGN copy rules (no em dashes, one middle dot per line).

## 6. Performance
- **Targets** (500k-event reference, `perf_listening`): every tab's load < 500 ms; Explore query < 200 ms; lifetime months < 100 ms (from `listening_day` / `listening_month_artist`).
- **Rollups:** rebuilt within the import transaction; the import of 10 years stays < 60 s.
- **Proof:** measured numbers are recorded in kb/perf-integrity.md.

## 7. Genres & decades (gated: U-11, O8)
- **Approach:** look up each artist and album **name** once in MusicBrainz (genres via community tags, first release year, artist area). Fall back to Wikidata where MusicBrainz has nothing.
- **Privacy:** no account and nothing about the user. Only names are sent, over HTTPS, at ≤ 1 request/s (MusicBrainz rule). The user-agent names the app.
- **Storage:** results stay forever in a separate `enrich` table set, as a new data source with its own letter (proposed **M**, "music database"). It can be deleted with one switch, which also stops lookups.
- **Unlocks:** top genres, genres over time, music by decade, the average age of your music, the share released this year, where your artists are from, and coverage. Genres also fill gaps in the top tags card.

**Needs before building (the user's decision first):**
- **U-11:** the user approves sending names to an open database. That breaks "nothing leaves this PC except to Spotify" (`check.py` allowed hosts, NFR-07 wording) and adds a source to BRD §5.1.
- **T6:** verify licensing (core data CC0; tags believed CC BY-NC-SA, so non-commercial with attribution), the rate limits, and the match rate on the user's real artists and albums (target ≥ 85% of listening time).

If U-11 is declined, WI 9 is dropped. Nothing else depends on it.

## 8. Calls made in this design (not asked of the user)
Check these while reading:
- **Tab names and order:** Report, Trends, Discovery, How you listen, Records, Explore, Your data. Existing tabs last.
- **New controls:** Week / Month / Year / All time / Custom replaces 30 days / 12 months / All time; comparison choice: previous, same last year, usual.
- **Thresholds:** every number in §4.2 and §5.1, including the 30-min session gap, the stick-rate window and the era rule.
- **Fingerprint:** five measures named Regularity, Discovery, Focus, Variety and Repeat.
- **Export reminder:** off by default.
- **Genres tab:** hidden, not teased, until U-11.
- **Rollups,** and the three logger/schema changes in §4.3.
- **Coverage classes (§4.4)** from the builder check: all-time figures use all-device dates only, and periods past the export date are this-PC only and labelled so.
- **FR-048 covers Home too:** its period strip gets the same coverage function and note.

## 9. Acceptance (summary; full list in MILESTONES M10)
- **Every card computes from U/S/E only.** The A-isolation check stays green.
- **Hand-verified values:** each definition in §4.2 has a Rust unit test on a fixture with hand-computed values, and the export synthetic fixture covers each `reason_*` value.
- **Coverage rule:** a test where the current period passes the export end shows the banner and a desktop-only delta, never an all-device vs desktop delta.
- **Performance:** NFR-04 numbers measured on 500k events, recorded in the KB.
- **Journeys:** keyboard journeys for the new tabs; every state rendered; reduced motion respected.

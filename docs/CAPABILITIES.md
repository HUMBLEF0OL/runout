# Capability Map — Runout

What the user can do, where it is built, and which requirements it satisfies. Source: [BRD](../BRD.md). Build detail: [TSD](TSD.md). Plan: [Milestones](MILESTONES.md). Screens: [DESIGN.md §6](DESIGN.md#6-screen-inventory-m1-designs-all-of-these).

Data sources: **U** user-authored · **S** desktop logger · **E** Spotify data export · **A** Spotify Platform (embed / Web API).

## MVP — your personal music notebook (no Spotify login)

| ID | Capability (user terms) | Milestones (design · UI · backend · integration) | FRs | Sources |
|---|---|---|---|---|
| CAP-01 | Save an album, track or artist by pasting a Spotify link (URL, URI or short link) | M1 · M2 · M3 · M6 | FR-001, 002, 011, 025 | U (+A title prefill, promoted) |
| CAP-02 | Edit or delete any record; link albums, tracks and artists; link alternate versions | M1 · M2 · M3 · M6 | FR-003, 004 | U |
| CAP-03 | Duplicates are blocked, with a shortcut to open the existing record | M1 · M2 · M3 · M6 | FR-004 | U |
| CAP-04 | See artwork and Spotify’s player card on each item | M1 · M2 · M3 · M6 | FR-017, 044 | A (not stored) |
| CAP-05 | Open any item in Spotify | M1 · M2 · M3 · M6 | FR-016 | U |
| CAP-06 | Set status: Planned, In Progress, Explored, Not for me, Abandoned — with history | M1 · M2 · M3 · M6 | FR-005, 006 | U |
| CAP-07 | Flag Favourite / Revisit; set priority | M1 · M2 · M3 · M6 | FR-007, 010 | U |
| CAP-08 | Rate (½–5★), review, add notes, tags, “recommended by” | M1 · M2 · M3 · M6 | FR-008, 009, 010 | U |
| CAP-09 | Work through a backlog: filter, sort, reorder, remove without deleting | M1 · M2 · M3 · M6 | FR-012 | U |
| CAP-10 | Search everything (titles, notes, reviews, tags) | M1 · M2 · M3 · M6 | FR-014 | U |
| CAP-11 | Rediscover: revisit list, top-rated, stale backlog, incomplete reviews, saved filters | M1 · M2 · M3 · M6 | FR-013 | U |
| CAP-12 | Home dashboard: counts, in progress, recent activity | M1 · M2 · M3 · M6 | FR-015 | U |
| CAP-13 | *(Optional fallback, D33)* Share a song from Spotify on iPhone over the home Wi-Fi straight into the backlog (name confirmed later on the PC) | M1 · M2 · M3 · M6 | FR-018 | U |
| CAP-14 | Export your data (JSON with provenance) | M1 · M2 · M3 · M6 | FR-020, 025 | U |
| CAP-15 | Automatic weekly backups (max 3, compressed), restore with verification | M1 · M2 · M3 · M6 | FR-021 | U |
| CAP-16 | Delete a record or the whole library; choose what uninstall removes | M1 · M2 · M3 · M6 | FR-022, 024 | U |
| CAP-17 | Use everything offline | M1 · M2 · M3 · M6 | FR-019, 023 | U |
| CAP-35 | See your own success metrics (usage counted on your PC, never sent) | M1 · M2 · M3 · M6 | FR-045 | U |

## Release 2 — your listening story (no Spotify login)

| ID | Capability | Milestones (design · UI · backend · integration) | FRs | Sources |
|---|---|---|---|---|
| CAP-18 | Automatic diary of what you play on your PC (tray logger, with pause) | M1 · M2 · M4 · M7 | FR-026 | S |
| CAP-19 | Import your lifetime Spotify history from all devices | M1 · M2 · M4 · M7 | FR-027, 029 | E |
| CAP-20 | No double counting between PC log and imported history | M1 · M2 · M4 · M7 | FR-028 | S, E |
| CAP-21 | Plays linked to your library records | M1 · M2 · M4 · M7 | FR-030 | U, S, E |
| CAP-22 | Playlist check-up: duplicates, low-rated and never-played tracks (view only) | M1 · M2 · M4 · M7 | FR-033 | E, U |
| CAP-23 | Listening stats: top artists/albums/tracks by time, skips, time-of-day, trends; unfinished albums (for albums whose tracks you’ve entered) | M1 · M2 · M4 · M7 | FR-031, 032 | S, E (+U) |
| CAP-24 | Collection stats: backlog growth, completion, rating spread, tag trends | M1 · M2 · M4 · M7 | FR-031 | U |
| CAP-25 | Opinions vs habits: rated-vs-played, never-played backlog, loved-but-forgotten | M1 · M2 · M4 · M7 | FR-031 | U, S, E |
| CAP-26 | Every stat shows its data sources and how fresh they are | M1 · M2 · M4 · M7 | FR-032 | — |
| CAP-27 | Year in review on demand | M1 · M2 · M4 · M7 | FR-034 | U, S, E |

## Release 3 — connected to Spotify (login, renewed every 6 months)

| ID | Capability | Milestones (design · UI · backend · integration) | FRs | Sources |
|---|---|---|---|---|
| CAP-28 | Connect / disconnect Spotify; reconnect prompt; app keeps working if Spotify is unavailable | M1 · M2 · M5 · M7 | FR-035, 036, 043 | A |
| CAP-29 | Clean up your playlists using your ratings and tags: keep/cut, remove duplicates, preview, apply, undo | M1 · M2 · M5 · M7 | FR-037, 038 | A, U (ratings/tags only) |
| CAP-30 | Make a Spotify playlist from any saved filter | M1 · M2 · M5 · M7 | FR-039 | U → A |
| CAP-31 | Autofill artist, album, year, track list when you paste a link (confirm to keep) | M1 · M2 · M5 · M7 | FR-040 | A → U on confirm |
| CAP-32 | See your Spotify Top 50 (4 weeks / 6 months / 1 year) and add to backlog | M1 · M2 · M5 · M7 | FR-041 | A |
| CAP-33 | See your last 50 plays from any device; save or rate them | M1 · M2 · M5 · M7 | FR-041 | A |
| CAP-34 | Save or remove items in your Spotify library | M1 · M2 · M5 · M7 | FR-042 | A |
| CAP-36 | Capture from the phone anywhere: add a song or album to the “Runout Inbox” Spotify playlist; Runout moves it into the backlog (name confirmed on the PC) | M7 (design, UI and backend built in M7; D33) | FR-046 | A → U on confirm |

## Not capabilities (explicitly excluded)
In-app audio playback (beyond Spotify’s own embed) · BPM/mood/energy · Spotify recommendations · popularity · other users’ playlists · podcasts/episodes · stats from Spotify API data · reading Spotify’s internal files · cloud sync · multiple users · native mobile app.

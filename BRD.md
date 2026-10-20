<!-- Generated from BRD.docx by scripts/brd_to_md.py. Edit the .docx, then regenerate. -->

# Business Requirements Document (BRD)

**Runout — A Local-First Spotify Companion**

| Attribute | Value |
|---|---|
| Document version | 0.7 |
| Status | Scope finalized; one accepted compliance risk documented (section 11.1); ready for technical design |
| Date | 19 October 2026 |
| Supersedes | Version 0.6 |
| Product type | Personal productivity / media management application |
| Primary user | Individual music listener (single user; Spotify Development Mode allows up to 5) |
| Platform | Windows desktop; iOS quick-capture via Shortcuts |
| Data strategy | Local-first; no hosted backend |
| Cost target | ₹0 new project expenditure (existing Spotify Premium is a user expense) |
| Playback provider | Spotify |

## 0. Change History

### 0.1 Changes in Version 0.7 (agentic delivery)

| Ref | Change |
|---|---|
| P1 | Milestone efforts re-estimated for agentic development (D31): Claude Code agents build and review in parallel. M3 took about 3 hours against a 3–4 week estimate. Section 14 now gives build hours plus the real-world waits (user actions, daily use, soak runs) that agents cannot shorten. |
| P2 | O2 partly resolved (S14, section 16 assumption): the Extended streaming history export arrived about 1 day after the request for one account (19 Oct 2026). Account data export delivery and allowed request frequency remain open. |
| P3 | O1 resolved (S1): a Premium Student plan qualifies for Development Mode. Tested 19 Oct 2026 with the user's own developer app: PKCE login and GET /me returned 200 with product = premium. |
| P4 | O4 and O5 resolved by the technical design and built in M5: playlist snapshots kept 30 days, Spotify cache entries 7 days, both swept at startup and daily; disconnect deletes both at once. |
| P5 | U-5 no longer gates M7 (D32, user decision 20 Oct 2026): M7 starts once M6 code, installer and U-3/U-4 pass. Two weeks of daily use and the section 15 metrics run in parallel; their review is the MVP’s final sign-off. |

### 0.2 Changes in Version 0.6 (design-first delivery)

| Ref | Change |
|---|---|
| P1 | UI direction D · Pulse chosen: Last.fm-inspired listening story with the user’s opinions, Spotify-compatible dark theme (D29). Design spec: docs/DESIGN.md. |
| P2 | Delivery order changed to design, then UI on mock data, then backend, then integration (D30). Milestones renumbered M0–M8 (section 14); dates in sections 11, 12, 15 and 18 follow. |
| P3 | New hard stop: the user approves all screen designs before UI build (end of M1). |

### 0.3 Changes in Version 0.5 (document review fixes)

| Ref | Change |
|---|---|
| R1 | Playlist Studio uses only the user’s ratings and tags, never listening data, keeping the 11.1 separation (D27). |
| R2 | All Spotify-provided data, including playlist snapshots, is stored outside the backed-up library; uninstall cleanup covers cache, snapshots, tokens and web-view data; default backups live outside application data (D28). |
| R3 | oEmbed: only a title the user confirms is stored; embeds run in a non-persistent web view. Embeds Terms clause references corrected. |
| R4 | Phone captures are stored as “name needed” until the user confirms a name. |
| R5 | Duplicate links are blocked with “open existing” (was: warn). |
| R6 | Success metrics made measurable: local usage counters (FR-045); capture-progress metric over 14 days. |
| R7 | “Unfinished albums” defined from user-entered track lists; logger private-session limitation and pause toggle; reconciliation by track name with artist tie-breaker, one-to-one. |
| R8 | Priorities aligned (FR-018 P0, FR-042 P1); O3 and O6 resolved by the technical design; NFR-12/13/14 made testable; S6 and S14 bases clarified. |

### 0.4 Changes in Version 0.4

| Ref | Change |
|---|---|
| N1 | Product named Runout (was the working title “Personal Music Manager”). Versions 0.1–0.3 merged into this single document. |

### 0.5 Changes in Version 0.3 (audit fixes)

| Ref | Change |
|---|---|
| C1, C2, M7 | Added the Promotion Rule (5.2): Spotify-provided data becomes user data only when the user explicitly adds or confirms it. |
| C3 | Export-data compliance documented as an accepted risk with mitigations (11.1); no longer presented as settled. |
| C4 | Uninstall removes Spotify-provided data always; asks before removing the user library; never touches backups in user-chosen folders. |
| H1 | Embeds: lists are text-first with at most 20 compact embeds per page; full embed on detail pages only. |
| H2 | One canonical play rule for all sources: ≥30 seconds played. (The export has no track duration, so the “half the track” rule cannot apply to it.) |
| H3 | Reconciliation rule for logger vs export plays. |
| H4 | Freshness of mobile data stated explicitly in value statements and on the Insights page. |
| H5 | Revisit is a flag only; removed as a status. |
| H6 | Restored Business Success Metrics (15) and Assumptions (16). |
| H7 | Security requirements for the phone-capture listener (NFR-18). |
| M1–M4 | Podcasts/episodes skipped; private-session plays excluded by default; UTC→local time conversion; spotify.link short links handled. |
| M5 | Measurable performance targets (NFR-04). |
| M6 | Section 11 now states the basis of each finding (Documented / Interpretation / Unverified). |
| L1 | Read-only playlist analysis from the Account Data export added to Release 2. |
| L2–L6 | Other-service links constraint; embed-playback claim removed; proposed retention values; milestone dependency noted; references hyperlinked. |

### 0.6 Changes in Version 0.2

| Area | Change |
|---|---|
| Scope | Added Listening Log & Insights (Release 2) and Spotify-connected features incl. Playlist Studio (Release 3). |
| Data sources | Defined four data sources and separation between user data and Spotify-provided data. |
| Compliance | Spotify-side questions answered from official documentation. |
| Statuses | Added “Not for me” and “Abandoned”. |
| Artwork | Included in MVP via Spotify’s official embed; nothing cached. |
| Mobile | Native app removed; iOS quick-capture via Shortcuts. |
| Removed | Reading Spotify client internal files; Spotify API-derived analytics. |

## 1. Executive Summary

### 1.1 Overview

Runout is a private, local-first Windows application that helps one listener organize, explore, document, review and rediscover their music while Spotify remains the music player.

It manages three first-class entities — Albums, Tracks and Artists — and adds three capabilities on top: a personal Listening Log with insights, a Playlist Studio for refining the user’s own Spotify playlists, and quick capture from the phone.

### 1.2 Product principle

Spotify plays the music; Runout manages the user’s relationship with that music. The product adds independent value — the user’s opinions, intentions and history — and never recreates Spotify’s playback or streaming experience.

### 1.3 Problem statement

- Interesting albums and tracks are forgotten.
- Artists are discovered but never explored beyond one or two songs.
- There is no personal record of what was explored and what was liked.
- Items intended for later become mixed with items already explored.
- Playlists accumulate duplicates and tracks the user no longer wants.
- The user cannot see their real listening habits over time or across devices, because Spotify’s developer access exposes only a small recent window.
### 1.4 Value to the user

| Benefit | What the user gets |
|---|---|
| Never forget music | Capture anything in seconds from PC or phone into a backlog. |
| A to-do list for listening | See what is waiting, in progress, and done; explore artists deliberately. |
| A personal music diary | Ratings, reviews, tags, notes, “who recommended it” — data Spotify does not keep. |
| Effortless rediscovery | One-click lists such as “5★ but not played in a year”, then open in Spotify. |
| Real listening story | Lifetime history: desktop plays live; all devices (including phone) up to the latest Spotify data export. Time per artist, skips, patterns, year in review. |
| Opinions vs habits | “Rated 5★ but played twice”; “played 80 times, never rated”. |
| Cleaner playlists | Find duplicates and dead weight (from Release 2, read-only); decide keep/cut, preview, apply and undo (Release 3). |
| Ownership and privacy | Everything local; no account, no cloud, no cost; export any time. |

## 2. Business Objectives

| ID | Objective | Expected outcome |
|---|---|---|
| BO-01 | Centralize personal music interests | One searchable library for albums, tracks and artists |
| BO-02 | Manage a discovery backlog | Clear visibility of items awaiting exploration |
| BO-03 | Track exploration progress | Distinguish planned, in-progress, explored, not-for-me and abandoned items; flag items to revisit |
| BO-04 | Preserve personal opinions and context | Ratings, notes, tags and reviews recorded |
| BO-05 | Enable intentional rediscovery | Easy return to overlooked or previously enjoyed music |
| BO-06 | Understand personal listening and collection trends | Insights from the user’s own records and own listening history |
| BO-07 | Refine personal playlists | Duplicate-free, intentional playlists maintained with preview and undo |
| BO-08 | Maintain data ownership | Local storage, documented export, backup and restore |
| BO-09 | Avoid recurring expenditure | Free components only; no paid infrastructure |
| BO-10 | Stay compliant with Spotify | Every Spotify-dependent feature follows current Spotify terms and policy |

### 2.1 Success definition

The application succeeds if it becomes a useful daily companion for exploration, reflection and playlist upkeep — not merely another interface for browsing Spotify. Measures are in section 15.

## 3. Target User and Usage Context

### 3.1 Persona — the intentional music explorer

A listener who regularly discovers music on desktop and phone, wants to explore deliberately, values personal organization over social features, and wants local ownership of their data.

### 3.2 Typical use cases

- A friend recommends an album on the phone; the user shares it from Spotify to the backlog.
- The user explores an artist’s albums over several weeks and keeps notes.
- The user reviews an album after listening in Spotify.
- The user finds all items tagged “jazz” and rated 4★ or higher.
- The user imports their Spotify lifetime streaming history and sees their top artists by listening time.
- The user finds duplicate tracks across their playlists from the Account Data export.
- The user cleans up a playlist: removes duplicates and low-rated tracks, previews, applies.
- The user builds a new Spotify playlist from a saved filter.
- The user backs up their library and restores it on a new PC.
## 4. Scope

### 4.1 Release overview

| Release | Theme | Spotify developer access needed? |
|---|---|---|
| MVP | Personal library, exploration workflow, journal, rediscovery, artwork via embed, mobile quick-capture, data management | No |
| Release 2 | Listening Log (desktop logger + Spotify data-export import), Insights, read-only playlist analysis | No |
| Release 3 | Spotify-connected: Playlist Studio write-back, playlist from filter, metadata autofill, Top items, recent plays, library save | Yes — Premium, Development Mode |

### 4.2 In scope

- Album, track and artist records with relationships.
- Exploration statuses with history; revisit flag; priority.
- Personal ratings, reviews, notes, tags, favourite flag, recommendation context.
- Backlog, rediscovery, search, filters, dashboard.
- Artwork and title display through Spotify’s official embed.
- iOS quick-capture through an Apple Shortcut.
- Desktop listening logger via the Windows media session (SMTC).
- Import of Spotify Extended Streaming History and Account Data exports (music only).
- Listening and collection insights derived only from sources U, S and E (section 5).
- Read-only playlist analysis (R2) and Playlist Studio write-back for owned/collaborative playlists (R3).
- Spotify Web API features listed in Release 3, in Development Mode.
- Local storage, export, backup (max 3 copies), restore, delete.
### 4.3 Out of scope

- Playing audio inside the application. Spotify’s own embed widget is shown unaltered as Spotify provides it.
- Replacing Spotify or recreating its core experience.
- Hosted backend, cloud sync, accounts.
- Social features, public profiles, multi-user collaboration.
- Native mobile application.
- Podcasts, episodes and audiobooks (skipped on import).
- Analytics, metrics or profiling derived from Spotify Web API data.
- Features needing removed Spotify endpoints: audio features (BPM, energy, mood), audio analysis, recommendations, related artists, popularity, follower counts, genres.
- Reading other users’ playlist contents.
- Reading, decoding or hooking Spotify client internal files, caches or code.
- Training AI/ML models on Spotify data.
- Paid APIs, paid databases, paid subscriptions to the app.
### 4.4 Future candidates (not committed)

- Last.fm relay for near-real-time phone listening history.
- Progressive web app (PWA) companion served over local Wi-Fi.
- Cross-device sync via user-owned storage (iCloud Drive / OneDrive file).
- Custom fields; per-track tracking inside albums.
- Links to other services (Bandcamp, YouTube Music) — must never be displayed in the same row or set as Spotify content (Spotify Design Guidelines).
- Markdown export (Obsidian / Notion); CSV export.
## 5. Data Sources and Compliance Model

### 5.1 Sources

Every piece of data belongs to one of four sources. The source decides what the application may store and what it may analyse.

| Source | Contents | Release | May store? | May analyse? |
|---|---|---|---|---|
| U — User-authored | Records, ratings, notes, tags, statuses, history, and promoted items (5.2) | MVP | Yes, indefinitely | Yes |
| S — Desktop logger (Windows SMTC) | Title, artist, album, ms played, play state of desktop plays | R2 | Yes | Yes |
| E — Spotify data exports | Extended Streaming History and Account Data (playlists) delivered by Spotify to the user | R2 | Yes (minimized) | Yes — accepted risk, see 11.1 |
| A — Spotify Platform (Web API, oEmbed) | Metadata, artwork, playlists, top items, recent plays | MVP (embed), R3 (API) | Only temporarily, kept current; never for embeds | No — display and user actions only |

Separation rule: Insights may be computed only from sources U, S and E. Source A data is displayed and used to perform user-requested actions; it never feeds statistics, trends or profiles (Spotify Developer Policy III.13).

### 5.2 Promotion rule

Source A data becomes user data (source U) only through an explicit user action:

- Pasting a Spotify link, or choosing “Add to backlog” on any Spotify-provided item, creates a user record containing the Spotify link and the display name as confirmed by the user. A link received from the phone is stored with the link only and marked “name needed” until the user confirms a name.
- Autofilled fields (artist, album, year, track list) are shown from the expiring cache and become user data only when the user confirms or edits them.
- Each record field stores its provenance (user / promoted). Unconfirmed cached values never enter the library, so exports and deletions can separate them.
- Unconfirmed cached fields are deleted on cache expiry and on disconnect.
### 5.3 Data minimization

- On import, discard IP address, user agent and username; keep only fields used by the product.
- Podcast, episode and audiobook entries are skipped.
- Raw export files are not retained after a successful import.
- In the MVP, no Spotify-provided artwork or metadata is written to disk except a title the user confirms (5.2). Embeds run in a non-persistent web view, so the browser cache does not keep them.
## 6. Product Modules

### 6.1 MVP modules

#### 6.1.1 Home Dashboard

- Recently added, in progress, recently explored, flagged for revisit.
- Counts by type and status.
- Quick-add actions.
- Built only from local data.
#### 6.1.2 Library — Albums, Tracks, Artists

- Add by pasting a Spotify link or URI (type detected) or manual entry; edit; delete.
- Accepted link forms: open.spotify.com URLs (tracking parameters stripped), spotify: URIs, and spotify.link short links (resolved when online; queued as “unresolved” when offline).
- Relationships: album ↔ artist, track ↔ album/artist.
- Adding a Spotify link that already exists is blocked and offers to open the existing record; alternate versions (remaster, deluxe) are separate records that can be linked.
- Album tracks are added manually in MVP (autofill in Release 3).
- Artists have their own exploration status and a list of albums/tracks to explore.
- The app never implies it holds an artist’s complete discography.
#### 6.1.3 Artwork and Spotify embed

- Detail pages show Spotify’s official embed (embed address derived from the link): artwork, title and Spotify’s player card, displayed unaltered.
- List views are text-first. An optional “show artwork” mode displays compact embeds for at most 20 items per page, each set ending with an “Open Spotify” link (Spotify Design Guidelines).
- Embeds load only for visible items; no background or bulk oEmbed calls (Embeds Terms IV.1.g(ii)).
- Spotify-sourced content carries Spotify attribution and an “Open Spotify” link (FR-044).
- oEmbed is used only to pre-fill the title in the add dialog; nothing it returns is stored except the title the user confirms (Embeds Terms IV.2.c; Promotion Rule 5.2). Embeds run in a non-persistent web view.
- Offline: a neutral placeholder is shown; all user data remains available.
- The embed is displayed exactly as Spotify provides it, including its play control (Embeds Terms IV.3.a, IV.3.b).
#### 6.1.4 Exploration workflow

| Status | Meaning |
|---|---|
| Planned | Saved for later |
| In Progress | Currently exploring |
| Explored | Finished; the user marks this explicitly |
| Not for me | Finished and disliked |
| Abandoned | Stopped partway |

Revisit is a flag, not a status, so it can be set on any item regardless of its assessment. Every status change is recorded with a timestamp and optional note. Optional priority: High / Medium / Low.

#### 6.1.5 Personal Journal

- Rating: 5 stars with half stars.
- Review and free-form notes.
- Multiple user-defined tags (create, rename, merge).
- Favourite and revisit flags.
- Recommended by / why saved.
- Dates added, explored, reviewed.
#### 6.1.6 Discovery Backlog

- One-field quick add (paste link) with optional details.
- Filter by type, status, tag, priority; sort by date, priority or manual order.
- Remove from backlog without deleting the record.
#### 6.1.7 Rediscovery

- Revisit flag; highly rated; incomplete reviews; stale backlog items; user-saved filters.
- Transparent, user-defined rules only.
#### 6.1.8 Search

Full-text search across titles, artists, notes, reviews and tags.

#### 6.1.9 Mobile quick-capture (iOS Shortcut)

- User shares a Spotify link from the Spotify iOS app to an Apple Shortcut.
- Transport: a local-network listener on the PC (O6, resolved in the technical design). A shared-file transport remains a fallback option.
- The listener is off by default, accepts connections from the local network only, requires a pairing code, accepts only Spotify links, validates and rate-limits input (NFR-18).
- The item lands in the backlog as Planned, marked “name needed” until the user confirms its name on the PC.
#### 6.1.10 Data Management

- Local database; view and change storage location.
- JSON export with documented format and per-field provenance (user / promoted).
- Backup: compressed, database only, automatic weekly plus manual; maximum 3 copies, oldest removed first; warning when a backup exceeds a configurable size (default 100 MB), never silent truncation. Default backup folder: Documents\Runout\Backups (outside application data).
- Restore with verification.
- Delete a record; delete the entire library.
- Uninstall: Spotify-provided data (cache, playlist snapshots, tokens, web-view data) is always removed; the user is asked whether to remove their library; backups are never removed. A library moved to a custom folder is not removed automatically.
### 6.2 Release 2 modules — Listening Log and Insights

#### 6.2.1 Canonical play rule

A play is any listening event with ≥ 30 seconds played, applied identically to logger and export data. Listening-time metrics use milliseconds played for all events. Rationale: the data export contains milliseconds played but no track duration, so a “half the track” rule cannot be applied consistently.

#### 6.2.2 Desktop listening logger

- Reads the Windows media session (SMTC) published by the Spotify desktop app; no Spotify API used.
- Records each listening event with start, end, milliseconds played and metadata; the canonical rule decides what counts as a play.
- Logs only the Spotify source by default; optional start with Windows; runs in the system tray.
- Matches events to library records by normalized title and artist (SMTC provides no Spotify identifier).
- Limitation: captures desktop plays only while the logger runs.
- Limitation: the Windows media session has no private-session flag, so desktop private-session plays cannot be excluded automatically; a tray “Pause logging” toggle is provided.
#### 6.2.3 Spotify data-export import

- Imports Extended Streaming History JSON (lifetime, all devices) and Account Data playlists. The library file is not imported (data minimization).
- Fields used: end time, milliseconds played, platform, track/album/artist names, track URI, start and end reason, shuffle, skipped, offline, private session.
- Skips podcast, episode and audiobook entries.
- Private-session plays are excluded from insights by default (setting to include).
- Timestamps (UTC) are converted to local time using the PC’s time-zone rules for that date.
- Re-import is idempotent (no duplicates).
- Reconciliation: a logger event and an export event are the same play when normalized track names match (artist used as a tie-breaker) and end times are within ±2 minutes. Each export event reconciles at most one logger event; the export record is kept and the logger record is linked to it.
#### 6.2.4 Insights

| Group | Examples | Sources |
|---|---|---|
| Listening | Time per artist/album/track, skip rate, hour/day patterns, trends over time, unfinished albums (only for albums whose track list the user has entered or confirmed) | S, E (+U for unfinished albums) |
| Collection | Backlog growth, completion rate, time from Planned to Explored, rating distribution, tag trends, reviews per month | U |
| Cross | Ratings vs plays, backlog items never played, loved but not played recently | U + S/E |
| Year in review | Personal annual summary on demand | U, S, E |

- Each insight states which data sources it uses.
- The Insights page shows data freshness: “Desktop: live · All devices: up to <latest export date>”.
- Summary on the dashboard; full detail on a dedicated Insights page.
#### 6.2.5 Read-only playlist analysis

- Uses playlists from the Account Data export (no API).
- Duplicates within and across playlists; tracks rated low; tracks never played; playlist size over exports (playlists matched by name, as exports carry no playlist ID).
- Results are for review only and are not fed into Playlist Studio (11.1). The user applies changes in Playlist Studio, which uses only their own ratings and tags.
- Limitation: reflects playlists as of the export date.
### 6.3 Release 3 modules — Spotify connected

| ID | Feature | Description | Constraints |
|---|---|---|---|
| R3-1 | Playlist Studio | Import playlists the user owns or collaborates on; per-track keep/cut/move using the user’s own ratings and tags (no listening data, 11.1); duplicates across playlists; local draft; preview diff; apply to Spotify; undo via snapshots | Followed playlists return no items; conflicts detected by re-reading the playlist just before applying; snapshots expire (proposed 30 days, O4); explicit confirmation before every write |
| R3-2 | Playlist from filter | Create a Spotify playlist from any saved filter | Created on the user’s account |
| R3-3 | Metadata autofill | Suggest artist/album/year/track list on paste; bulk-add album tracks | Cached (proposed 7 days, O5); stored as user data only when confirmed (5.2) |
| R3-4 | Spotify Top items | Spotify’s top 50 artists/tracks for ~4 weeks, ~6 months, ~1 year, shown as provided | Display only; “Add to backlog” follows 5.2; never used in insights |
| R3-5 | Recent plays | Last 50 plays from any device | Display only; “Add to backlog” / rate follows 5.2; never used in insights |
| R3-6 | Save to Spotify library | Save/remove albums and tracks from within the app | Uses library endpoints |
| R3-7 | Connection and resilience | Secure login, status, re-login every 6 months, app fully functional when Spotify is unavailable, delete Spotify-provided data within 5 days of disconnect | Prerequisite for R3-1…R3-6 |

## 7. Functional Requirements

Priority: P0 must have for the release; P1 should have; P2 could have.

| ID | Requirement | Release | Priority |
|---|---|---|---|
| FR-001 | Support album, track and artist records with relationships. | MVP | P0 |
| FR-002 | Add a record by pasting a Spotify URL, URI or spotify.link short link with type detection, or manually; queue unresolved short links when offline. | MVP | P0 |
| FR-003 | Edit and delete records. | MVP | P0 |
| FR-004 | Block adding a duplicate Spotify identifier and offer to open the existing record; allow linking alternate versions. | MVP | P1 |
| FR-005 | Statuses: Planned, In Progress, Explored, Not for me, Abandoned; artists have their own status. | MVP | P0 |
| FR-006 | Record every status change with timestamp and optional note. | MVP | P1 |
| FR-007 | Optional priority. | MVP | P1 |
| FR-008 | 5-star rating with half stars; review; notes. | MVP | P0 |
| FR-009 | Multiple user-defined tags with rename and merge. | MVP | P0 |
| FR-010 | Favourite and revisit flags; recommended-by field. | MVP | P0 |
| FR-011 | Save an item before all metadata is known. | MVP | P0 |
| FR-012 | Backlog view filtered by type, status, tag, priority. | MVP | P0 |
| FR-013 | Rediscovery view and saved filters. | MVP | P0 |
| FR-014 | Full-text search. | MVP | P0 |
| FR-015 | Dashboard of local counts and activity. | MVP | P0 |
| FR-016 | Open any record in Spotify. | MVP | P0 |
| FR-017 | Show Spotify’s official embed on detail pages; optional compact embeds in lists, max 20 per page, visible items only; store nothing from oEmbed except a user-confirmed title; non-persistent web view; placeholder offline. | MVP | P0 |
| FR-018 | Receive captured links from an iOS Shortcut per 6.1.9 and NFR-18; store them as “name needed” until confirmed. | MVP | P0 |
| FR-019 | Store all user data locally; work offline. | MVP | P0 |
| FR-020 | Documented JSON export with per-field provenance. | MVP | P0 |
| FR-021 | Compressed backup, max 3 copies, size warning; restore with verification. | MVP | P0 |
| FR-022 | Delete a record and the entire library. | MVP | P0 |
| FR-023 | Never upload personal records to a remote service. | MVP | P0 |
| FR-024 | Uninstall removes Spotify-provided data, asks before removing the library, leaves user-chosen backup folders untouched. | MVP | P0 |
| FR-025 | Apply the Promotion Rule (5.2) and store per-field provenance. | MVP | P0 |
| FR-026 | Log desktop Spotify listening events from SMTC; apply the canonical play rule (≥30 s); tray toggle to pause logging. | R2 | P0 |
| FR-027 | Import Extended Streaming History idempotently; drop IP, user agent, username; skip podcasts/episodes/audiobooks. | R2 | P0 |
| FR-028 | Reconcile logger and export plays (track-name match with artist tie-breaker, end time ±2 min, one-to-one); no double counting. | R2 | P0 |
| FR-029 | Convert UTC timestamps to local time; exclude private-session plays by default (configurable). | R2 | P1 |
| FR-030 | Match plays to library records. | R2 | P1 |
| FR-031 | Listening, collection and cross insights computed only from sources U, S, E. | R2 | P0 |
| FR-032 | Each insight identifies its data sources; Insights page shows data freshness per source. | R2 | P0 |
| FR-033 | Read-only playlist analysis from Account Data export. | R2 | P1 |
| FR-034 | Year in review. | R2 | P2 |
| FR-035 | Spotify login (Authorization Code with PKCE), connection status, re-login prompt at 6-month expiry. | R3 | P0 |
| FR-036 | All core and R2 features work when Spotify is disconnected or unavailable. | R3 | P0 |
| FR-037 | Playlist Studio: import owned/collaborative playlists, review using the user’s ratings and tags only, duplicates, draft, diff preview, conflict check, apply, undo (time-limited snapshots). | R3 | P0 |
| FR-038 | Confirm explicitly before every write to Spotify. | R3 | P0 |
| FR-039 | Create a Spotify playlist from a saved filter. | R3 | P0 |
| FR-040 | Metadata autofill and album track bulk-add, cached with expiry, promoted only on confirmation. | R3 | P1 |
| FR-041 | Display Spotify Top items and recent plays; exclude them from insights; add-to-backlog follows 5.2. | R3 | P1 |
| FR-042 | Save/remove items in the user’s Spotify library. | R3 | P1 |
| FR-043 | Delete all unpromoted Spotify-provided data within 5 days of disconnect. | R3 | P0 |
| FR-044 | Follow Spotify design guidelines wherever Spotify metadata or artwork is shown. | MVP/R3 | P0 |
| FR-045 | Record local usage counters (app opens, page visits, capture duration) for the success metrics; never transmitted. | MVP | P1 |

## 8. Key User Journeys

### Journey A — Save music for later (desktop)

1. User copies a Spotify link.

2. User pastes it into quick add.

3. Type is detected; the detail page shows the embed.

4. User confirms or edits the display name.

5. Item saved as Planned and appears in the backlog.

Expected result: Capture in seconds without Spotify API access.

### Journey B — Capture from phone

1. User taps Share in Spotify iOS and picks the Shortcut.

2. Shortcut delivers the link to the desktop app.

3. Item appears in the backlog as Planned, marked “name needed”; the user confirms the name later on the PC (short links resolve when online).

Expected result: Phone discoveries are not lost.

### Journey C — Explore and document an album

1. User sets status In Progress.

2. Opens in Spotify and listens.

3. Returns and records rating, review, tags.

4. Sets Explored, Not for me or Abandoned, and optionally flags Revisit.

Expected result: The user’s assessment and state are preserved.

### Journey D — Explore an artist

1. Add artist and why they are interesting.

2. Add selected albums/tracks to backlog.

3. Explore them; update notes and artist status.

Expected result: Artist exploration becomes intentional and trackable.

### Journey E — Rediscover

1. Open Rediscovery.

2. Filter by tag, rating or Revisit flag.

3. Open in Spotify; optionally update notes.

Expected result: Saved music is easy to return to.

### Journey F — Review listening history

1. User requests the Spotify data exports and later imports the files.

2. Desktop logger adds new desktop plays continuously; duplicates are reconciled.

3. User opens Insights and sees data freshness per source.

Expected result: A lifetime view of the user’s own listening, honest about how current each source is.

### Journey G — Clean up a playlist

1. Read-only analysis (R2) highlights duplicates and low-rated tracks.

2. User connects Spotify (R3) and opens the playlist in Playlist Studio.

3. User marks keep/cut, previews the change list and confirms.

4. Changes apply to Spotify; a snapshot allows undo until it expires.

Expected result: Intentional playlists with no accidental damage.

### Journey H — Back up and restore

1. User opens Data Management and creates a backup.

2. System confirms location, size and result.

3. Later the user restores and the system verifies the data.

Expected result: Records are portable and recoverable.

## 9. Data Requirements

| Entity | Key fields |
|---|---|
| Music Item (Album / Track / Artist) | ID, type, display name (or “name needed”), Spotify URI (optional), status, priority, rating, review, notes, favourite, revisit, recommended by, dates added/explored/reviewed, created/modified; per-field provenance |
| Relationship | Album↔Artist, Track↔Album, Track↔Artist, version-of links |
| Tag | ID, name, optional description; many-to-many with items |
| Status History | Item ID, previous status, new status, timestamp, note |
| Saved Filter | ID, name, filter definition |
| Unresolved Capture | Raw link, received at, source (paste / Shortcut), resolution state |
| Listening Event (R2) | ID, source (logger / export), ended at (UTC) and local time, ms played, counts-as-play, track/artist/album names, Spotify URI (export only), platform, start/end reason, shuffle, skipped, offline, private session, matched item ID, reconciled-with ID |
| Import Batch (R2) | ID, export type, imported at, record count, latest event date, checksum |
| Exported Playlist (R2) | Name, items (URIs/names), export date |
| Spotify Cache (R3) | Provider data with fetched-at and expiry; separate store; purged on expiry and disconnect |
| Playlist Snapshot (R3) | Playlist URI, item URIs and order, taken at, expiry; stored with the Spotify cache, never backed up |
| Usage Event | Type (app open, page visit, capture), timestamp, duration; local only (FR-045) |

### 9.1 Data ownership principles

- The user owns all source U, S and E data, including items promoted under 5.2.
- Unpromoted Spotify-provided (source A) data is held only as long as necessary, kept current, deleted on expiry, within 5 days of disconnect, and on uninstall.
- Exports carry per-field provenance.
### 9.2 Storage

A local relational database with no server (SQLite is the leading candidate; final choice in technical design). Must support relationships, fast search, safe interrupted writes, backups and schema migrations.

## 10. Non-Functional Requirements

| ID | Requirement | Acceptance expectation |
|---|---|---|
| NFR-01 | Local-first | All core functions work offline; only embeds, short-link resolution and R3 features need internet. |
| NFR-02 | No hosting | No cloud server required. |
| NFR-03 | No paid dependencies | Free tools and libraries only. |
| NFR-04 | Performance | On a reference dataset of 10,000 records and 500,000 listening events, on a typical Windows laptop: search and filter results < 200 ms; screen navigation < 500 ms; import of 10 years of history < 60 s; app start < 3 s. |
| NFR-05 | Data integrity | Interrupted operations never corrupt the library. |
| NFR-06 | Recovery | A backup restores and verifies successfully. |
| NFR-07 | Privacy | No telemetry; no remote transmission of personal data. |
| NFR-08 | Credential security | Spotify tokens stored in Windows Credential Manager; never in the database, logs or exports. |
| NFR-09 | Graceful degradation | Expired login, lapsed Premium, API errors or offline state never block core features; the cause is shown clearly. |
| NFR-10 | Write safety | No change reaches Spotify without explicit confirmation and a preview. |
| NFR-11 | Backup footprint | Compressed, max 3 copies, size warning; typical backup 10–30 MB (estimate, to confirm in testing). |
| NFR-12 | Accessibility | Keyboard shortcuts for add, search and status change; text contrast meets WCAG 2.1 AA (4.5:1). |
| NFR-13 | Maintainability | Schema and modules documented in the technical specification; core rules covered by automated tests. |
| NFR-14 | Extensibility | Adding a field or status needs only a database migration and a UI change, with no redesign of existing tables. |
| NFR-15 | Transparency | API failures, unavailable metadata, data freshness and backup outcomes clearly communicated. |
| NFR-16 | Portability | Documented, machine-readable export with provenance. |
| NFR-17 | Low resource logger | Tray logger < 1% average CPU and < 50 MB memory. |
| NFR-18 | Capture listener security | Off by default; local network only; pairing code; accepts Spotify links only; input validated; rate-limited; can be disabled at any time. Accepted risk: the pairing code travels unencrypted on the home network. |

## 11. Spotify Constraints

Basis: Documented = stated in official Spotify documentation; Interpretation = our reading where documentation is silent or ambiguous; Unverified = not yet confirmed.

| # | Topic | Finding | Basis | Impact |
|---|---|---|---|---|
| S1 | Premium requirement | Development Mode needs the owner to hold active Premium; app stops if it lapses. Plan tiers not specified by Spotify. Tested: a Premium Student account works (PKCE login and GET /me returned 200 with product = premium, 19 Oct 2026). | Documented / Student: Verified | Resolved in M5 gate (O1). |
| S2 | User limit | 5 users in Development Mode; extended quota only for organizations with 250k+ MAU. | Documented | Accepted: personal use. |
| S3 | Playlists | Items readable only for owned/collaborative playlists; write endpoints available. | Documented | Playlist Studio scope. |
| S4 | Top items | Top 50 for ~4 weeks / ~6 months / ~1 year. | Documented | Display only. |
| S5 | Recently played | Max 50 items. | Documented | Display only. |
| S6 | Removed functionality | Audio features/analysis, recommendations, related artists, previews, popularity, followers, batch fetch and browse removed; genres listed as a deprecated field in the Top Items reference. | Documented | Out of scope. |
| S7 | Token lifetime | Refresh tokens expire 6 months after authorization; refresh does not extend. | Documented | Re-login prompt. |
| S8 | Storage of Spotify content | Only as strictly necessary; not indefinitely; keep current; temporary caching; delete within 5 days of disconnect. | Documented | Cache + Promotion Rule. |
| S9 | Analytics | No derived listenership or user metrics, no profiling. | Documented | Insights only from U, S, E. |
| S10 | Design | Logo with metadata/artwork; link back; artwork unaltered; ≤20 items per set ending with a Spotify link; no mixing with other services. | Documented | FR-017, FR-044, 4.4. |
| S11 | Client internals | User Guidelines prohibit reverse-engineering and circumventing Spotify technology. | Documented | No reading of client files or code. |
| S12 | Windows media session | Spotify publishes it to Windows; any app may read it; no Spotify term addresses it. | Interpretation | Desktop logger permitted. |
| S13 | Data export contents | Lifetime, all devices, JSON; includes IP and user agent; no track duration. | Documented | Minimized import; canonical play rule. |
| S14 | Export delivery time and frequency | Not stated in Spotify documentation. Measured for one account: the Extended streaming history export arrived about 1 day after the request (requested 18 Oct, received 19 Oct 2026). Account data export not yet received; allowed request frequency unknown. | Partly verified | Delivery time measured in M4; frequency open (O2). |
| S15 | Licence | Developer licence covers private personal use. | Documented | Fits the product. |
| S16 | oEmbed / embeds | No authentication needed (tested); embed unaltered; no local caching; no excessive calls. | Documented + tested | FR-017. |
| S17 | Definition of Spotify Content | Includes data “made available … by Spotify”, which may include the user’s data export. | Documented; application to exports is Interpretation | Accepted risk 11.1. |

### 11.1 Accepted compliance risk — export data

From Release 3 the application is a registered Spotify developer application. Because the Developer Terms define Spotify Content broadly (S17), the Developer Policy’s analytics restriction (S9) and storage limits (S8) could be read as covering the user’s own data export once the app is registered.

Decision (D26): accepted for private, single-user, non-commercial use, with these mitigations:

- Release 2 (which uses export data) ships before any Spotify registration and needs none.
- Export-derived insights never combine with, enrich, or are enriched by Web API data. This includes Playlist Studio, which uses only the user’s ratings and tags.
- Insights are displayed only to the user; nothing is shared or transmitted.
- Optional: the user may ask Spotify developer support for guidance (O7). If guidance is restrictive, Release 3 remains usable while Spotify-connected, with Listening Log insights disabled, or Release 3 is dropped.
## 12. Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Export-data analysis judged to fall under Developer Terms (11.1) | Insights or R3 must change | Accepted risk with mitigations; optional Spotify guidance (O7) |
| Spotify changes or restricts API access | R3 features stop | Core and R2 independent of the API; graceful degradation |
| Student Premium not accepted for Development Mode | R3 unavailable | Verify at start of M5; MVP and R2 unaffected |
| Premium lapses | R3 stops until renewed | Clear status message; no data loss |
| SMTC metadata changes or is incomplete | Desktop logging gaps | Export import fills history |
| Name-based matching errors | Plays attached to wrong records | Prefer export URIs; manual correction |
| Double counting between logger and export | Inflated statistics | Reconciliation rule (FR-028) |
| Stale mobile data between exports | Misleading insights | Freshness shown per source (FR-032) |
| Capture listener exposed on the network | Unwanted input | NFR-18 controls; off by default |
| Embed unavailable or offline | No artwork | Placeholder; user data unaffected |
| Accidental playlist damage | Lost curation | Preview, confirmation, snapshot undo |
| Backup bloat | Disk usage grows | Max 3 compressed copies; DB only; size warning |
| User stops maintaining records | App loses value | Quick capture; automatic listening log; success metrics review |
| Scope expansion | Project stalls | Strict releases; 2-week daily-use check after MVP |

## 13. Release Acceptance Criteria

### 13.1 MVP

- Create, edit, delete album, track and artist records; URL, URI and short-link detection works (short links queued offline).
- All five statuses and the revisit flag work; status history is recorded.
- Ratings, notes, tags, flags persist after restart.
- Search, filters, backlog, rediscovery and dashboard reflect the local library accurately and meet the NFR-04 targets for records (search, navigation, start).
- Embeds display on detail pages; list embeds respect the 20-item limit; nothing from oEmbed is stored except confirmed titles; offline placeholder works.
- iOS Shortcut capture lands an item in the backlog; listener (if used) meets NFR-18.
- Export with provenance, backup (max 3, compressed, size warning) and verified restore work.
- Uninstall behaves per FR-024.
- All core functions work offline; nothing is transmitted silently.
### 13.2 Release 2

- Logger records desktop events and applies the ≥30 s play rule.
- Export import is complete, idempotent, skips podcasts/episodes, and drops IP, user agent and username.
- Reconciliation produces no double counts on an overlapping test set.
- Insights match a manual check, display their sources and freshness, and use local time.
- No source A data is used in any insight.
- Read-only playlist analysis finds known duplicates in a test export.
- Import of the reference history meets the NFR-04 import target (< 60 s); year in review produces a summary for a chosen year.
### 13.3 Release 3

- O1 resolved (Premium plan accepted).
- Login, re-login prompt and disconnect (with deletion within 5 days) work.
- Promotion Rule verified: unconfirmed cached fields disappear on expiry; confirmed fields persist.
- A playlist edit round-trips with preview, confirmation and undo.
- Playlist from filter creates the expected playlist.
- Top items and recent plays display, and “Add to backlog” follows the Promotion Rule; save to library round-trips.
- All core and R2 features work with Spotify disconnected.
- Spotify design guidelines are met wherever Spotify data appears.
## 14. Milestones

| Milestone | Contents | Exit criterion | Indicative effort (agentic) |
|---|---|---|---|
| M0 Foundations | Scaffold, proof script; user requests both Spotify data exports | Documents reviewed and consistent; app runs; exports requested | Done |
| M1 Design | Design spec (docs/DESIGN.md) and every screen designed in direction D, with empty, offline and error states | All screens designed; user approves the designs | Done |
| M2 UI build | All screens in React on typed mock data: tokens, components, embeds, keyboard, accessibility | Journeys A–H clickable on mock data | Done |
| M3 Backend: MVP core | Database, link parsing, library, Promotion Rule, search, capture listener, export, backup, delete, usage counters | Core tests and MVP performance checks pass | Done (about 3 h agentic) |
| M4 Backend: listening and insights | Desktop logger, export import, play rule, reconciliation, matching, playlist check-up, insight queries | Lifetime history imported; no double counts; insights hand-verified | 4–6 h agentic, plus export download (U-2) and a 24 h logger run |
| M5 Backend: Spotify connected | Gate: O1 (Premium plan) and optional O7; login; R3-1 to R3-7 logic | Test-playlist round-trip passes | 4–6 h agentic, after the O1 gate (U-6) |
| M6 Integration: MVP release | MVP screens on real data, installer, phone capture | MVP criteria pass on the installed build; 2 weeks of daily use measured against section 15, reviewed as the MVP’s final sign-off (runs in parallel with M7, D32) | 4–6 h agentic, plus U-3/U-4; 2 weeks of daily use (U-5) in parallel, not blocking M7 |
| M7 Integration: R2 and R3 | Listening, insights and Spotify screens on real data | R2 and R3 criteria pass | 3–4 h agentic |
| M8 Future candidates | Selected items from 4.4 | Per item | As needed |

## 15. Business Success Metrics

Measured from local usage counters (FR-045) and the user’s records; nothing is transmitted. Reviewed at the end of the M6 daily-use period and after M7.

| Metric | Target |
|---|---|
| Time to capture a new item (desktop paste or phone share) | < 10 seconds |
| Backlog items added during the daily-use period with a status other than Planned after 14 days | ≥ 30% |
| Explored items with a rating or review | ≥ 70% |
| Days per week the app is opened during the daily-use period | ≥ 4 |
| Rediscovery or search used per week | ≥ 3 times |
| Backup and restore test | Succeeds with data verified |
| User judgement after the daily-use period | Fewer forgotten discoveries; exploration feels more intentional |

## 16. Assumptions

- The application is for one person’s private, non-commercial use.
- Spotify remains the playback destination.
- Albums, tracks and artists are separate first-class entities.
- Local persistence is mandatory; no new project expenditure is acceptable.
- The user is willing to enter some metadata manually when automatic retrieval is unavailable.
- The user can request Spotify data exports periodically; the Extended streaming history took about 1 day for one account; request frequency is unknown (O2).
- The Spotify desktop app continues to publish now-playing information to Windows.
- Core workflows must not break when Spotify API access is unavailable.
- The first implementation targets Windows.
- Spotify policy compliance takes precedence over any desired feature, except the documented accepted risk in 11.1.
## 17. Decisions Log

| # | Question | Decision |
|---|---|---|
| D1 | When is an album Explored? | Only when the user marks it |
| D2 | Do artists have their own lifecycle? | Yes, same statuses |
| D3 | Album tracks managed how? | Manual in MVP; autofill (confirmed by user) in R3 |
| D4 | Completed vs not-for-me vs abandoned? | Explored, Not for me and Abandoned are statuses |
| D5 | Rating scale? | 5 stars with half stars |
| D6 | Tags user-defined? | Fully user-defined |
| D7 | Is pasting a URL enough for capture? | Yes for MVP; autofill in R3 |
| D8 | Artwork in MVP? | Yes, via Spotify’s official embed; detail pages; lists max 20; nothing cached |
| D9 | Duplicates and versions? | Same link blocked with “open existing”; versions are linked separate records |
| D10 | Where do insights live? | Dashboard summary plus Insights page |
| D11 | Startup view? | Dashboard |
| D12 | Quick add vs rich form? | One field with optional details |
| D13 | Keyboard shortcuts? | Yes |
| D14 | Windows permanent? | Starting point; others later |
| D15 | Database location? | Default folder, changeable |
| D16 | Automatic backup? | Weekly, max 3 compressed copies, size warning |
| D17 | Export format? | JSON; CSV later |
| D18 | Which API capabilities are necessary? | All of R3-1 to R3-7 |
| D19 | Mobile app? | No native app; iOS Shortcut capture in MVP; PWA future candidate |
| D20 | More than 5 users? | Not needed |
| D21 | Data on uninstall? | Spotify-provided data always removed; user asked about library; backups untouched |
| D22 | Revisit: status or flag? | Flag only |
| D23 | Podcasts and episodes? | Skipped |
| D24 | Private-session plays in insights? | Excluded by default; configurable |
| D25 | What counts as a play? | ≥ 30 seconds played, for all sources |
| D26 | Export-data compliance risk (11.1)? | Accepted for private personal use with mitigations |
| D27 | Does Playlist Studio use listening data? | No; only the user’s ratings and tags (keeps the 11.1 separation) |
| D28 | Default backup folder? | Documents\Runout\Backups, outside application data, so uninstall never removes it |
| D29 | Which UI direction? | D · Pulse: listening story plus the user’s opinions; Spotify-compatible dark theme, amber accent, never Spotify’s look (docs/DESIGN.md) |
| D30 | Delivery order? | Design, then UI on mock data, then backend, then integration; each milestone starts from current specs and a written plan |
| D31 | How is effort estimated? | Agentic development: build and review hours per milestone, plus the real-world waits agents cannot shorten (user actions, 24 h logger run, 2 weeks of daily use). Calibrated on M3: about 3 hours against a 3–4 week estimate |
| D32 | Does daily use gate R2/R3 integration? | No (user decision 20 Oct 2026). M7 starts once M6 code, installer and U-3/U-4 pass. U-5 (2 weeks of daily use, section 15 metrics) runs in parallel and is the MVP’s final sign-off; findings feed fixes, not a gate |

## 18. Remaining Open Items

| # | Item | Owner | Resolve by |
|---|---|---|---|
| O1 | Does a Premium Student plan qualify for Development Mode? (S1). Resolved: yes, PKCE login and GET /me returned 200 with product = premium (19 Oct 2026). | Resolved by the user (developer app created, one call tested) | Done |
| O2 | Export delivery time and allowed request frequency (S14). Partly resolved: Extended streaming history took about 1 day (one account, 19 Oct 2026); Account data export delivery and allowed frequency still open. | User — request both exports in M0 | M4 |
| O3 | Technology stack and detailed technical design | Resolved: Tauri 2 + React + SQLite (docs/TSD.md) | Done |
| O4 | Playlist snapshot retention. Resolved: 30 days (TSD §5.2; built in M5, swept at startup and daily). | Resolved by technical design | Done |
| O5 | Spotify cache expiry. Resolved: 7 days (TSD §5.2; built in M5, swept at startup and daily). | Resolved by technical design | Done |
| O6 | iOS capture transport: local-network listener or shared file | Resolved: local-network listener (docs/TSD.md §5.1) | Done |
| O7 | Optional: ask Spotify developer support about export-data analysis in a registered app (11.1) | User | Before M5 |

## 19. Final Product Definition

Runout is a local-first music organization, exploration and reflection tool that works alongside Spotify. It lets one listener manage albums, tracks and artists; maintain a backlog; document opinions; track exploration; rediscover music; understand their lifetime listening from their own data; and refine their own Spotify playlists safely.

Its value is not playing music. Its value is helping the user remember, organize, explore and understand their musical life, while keeping personal data local, staying within Spotify’s rules, and adding no recurring cost.

## 20. References

- Spotify Developer Terms — https://developer.spotify.com/terms
- Spotify Developer Policy — https://developer.spotify.com/policy
- Quota modes — https://developer.spotify.com/documentation/web-api/concepts/quota-modes
- February 2026 Development Mode migration guide — https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide
- Web API changes, 27 Nov 2024 — https://developer.spotify.com/blog/2024-11-27-changes-to-the-web-api
- Refresh token expiration, 18 Jun 2026 — https://developer.spotify.com/blog/2026-06-18-refresh-token-expiration
- Get User’s Top Items — https://developer.spotify.com/documentation/web-api/reference/get-users-top-artists-and-tracks
- Get Recently Played Tracks — https://developer.spotify.com/documentation/web-api/reference/get-recently-played
- Spotify Design Guidelines — https://developer.spotify.com/documentation/design
- oEmbed reference — https://developer.spotify.com/documentation/embeds/reference/oembed
- Embeds Terms of Use — https://developer.spotify.com/documentation/embeds/terms
- Understanding my data — https://support.spotify.com/us/article/understanding-my-data/
- Spotify User Guidelines — https://www.spotify.com/us/legal/user-guidelines/
- Last.fm scrobbling documentation — https://www.last.fm/api/scrobbling

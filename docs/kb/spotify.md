# Spotify — knowledge base

Constraints table with sources: [BRD §11](../../BRD.md) (S1–S17). This file holds the working facts behind them. Format: fact — **status**, date, source.

## Links and IDs
- IDs are 22 base-62 chars; `open.spotify.com` URLs may carry an `intl-xx/` locale segment and `?si=` query — **Documented + tested**, 2026-10-18, TSD §4.1.
- `spotify.link/{code}` short links need an HTTP redirect follow to reach an `open.spotify.com` URL — **Verified**, 2026-10-18, BRD research.
- The user's Spotify apps share full `https://open.spotify.com/{type}/{id}?si=…` links, with `&utm_source=whatsapp&rowId=…` when shared to WhatsApp; no `spotify.link` short link appeared (10 shares, desktop and phone) — **Verified** (observed), 2026-10-21. Runout stores the canonical URI with every parameter stripped (M6 installed run). Real short-link resolution stays unverified (G-37).
- An unknown `spotify.link` code answers `200 text/html` (~7 KB, openresty), not 404 — **Verified** (curl, synthetic code), 2026-10-18. So `links::resolve_short` follows allow-listed redirects, then falls back to the first `open.spotify.com` item link in the page body; a hop to a non-listed host (e.g. a branch.io domain) is refused. Real-code redirect chain **Unverified**: check with a real short link in M6.
- `spotify:` URIs open the desktop app through the OS shell handler — **Documented**, 2026-10-18, BRD FR-016.

## Embeds and oEmbed
- Embed URL can be derived without any call: `https://open.spotify.com/embed/{type}/{id}` — **Verified**, 2026-10-18, BRD S16.
- oEmbed `https://open.spotify.com/oembed?url=…` needs no auth; returns `title`, `thumbnail_url`, `html` — **Verified** (tested), 2026-10-18, BRD S16.
- Rust `links::prefill_title` live (G-10): an `open.spotify.com/album/…?si=` link and a `spotify:track:` URI each prefilled the right title (only `title` kept); an episode link was rejected with the FR-003 message before any request — **Verified** (tested), 2026-10-20, Spotify Web API doc example IDs.
- Embed CSP: `frame-src https://open.spotify.com` in the host page is sufficient for `/embed/{track|album|artist}/{id}`; the embed's own sub-resources (image CDN, scripts, nested frames) fall under Spotify's document policy, not ours — **Verified** (tested, no CSP violation, all three render), 2026-10-18; see [windows-tauri.md T4](windows-tauri.md#webview2). Embed HTML is served `cache-control: private, no-cache, no-store` — **Verified** (curl), 2026-10-18.
- Recommended iframe attributes (Spotify embed code): `allow="autoplay; clipboard-write; encrypted-media; fullscreen; picture-in-picture"`, `frameborder="0"`; heights 152 (compact) / 352 — **Documented**, https://developer.spotify.com/documentation/embeds , 2026-10-18.
- Content must not be cached locally or altered; ≤20 items per set ending with a Spotify link — **Documented**, BRD S8, S10. Docs: https://developer.spotify.com/documentation/embeds , https://developer.spotify.com/documentation/design

## Brand assets (logo)
- Official downloads, no login: full logo https://developer.spotify.com/images/guidelines/design/2024-spotify-full-logo.zip , icon https://developer.spotify.com/images/guidelines/design/2024-spotify-logo-icon.zip (linked from https://developer.spotify.com/documentation/design ) — **Verified** (curl 200, zip), 2026-10-18. Files dated 2024-08-19.
- Saved unaltered (renamed only): `src/assets/spotify/spotify-logo-white.svg` = `Full_Logo_White_RGB.svg` (sha256 `31cdfcdd…684f053c`), `src/assets/spotify/spotify-icon-white.svg` = `Primary_Logo_White_RGB.svg` (sha256 `8929d148…2ef2daa1`) — 2026-10-18.
- Rules: logo ≥ 70 px, icon ≥ 21 px (digital); green logo only on black or white, otherwise monochrome — **white on dark backgrounds** (our graphite UI); clear space = ½ icon height; never modify, rotate, recolour, stretch or use in a sentence; wordmark never without the icon — **Documented**, https://developer.spotify.com/documentation/design , 2026-10-18.
- Gotcha: the SVGs use a generic `.cls-1` class in an inline `<style>` — use them via `<img src>` (not inlined into the DOM) so the class can't collide.

## Web API (Release 3)
- Development Mode: owner needs active Premium; max 5 users — **Documented**, BRD S1–S2. Student Premium works: PKCE login + `GET /me` → 200, `product=premium` — **Verified (O1)**, 2026-10-19, user's own app (gate script, PKCE S256, scope `user-read-private`; refresh token returned).
- Redirect URI (T3): the dashboard rejects `http://127.0.0.1/callback` (no port) as "not secure", although the redirect-URI docs allow a port-less loopback. Registered `http://127.0.0.1:8888/callback`; a login with `http://127.0.0.1:<random>/callback` (58163) was accepted, so the loopback port is dynamic at authorization time — **Verified (T3)**, 2026-10-19.
- Client ID handling (user rule, 2026-10-19): keep it out of the repo. The app reads it at run time (settings/env), never hard-coded, never logged — **Decision**.
- Refresh tokens expire 6 months after authorization; refreshing does not extend — **Documented**, BRD S7.
- Gotcha (2026-10-19, M5 audit): a refresh may rotate the refresh token (widely reported to revoke the old one; **Assumption**, not verified here). So no second process may share the `Runout/spotify` credential: its refresh would leave the running app with a revoked token, `invalid_grant`, sign-out and cache delete. Dev examples sign in with memory-only tokens (check.py flags `cleanup::CREDENTIAL` in examples/).
- Client ID source (M5 code, 2026-10-19): `RUNOUT_SPOTIFY_CLIENT_ID` env var, read at run time; missing → `spotify_auth` — **Decision** (TSD §5.2).
- Library save/remove/contains (G-22): `PUT /me/library`, `DELETE /me/library` and `GET /me/library/contains` all take `uris` as a **required query parameter**, a comma-separated list of `spotify:{type}:{id}`, **max 40** per call, no request body (the reference examples encode `:` as `%3A` and keep `,` literal). PUT/DELETE answer `200` with an empty body; contains answers a JSON array of booleans in input order — **Documented**, 2026-10-21, https://developer.spotify.com/documentation/web-api/reference/save-library-items , …/remove-library-items , …/check-library-contains . Not yet called live (G-32).
- Gotcha (G-22): save/remove list supported types track, album, episode, show, audiobook, user, playlist — **no `spotify:artist:`**; contains does list artist. Scopes are given per endpoint, not per type: save/remove `user-library-modify`, `user-follow-modify`, `playlist-modify-public`; contains `user-library-read`, `user-follow-read`, `playlist-read-private`. Interpretation: an artist URI (= follow) needs the follow scopes, which Runout doesn't request; BRD R3-6 covers albums and tracks only — **Documented** (same pages), 2026-10-21.
- Feb 2026 Dev Mode renames: `/playlists/{id}/tracks` → `/playlists/{id}/items` (GET/POST/PUT/DELETE); playlist field `tracks` → `items`, entry `track` → `item` (old names still returned, marked deprecated); `POST /users/{id}/playlists` → `POST /me/playlists`; per-type save/remove/contains and follow/unfollow → `/me/library`; removed: `GET /users/{id}`, `GET /users/{id}/playlists`, "several" batch gets, new releases, categories, artist top tracks, markets; search `limit` max 10 — **Documented**, 2026-10-21 (guide undated; new apps 11 Feb 2026, existing apps 9 Mar 2026), https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide , https://developer.spotify.com/documentation/web-api/references/changes/february-2026 . March 2026 changelog only restores `external_ids` on albums/tracks — https://developer.spotify.com/documentation/web-api/references/changes/march-2026 .
- Album tracks (G-23): `GET /albums/{id}/tracks` is **still available** after the Feb 2026 changes (not in the removed list; reference not deprecated): paged `{items: [SimplifiedTrack], next, total}`, `limit` ≤ 50 — **Documented**, 2026-10-21, changelog above + https://developer.spotify.com/documentation/web-api/reference/get-an-albums-tracks . Not yet called live (G-32).
- Remove playlist items (T5): `DELETE /playlists/{id}/items`, JSON body `{"items": [{"uri": "spotify:track:…"}], "snapshot_id": "…"}` (`items` required, max 100 objects; `snapshot_id` optional; the old `tracks` key is renamed `items`; no `positions` field documented, so every copy of a URI is removed); scopes `playlist-modify-public`/`-private`; `200 {snapshot_id}` — **Documented**, 2026-10-21, https://developer.spotify.com/documentation/web-api/reference/remove-items-playlist .
- Create playlist (T5): `POST /me/playlists`, JSON `{name (required), public (default true), collaborative (default false), description}`; `public: false` needs `playlist-modify-private`; `201` + playlist object (`id`, `uri`, `snapshot_id`, …) — **Documented**, 2026-10-21, https://developer.spotify.com/documentation/web-api/reference/create-playlist .
- Playlist entries carry `added_at` (date-time, may be null on very old playlists), `added_by`, `is_local`, and `item` (`track` deprecated); `GET /playlists/{id}/items` is only for owned/collaborative playlists (else 403); reference says `limit` ≤ 50 — **Documented**, 2026-10-21, https://developer.spotify.com/documentation/web-api/reference/get-playlists-items . Studio's live round trip (2026-10-20) went through `studio::items`, which sends `limit=100`, without error — **Interpretation** (not a dedicated test).
- Followed artists (T5): `GET /me/following?type=artist` is **still available** in Dev Mode (only `PUT|DELETE /me/following` and `/me/following/contains` were removed); scope `user-follow-read`; cursor paging `{artists: {items, next, cursors.after, total}}`, `limit` ≤ 50; **no followed-at date** on items — **Documented**, 2026-10-21, changelog above + https://developer.spotify.com/documentation/web-api/reference/get-followed , https://developer.spotify.com/documentation/web-api/concepts/scopes . So "followed since the last check" can only be a diff against artist IDs seen before. Not yet called live (U-10).
- Saved albums (T5): there is **no `GET /me/library`** list endpoint (Library = save/remove/contains only). The read stays `GET /me/albums` (`user-library-read`, not removed): paged `{items: [{added_at: "YYYY-MM-DDTHH:MM:SSZ", album}], next, total}`, `limit` ≤ 50 — **Documented**, 2026-10-21, https://developer.spotify.com/documentation/web-api/reference/get-users-saved-albums .
- Playlist `snapshot_id` gives no write protection: item writes apply even if the playlist changed since that id — **Documented** (TSD §5.2).
- Gotcha: `GET /playlists/{id}?fields=snapshot_id` is stale after a write — the PUT `/items` answered a new id, but GET kept returning the pre-write id immediately, +3 s and ~30 s later, while the write had landed (seen in the Spotify app) — **Verified**, 2026-10-20, user's test playlist, `examples/studio_roundtrip.rs`. So Studio's apply and undo compare the playlist's **track URIs** (`GET /playlists/{id}/items`), never `snapshot_id` (TSD §5.2). `GET /playlists/{id}/items` is fresh: after a reorder the moved track was first at once, while every `snapshot_id` read (`?fields=`, full, `market=from_token`, `/me/playlists`) lagged ~33–36 s — **Verified**, 2026-10-20, `studio_roundtrip --probe`; the live round trip's undo, run seconds after apply, read the written list via `studio::current_uris` (G-30 closed). An identical rewrite returns the same `snapshot_id` (no version bump).
- Gotcha (2026-10-19, M5 audit; revised 2026-10-20): a failed playlist write (timeout, reset, 5xx, unreadable 2xx) may still have landed. An immediate re-read showing the old list proves nothing (the server may finish it later; the `snapshot_id` GET is verified stale). Only a 4xx (incl. 429) means it didn't land; otherwise keep both the before and after lists so undo can recognise its own write (`client::refused`, TSD §5.2, audit 2026-10-20). Undo that finds the *before* list within 2 minutes of such a write waits (`spotify_busy`) instead of restoring and dropping its undo (audit r3); apply waits the same way so it can't be cut by that write (audit r5). `studio_roundtrip --probe` reads the list with `studio::current_uris` itself, so its verdict covers the read Studio decides on.
- Gotcha (2026-10-19, M5 audit): concurrent 401s must share one refresh (single-flight), or the second sends an already-rotated refresh token and gets `invalid_grant` → sign-out.
- Removed for new apps: audio features/analysis, recommendations, related artists, previews, popularity — **Documented**, BRD S6.
- Gotcha: an edit made in the Spotify app reaches the Web API a few seconds later (~9 s observed) — **Verified**, 2026-10-20, `studio_roundtrip --conflict`. Until then no Web API read shows it, so Studio's pre-apply check can't either (accepted, TSD §5.2).
- Gotcha: OAuth `state` must be letters and digits only. A `~` came back percent-encoded (`%7E`) in the loopback callback and failed the raw compare — **Verified**, 2026-10-20, live sign-in (`auth::random_state`).
- Playlist items readable only for owned/collaborative playlists; Top items 50 × 3 ranges; Recently played max 50 — **Documented**, BRD S3–S5.
- Docs: https://developer.spotify.com/documentation/web-api

## Data export
- "Extended streaming history" = lifetime, all devices, JSON, includes IP + user agent, no track duration — **Documented**, BRD S13.
- Delivery time not published by Spotify. **Measured (O2)**: Extended streaming history requested ~18 Oct 2026; files inside the zip dated 2026-10-18 22:29; downloaded 19 Oct 2026, so it took about 1 day for one account. It was delivered as a separate package with no Account data part. — **Verified**, 2026-10-19, user's own export (U-1/U-2).

### Extended streaming history format (T1) — **Verified**, 2026-10-19
Source: the user's real export, read in place with `python -I` + `zipfile` (structure only; nothing copied), plus the bundled `ReadMeFirst_ExtendedStreamingHistory.pdf` (English pp. 3–5). Counts are from one account (~180k rows, 2019–2026).
- **Zip layout**: `my_spotify_data.zip` → one folder `Spotify Extended Streaming History/` containing
  - `Streaming_History_Audio_<YYYY>[_<n>].json`: one or more files per year, split at about 12 MB; for example `_2023`, `_2023_1`, `_2023_2`. A few rows (9 here) fall in a different year from the file name, so don't rely on it.
  - `Streaming_History_Video_<YYYY>.json`
  - `ReadMeFirst_ExtendedStreamingHistory.pdf`
- **File shape**: UTF-8 without BOM. A top-level JSON array of flat objects, pretty-printed Jackson style (`[ {`, `"key" : value`). Each file is sorted by `ts`.
- **Keys**: every row in both Audio and Video files has the same 23 keys, in this order. Null fields are present as `null`, not left out.

| Field | JSON type | Nulls (audio / video) | Notes |
|---|---|---|---|
| `ts` | string | 0 | `YYYY-MM-DDTHH:MM:SSZ`, UTC, when the stream **stopped** (PDF) |
| `platform` | string | 0 | free text, e.g. `windows …`, `android …`, `ios …`, `osx …`, `unknown` |
| `ms_played` | int | 0 | 0 occurs (~0.5%) |
| `conn_country` | string | 0 | 2-letter code. **Personal, drop.** |
| `ip_addr` | string | 0 | IPv4 or IPv6. **Personal, drop.** Older exports and the PDF call it `ip_addr_decrypted` |
| `master_metadata_track_name` | string\|null | 44 / 28 | null exactly when `spotify_track_uri` is null |
| `master_metadata_album_artist_name` | string\|null | 44 / 28 | the track's artist (PDF: “artist, band or podcast”) |
| `master_metadata_album_album_name` | string\|null | 44 / 28 | |
| `spotify_track_uri` | string\|null | 44 / 28 | `spotify:track:<22 base62>` |
| `episode_name`, `episode_show_name` | string\|null | non-null only on episode rows | |
| `spotify_episode_uri` | string\|null | non-null 41 / 28 | `spotify:episode:<22 base62>` |
| `audiobook_title`, `audiobook_uri`, `audiobook_chapter_uri`, `audiobook_chapter_title` | null here | all null | present as keys; not described in the PDF |
| `reason_start` | string | 0 | seen: `trackdone clickrow fwdbtn playbtn appload remote unknown backbtn trackerror switched-to-video switched-to-audio` |
| `reason_end` | string | 0 | seen: `trackdone endplay fwdbtn logout remote unexpected-exit-while-paused unknown backbtn unexpected-exit trackerror switched-to-video` |
| `shuffle`, `skipped` | bool | 0 | the PDF says they may be null |
| `offline` | bool\|null | 0 / 323 | |
| `offline_timestamp` | int\|null | ~1.3% null | usually epoch **seconds** (10 digits), but some rows hold epoch **ms** (13 digits). Not needed; drop |
| `incognito_mode` | bool\|null | 30 / 0 | → `private_session`. Null → false |

- **Telling rows apart**: a music row has `spotify_track_uri` non-null. A podcast row has `spotify_episode_uri` non-null. An audiobook row has `audiobook_*` set (none in this export). A few rows (3 here, all `reason_start=unknown`, under 4 s) have no URI at all. Audio files also contain episodes. Video files contain music-track rows (706 here, no overlap with the Audio files) and video-podcast rows. The file kind does not tell you the content kind.
- **Not present** in this export, though listed in the PDF example: `username`, `user_agent_decrypted`, `ip_addr_decrypted`. The allow-list must still drop them if they appear.
- **Dedupe key** `ts|spotify_track_uri|ms_played`: 25 groups collide (~0.03%). 9 groups are byte-identical rows. In the other 16, the first two rows differ only in `offline_timestamp`, and 1 of those has a further member that differs. In every case it is the same play logged twice, so merging them is correct.
- Synthetic fixture with this structure (fake values): `src-tauri/tests/fixtures/export_synthetic/`.

## Local client data (desktop)
- Windows media session (SMTC) exposes title, artist, album, album artist, track #, position, duration, play state, shuffle/repeat, thumbnail; **no Spotify URI**, no private-session flag — **Verified**, 2026-10-18, Spotify Store build 1.305.
- Spotify's SMTC `SourceAppUserModelId` contains `Spotify` (Store package `SpotifyAB.SpotifyMusic_zpdnekdrzrea0`) — **Verified**, 2026-10-18.
- Client file `list_player_play_history` (protobuf, ~500 plays with URIs) exists — **Verified**, 2026-10-18 — but **excluded**: reading client internals breaches S11. Do not use.

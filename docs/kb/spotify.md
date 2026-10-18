# Spotify — knowledge base

Constraints table with sources: [BRD §11](../../BRD.md) (S1–S17). This file holds the working facts behind them. Format: fact — **status**, date, source.

## Links and IDs
- IDs are 22 base-62 chars; `open.spotify.com` URLs may carry an `intl-xx/` locale segment and `?si=` query — **Documented + tested**, 2026-10-18, TSD §4.1.
- `spotify.link/{code}` short links need an HTTP redirect follow to reach an `open.spotify.com` URL — **Verified**, 2026-10-18, BRD research.
- `spotify:` URIs open the desktop app through the OS shell handler — **Documented**, 2026-10-18, BRD FR-016.

## Embeds and oEmbed
- Embed URL can be derived without any call: `https://open.spotify.com/embed/{type}/{id}` — **Verified**, 2026-10-18, BRD S16.
- oEmbed `https://open.spotify.com/oembed?url=…` needs no auth; returns `title`, `thumbnail_url`, `html` — **Verified** (tested), 2026-10-18, BRD S16.
- Content must not be cached locally or altered; ≤20 items per set ending with a Spotify link — **Documented**, BRD S8, S10. Docs: https://developer.spotify.com/documentation/embeds , https://developer.spotify.com/documentation/design

## Web API (Release 3)
- Development Mode: owner needs active Premium; max 5 users; Student plan eligibility unknown — **Documented / Unverified (O1)**, BRD S1–S2.
- Refresh tokens expire 6 months after authorization; refreshing does not extend — **Documented**, BRD S7.
- Removed for new apps: audio features/analysis, recommendations, related artists, previews, popularity — **Documented**, BRD S6.
- Playlist items readable only for owned/collaborative playlists; Top items 50 × 3 ranges; Recently played max 50 — **Documented**, BRD S3–S5.
- Docs: https://developer.spotify.com/documentation/web-api

## Data export
- "Extended streaming history" = lifetime, all devices, JSON, includes IP + user agent, no track duration — **Documented**, BRD S13.
- Expected file `Streaming_History_Audio_*.json` and fields in TSD §4.8 — **Unverified (T1)** until the real export arrives (U-2).
- Delivery time not published (third-party: up to 30 days) — **Unverified (O2)**; U-1 requested: _date pending_.

## Local client data (desktop)
- Windows media session (SMTC) exposes title, artist, album, album artist, track #, position, duration, play state, shuffle/repeat, thumbnail; **no Spotify URI**, no private-session flag — **Verified**, 2026-10-18, Spotify Store build 1.305.
- Spotify's SMTC `SourceAppUserModelId` contains `Spotify` (Store package `SpotifyAB.SpotifyMusic_zpdnekdrzrea0`) — **Verified**, 2026-10-18.
- Client file `list_player_play_history` (protobuf, ~500 plays with URIs) exists — **Verified**, 2026-10-18 — but **excluded**: reading client internals breaches S11. Do not use.

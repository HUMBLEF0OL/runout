# Spotify — knowledge base

Constraints table with sources: [BRD §11](../../BRD.md) (S1–S17). This file holds the working facts behind them. Format: fact — **status**, date, source.

## Links and IDs
- IDs are 22 base-62 chars; `open.spotify.com` URLs may carry an `intl-xx/` locale segment and `?si=` query — **Documented + tested**, 2026-10-18, TSD §4.1.
- `spotify.link/{code}` short links need an HTTP redirect follow to reach an `open.spotify.com` URL — **Verified**, 2026-10-18, BRD research.
- An unknown `spotify.link` code answers `200 text/html` (~7 KB, openresty), not 404 — **Verified** (curl, synthetic code), 2026-10-18. So `links::resolve_short` follows allow-listed redirects, then falls back to the first `open.spotify.com` item link in the page body; a hop to a non-listed host (e.g. a branch.io domain) is refused. Real-code redirect chain **Unverified**: check with a real short link in M6.
- `spotify:` URIs open the desktop app through the OS shell handler — **Documented**, 2026-10-18, BRD FR-016.

## Embeds and oEmbed
- Embed URL can be derived without any call: `https://open.spotify.com/embed/{type}/{id}` — **Verified**, 2026-10-18, BRD S16.
- oEmbed `https://open.spotify.com/oembed?url=…` needs no auth; returns `title`, `thumbnail_url`, `html` — **Verified** (tested), 2026-10-18, BRD S16.
- Embed CSP: `frame-src https://open.spotify.com` in the host page is sufficient for `/embed/{track|album|artist}/{id}`; the embed's own sub-resources (image CDN, scripts, nested frames) fall under Spotify's document policy, not ours — **Verified** (tested, no CSP violation, all three render), 2026-10-18; see [windows-tauri.md T4](windows-tauri.md#webview2). Embed HTML is served `cache-control: private, no-cache, no-store` — **Verified** (curl), 2026-10-18.
- Recommended iframe attributes (Spotify embed code): `allow="autoplay; clipboard-write; encrypted-media; fullscreen; picture-in-picture"`, `frameborder="0"`; heights 152 (compact) / 352 — **Documented**, https://developer.spotify.com/documentation/embeds , 2026-10-18.
- Content must not be cached locally or altered; ≤20 items per set ending with a Spotify link — **Documented**, BRD S8, S10. Docs: https://developer.spotify.com/documentation/embeds , https://developer.spotify.com/documentation/design

## Brand assets (logo)
- Official downloads, no login: full logo https://developer.spotify.com/images/guidelines/design/2024-spotify-full-logo.zip , icon https://developer.spotify.com/images/guidelines/design/2024-spotify-logo-icon.zip (linked from https://developer.spotify.com/documentation/design ) — **Verified** (curl 200, zip), 2026-10-18. Files dated 2024-08-19.
- Saved unaltered (renamed only): `src/assets/spotify/spotify-logo-white.svg` = `Full_Logo_White_RGB.svg` (sha256 `31cdfcdd…684f053c`), `src/assets/spotify/spotify-icon-white.svg` = `Primary_Logo_White_RGB.svg` (sha256 `8929d148…2ef2daa1`) — 2026-10-18.
- Rules: logo ≥ 70 px, icon ≥ 21 px (digital); green logo only on black or white, otherwise monochrome — **white on dark backgrounds** (our graphite UI); clear space = ½ icon height; never modify, rotate, recolour, stretch or use in a sentence; wordmark never without the icon — **Documented**, https://developer.spotify.com/documentation/design , 2026-10-18.
- Gotcha: the SVGs use a generic `.cls-1` class in an inline `<style>` — use them via `<img src>` (not inlined into the DOM) so the class can't collide.

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

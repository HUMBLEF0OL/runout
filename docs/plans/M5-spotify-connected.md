# Plan: M5 Backend — Spotify connected

Specs: [MILESTONES M5](../MILESTONES.md#m5--backend-spotify-connected) · [API.md §5](../API.md#5-r3-commands-backend-m5--switched-in-m7) · TSD §3 (`spotify_cache.db`), §4.2, §5.2, §6, §7 · BRD FR-035–043, §11 Spotify constraints, D27 · KB [spotify.md](../kb/spotify.md).
Agents: `spotify-researcher` (gate + T3), `rust-core`, `spec-reviewer` after steps 3 and 5.

**Entry:** M3 done (M4 not required). UI is already built (M2): implement [API.md](../API.md) as the UI expects it; no page changes (see [plans README](README.md#rules-for-backend-milestones-m3m5)). **Gate before any R3 code:** U-6 developer app created → one `GET /me` call with the user’s token proves the Student Premium plan works in Development Mode (**O1**). Fails → stop R3, report to the user (scope decision). Optional U-8/O7.

**Known gaps owned here:** G-9. See [MILESTONES Known gaps](../MILESTONES.md#known-gaps); remove each row when fixed.

## Steps
0. **Gate.** `spotify-researcher` re-checks T3 (redirect rules, `/me/library` scope); U-6 + U-7; minimal PKCE script in `src-tauri/examples/spotify_gate.rs` calls `GET /me`. Record O1 result in KB + BRD §18 via `change-spec`. **Done 19 Oct 2026:** O1 passed (`GET /me` 200, premium), T3 dynamic loopback port verified ([KB](../kb/spotify.md#web-api-release-3)). **Client ID rule (user):** never in the repo, never logged; read at run time (settings or env) — a `check.py` secret scan must cover it.
1. **Auth (WI 1).** `spotify/auth.rs`: PKCE (S256 verifier), bind free loopback port, open browser, receive `/callback`, exchange code; `keyring` entry `Runout/spotify` holds `{access, refresh, expires_at}`; refresh on 401 and **store rotated refresh token**; `invalid_grant` → delete tokens, status `reconnect`, `spotify-status` event. `client.rs` wraps `net` with auth header, 429 `Retry-After` backoff, max 1 in-flight write.
   - Tests: PKCE challenge vector; token store round-trip with a test credential target; refresh-rotation handler with a stubbed response.
2. **Cache DB (WI 2).** `spotify/cache.rs`: separate `spotify_cache.db` (`cache_entry`, `playlist_snapshot`), own tiny migration; sweeper at startup + daily (cache 7 d, snapshots 30 d); `disconnect_spotify` closes the connection and deletes the file + tokens.
   - Tests: expired entry gone after sweep; disconnect leaves no file; backup zip contains no `spotify_cache` content.
3. **Playlist Studio (WI 3).** `spotify/studio.rs`: list own/collab playlists → load draft (items + current `snapshot_id`, join **only** `item` ratings/tags by URI, dupes within/across) → `preview_studio_apply` diff → apply: re-`GET` playlist, compare `snapshot_id`, mismatch → `conflict`; else store 30-day undo snapshot in cache DB, then replace items in chunks of 100 → `undo_last_apply` restores from snapshot.
   - Tests: diff function table; conflict path with stubbed GET; grep check in `check.py`: `spotify/` never references `listening_event` or `exported_playlist`.
4. **Playlist from filter (WI 4).** Resolve `SavedFilter` via M3 `filters.rs` → URIs (skip items without one, count them) → `POST /me/playlists` + add items.
5. **Autofill (WI 5).** `autofill(uri)`: cache hit or `GET /albums|tracks|artists/{id}` → cache entry; return fields without writing. Confirm path = M3 `update_item`/`create_item` with `promoted_fields`. `bulk_add_album_tracks` creates promoted track items linked `on_album`.
   - Tests: promotion rule — unconfirmed cached fields vanish after expiry; confirmed fields persist in `item` (BRD §13.3).
6. **Top items + recent plays (WI 6).** `GET /me/top/{type}?time_range=`, `GET /me/player/recently-played`; display only, served through the cache; never written to `library.db`; `in_library_id` by URI lookup.
7. **Library save/remove (WI 7).** `PUT|DELETE /me/library`, `GET /me/library/contains`; batch ≤ 40.
8. **Commands + record.** Register API.md §5; status/disabled behaviour when disconnected (every R3 command → `spotify_auth`, rest of app unaffected); SOURCE_MAP `spotify/` → `built`; KB facts; `close-milestone M5`.

## Parallelism
0 blocks all. Then 1 → 2 → (3, 4, 5, 6, 7 in parallel; 4 needs M3 filters).

## Rules
- Spotify-provided data lives only in `spotify_cache.db` until the user confirms (TSD §4.2).
- Playlist Studio uses ratings and tags only — no listening data (BRD 11.1, D27).
- All round-trip testing on a **test playlist** the user created for this; never touch real playlists in tests.
- Tokens never logged; `check.py` secret scan covers `spotify/`.

## Risks
| Risk | Response |
|---|---|
| O1 fails (Premium plan not accepted) | R3 stops; user decides (USER_ACTIONS); M6 unaffected |
| Endpoint names change (Feb 2026 Dev Mode guide) | Re-verify each endpoint in step 0; KB holds checked date |
| Snapshot race between GET and write | Accepted (TSD §5.2); undo snapshot covers recovery |

## Done when
MILESTONES M5 acceptance passes; `python scripts/check.py` exits 0.

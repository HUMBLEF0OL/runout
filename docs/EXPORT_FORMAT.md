# Runout library export format

| | |
|---|---|
| Version | `format_version: 1` |
| Implements | BRD FR-020 · [API.md](API.md) `export_library` · `src-tauri/src/export.rs` |
| Checked by | `export::tests::doc_matches_a_real_export`: the example below must have the same keys as a real export |

**Settings → Export library** writes one UTF-8 JSON file you choose (default name `runout-library.json`). It holds everything you entered in Runout, so you can read it, keep it, or move to another tool. It is not a backup: to restore Runout, use a backup (`.zip`) instead.

**What isn't in it:**
- Usage counters (`usage_event`): they stay on this PC (TSD §4.12).
- Anything Spotify provides (`spotify_cache.db`): it lives only in the cache and is never exported.
- Listening history and import batches (Release 2): not part of format 1.
- Pending phone shares (`unresolved_capture`): short links still waiting to resolve; they become records once resolved.

## Top level
| Key | Type | What |
|---|---|---|
| `format_version` | integer | `1`. Raised when a key is renamed or removed; new keys can appear without a raise |
| `exported_at` | string | ISO-8601 UTC, e.g. `2026-10-18T14:23:07Z` |
| `items` | array | Every album, track and artist record (below) |
| `tags` | array | `{id, name, description}` |
| `links` | array | `{from_id, to_id, kind}`; `kind` is `by_artist`, `on_album` or `version_of`; ids are `items[].id` |
| `history` | array | Status changes `{id, item_id, from_status, to_status, at, note}`, oldest first; `from_status` is `null` for the first entry |
| `saved_filters` | array | `{id, name, definition}`; `definition` is an `ItemFilter` object (API.md §2) |

## Items
Every column of the `item` table (TSD §3), plus two keys:
| Key | Type | What |
|---|---|---|
| `id` | integer | Stable within one export |
| `type` | string | `album`, `track` or `artist` |
| `display_name` | string or null | `null` = "name needed" (saved from the phone, not yet named) |
| `spotify_uri` | string or null | `spotify:{type}:{22-char id}` |
| `status` | string | `planned`, `in_progress`, `explored`, `not_for_me`, `abandoned` |
| `priority` | string or null | `high`, `medium`, `low` |
| `rating` | number or null | 0.5 to 5 in half steps |
| `review`, `notes`, `recommended_by` | string or null | Your own text |
| `favourite`, `revisit`, `in_backlog` | integer | `1` = yes, `0` = no |
| `backlog_order` | integer or null | Position in the backlog (lower = sooner) |
| `added_via` | string | `pc` or `phone` |
| `added_at`, `created_at`, `modified_at`, `explored_at`, `reviewed_at` | string or null | ISO-8601 UTC |
| `tags` | array of strings | Tag names on this item |
| `provenance` | object | Field name → `user` (you typed it) or `promoted` (you confirmed it from a Spotify link, TSD §4.2). Fields not listed have no recorded source |

## Example
A real export (the test `export::tests::doc_matches_a_real_export` keeps it in step) of a synthetic library: an album pasted with its artist, tagged, explored and reviewed, and one saved filter. Ids are random, not real Spotify ids; timestamps vary:

```json
{
  "exported_at": "2026-10-18T14:23:07Z",
  "format_version": 1,
  "history": [
    {
      "at": "2026-10-18T14:23:07Z",
      "from_status": null,
      "id": 1,
      "item_id": 2,
      "note": null,
      "to_status": "planned"
    },
    {
      "at": "2026-10-18T14:23:07Z",
      "from_status": null,
      "id": 2,
      "item_id": 1,
      "note": null,
      "to_status": "planned"
    },
    {
      "at": "2026-10-18T14:23:07Z",
      "from_status": "planned",
      "id": 3,
      "item_id": 1,
      "note": null,
      "to_status": "explored"
    }
  ],
  "items": [
    {
      "added_at": "2026-10-18T14:23:07Z",
      "added_via": "pc",
      "backlog_order": null,
      "created_at": "2026-10-18T14:23:07Z",
      "display_name": "Fixture album",
      "explored_at": "2026-10-18T14:23:07Z",
      "favourite": 0,
      "id": 1,
      "in_backlog": 0,
      "modified_at": "2026-10-18T14:23:07Z",
      "notes": "my own note",
      "priority": null,
      "provenance": {
        "artist": "promoted",
        "display_name": "promoted",
        "notes": "user",
        "rating": "user",
        "recommended_by": "user",
        "review": "user",
        "spotify_uri": "promoted"
      },
      "rating": 4.5,
      "recommended_by": "a friend",
      "review": "a made-up review",
      "reviewed_at": "2026-10-18T14:23:07Z",
      "revisit": 0,
      "spotify_uri": "spotify:album:UyasRLN3R6mDSPiqdJ8pME",
      "status": "explored",
      "tags": [
        "late night"
      ],
      "type": "album"
    },
    {
      "added_at": "2026-10-18T14:23:07Z",
      "added_via": "pc",
      "backlog_order": null,
      "created_at": "2026-10-18T14:23:07Z",
      "display_name": "Synthetic Quartet",
      "explored_at": null,
      "favourite": 0,
      "id": 2,
      "in_backlog": 0,
      "modified_at": "2026-10-18T14:23:07Z",
      "notes": null,
      "priority": null,
      "provenance": {
        "display_name": "promoted"
      },
      "rating": null,
      "recommended_by": null,
      "review": null,
      "reviewed_at": null,
      "revisit": 0,
      "spotify_uri": null,
      "status": "planned",
      "tags": [],
      "type": "artist"
    }
  ],
  "links": [
    {
      "from_id": 1,
      "kind": "by_artist",
      "to_id": 2
    }
  ],
  "saved_filters": [
    {
      "definition": {
        "min_rating": 4.0,
        "tags": [
          "late night"
        ]
      },
      "id": 1,
      "name": "Late-night favourites"
    }
  ],
  "tags": [
    {
      "description": null,
      "id": 1,
      "name": "late night"
    }
  ]
}
```

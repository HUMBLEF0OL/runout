# M2 journeys A–H: keyboard-only evidence

Produced by `node scripts/journeys.mjs` (headless Edge over CDP, mock data, Release 3, 1440×900) on 2026-10-18. All user actions use keys only: Tab/Shift+Tab, Enter, Space, arrows, Escape, digits, Ctrl+N, Ctrl+K, and typed text. Every keyboard stop is checked for a visible focus ring. Result: **30/30 steps PASS**. Spotify hosts are blocked at DNS, so each embed slot is checked by its iframe `src`.

| Journey | Steps (BRD §8) | Result | Screenshots |
|---|---|---|---|
| A Save for later | Ctrl+N → focus in link field → paste album link → type detected, name filled in, embed slot and attribution → Save → toast → Backlog row, Planned (Start offered) → item page with embed iframe. Quick-add modes: duplicate (Open existing), unsupported (podcast episode), offline short link → Save for later → queued | PASS | A1-quick-add-link, A2-saved-backlog, A3-item-embed, A4-quick-add-duplicate, A5-quick-add-unsupported, A6-quick-add-offline-save-later |
| B Capture from phone | Runout Inbox (D33): Settings → Check now imports the phone's Inbox add → Backlog "Name needed" +1 → Name it → naming dialog → Save → badge count back, named row listed | PASS | B0-inbox-checked, B1-share-name-needed, B2-quick-add-naming, B3-named-in-backlog |
| C Explore an album | Ctrl+K search → open → key 2 sets In progress → Open Spotify link reachable → rating with arrows (4★) → review (saved on blur) → tag added → key 3 sets Explored → Space flags Revisit | PASS | C1-in-progress, C2-explored-rated-reviewed |
| D Explore an artist | Ctrl+N artist link → "Why this artist" notes → Save → artist page → Add an album by them (manual) → listed under In your library → key 2 sets In progress → notes updated | PASS | D1-quick-add-artist, D2-artist-page |
| E Rediscover | Tag filter changes the shelves → rating 4 and up + Revisit only → "Flagged to revisit" shows the album → Show artwork ends with Spotify logo and Open Spotify → record opens with Open Spotify | PASS | E1-filter-tag, E2-filter-rating-revisit, E3-artwork-open-spotify |
| F Listening history | Rail "Import a newer export" → Choose → Next → Import N plays → Done → See your insights → freshness note "Desktop plays: live. All devices: up to your export of …" + source tags | PASS | F1-import-choose, F2-import-check, F3-import-done, F4-insights-freshness |
| G Clean up a playlist | Check-up shows duplicate and low-rated counts and rows → Open in Studio → Keep/Cut with arrow keys → Apply → preview dialog → confirm → toast with Undo → undo banner → Undo last apply restores | PASS | G1-checkup, G2-studio-draft, G3-studio-preview, G4-studio-undo-banner |
| H Back up and restore | Back up now → toast with folder and size, "(latest)" listed → Restore → pick → check passes with record/album/track/artist counts → Restore → "N records verified" | PASS | H1-backup-done, H2-restore-check, H3-restored |

Also checked: with `prefers-reduced-motion: reduce`, `.btn` transition and `.skel` animation are off (M1). Every rendered `<button>` on each visited route has an accessible name (M2). No focus ring was missing at any keyboard stop (M3).

---
name: compliance-review
description: Check a Runout change against Spotify developer rules, the promotion rule and privacy rules before calling it done. Use when reviewing any change that stores, fetches, shows or logs data, and before closing a milestone.
---

# Compliance review

## Process
1. Run `python scripts/check.py`; note the result.
2. List the changed files; for each, walk the checklist below. Skip items that can't apply to that file.
3. Cite the rule for every finding (TSD §, BRD S-id/D-id/FR). No rule → not a finding.
4. Mechanically detectable finding → propose the exact check to add to `scripts/check.py`.

## Toolbox
- BRD §5 (sources, promotion, minimization), §11 (S1–S17), §11.1 (export risk), D21, D24, D27.
- TSD §4.2 promotion, §4.3 embeds/CSP, §4.8 import allow-list, §4.9 insights isolation, §7 security.
- `docs/kb/spotify.md`.

## Proof — the checklist
- [ ] **Hosts**: outbound only to `open.spotify.com`, `spotify.link`, `api.spotify.com`, `accounts.spotify.com`, plus `musicbrainz.org`, `www.wikidata.org` from `enrich/` (artist/album names from U/S/E only, source M, TSD §5.4), via `net.rs` (TSD §7).
- [ ] **Source A storage**: Spotify-provided data only in `spotify_cache.db` with expiry; into `library.db` only via user-confirmed promotion with provenance (TSD §4.2, S8).
- [ ] **oEmbed**: response never persisted; only the confirmed title (TSD §4.3).
- [ ] **Insights**: computed from U/S/E only; no path to the cache DB (S9, TSD §4.9).
- [ ] **Playlist Studio**: uses ratings/tags only, never listening data (D27).
- [ ] **Import**: allow-listed fields; `ip_addr*`, `user_agent*`, `username` dropped; episodes skipped (BRD §5.3).
- [ ] **Private sessions**: excluded from insights by default (D24).
- [ ] **Logs**: no tokens, notes, reviews (TSD §7).
- [ ] **Client internals**: no reading of Spotify client files/code (S11).
- [ ] **Design**: logo + "Open Spotify" link with Spotify content; ≤20 per set; artwork unaltered (S10, FR-044).
- [ ] **Disconnect/uninstall**: cache DB, tokens, EBWebView removed (D21, TSD §4.11).
- [ ] **Fixtures/commits**: no real export values, tokens or personal data.

## Gotchas
<!-- Lessons from corrections go here, newest first. -->

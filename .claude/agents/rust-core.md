---
name: rust-core
description: Implements and fixes Runout's Rust core in src-tauri/ (db, migrations, links, net, library, capture, backup, logger/SMTC, importer, insights, spotify). Use proactively for any work item that touches Rust, SQL migrations or Tauri commands.
skills:
  - build-work-item
color: orange
---

You build Runout's Rust core: Tauri 2, rusqlite (bundled, FTS5, WAL), reqwest, tiny_http, `windows` crate (SMTC), keyring.

Follow the preloaded `build-work-item` playbook exactly. Before coding, read:
- the TSD sections the work item names, and `docs/SOURCE_MAP.md` for where the code goes;
- `docs/kb/windows-tauri.md` and `docs/kb/spotify.md` for platform facts and gotchas.

Hard rules:
- All data access and network live here. Every outbound request goes through `net.rs`'s allow-list (TSD §7).
- Schema changes = new numbered migration; never edit a shipped one.
- Spotify-provided data (source A) only in `spotify_cache.db`; `library.db` gets it only via the promotion rule (TSD §4.2).
- Import keeps only allow-listed fields; drop `ip_addr*`, `user_agent*`, `username`.
- Never log tokens, notes or reviews. Never read Spotify client files (BRD S11).
- Rules/algorithms get a failing test first (`cargo test`); fixtures are synthetic.

Finish with `python scripts/check.py` green, the source map updated, and a ≤3-line report: done, proof, deferred.

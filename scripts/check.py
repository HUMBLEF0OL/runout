"""Universal proof for Runout. Exit 0 = all checks pass. Run: python scripts/check.py
Each check skips itself until the code it checks exists. Add a check here whenever a
correction is automatable (see CLAUDE.md, "the loop").
--hook: Stop-hook mode (.claude/settings.json). Exit 2 + failures on stderr keeps Claude working.
ponytail: runs the full suite on every stop; gate on changed files if cargo gets slow."""
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TAURI = ROOT / "src-tauri"
ALLOWED_HOSTS = {"open.spotify.com", "spotify.link", "api.spotify.com", "accounts.spotify.com", "127.0.0.1", "localhost"}
HOOK = "--hook" in sys.argv
failures = []
if HOOK and json.load(sys.stdin).get("stop_hook_active"):
    sys.exit(0)  # already continuing because of this hook; don't loop
if HOOK:
    sys.stdout = sys.stderr  # Claude only sees stderr from a blocking Stop hook


def run(name, cmd, cwd):
    print(f"--- {name}", flush=True)
    r = subprocess.run(cmd, cwd=cwd, shell=True, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if r.returncode:
        failures.append(name)
        print((r.stdout + r.stderr)[-3000:])


def source_files():
    for d, exts in ((TAURI / "src", {".rs"}), (ROOT / "src", {".ts", ".tsx"})):
        if d.exists():
            yield from (p for p in d.rglob("*") if p.suffix in exts)


def check_hosts():
    # TSD §7: only allow-listed outbound hosts may appear in code.
    bad = [f"{p.relative_to(ROOT)}: {h}" for p in source_files()
           for h in re.findall(r"https?://([\w.-]+)", p.read_text(encoding="utf-8"))
           if h not in ALLOWED_HOSTS]
    if bad:
        failures.append("allowed hosts")
        print("--- allowed hosts: FAIL\n" + "\n".join(bad))


def check_one_item_summary():
    # Review M3 6-10: event payloads use library::items::summary; a hand-built ItemSummary drifts from API.md.
    bad = [str(p.relative_to(ROOT)) for p in (TAURI / "src").rglob("*.rs")
           if "library" not in p.parts and "json_object('id'" in p.read_text(encoding="utf-8")]
    if bad:
        failures.append("one ItemSummary")
        print("--- one ItemSummary: FAIL (build it with library::items::summary)\n" + "\n".join(bad))


def check_events():
    # Review M3 step 11: every MVP event in API.md §6, and each 'literal' in its payload, is emitted
    # somewhere in Rust (capture-disabled {reason:'error'} was specified but never sent).
    # non-test code only: a test's expected payload doesn't prove the app sends it
    rs = "\n".join(re.split(r"#\[cfg\(test\)\]\r?\nmod tests", p.read_text(encoding="utf-8"))[0]
                   for p in (TAURI / "src").rglob("*.rs"))
    if "emit(" not in rs:
        return
    spec = (ROOT / "docs/API.md").read_text(encoding="utf-8")
    sec = re.search(r"^## 6\..*?(?=^## )", spec, re.S | re.M).group(0)
    rows = re.findall(r"^\| `([a-z-]+)` \| (.*?) \| MVP \|$", sec, re.M)
    bad = [f"API.md section 6 '{ev}': {lit} never appears as a Rust string literal"
           for ev, payload in rows for lit in [ev, *re.findall(r"'([a-z_]+)'", payload)]
           if f'"{lit}"' not in rs]
    if not rows:
        bad.append("no MVP events parsed from API.md §6 (table format changed?)")
    if bad:
        failures.append("events")
        print("--- events: FAIL\n" + "\n".join(bad))


def check_settings_paths():
    # API.md §1 (pickers by Rust): update_settings must refuse both paths the UI may not choose.
    p = TAURI / "src/settings.rs"
    if not p.exists():
        return
    m = re.search(r"pub fn merge\(.*?\n}\n", p.read_text(encoding="utf-8"), re.S)
    missing = [k for k in ("library_path", "backup_dir") if not m or f'"{k}"' not in m.group(0)]
    if missing:
        failures.append("settings paths")
        print("--- settings paths: FAIL (settings::merge must refuse) " + ", ".join(missing))


def check_restore_marker_guards():
    # Review M3 close: while a failed restore's marker exists, pre-restore may be the real library
    # (TSD §4.10); each of these must check it (delete_library didn't, and wiped it).
    bad = []
    for f, fn in (("backup.rs", "restore_locked"), ("state.rs", "move_library"), ("cleanup.rs", "delete_library")):
        p = TAURI / "src" / f
        if not p.exists():
            continue
        m = re.search(rf"fn {fn}\(.*?(?=\n\s*(?:pub(?:\(crate\))? )?fn |\Z)", p.read_text(encoding="utf-8"), re.S)
        if not m or "marker(" not in m.group(0):
            bad.append(f"{f}: {fn} must refuse while backup::marker(..) exists")
    if bad:
        failures.append("restore marker guards")
        print("--- restore marker guards: FAIL\n" + "\n".join(bad))


def check_no_oembed_or_tokens_logged():
    # TSD §7: tokens never logged.
    pat = re.compile(r"(e?println!|dbg!|log::\w+!|tracing::\w+!|console\.log)\(.*(token|secret|refresh|client_id|verifier)", re.I)
    examples = list((TAURI / "examples").glob("*.rs")) if (TAURI / "examples").exists() else []
    bad = [f"{p.relative_to(ROOT)}:{i}" for p in list(source_files()) + examples
           for i, line in enumerate(p.read_text(encoding="utf-8").splitlines(), 1) if pat.search(line)]
    if bad:
        failures.append("no secrets in logs")
        print("--- no secrets in logs: FAIL\n" + "\n".join(bad))


def check_spotify_rules():
    # M5 plan rules (BRD 11.1, D27; TSD §7; KB client-ID decision) for src-tauri/src/spotify/:
    # Playlist Studio etc. never touch listening data or exported playlists; nothing logs; no
    # token/client id/verifier/code is interpolated into a string (errors, details, panics).
    d = TAURI / "src" / "spotify"
    if not d.exists():
        return
    data = re.compile(r"listening_\w+|exported_playlist")
    logs = re.compile(r"\b(println!|eprintln!|print!|eprint!|dbg!|log::\w+!|tracing::\w+!)")
    interp = re.compile(r"\{[^{}\"]*\b(access|refresh|token|client_id|verifier|code|secret)\w*[^{}\"]*\}")
    # audit M5: the same names passed as positional args after the format string (`"{}", t.refresh`)
    positional = re.compile(r"\"\s*,.*\b(access|refresh|access_token|refresh_token|client_id|verifier|code_verifier|secret)\b")
    bad = []
    # examples run against the real credential: same data + interpolation rules (they may print)
    examples = list((TAURI / "examples").glob("*.rs")) if (TAURI / "examples").exists() else []
    for p in list(d.rglob("*.rs")) + examples:
        rel = p.relative_to(ROOT).as_posix()
        in_spotify = p.parent.name == "spotify"
        for i, line in enumerate(p.read_text(encoding="utf-8").splitlines(), 1):
            if in_spotify and data.search(line):
                bad.append(f"{rel}:{i}: spotify/ must not reference listening_* tables / exported_playlist")
            if in_spotify and logs.search(line):
                bad.append(f"{rel}:{i}: no logging in spotify/ (tokens, client id)")
            if re.search(r"format!|panic!|assert\w*!|with_detail|AppError|e?print(ln)?!", line) and (
                    interp.search(line) or positional.search(line)):
                bad.append(f"{rel}:{i}: secret-looking value interpolated into a string")
            # audit M5: a Credential Manager read error is not "signed out" (it must not be cached
            # or trigger the sweeper's cache delete); keep the Err
            if in_spotify and re.search(r"load_tokens\([^)]*\)\.ok\(\)", line):
                bad.append(f"{rel}:{i}: load_tokens(..).ok() turns a read error into 'signed out'")
            # audit M5: examples never share the app's credential (a refresh there rotates the
            # app's refresh token and signs the app out)
            # audit r6: Spotify::new(..) passes the same credential, and so does the literal name
            code = line.split("//", 1)[0]
            if not in_spotify and re.search(r"cleanup::CREDENTIAL|Spotify::new\(|Runout/spotify", code):
                bad.append(f"{rel}:{i}: examples must not use the app's credential (CREDENTIAL / Spotify::new / Runout/spotify)")
    # Studio: every fn that writes a playlist (calls replace(..)) holds studio_lock across the
    # whole check -> snapshot -> chunks sequence, so two applies/undos never interleave.
    studio = d / "studio.rs"
    if studio.exists():
        # audit M5 (r3): the signature may span lines (replace's does); a regex that skips a fn
        # silently disables every rule below, so the fns it must find are asserted
        fn_re = r"\n(?:pub(?:\(crate\))? )?(?:async )?fn (\w+)\b[^{;]*\{(.*?)\n\}"
        found = {m.group(1) for m in re.finditer(fn_re, studio.read_text(encoding="utf-8"), re.S)}
        for need in ("replace", "apply_studio_draft", "undo_last_apply", "load_studio_draft"):
            if need not in found:
                bad.append(f"{studio.relative_to(ROOT).as_posix()}: check.py's fn regex no longer finds {need}; fix the regex")
        for m in re.finditer(fn_re, studio.read_text(encoding="utf-8"), re.S):
            if m.group(1) != "replace" and re.search(r"\breplace\(", m.group(2)) and "studio_lock" not in m.group(2):
                bad.append(f"{studio.relative_to(ROOT).as_posix()}: {m.group(1)} calls replace( without sp.studio_lock")
            # audit M5 (r2): a failed playlist write may still have landed (timeout, 5xx): replace
            # decides by client::refused(&e) (4xx = not landed, else unknown), never `?` and never
            # a re-read (a re-read proves nothing, TSD §5.2)
            if m.group(1) == "replace" and re.search(r"send_json\([^;]*?\)\s*\.await\?", m.group(2), re.S):
                bad.append(f"{studio.relative_to(ROOT).as_posix()}: replace `?`s a write error; use client::refused(&e) (4xx = not landed, else unknown; TSD §5.2)")
            if m.group(1) == "replace" and re.search(r"current_uris\(|items\(sp", m.group(2)):
                bad.append(f"{studio.relative_to(ROOT).as_posix()}: replace re-reads the items; a re-read proves nothing after a failed write (TSD §5.2)")
    # audit M5 (r3): --probe must check the read Studio decides on (studio::current_uris, whole
    # list), not a hand-built /items? GET (a fresh limit=1 says nothing about it, G-30)
    rt = TAURI / "examples/studio_roundtrip.rs"
    if rt.exists():
        m = re.search(r"\nasync fn probe_row\b.*?\n\}\n", rt.read_text(encoding="utf-8"), re.S)
        body = m.group(0) if m else ""
        if "studio::current_uris(" not in body or "/items?" in body:
            bad.append("src-tauri/examples/studio_roundtrip.rs: probe_row must read the list with studio::current_uris(, not its own /items? GET (G-30)")
    # audit M5: status never turns a Credential Manager read error into 'disconnected'
    spmod = d / "mod.rs"
    if spmod.exists():
        fn_re = r"\n    pub (?:async )?fn (status_now|status)\b[^\n]*\{(.*?)\n    \}"
        for m in re.finditer(fn_re, spmod.read_text(encoding="utf-8"), re.S):
            if "self.tokens()" in m.group(2):
                bad.append(f"{spmod.relative_to(ROOT).as_posix()}: {m.group(1)} uses self.tokens(); use load_tokens()?")
    if bad:
        failures.append("spotify rules")
        print("--- spotify rules: FAIL\n" + "\n".join(bad))


def check_boundary_values_kept():
    # audit M5: check_uri trims; a call whose result is thrown away validates one value and uses another.
    pat = re.compile(r"^\s*(?:spotify::)?check_uri\(.*\)\?;\s*$")
    bad = [f"{p.relative_to(ROOT).as_posix()}:{i}: check_uri(..)?; discards the trimmed URI; use its result"
           for p in (TAURI / "src").rglob("*.rs")
           for i, line in enumerate(p.read_text(encoding="utf-8").splitlines(), 1) if pat.search(line)]
    if bad:
        failures.append("boundary values kept")
        print("--- boundary values kept: FAIL\n" + "\n".join(bad))


def check_cache_schema_in_tsd():
    # audit M5 / CLAUDE.md flow step 5: every spotify_cache.db column (spotify/cache.rs MIGRATIONS:
    # CREATE TABLE + ADD COLUMN) is named in TSD §3's spotify_cache.db block.
    cache, tsd = TAURI / "src/spotify/cache.rs", ROOT / "docs/TSD.md"
    if not cache.exists():
        return
    src = cache.read_text(encoding="utf-8")
    m = re.search(r"^`spotify_cache\.db`[^\n]*:\n```sql\n(.*?)```", tsd.read_text(encoding="utf-8"), re.S | re.M)
    block = m.group(1) if m else ""
    want = []
    for table, body in re.findall(r"CREATE TABLE (\w+)\((.*?)\);", src, re.S):
        want += [(table, c) for c in re.findall(r"(?:^|,)\s*(\w+)\s+[A-Z]", body)]
    want += re.findall(r"ALTER TABLE (\w+) ADD COLUMN (\w+)", src)
    rows = dict(re.findall(r"^(\w+)\((.*)$", block, re.M))
    bad = [f"TSD §3 spotify_cache.db: {t}.{c} missing" for t, c in want
           if not re.search(rf"\b{c}\b", rows.get(t, ""))]
    if not want:
        bad.append("no columns parsed from spotify/cache.rs MIGRATIONS (format changed?)")
    if bad:
        failures.append("cache schema in TSD")
        print("--- cache schema in TSD: FAIL\n" + "\n".join(bad))


def check_dropped_cache_columns():
    # M5 audit r1: examples/studio_roundtrip.rs kept querying a column cache migration 4 dropped and
    # .ok() hid the error. No file under src-tauri/ but cache.rs names a dropped/renamed column.
    cache = TAURI / "src/spotify/cache.rs"
    if not cache.exists():
        return
    gone = set(re.findall(r"(?:DROP COLUMN|RENAME COLUMN) (\w+)", cache.read_text(encoding="utf-8")))
    bad = [f"{p.relative_to(ROOT).as_posix()}: names {c} (dropped/renamed by a cache migration)"
           for d in ("src", "examples", "tests") if (TAURI / d).exists()
           for p in (TAURI / d).rglob("*.rs") if p != cache
           for c in gone if re.search(rf"\b{c}\b", p.read_text(encoding="utf-8"))]
    if bad:
        failures.append("dropped cache columns")
        print("--- dropped cache columns: FAIL\n" + "\n".join(bad))


def check_no_snapshot_id_compare_in_docs():
    # M5 audit r1: the M5 plan still said "compare snapshot_id" after the live 2026-10-20 result
    # (GET serves a stale one); a session following the plan would bring the check back (TSD §5.2).
    bad = [f"{p.relative_to(ROOT).as_posix()}:{i}" for p in (ROOT / "docs").rglob("*.md")
           for i, line in enumerate(p.read_text(encoding="utf-8").splitlines(), 1)
           if re.search(r"compar\w*\s+(?:the\s+)?`?snapshot_id", line, re.I)
           # audit r2: only the snapshot_id GET is verified stale; items freshness is an Assumption
           or ("eventually consistent" in line and "snapshot_id" not in line and "Assumption" not in line)]
    if bad:
        failures.append("no snapshot_id compare in docs")
        print("--- no snapshot_id compare in docs: FAIL (Studio checks track content, TSD §5.2)\n" + "\n".join(bad))


def check_example_usage():
    # audit r2: a dev example's `--flag` that its `//!` usage line doesn't name is a step only a
    # commit message describes (studio_roundtrip --probe).
    bad = []
    for p in (ROOT / "src-tauri" / "examples").glob("*.rs"):
        src = p.read_text(encoding="utf-8")
        usage = "\n".join(l for l in src.splitlines() if l.startswith("//!"))
        for flag in re.findall(r'a == "(--[\w-]+)"', src):
            if flag not in usage:
                bad.append(f"{p.relative_to(ROOT).as_posix()}: {flag} missing from the //! usage")
    if bad:
        failures.append("example usage")
        print("--- example usage: FAIL\n" + "\n".join(bad))


def check_mock_conflicts():
    # audit M5 / G-9: every API.md §5 command whose notes promise `conflict` fails with it in the
    # mocks' "conflict" dev state (CONFLICT map in src/mocks/index.ts), so the screen is reachable.
    mock = ROOT / "src/mocks/index.ts"
    sec = re.search(r"^## 5\..*?(?=^## )", (ROOT / "docs/API.md").read_text(encoding="utf-8"), re.S | re.M)
    if not (mock.exists() and sec):
        return
    m = re.search(r"const CONFLICT[^=]*=\s*\{(.*?)\n\};", mock.read_text(encoding="utf-8"), re.S)
    keys = set(re.findall(r"^\s*(\w+):", m.group(1), re.M)) if m else set()
    bad = []
    for cmds, notes in re.findall(r"^\| (`[a-z_]+`(?: / `[a-z_]+`)*) \|(.*)$", sec.group(0), re.M):
        if "`conflict`" in notes:
            bad += [f"src/mocks/index.ts CONFLICT lacks '{c}' (API.md §5 says it returns conflict)"
                    for c in re.findall(r"`([a-z_]+)`", cmds) if c not in keys]
    if bad:
        failures.append("mock conflicts")
        print("--- mock conflicts: FAIL\n" + "\n".join(bad))


def check_mock_r3_parity():
    # M5 audit round 4: the R3 mocks behave like Rust at the M7 switch (API.md §7, M2 WI 3).
    # Every API.md §5 command but status/connect/disconnect answers spotify_auth when disconnected
    # (Rust require_connected); bulk_add_album_tracks skips tracks already in the library; apply
    # compares the current items with the draft before the snapshot and the write, and apply/undo
    # never decide on snapshot_id (GET serves a stale one after a write, TSD §5.2, 2026-10-20).
    mock = (ROOT / "src/mocks/index.ts").read_text(encoding="utf-8")
    sec = re.search(r"^## 5\..*?(?=^## )", (ROOT / "docs/API.md").read_text(encoding="utf-8"), re.S | re.M)
    bad = []
    cmds = re.findall(r"`([a-z_]+)`", " ".join(re.findall(r"^\| (`[a-z_]+`(?: / `[a-z_]+`)*) \|", sec.group(0), re.M))) if sec else []
    for c in cmds:
        if c in ("get_spotify_status", "connect_spotify", "disconnect_spotify"):
            continue
        m = re.search(rf"^  {c}: (.*?)(?=^  \w+: |^}};)", mock, re.S | re.M)
        if not m or "spotifyOn()" not in m.group(1):
            bad.append(f"src/mocks/index.ts: {c} handler lacks spotifyOn() (Rust answers spotify_auth when disconnected)")
    m = re.search(r"^  bulk_add_album_tracks: (.*?)(?=^  \w+: )", mock, re.S | re.M)
    if not m or "existingFor(" not in m.group(1) or m.group(1).find("existingFor(") > m.group(1).find("items.push"):
        bad.append("src/mocks/index.ts: bulk_add_album_tracks must look up existingFor( before items.push (API.md §5)")
    studio = (TAURI / "src/spotify/studio.rs").read_text(encoding="utf-8")
    m = re.search(r"pub async fn apply_studio_draft.*?\n}\n", studio, re.S)
    body = m.group(0) if m else ""
    if not (0 <= body.find("current_uris(sp") < body.find("Code::Conflict") < body.find("put_snapshot(") < body.find("replace(")):
        bad.append("src-tauri/src/spotify/studio.rs: apply_studio_draft must read current_uris(, conflict, put_snapshot(, then replace( (TSD §5.2)")
    if not (0 <= body.find("Code::Conflict") < body.find("unsure_since(") < body.find("put_snapshot(")):
        bad.append("src-tauri/src/spotify/studio.rs: apply_studio_draft must check unsure_since( (UNSURE_WAIT) after the conflict check and before put_snapshot( (TSD §5.2, audit r5)")
    for fn in ("apply_studio_draft", "undo_last_apply", "replace"):
        m = re.search(rf"\n(?:pub )?async fn {fn}\b.*?\n}}\n", studio, re.S)
        if not m or "snapshot_id(sp" in m.group(0):
            bad.append(f"src-tauri/src/spotify/studio.rs: {fn} must not decide on snapshot_id( (GET serves a stale one, TSD §5.2)")
    if bad:
        failures.append("mock R3 parity")
        print("--- mock R3 parity: FAIL\n" + "\n".join(bad))


def check_insights_no_cache():
    # M4 acceptance / FR-031: insights read the library DB only, never the Spotify cache (source A).
    d = TAURI / "src" / "insights"
    bad = [str(p.relative_to(ROOT)) for p in d.rglob("*.rs") if "spotify_cache" in p.read_text(encoding="utf-8")] if d.exists() else []
    if bad:
        failures.append("insights no cache")
        print("--- insights no cache: FAIL\n" + "\n".join(bad))


def check_listening_event_reads():
    # G-20 / audit 4f53129: listening_event reads go through insights::base (reconciled rows out,
    # private sessions per setting). A direct read needs `// unfiltered: <reason>` on the line or up to 2 lines above.
    # Every reader is scanned; importer/ (except playlists.rs) and db/ are the writers and migrations.
    src = TAURI / "src"
    read_pat = re.compile(r"\b(?:FROM|JOIN)\s+listening_event\b", re.S)
    bad = []
    for p in src.rglob("*.rs"):
        rel = p.relative_to(src).as_posix()
        if rel.startswith("db/") or (rel.startswith("importer/") and rel != "importer/playlists.rs"):
            continue
        text = p.read_text(encoding="utf-8")
        lines = text.splitlines()
        for m in read_pat.finditer(text):
            i = text.count("\n", 0, m.start())
            fn = next((f.group(1) for l in reversed(lines[:i + 1]) if (f := re.search(r"\bfn (\w+)", l))), None)
            if rel == "insights/mod.rs" and fn == "base":
                continue
            if not any(re.search(r"// unfiltered: \S", l) for l in lines[max(i - 2, 0):i + 1]):
                bad.append(f"src-tauri/src/{rel}:{i + 1}: read insights::base, or add `// unfiltered: <reason>`")
    if bad:
        failures.append("listening_event reads")
        print("--- listening_event reads: FAIL\n" + "\n".join(bad))


def check_settings_clone_then_assign():
    # G-20: cloning the settings, releasing the guard, then assigning loses a concurrent change.
    # Use AppState::edit_settings (one guard from read to write) instead.
    clone = re.compile(r"settings_now\(|settings\.lock\(\)\.unwrap\(\)\.clone\(\)")
    assign = re.compile(r"\*[^;\n]*settings\.lock\(\)\.unwrap\(\)\s*=[^=]")
    bad = []
    for p in (TAURI / "src").rglob("*.rs"):
        for body in re.split(r"\n\s*(?:pub(?:\(crate\))? )?fn ", p.read_text(encoding="utf-8")):
            c = clone.search(body)
            # also `let mut g = …settings.lock().unwrap(); … *g = s;`
            guards = [g.group(1) for g in re.finditer(r"let mut (\w+) = [^;]*settings\.lock\(\)\.unwrap\(\);", body)]
            via_guard = c and any(re.search(rf"\*{g}\s*=[^=]", body[c.end():]) for g in guards)
            if c and (assign.search(body, c.end()) or via_guard):
                bad.append(f"{p.relative_to(ROOT)}: fn {body.split('(')[0].split()[-1]} clones settings then assigns; use edit_settings")
    if bad:
        failures.append("settings clone-then-assign")
        print("--- settings clone-then-assign: FAIL\n" + "\n".join(bad))


def check_ui_purity():
    # TSD §1 rule, M2 plan: src/ has no network or data code; only api.ts talks to Tauri.
    src = ROOT / "src"
    if not src.exists():
        return
    pat = re.compile(r"\bfetch\(|XMLHttpRequest|new WebSocket|@tauri-apps/plugin-|sqlite", re.I)
    bad = []
    for p in (p for p in src.rglob("*") if p.suffix in {".ts", ".tsx"}):
        rel = p.relative_to(ROOT).as_posix()
        for i, line in enumerate(p.read_text(encoding="utf-8").splitlines(), 1):
            if pat.search(line) and not line.lstrip().startswith("//"):
                bad.append(f"{rel}:{i}: {line.strip()[:80]}")
            if re.search(r"\?\? \[\s*\"[USE]\"", line):  # FR-032: source tags come from the response, never a literal fallback
                bad.append(f"{rel}:{i}: hard-coded source tag fallback; use `?? []`")
            if "]${" in line:  # Tailwind can't see an arbitrary-value class glued to ${...}; the rule is never generated
                bad.append(f"{rel}:{i}: put a space between ']' and '${{' in class strings")
            if re.search(r"\be as AppError\b|isAppError\(e\) \? e : undefined", line):  # a non-AppError throw would vanish or crash
                bad.append(f"{rel}:{i}: convert caught errors with toAppError(e) from api.ts")
            if "@tauri-apps/api" in line and rel != "src/api.ts":
                bad.append(f"{rel}:{i}: only src/api.ts may import @tauri-apps/api")
            # audit M5 (r3): a toast's Undo that calls api.* bare drops the promise: a failure or
            # conflict is silent and nothing reloads; pass the page's handler instead
            if re.search(r"toast\([^;]*,\s*\(\)\s*=>\s*api\.\w+\([^)]*\)\s*\)", line):
                bad.append(f"{rel}:{i}: toast undo calls api.* bare; pass a handler that shows errors and reloads")
    # audit M5 (r3): a failed write may still have changed data (a Spotify write that timed out
    # keeps an undo, TSD §5.2), so readers refetch after every write command, failed or not
    api = src / "api.ts"
    if api.exists():
        m = re.search(r"\nasync function call<T>.*?\n\}\n", api.read_text(encoding="utf-8"), re.S)
        if not (m and re.search(r"finally\s*\{[^}]*invalidate\(\)", m.group(0))):
            bad.append("src/api.ts: call() must invalidate() in a finally (a failed write may have changed data)")
    if bad:
        failures.append("ui purity")
        print("--- ui purity: FAIL\n" + "\n".join(bad))


def check_contract_and_attribution():
    # API.md §7: the contract comes first, so every field in an api.ts interface is named in API.md.
    # DESIGN §7 / FR-044: the Spotify logo never appears without "Open Spotify" in the same file.
    api, doc = ROOT / "src/api.ts", ROOT / "docs/API.md"
    if not api.exists():
        return
    text, spec = api.read_text(encoding="utf-8"), doc.read_text(encoding="utf-8")
    bad = []
    for body in re.findall(r"export interface \w+[^{]*\{(.*?)\n?\}", text, re.S):
        for field in re.findall(r"(?:^|[;{\s])(\w+)\??:", body):
            if not re.search(rf"\b{field}\b", spec):
                bad.append(f"src/api.ts field '{field}' missing from docs/API.md")
    for p in (ROOT / "src").rglob("*.tsx"):
        s = p.read_text(encoding="utf-8")
        if "<SpotifyLogo" in s and "<OpenSpotify" not in s and "<Attribution" not in s and p.name != "ui.tsx":
            bad.append(f"{p.relative_to(ROOT).as_posix()}: <SpotifyLogo> without <OpenSpotify> (use <Attribution>)")
    if bad:
        failures.append("contract + attribution")
        print("--- contract + attribution: FAIL\n" + "\n".join(sorted(set(bad))))


# API.md sections whose commands must be registered in commands.rs. M4 adds "4", M5 adds "5".
COMMAND_SECTIONS = ["3", "5"]


def check_commands():
    # API.md §7: every API.md command in COMMAND_SECTIONS is in commands.rs's generate_handler! list
    # and has its camelCase export in src/api.ts.
    cmds = TAURI / "src/commands.rs"
    if not cmds.exists() or "generate_handler!" not in cmds.read_text(encoding="utf-8"):
        return
    spec = (ROOT / "docs/API.md").read_text(encoding="utf-8")
    want = []
    for sec in COMMAND_SECTIONS:
        body = re.search(rf"^## {sec}\..*?(?=^## )", spec, re.S | re.M).group(0)
        for row in re.findall(r"^\| (`[a-z_]+`(?: / `[a-z_]+`)*) \|", body, re.M):
            want += re.findall(r"`([a-z_]+)`", row)
    reg = re.search(r"generate_handler!\[(.*?)\]", cmds.read_text(encoding="utf-8"), re.S).group(1)
    registered = set(re.findall(r"\w+", reg))
    api = (ROOT / "src/api.ts").read_text(encoding="utf-8")
    camel = lambda n: re.sub(r"_(\w)", lambda m: m.group(1).upper(), n)
    bad = [f"API.md command '{n}' not registered in commands.rs" for n in want if n not in registered]
    bad += [f"API.md command '{n}' has no `export const {camel(n)}` in src/api.ts" for n in want
            if not re.search(rf"export const {camel(n)}\b", api)]
    if not want:
        bad.append("no commands parsed from API.md (table format changed?)")
    # Each api.ts wrapper is named camel(cmd), and every arg key it passes is a camelCased parameter of
    # that command in commands.rs (state/app handles excluded). Commands not in commands.rs yet are skipped.
    rs = cmds.read_text(encoding="utf-8")
    calls = re.findall(r'export const (\w+) = [^;]*?\bcall<.*?>\("(\w+)"(.*?)\);\n', api, re.S)
    for name, cmd, rest in calls:
        if camel(cmd) != name:
            bad.append(f"src/api.ts: '{name}' calls '{cmd}' (expected export const {camel(cmd)})")
        fn = re.search(rf"pub (?:async )?fn {cmd}(?:<[^>]*>)?\((.*?)\) ->", rs, re.S)
        obj = rest.strip().removeprefix(",").strip()
        if not fn or not obj.startswith("{"):
            continue
        params = {camel(n) for n in re.findall(r"(\w+): (?!St<|State<|AppHandle<|tauri::)", fn.group(1))}
        depth, keys = 0, [""]
        for ch in obj[1:]:  # top-level keys of the args object literal
            if ch in "})]" and depth == 0:
                break
            depth += (ch in "{([") - (ch in "})]")
            if ch == "," and depth == 0:
                keys.append("")
            else:
                keys[-1] += ch
        for k in (m.group(1) for m in (re.match(r"\s*(\w+)", x) for x in keys) if m):
            if k not in params:
                bad.append(f"src/api.ts {name}: arg '{k}' is not a parameter of commands.rs {cmd}({', '.join(sorted(params))})")
    if not calls:
        bad.append('no call<...>("cmd") wrappers parsed from src/api.ts')
    if bad:
        failures.append("commands")
        print("--- commands: FAIL" + chr(10) + chr(10).join(bad))


def check_release_build():
    # M6 audit: the installed build ran on mocks (no VITE_MOCK=0 for `vite build`); a custom NSIS language
    # file replaces Tauri's whole table (27 strings in cli 2.12.3); the pre-uninstall hook runs before the
    # template closes the app, so the hook must close it before cleaning up.
    bad = []
    env = ROOT / ".env.production"
    if not (env.exists() and re.search(r"^VITE_MOCK=0\s*$", env.read_text(encoding="utf-8"), re.M)):
        bad.append(".env.production must set VITE_MOCK=0 (tauri build -> npm run build)")
    lang = TAURI / "windows" / "English.nsh"
    if lang.exists():
        keys = re.findall(r"^LangString (\w+) ", lang.read_text(encoding="utf-8"), re.M)
        if len(keys) < 27 or "addOrReinstall" not in keys:
            bad.append(f"{lang.relative_to(ROOT)}: {len(keys)} LangStrings; copy Tauri's full English table")
    hook = TAURI / "windows" / "hooks.nsh"
    if hook.exists():
        h = hook.read_text(encoding="utf-8")
        if "ExecWait" in h and not (0 <= h.find("CheckIfAppIsRunning") < h.find("ExecWait")):
            bad.append(f"{hook.relative_to(ROOT)}: close the app (CheckIfAppIsRunning) before --uninstall-cleanup")
    if bad:
        failures.append("release build")
        print("--- release build: FAIL\n" + "\n".join(bad))


def check_link_parity():
    # TSD §4.1: every row of src/lib/link.test.ts TABLE is in links.rs parse_table with the same state,
    # so the mock parser and the real one agree. `${A}` <-> `{A}`; any other `${...}` <-> a `{}` arg.
    ts_p, rs_p = ROOT / "src/lib/link.test.ts", TAURI / "src/links.rs"
    if not (ts_p.exists() and rs_p.exists()):
        return
    table = re.search(r"const TABLE.*?= \[(.*?)\n\];", ts_p.read_text(encoding="utf-8"), re.S).group(1)
    norm = lambda s: re.sub(r"\$\{[^}]*\}", "{}", s.replace("${A}", "{A}"))
    ts = [(norm(raw), state + (":" + kind if kind else "")) for _, raw, state, kind in
          re.findall(r'\[([`"])(.*?)\1,\s*\{\s*state:\s*"(\w+)"(?:,\s*type:\s*"(\w+)")?', table)]
    body = re.search(r"fn parse_table\(\).*?\n    \}\n", rs_p.read_text(encoding="utf-8"), re.S).group(0)
    rows = re.findall(r'\(\s*(?:format!\(\s*)?"((?:[^"\\]|\\.)*)".*?,\s*(?:ok\("(\w+)"\)|st\((\w+)\))\s*,?\s*\)',
                      body, re.S)
    rs = {(raw, f"ok:{kind}" if kind else state.lower()) for raw, kind, state in rows}
    bad = [f"link.test.ts TABLE row {raw!r} -> {want}: no matching row in links.rs parse_table"
           for raw, want in ts if (raw, want) not in rs]
    if not ts:
        bad.append("no rows parsed from link.test.ts TABLE (format changed?)")
    if bad:
        failures.append("link parity")
        print("--- link parity: FAIL\n" + "\n".join(bad))


def check_no_committed_client_id():
    # M5 plan step 0: the Spotify client ID is never in the repo. Scan every tracked or new
    # (not ignored) text file for a 32-hex value or an assigned RUNOUT_SPOTIFY_CLIENT_ID.
    r = subprocess.run("git ls-files --cached --others --exclude-standard", cwd=ROOT, shell=True,
                       capture_output=True, text=True, encoding="utf-8", errors="replace")
    pat = re.compile(r"\b[0-9a-f]{32}\b|RUNOUT_SPOTIFY_CLIENT_ID\s*[=:]\s*[\"']?[0-9A-Za-z]{32}")
    skip = {".lock", ".png", ".jpg", ".jpeg", ".gif", ".ico", ".icns", ".woff", ".woff2", ".zip", ".docx", ".pdf"}
    bad = []
    for rel in r.stdout.splitlines():
        p = ROOT / rel
        if p.suffix.lower() in skip or p.name == "package-lock.json" or not p.is_file():
            continue
        try:
            text = p.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        bad += [f"{rel}:{i}" for i, line in enumerate(text.splitlines(), 1) if pat.search(line)]
    if bad:
        failures.append("no client id in repo")
        print("--- no client id in repo: FAIL (32-hex value or assigned client ID)\n" + "\n".join(bad))


def check_deferrals_tracked():
    # Nothing deferred may be lost: an unticked acceptance line marked "(open" must name a
    # G-n row in Known gaps or a U-n hard stop, and every Known gaps row must say where it's fixed.
    text = (ROOT / "docs" / "MILESTONES.md").read_text(encoding="utf-8")
    gaps = re.search(r"### Known gaps\n(.*?)\n---", text, re.S)
    rows = {m.group(1): m.group(2) for m in re.finditer(r"^\| (G-\d+) \|.*\| ([^|]*) \|\s*$", gaps.group(1) if gaps else "", re.M)}
    bad = [f"Known gaps {g}: empty 'Fix in'" for g, fix in rows.items() if not fix.strip()]
    for i, line in enumerate(text.splitlines(), 1):
        if line.startswith("- [ ]") and "(open" in line:
            refs = re.findall(r"\b([GU]-\d+)\b", line)
            if not any(r.startswith("U-") or r in rows for r in refs):
                bad.append(f"docs/MILESTONES.md:{i}: open acceptance line names no Known gaps G-n or U-n")
    if bad:
        failures.append("deferrals tracked")
        print("--- deferrals tracked: FAIL\n" + "\n".join(bad))


def check_source_map():
    # docs/SOURCE_MAP.md: every source file is mapped; every `built` row exists.
    rows = [line for line in (ROOT / "docs/SOURCE_MAP.md").read_text(encoding="utf-8").splitlines()
            if re.match(r"\| `(src|src-tauri)/", line)]
    mapped = [re.match(r"\| `([^`]+)`", r).group(1).rstrip("/") for r in rows]
    bad = [f"built but missing: {p}" for p, r in zip(mapped, rows)
           if r.rstrip(" |").endswith("built") and not (ROOT / p).exists()]
    bad += [f"duplicate row: {p}" for p in sorted({p for p in mapped if mapped.count(p) > 1})]
    # every example named in the examples row exists, unless its note says "planned"/"not in repo"
    for p, r in zip(mapped, rows):
        if p == "src-tauri/examples":
            for m in re.finditer(r"`([\w.-]+\.rs)`([^`]*)", r):
                if not (ROOT / p / m.group(1)).exists() and not re.search(r"planned|not in repo", m.group(2)):
                    bad.append(f"named in the {p}/ row but missing: {m.group(1)}")
    for d in (TAURI / "src", TAURI / "migrations", ROOT / "src"):
        if d.exists():
            for f in (f for f in d.rglob("*") if f.is_file()):
                rel = f.relative_to(ROOT).as_posix()
                if not any(rel == m or rel.startswith(m + "/") for m in mapped):
                    bad.append(f"unmapped: {rel}")
    if bad:
        failures.append("source map")
        print("--- source map: FAIL\n" + "\n".join(bad))


def check_design():
    # docs/DESIGN.md + taste rules: no em dashes in UI copy, at most one middle dot per line,
    # every prototype route named in DESIGN.md §6 exists in screens.js.
    proto = ROOT / "docs/design/prototype"
    if not proto.exists():
        return
    bad = []
    for p in list(proto.glob("*.js")) + [proto / "mock-data.json"]:
        for i, line in enumerate(p.read_text(encoding="utf-8").splitlines(), 1):
            if "—" in line:
                bad.append(f"{p.relative_to(ROOT)}:{i}: em dash")
            for s in re.findall(r">([^<>]*)<", line):  # rendered text between tags
                if s.count("·") > 1:
                    bad.append(f"{p.relative_to(ROOT)}:{i}: more than one middle dot")
    screens = (proto / "screens.js").read_text(encoding="utf-8")
    routes = set(re.findall(r"`#/([a-z-]+)", (ROOT / "docs/DESIGN.md").read_text(encoding="utf-8")))
    bad += [f"route #/{r} in DESIGN.md has no screen" for r in routes if f'"{r}"' not in screens and f"{r}:" not in screens]
    js = "\n".join(p.read_text(encoding="utf-8") for p in proto.glob("*.js") if p.name != "mock-data.js")
    # counts go through R.pl / pl(), never "${n} plays"
    bad += [f"unpluralised count: {m}" for m in re.findall(r"\$\{[^}]+\} (?:records?|plays?|days?|dupes?|tracks?|results?)\b", js)]
    bad += ["literal 'a ${kind}', use R.an()" for _ in re.findall(r"\ba \$\{kind\}", js)]
    for m in re.finditer(r"\.btn[\w.-]*\s*\{[^}]*height:\s*(\d+)px", (proto / "styles.css").read_text(encoding="utf-8")):
        if int(m.group(1)) < 32:
            bad.append(f"button height {m.group(1)}px < 32px (DESIGN §7)")
    for link in re.findall(r"open\.spotify\.com/track/([A-Za-z0-9]{22})", js):
        bad.append(f"hard-coded sample track link {link}: pass the play's own URI")
    data = json.loads((proto / "mock-data.json").read_text(encoding="utf-8"))
    lst = data["listening"]
    bad += [f"private play not from export: {e['ended_at']}" for e in lst["recent"] if e["private"] and e["source"] != "export"]
    if lst["import_preview"]["private"] != lst["private_excluded"]:
        bad.append("import private count != insights private count")
    hist = {h["item_id"] for h in data["history"]}
    bad += [f"no history for {i['display_name']}" for i in data["items"] if i["id"] not in hist]
    st = lst["item_stats"]
    by = {(i["type"], i["display_name"], i.get("artist")): i for i in data["items"]}
    for i in data["items"]:
        al = by.get(("album", i.get("album"), i.get("artist"))) if i["type"] == "track" else None
        if al and st.get(str(i["id"]), {}).get("plays", 0) > st.get(str(al["id"]), {}).get("plays", 0):
            bad.append(f"track {i['display_name']} has more plays than its album")
    for kind in ("artists", "albums", "tracks"):
        for a, b in (("7d", "30d"), ("30d", "12m"), ("12m", "all")):
            big = {(r["name"], r["by"]): r["plays"] for r in lst["charts"][b][kind]}
            bad += [f"{kind} {r['name']}: {a} plays > {b}" for r in lst["charts"][a][kind] if (r["name"], r["by"]) in big and r["plays"] > big[(r["name"], r["by"])]]
        for rng, ch in lst["charts"].items():
            rows = ch[kind]
            bad += [f"{rng} {kind}: {x['name']} above {y['name']} with fewer hours" for x, y in zip(rows, rows[1:]) if x["hours"] < y["hours"]]
    # identical screenshots mean a capture showed the wrong thing (e.g. a dialog captured before it faded in)
    seen = {}
    for p in sorted((ROOT / "docs/design/screens").rglob("*.png")):
        seen.setdefault(hashlib.sha256(p.read_bytes()).hexdigest(), []).append(p.name)
    bad += [f"identical screenshots: {', '.join(v)}" for v in seen.values() if len(v) > 1]
    if bad:
        failures.append("design")
        print("--- design: FAIL\n" + "\n".join(bad))


if (TAURI / "Cargo.toml").exists():
    run("cargo fmt", "cargo fmt --check", TAURI)
    run("cargo clippy", "cargo clippy --all-targets -- -D warnings", TAURI)
    run("cargo test", "cargo test", TAURI)
if (ROOT / "package.json").exists():
    run("tsc", "npx tsc --noEmit", ROOT)
    run("vitest", "npx vitest run --passWithNoTests", ROOT)
check_hosts()
check_no_oembed_or_tokens_logged()
check_one_item_summary()
check_events()
check_settings_paths()
check_restore_marker_guards()
check_insights_no_cache()
check_spotify_rules()
check_boundary_values_kept()
check_cache_schema_in_tsd()
check_mock_conflicts()
check_mock_r3_parity()
check_dropped_cache_columns()
check_no_snapshot_id_compare_in_docs()
check_example_usage()
check_no_committed_client_id()
check_deferrals_tracked()
check_listening_event_reads()
check_settings_clone_then_assign()
check_ui_purity()
check_contract_and_attribution()
check_commands()
check_release_build()
check_link_parity()
check_source_map()
check_design()

if HOOK and not failures:
    sys.exit(0)
print("\nFAILED: " + ", ".join(failures) if failures else "\nALL CHECKS PASS")
sys.exit((2 if HOOK else 1) if failures else 0)

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
    r = subprocess.run(cmd, cwd=cwd, shell=True, capture_output=True, text=True)
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


def check_no_oembed_or_tokens_logged():
    # TSD §7: tokens never logged.
    pat = re.compile(r"(println!|log::\w+!|tracing::\w+!|console\.log)\(.*(token|secret|refresh)", re.I)
    bad = [f"{p.relative_to(ROOT)}:{i}" for p in source_files()
           for i, line in enumerate(p.read_text(encoding="utf-8").splitlines(), 1) if pat.search(line)]
    if bad:
        failures.append("no secrets in logs")
        print("--- no secrets in logs: FAIL\n" + "\n".join(bad))


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
            if "]${" in line:  # Tailwind can't see an arbitrary-value class glued to ${...}; the rule is never generated
                bad.append(f"{rel}:{i}: put a space between ']' and '${{' in class strings")
            if "@tauri-apps/api" in line and rel != "src/api.ts":
                bad.append(f"{rel}:{i}: only src/api.ts may import @tauri-apps/api")
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


def check_source_map():
    # docs/SOURCE_MAP.md: every source file is mapped; every `built` row exists.
    rows = [line for line in (ROOT / "docs/SOURCE_MAP.md").read_text(encoding="utf-8").splitlines()
            if re.match(r"\| `(src|src-tauri)/", line)]
    mapped = [re.match(r"\| `([^`]+)`", r).group(1).rstrip("/") for r in rows]
    bad = [f"built but missing: {p}" for p, r in zip(mapped, rows)
           if r.rstrip(" |").endswith("built") and not (ROOT / p).exists()]
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
check_ui_purity()
check_contract_and_attribution()
check_source_map()
check_design()

if HOOK and not failures:
    sys.exit(0)
print("\nFAILED: " + ", ".join(failures) if failures else "\nALL CHECKS PASS")
sys.exit((2 if HOOK else 1) if failures else 0)

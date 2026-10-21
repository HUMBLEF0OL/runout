// M6/M7 evidence: journeys A–H, KEYBOARD ONLY, on the INSTALLED app with its real library (not the mocks).
// 1. Launch: node -e "import('./scripts/installed/app.mjs').then((m) => m.launch())"  (WebView2 CDP on :9333)
//    Offline run: launch({ offline: true }), then RUNOUT_OFFLINE=1 node scripts/installed/journeys.mjs (journey O only).
// 2. node scripts/installed/journeys.mjs, with env (every value kept OUT of the repo):
//    RUNOUT_IDS            path to json {album, track, artist: [22-char Spotify IDs]}; needed by A and D (C, E reuse A)
//    RUNOUT_EXPORT         path to the Extended Streaming History .zip or folder; journey F runs only when set
//    RUNOUT_TEST_PLAYLIST  name of a throwaway playlist you own (>= 4 tracks, >= 1 duplicate); journey G runs only when
//                          set and Spotify is connected. G writes to that playlist (apply, then undo), saves then removes
//                          one album in your Spotify library (FR-042) and adds top/recent items to the backlog (FR-041).
//    RUNOUT_INBOX=1        journey B on the Runout Inbox (opt-in, see the warning below)
//    RUNOUT_CAPTURE_POST=1 journey B posts to the Wi-Fi capture listener (the old Shortcut path) instead of the Inbox
//    RUNOUT_JOURNEYS       letters to run (default ACDEH, plus B/F/G when their env is set); a letter named here whose
//                          steps all skip counts as a failure
// Every Spotify write (Studio apply/undo, library save/remove, Inbox check/create) sits in a block gated by
// RUNOUT_TEST_PLAYLIST or RUNOUT_INBOX (check.py "installed journeys gate Spotify writes").
// Needs network (prefill, embeds). Writes to the live library: use a test library, never the user's own.
// B (RUNOUT_INBOX=1): Settings → Runout Inbox → Check now. WARNING: Check now imports EVERYTHING in the signed-in
//   account's real "Runout Inbox" playlist into the library under test and removes it from the Inbox. No command reads
//   the Inbox's contents first (get_inbox is status only), so before running: open the Inbox on the phone and make sure
//   it holds only the throwaway track(s) you added for this test. Skipped when Spotify isn't connected.
// Screenshots go to %TEMP%/runout-installed-journeys (real catalog content: never committed).
import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const SHOTS = join(tmpdir(), "runout-installed-journeys");
mkdirSync(SHOTS, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/* ------------------------------------------------------------------ installed app (CDP via WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS) */
const targets = await (await fetch("http://127.0.0.1:9333/json")).json();
const ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let seq = 0; const pending = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
const send = (method, params = {}) => new Promise((res, rej) => { pending.set(++seq, (d) => (d.error ? rej(new Error(`${method}: ${d.error.message}`)) : res(d.result))); ws.send(JSON.stringify({ id: seq, method, params })); });
const done = (code) => { try { ws.close(); } catch { /* gone */ } setTimeout(() => process.exit(code), 300); };

/* ------------------------------------------------------------------ page reads (assertions only) */
// Accessible name, close enough for buttons/links/inputs: aria-label, aria-labelledby, <label>, text, title.
const PRE = `const nm = (el) => { if (!el) return ""; const l = el.getAttribute("aria-label"); if (l) return l.trim();
  const lb = el.getAttribute("aria-labelledby"); if (lb) return lb.split(" ").map((i) => document.getElementById(i)?.textContent ?? "").join(" ").trim();
  if (el.labels?.length) return el.labels[0].textContent.trim(); return (el.textContent || el.title || el.placeholder || "").trim(); };
  const $$ = (s) => [...document.querySelectorAll(s)]; const txt = () => document.body.innerText;`;
async function js(expr) {
  const r = await send("Runtime.evaluate", { expression: `(() => { ${PRE} return (${expr}); })()`, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) throw new Error(`eval: ${r.exceptionDetails.exception?.description ?? r.exceptionDetails.text}`);
  return r.result.value;
}
async function waitFor(expr, ms = 4000) {
  const end = Date.now() + ms; let v;
  while (Date.now() < end) { try { v = await js(expr); if (v) return v; } catch { /* page mid-render */ } await sleep(80); }
  throw new Error(`timed out waiting for: ${expr}`);
}

/* ------------------------------------------------------------------ keyboard (the only way user actions happen) */
const KEYS = {
  Tab: [9, "Tab"], Enter: [13, "Enter", "\r"], " ": [32, "Space", " "], Escape: [27, "Escape"], Home: [36, "Home"], End: [35, "End"],
  ArrowUp: [38, "ArrowUp"], ArrowDown: [40, "ArrowDown"], ArrowLeft: [37, "ArrowLeft"], ArrowRight: [39, "ArrowRight"],
  ...Object.fromEntries("12345".split("").map((d) => [d, [48 + Number(d), `Digit${d}`, d]])),
  n: [78, "KeyN"], k: [75, "KeyK"], a: [65, "KeyA"],
};
const MOD = { Alt: 1, Ctrl: 2, Shift: 8 };
async function key(combo) {
  const parts = combo.split("+"); const k = parts.pop(); const modifiers = parts.reduce((m, p) => m | MOD[p], 0);
  const [vk, code, text] = KEYS[k];
  const base = { key: k, code, windowsVirtualKeyCode: vk, modifiers };
  await send("Input.dispatchKeyEvent", { ...base, type: text && !modifiers ? "keyDown" : "rawKeyDown", ...(text && !modifiers ? { text, unmodifiedText: text } : {}) });
  await send("Input.dispatchKeyEvent", { ...base, type: "keyUp" });
  await sleep(60);
}
const type = async (text) => { await send("Input.insertText", { text }); await sleep(120); };
const press = async (k, n = 1) => { for (let i = 0; i < n; i++) await key(k); };

const focusRings = []; // [step, element, outline-style]
/** Tab (or Shift+Tab) until document.activeElement satisfies `pred` (JS over `el`). Fails if unreachable within `max`. */
async function tabTo(pred, { back = false, max = 900, what = pred } = {}) {
  const test = `(() => { const el = document.activeElement; return !!el && el !== document.body && (${pred}); })()`;
  for (let i = 0; i <= max; i++) {
    if (await js(test)) {
      const ring = await js(`(() => { const el = document.activeElement; const cs = getComputedStyle(el);
        return { name: nm(el).slice(0, 60), tag: el.tagName, style: cs.outlineStyle, width: cs.outlineWidth, fv: el.matches(":focus-visible") }; })()`);
      focusRings.push({ step: current, what, ...ring });
      if (ring.style === "none" || ring.width === "0px") throw new Error(`no visible focus ring on ${ring.tag} "${ring.name}"`);
      return ring;
    }
    await key(back ? "Shift+Tab" : "Tab");
  }
  throw new Error(`NOT REACHABLE BY KEYBOARD: ${what}`);
}
const byName = (s) => `nm(el) === ${JSON.stringify(s)}`;
const nameStarts = (s) => `nm(el).startsWith(${JSON.stringify(s)})`;
/** Tab to a control and activate it with Enter. */
const activate = async (pred, opts) => { await tabTo(pred, opts); await key("Enter"); await sleep(250); };
/** Move via a rail link (keyboard) to a route. */
async function go(label, hash) {
  await activate(`el.matches(".rail a") && ${byName(label)}`, { what: `rail link ${label}` });
  await waitFor(`location.hash.startsWith(${JSON.stringify(hash)})`);
  await sleep(500);
}
const toast = (re) => waitFor(`(() => { const t = document.querySelector(".toast"); return t && ${re}.test(t.textContent) && t.textContent; })()`, 4000);

/* ------------------------------------------------------------------ evidence */
async function shot(name) {
  const r = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(SHOTS, `${name}.png`), Buffer.from(r.data, "base64"));
  shots.push(name);
}
const shots = [];
const unnamed = new Set();
/** Static scan: every rendered, non-hidden <button> has an accessible name. */
async function scanButtons() {
  const bad = await js(`$$("button").filter((b) => !b.closest("[aria-hidden=true]") && b.offsetParent !== null && !nm(b)).map((b) => location.hash + " " + b.outerHTML.slice(0, 100))`);
  bad.forEach((b) => unnamed.add(b));
}
const results = []; let current = "";
/** Thrown by a step whose precondition (env, Spotify connection, data) is missing: reported, not a failure. */
class Skip extends Error {}
async function step(name, fn) {
  current = name;
  try { await fn(); results.push([name, true]); console.log(`PASS ${name}`); }
  catch (e) {
    if (e instanceof Skip) { results.push([name, true, "skip"]); console.log(`SKIP ${name} — ${e.message}`); }
    else { results.push([name, false, e.message]); console.log(`FAIL ${name} — ${e.message}`); }
  }
  try { await scanButtons(); } catch { /* page mid-render */ }
}

/* dev panel (simulates things outside the app: release/offline state, the iPhone share) — reached by keyboard too */
async function devPanel(open) {
  if (open === await js(`document.querySelector("details.proto").open`)) return;
  await tabTo(`el.matches("details.proto summary")`, { back: true, what: "dev panel" }); await key("Enter");
  await waitFor(`document.querySelector("details.proto").open === ${open}`);
}
async function devState(view) {
  const order = ["normal", "empty", "loading", "offline", "error"];
  await devPanel(true);
  await tabTo(`el.matches("details.proto select") && el.closest("label").textContent.startsWith("State")`, { what: "dev State select" });
  const now = await js(`document.activeElement.value`);
  const d = order.indexOf(view) - order.indexOf(now);
  await press(d > 0 ? "ArrowDown" : "ArrowUp", Math.abs(d));
  await waitFor(`document.activeElement.value === ${JSON.stringify(view)}`);
  await sleep(400);
  await devPanel(false);
}

/* ------------------------------------------------------------------ run (installed app, real library) */
import { readFileSync } from "node:fs";
import http from "node:http";
const OFFLINE = !!process.env.RUNOUT_OFFLINE;
const EXPORT = process.env.RUNOUT_EXPORT;
const PLAYLIST = process.env.RUNOUT_TEST_PLAYLIST;
const INBOX = !!process.env.RUNOUT_INBOX;
const POST = !!process.env.RUNOUT_CAPTURE_POST;
const letters = (v) => v.toUpperCase().replace(/[^A-Z]/g, ""); // "A, B" / "a b" / "AB" all mean A and B
const NAMED = new Set(letters(process.env.RUNOUT_JOURNEYS ?? "")); // a named journey must not skip entirely
const J = new Set(OFFLINE ? ["O"] : letters(process.env.RUNOUT_JOURNEYS ?? `AC${INBOX || POST ? "B" : ""}DEH${EXPORT ? "F" : ""}${PLAYLIST ? "G" : ""}`));
const IDS = process.env.RUNOUT_IDS ? JSON.parse(readFileSync(process.env.RUNOUT_IDS)) : { album: [], track: [], artist: [] };
if (["A", "D"].some((x) => J.has(x)) && !process.env.RUNOUT_IDS) { console.log("FAIL journeys A and D need RUNOUT_IDS"); done(1); }
const invoke = (cmd, args = {}) => js(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
// First catalog IDs not yet in the library, so re-runs never hit "Already saved".
const have = new Set((await invoke("list_items", { filter: { limit: 1000 } })).rows.map((r) => r.spotify_uri));
const fresh = (k, skip = 0) => IDS[k].filter((i) => !have.has(`spotify:${k}:${i}`))[skip];
const ALBUM = `https://open.spotify.com/album/${fresh("album")}?si=journey`;
const ARTIST = `https://open.spotify.com/artist/${fresh("artist")}`;
const SHARE = `https://open.spotify.com/track/${fresh("track")}?si=phone`;
const CONNECTED = !OFFLINE && (await invoke("get_spotify_status").catch(() => ({ state: "disconnected" }))).state === "connected";
const MON = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const dayD = (iso) => { const d = new Date(iso); return `${d.getDate()} ${MON[d.getMonth()]}`; }; // lib/format day()
const dayY = (iso) => `${dayD(iso)} ${new Date(iso).getFullYear()}`; // lib/format dayY()
/** Focus a Seg's selected tab/radio, then arrow to the option named `label` (roving tabindex: one stop per group). */
async function segTo(group, label) {
  const inG = `el.closest(${JSON.stringify(`[aria-label="${group}"]`)})`;
  await tabTo(`${inG} && (el.getAttribute("aria-selected") === "true" || el.getAttribute("aria-checked") === "true")`, { what: `${group} control` });
  for (let i = 0; i < 6 && !(await js(`nm(document.activeElement) === ${JSON.stringify(label)}`)); i++) await key("ArrowRight");
  await waitFor(`nm(document.activeElement) === ${JSON.stringify(label)}`);
  await sleep(400);
}
await send("Page.bringToFront");
await send("Emulation.setFocusEmulationEnabled", { enabled: true }); // the real window may not be the OS foreground
await js(`(location.hash = "#/home", 1)`);
await waitFor(`!!document.querySelector(".rail")`, 15000);
await sleep(800);
const qaOpen = (title) => waitFor(`(() => { const d = document.querySelector("dialog[open]"); return d && d.querySelector("#dlg-t").textContent === ${JSON.stringify(title)}; })()`);
const closeDialog = async () => { await key("Escape"); await waitFor(`!document.querySelector("dialog[open]")`); };
let NAME = "", ARTIST_NAME = "";

if (J.has("A")) {
/* A — save music for later */
await step("A1 Ctrl+N opens quick add, focus in link field", async () => {
  await key("Ctrl+n"); await qaOpen("Add to backlog");
  await waitFor(`document.activeElement.id === "qa-link"`);
});
await step("A2 paste real link → type detected, name prefilled (oEmbed), embed + attribution", async () => {
  await type(ALBUM);
  await waitFor(`txt().includes("Detected an album. Not in your library yet.")`, 8000);
  NAME = await waitFor(`document.querySelector("#qa-name")?.value`, 10000);
  await waitFor(`!!document.querySelector('dialog iframe[src^="https://open.spotify.com/embed/album/"]')`, 8000);
  await waitFor(`txt().includes("Suggested by Spotify") && $$("dialog a").some((a) => a.textContent === "Open Spotify")`);
  await shot("A1-quick-add-link");
});
await step("A3 Save → toast, dialog closes", async () => {
  await activate(`el.type === "submit" && ${byName("Save")}`, { what: "Save" });
  await toast(new RegExp(`saved to your backlog`));
  await waitFor(`!document.querySelector("dialog[open]")`);
});
await step("A4 appears in the backlog as Planned", async () => {
  await go("Backlog", "#/backlog");
  await waitFor(`$$(".row a").some((a) => a.textContent.includes(${JSON.stringify(NAME)}))`);
  await waitFor(`$$("button").some((b) => nm(b) === ${JSON.stringify("Start " + NAME)})`);
  await shot("A2-saved-backlog");
});
await step("A5 item page shows the embed", async () => {
  await activate(`el.tagName === "A" && el.textContent.includes(${JSON.stringify(NAME)})`, { what: "backlog row link" });
  await waitFor(`document.querySelector("h1")?.textContent === ${JSON.stringify(NAME)}`);
  await waitFor(`!!document.querySelector('iframe[title="Spotify embed"][src*="/embed/album/"]')`, 8000);
  await waitFor(`document.querySelector('[role=radio][aria-checked=true]')?.textContent === "Planned"`);
  await shot("A3-item-embed");
});
await step("A6 duplicate link → Already saved + Open existing", async () => {
  await key("Ctrl+n"); await qaOpen("Add to backlog"); await type(ALBUM);
  await waitFor(`txt().includes("Already saved")`);
  await activate(`${byName("Open existing")}`, { what: "Open existing" });
  await waitFor(`document.querySelector("h1")?.textContent === ${JSON.stringify(NAME)}`);
});
await step("A7 unsupported (podcast episode) link", async () => {
  await key("Ctrl+n"); await qaOpen("Add to backlog"); await type("https://open.spotify.com/episode/512ojhOuo1ktJprKbVcKyQ");
  await waitFor(`txt().includes("This is a podcast episode. Runout keeps albums, tracks and artists.")`);
  await closeDialog();
});
}

/* B — capture from phone. Default: the Runout Inbox (D33, TSD §5.3): the phone adds to the Inbox playlist, Check now
   imports it. RUNOUT_CAPTURE_POST=1: a real LAN POST to the Wi-Fi capture listener (what the Shortcut sends, TSD §5.1). */
let needBefore = 0; let bImported = 0;
if (J.has("B") && INBOX && !POST) await step("B1 Settings → Runout Inbox → Check now → the phone's adds are Name needed in the backlog", async () => {
  if (!CONNECTED) throw new Skip("Spotify not connected");
  console.log("     B1 imports everything in the real Runout Inbox (see the header warning)");
  await go("Backlog", "#/backlog");
  needBefore = await js(`$$(".badge.need").length`);
  await go("Settings", "#/settings");
  await waitFor(`txt().includes("Runout Inbox · ready")`, 8000);
  await activate(byName("Check now"), { what: "Inbox Check now" });
  const t = await waitFor(`(() => { const t = document.querySelector(".toast")?.textContent ?? ""; return /from your Inbox saved|Nothing new in your Inbox/.test(t) && t; })()`, 60000);
  bImported = Number((/^(\d+) items? from your Inbox/.exec(t) ?? [0, 0])[1]);
  await waitFor(`/Last check.*· \\d+ items? imported/.test(txt().replace(/\\n/g, " "))`);
  await shot("B0-inbox-checked");
  if (!bImported) throw new Skip("Inbox was empty: add a track to Runout Inbox on the phone, then run B again");
  await go("Backlog", "#/backlog");
  await waitFor(`$$(".badge.need").length === ${needBefore + bImported}`, 8000);
  await shot("B1-share-name-needed");
});
if (J.has("B") && !INBOX && !POST) await step("B1 Runout Inbox", async () => { throw new Skip("set RUNOUT_INBOX=1 (or RUNOUT_CAPTURE_POST=1)"); });
if (J.has("B") && POST) await step("B1 phone share (LAN POST /capture) → backlog shows it as Name needed", async () => {
  await invoke("set_capture_enabled", { on: true }); // POST mode only
  const cap = await invoke("get_capture");
  if (!cap.on || !cap.address) throw new Error(`capture not on: ${JSON.stringify(cap)}`);
  await go("Backlog", "#/backlog");
  needBefore = await js(`$$(".badge.need").length`);
  const body = JSON.stringify({ code: cap.code, link: SHARE });
  const status = await new Promise((res, rej) => {
    const r = http.request({ host: cap.address, port: cap.port, path: "/capture", method: "POST", localAddress: cap.address,
      headers: { "Content-Type": "application/json", "Content-Length": Buffer.byteLength(body) } }, (x) => { x.resume(); res(x.statusCode); });
    r.on("error", rej); r.end(body);
  });
  if (status >= 300) throw new Error(`POST /capture → ${status}`);
  await waitFor(`$$(".badge.need").length === ${needBefore + 1}`, 8000);
  bImported = 1;
  await shot("B1-share-name-needed");
});
if (J.has("B")) await step("B2 name the newest by keyboard → one badge fewer", async () => {
  if (!bImported) throw new Skip("nothing arrived in B1");
  await activate(`el === $$('button[aria-label^="Name the link shared"]').at(-1)`, { what: "Name it (newest share)" });
  await qaOpen("Name a phone capture");
  await tabTo(`el.id === "qa-name"`, { what: "Name field" });
  const pre = await js(`document.activeElement.value`);
  if (!pre) await type("Shared from phone");
  const named = pre || "Shared from phone";
  await activate(`el.type === "submit" && ${byName("Save")}`, { what: "Save" });
  await toast(/ named/);
  await waitFor(`$$(".badge.need").length === ${needBefore + bImported - 1}`);
  await waitFor(`$$(".row a").some((a) => a.textContent.includes(${JSON.stringify(named)}))`);
  await shot("B3-named-in-backlog");
});

/* C — explore and document an album (the one saved in A) */
if (J.has("C")) {
await step("C1 Ctrl+K search finds the album; key 2 sets In progress; Open Spotify reachable", async () => {
  await key("Ctrl+k"); await waitFor(`document.activeElement.id === "top-field"`);
  await type(NAME); await key("Enter");
  await waitFor(`location.hash.startsWith("#/search/")`);
  await activate(`el.tagName === "A" && el.textContent.includes(${JSON.stringify(NAME)})`, { what: "search result link" });
  await waitFor(`document.querySelector("h1")?.textContent === ${JSON.stringify(NAME)}`);
  await tabTo(`${byName("Edit " + NAME)}`, { what: "Edit link (any non-input focus)" });
  await key("2");
  await toast(/is now In progress/);
  await waitFor(`document.querySelector('[role=radio][aria-checked=true]')?.textContent === "In progress"`);
  await tabTo(`${byName("Open " + NAME + " in Spotify")} && el.href.startsWith("https://open.spotify.com/album/")`, { back: true, what: "Open Spotify" });
  await shot("C1-in-progress");
});
await step("C2 rating by arrow keys (4.5 stars)", async () => {
  await tabTo(`el.getAttribute("role") === "slider"`, { what: "rating slider" });
  for (let i = 1; i <= 9; i++) { await key("ArrowRight"); await waitFor(`document.activeElement.getAttribute("aria-valuenow") === "${i / 2}"`); }
  await toast(/Rated .* 4\.5/);
});
await step("C3 review and tag by keyboard", async () => {
  await tabTo(`el.id === "dt-review"`, { what: "Review" });
  await type("Real-data journey: review saved by keyboard.");
  await key("Tab"); await toast(/Review saved/);
  await activate(`${byName("Add tag")}`, { back: true, what: "Add tag" });
  await waitFor(`document.activeElement.getAttribute("aria-label") === "New tag"`);
  await type("m6-journey"); await key("Enter");
  await waitFor(`$$("button").some((b) => nm(b) === "Remove tag m6-journey")`);
});
await step("C4 key 3 sets Explored; Revisit toggle by Space", async () => {
  await tabTo(byName("Edit " + NAME), { back: true, what: "Edit link" });
  await key("3");
  await toast(/is now Explored/);
  await tabTo(`el.type === "checkbox" && ${byName("Revisit later")}`, { what: "Revisit later" });
  await key(" ");
  await toast(/flagged to revisit/);
  await waitFor(`document.activeElement.checked === true`);
  await shot("C2-explored-rated-reviewed");
});
}

/* D — explore an artist */
if (J.has("D")) {
await step("D1 quick add artist with why-interesting notes", async () => {
  await key("Ctrl+n"); await qaOpen("Add to backlog");
  await type(ARTIST);
  await waitFor(`txt().includes("Detected an artist.")`, 8000);
  ARTIST_NAME = await waitFor(`document.querySelector("#qa-name")?.value`, 10000);
  await tabTo(`el.id === "qa-notes"`, { what: "Why this artist" });
  await type("Real-data journey: why this artist.");
  await activate(`el.type === "submit" && ${byName("Save")}`, { what: "Save" });
  await toast(/saved to your backlog/);
});
await step("D2 add an album of theirs from the artist page", async () => {
  await go("Backlog", "#/backlog");
  const aid = Math.max(...(await invoke("list_items", { filter: { limit: 1000 } })).rows.filter((r) => r.type === "artist" && r.display_name === ARTIST_NAME).map((r) => r.id));
  await activate(`el.tagName === "A" && el.getAttribute("href") === "#/item/${aid}"`, { what: "artist row link (the one just added)" });
  await waitFor(`document.querySelector("h1")?.textContent === ${JSON.stringify(ARTIST_NAME)} && document.querySelector("#dt-notes").value.includes("why this artist")`);
  await activate(byName("Add an album or track by " + ARTIST_NAME), { what: "Add an album by artist" });
  await qaOpen("Add without a link");
  await waitFor(`document.activeElement.id === "qa-name"`);
  await type("M6 Journey Album");
  await activate(`el.type === "submit" && ${byName("Save")}`, { what: "Save" });
  await toast(/M6 Journey Album saved to your backlog/);
  await waitFor(`$$("section a").some((a) => a.textContent.startsWith("M6 Journey Album"))`);
});
await step("D3 update artist status and notes", async () => {
  await tabTo(byName("Edit " + ARTIST_NAME), { what: "Edit link" });
  await key("2"); await toast(/is now In progress/);
  await tabTo(`el.id === "dt-notes"`, { what: "Why this artist" });
  await key("End"); await type(" More notes.");
  await key("Tab"); await toast(/Notes saved/);
  await shot("D2-artist-page");
});
}

/* E — rediscover */
if (J.has("E")) {
const shelfCount = `$$("main section[aria-label]").map((s) => s.getAttribute("aria-label") + ":" + s.querySelectorAll(".tiles > *").length).join("|")`;
await step("E1 filter by tag (only records with the tag remain)", async () => {
  await go("Rediscover", "#/rediscover");
  await waitFor(`$$("main section[aria-label]").length > 0`);
  await tabTo(`el.id === "rd-tag"`, { what: "Tag filter" });
  const tag = await js(`[...document.activeElement.options].find((o) => o.textContent.includes("m6-journey"))?.value`);
  const idx = await js(`[...document.activeElement.options].findIndex((o) => o.value === ${JSON.stringify(tag)})`);
  for (let i = 0; i < idx; i++) await key("ArrowDown");
  await waitFor(`document.activeElement.value === ${JSON.stringify(tag)}`);
  const tagged = new Set((await invoke("list_items", { filter: { tags: ["m6-journey"], limit: 1000 } })).rows.map((r) => r.display_name));
  await waitFor(`(() => { const t = $$("main section[aria-label] .tiles > a .font-bold").map((a) => a.textContent).filter(Boolean); const ok = ${JSON.stringify([...tagged])}; return t.includes(${JSON.stringify(NAME)}) && t.every((x) => ok.includes(x)); })()`);
  await shot("E1-filter-tag");
  for (let i = 0; i < idx; i++) await key("ArrowUp");
  await waitFor(`document.activeElement.value === ""`);
});
await step("E2 filter by rating and Revisit flag", async () => {
  await tabTo(`el.id === "rd-min"`, { what: "Rating filter" });
  await key("ArrowDown"); await waitFor(`document.activeElement.value === "4"`);
  await activate(`el.getAttribute("aria-pressed") !== null && nm(el).startsWith("Revisit only")`, { what: "Revisit only" });
  await waitFor(`document.activeElement.getAttribute("aria-pressed") === "true"`);
  await waitFor(`$$("main section[aria-label]").map((s) => s.getAttribute("aria-label")).includes("Flagged to revisit") && txt().includes(${JSON.stringify(NAME)})`);
  await shot("E2-filter-rating-revisit");
});
await step("E3 artwork set ends with Open Spotify; record opens with Open Spotify", async () => {
  await activate(byName("Show artwork"), { what: "Show artwork" });
  await waitFor(`$$("main a").some((a) => a.textContent === "Open Spotify") && $$("main img[alt=Spotify]").length > 0`);
  await waitFor(`!!document.querySelector('main iframe[src^="https://open.spotify.com/embed/"]')`, 8000);
  await shot("E3-artwork-open-spotify");
  await activate(`el.tagName === "A" && el.textContent === ${JSON.stringify(NAME)}`, { what: "record in shelf" });
  await waitFor(`document.querySelector("h1")?.textContent === ${JSON.stringify(NAME)}`);
  await tabTo(byName("Open " + NAME + " in Spotify"), { what: "Open Spotify" });
});
}

/* F — review listening history on the user's real export (M7 WI 1, journey F) */
if (J.has("F")) {
await step("F1 import: Settings → Import an export → choose → check → import", async () => {
  if (!EXPORT) throw new Skip("RUNOUT_EXPORT unset");
  await go("Settings", "#/settings");
  await activate(`el.tagName === "A" && ${byName("Import an export")}`, { what: "Import an export" });
  await waitFor(`location.hash === "#/import/1"`);
  // The OS file picker can't be driven over CDP: answer pick_import_source with RUNOUT_EXPORT. Every other command
  // still goes to Rust; the stub is removed right after.
  await js(`(() => { const i = window.__TAURI_INTERNALS__; i.__orig = i.__orig ?? i.invoke;
    i.invoke = (c, a, o) => (c === "pick_import_source" ? Promise.resolve(${JSON.stringify(EXPORT)}) : i.__orig(c, a, o)); return true; })()`);
  try {
    await activate(byName("Choose folder or .zip"), { what: "Choose folder or .zip" });
    await waitFor(`txt().includes("(selected)")`);
  } finally { await js(`(() => { const i = window.__TAURI_INTERNALS__; if (i.__orig) i.invoke = i.__orig; return true; })()`); }
  await activate(`el.tagName === "BUTTON" && ${byName("Next")}`, { what: "Next" });
  await waitFor(`location.hash === "#/import/2" && txt().includes("What we found")`, 180000); // a real zip takes a while
  await shot("F1-import-check");
  await activate(`/^Import [\\d,]+ plays?$/.test(nm(el))`, { what: "Import N plays" });
  await waitFor(`location.hash === "#/import/3" || txt().includes("Nothing new to import")`, 600000);
  await waitFor(`/Imported [\\d,]+ plays?\\./.test(txt()) || txt().includes("Nothing new to import")`, 10000);
  await shot("F2-import-done"); // a re-import of the same export: "Nothing new to import" (dedupe, M4)
});
await step("F2 Insights: all four tabs, every card shows its sources and real freshness dates", async () => {
  const f = await invoke("get_freshness");
  if (!f.export_latest_event && !f.logger_last_event) throw new Error(`get_freshness has no dates: ${JSON.stringify(f)}`);
  await go("Insights", "#/insights");
  await waitFor(`!!document.querySelector("main .freshness")`, 15000);
  if (f.export_latest_event) {
    const want = `up to your export of ${dayD(f.export_latest_event)}`;
    await waitFor(`document.querySelector("main .freshness").textContent.includes(${JSON.stringify(want)})`);
    await waitFor(`document.querySelector(".rail-box")?.textContent.includes(${JSON.stringify(want)})`); // rail freshness box
  }
  // Each listening source in a tag names its own date (InsightCard / SourcedTag, FR-032).
  const S = f.logger_last_event ? `(to ${dayY(f.logger_last_event)})` : "(none yet)";
  const E = f.export_latest_event ? `(to ${dayY(f.export_latest_event)})` : "(none yet)";
  const cardsOk = `(() => {
    const cards = $$("main section.card"); if (!cards.length) return false;
    const tag = (c) => [...c.querySelectorAll("span")].map((x) => x.textContent).find((t) => /^From: /.test(t));
    const tags = cards.map(tag);
    if (tags.some((t) => !t) || txt().includes("Sources unknown.")) return false;
    return tags.every((t) => (!t.includes("Desktop log") || t.includes("Desktop log " + ${JSON.stringify(S)}))
      && (!t.includes("Spotify export") || t.includes("Spotify export " + ${JSON.stringify(E)})));
  })()`;
  for (const [hash, label] of [["listening", "Listening"], ["collection", "Collection"], ["take", "Your take"], ["year", "Year in review"]]) {
    await segTo("Insight group", label);
    await waitFor(`location.hash === "#/insights/${hash}" || (${JSON.stringify(hash)} === "listening" && location.hash === "#/insights")`);
    if (hash === "year" && await waitFor(`txt().includes("No listening in") || !!document.querySelector("main section.card")`, 15000) && await js(`txt().includes("No listening in")`)) continue;
    await waitFor(cardsOk, 20000);
    await shot(`F3-insights-${hash}`);
  }
});
await step("F3 recent plays: save a play to the backlog, rate a play's record", async () => {
  await go("Recent plays", "#/plays");
  await waitFor(`$$("main .row").length > 0`, 15000);
  await shot("F4-recent-plays");
  const save = await js(`$$("main button").map(nm).find((n) => /^Save .+ to backlog$/.test(n)) ?? null`);
  if (save) {
    await activate(byName(save), { what: save });
    await qaOpen("Add to backlog");
    if (await js(`txt().includes("Already saved")`)) await closeDialog();
    else { await activate(`el.type === "submit" && ${byName("Save")}`, { what: "Save" }); await toast(/saved to your backlog/); }
  }
  await go("Recent plays", "#/plays");
  await waitFor(`$$("main .row").length > 0`, 15000);
  const rate = await js(`$$("main button").map(nm).find((n) => /^Rate .+/.test(n)) ?? null`);
  if (!rate && !save) throw new Skip("no play to save or rate in the recent days shown");
  if (rate) {
    await activate(byName(rate), { what: rate });
    await waitFor(`location.hash.startsWith("#/item/")`, 8000);
    await tabTo(`el.getAttribute("role") === "slider"`, { what: "rating slider" });
    let now = Number(await js(`document.activeElement.getAttribute("aria-valuenow") ?? 0`));
    if (now >= 4) { await press("ArrowLeft", now * 2 - 7); now = 3.5; } // always a change, so a toast
    await press("ArrowRight", 8 - now * 2); // 4 stars
    await waitFor(`document.activeElement.getAttribute("aria-valuenow") === "4"`);
    await toast(/Rated /);
  }
});
await step("F4 unmatched plays: link one to its record", async () => {
  await go("Recent plays", "#/plays");
  await sleep(1500);
  const before = await invoke("list_unmatched");
  if (!before.length) throw new Skip("no unmatched plays");
  await waitFor(`txt().includes("Unmatched plays")`, 8000);
  const u = before[0];
  if (u.candidates.length) await activate(byName(`${u.suggestion} for ${u.track_name}`), { what: "suggested link" });
  else throw new Skip(`first unmatched play has no candidate record: ${u.track_name}`);
  await toast(/^Linked\. /);
  const after = await invoke("list_unmatched");
  if (after.some((x) => x.norm_key === u.norm_key)) throw new Error(`still unmatched after linking: ${u.norm_key}`);
  await shot("F5-unmatched-linked");
});
}

/* G — Spotify (R3) on a throwaway playlist: Studio, Check other playlists, FR-041, FR-042 (M7 WI 2, journey G) */
if (J.has("G") && !PLAYLIST) await step("G1 Spotify journey", async () => { throw new Skip("RUNOUT_TEST_PLAYLIST unset"); });
if (J.has("G") && PLAYLIST) {
const gSkip = () => { if (!PLAYLIST) throw new Skip("RUNOUT_TEST_PLAYLIST unset"); if (!CONNECTED) throw new Skip("Spotify not connected"); };
let PL = null; let ORIG = []; let CUT = new Set();
const uris = async () => (await invoke("load_studio_draft", { playlistUri: PL.uri })).tracks.map((t) => t.uri);
const same = (a, b) => a.length === b.length && a.every((x, i) => x === b[i]);
await step("G1 Playlists → check-up (if in the export) → Studio on the test playlist", async () => {
  gSkip();
  PL = (await invoke("list_spotify_playlists")).find((p) => p.name === PLAYLIST && p.owned);
  if (!PL) throw new Error(`no owned Spotify playlist named ${PLAYLIST}`);
  await go("Playlists", "#/playlists");
  await sleep(1000);
  const inExport = (await invoke("list_exported_playlists")).some((p) => p.name === PLAYLIST);
  if (inExport) {
    await activate(`el.closest("nav[aria-label=Playlists]") && el.tagName === "A" && el.querySelector(".font-semibold")?.textContent === ${JSON.stringify(PLAYLIST)}`, { what: "check-up playlist" });
    await waitFor(`txt().includes("Read-only.")`, 8000);
    await shot("G1-checkup");
    await activate(`el.tagName === "A" && ${byName("Open in Studio")}`, { what: "Open in Studio" });
  } else {
    await segTo("Mode", "Studio (live from Spotify)"); // export has no such playlist (G-27): straight to Studio
    await activate(`el.closest("nav[aria-label=Playlists]") && el.tagName === "A" && el.querySelector(".font-semibold")?.textContent === ${JSON.stringify(PLAYLIST)}`, { what: "Studio playlist" });
  }
  await waitFor(`location.hash.startsWith("#/playlists/studio") && txt().includes("Your changes") && document.querySelector("h1")?.textContent === ${JSON.stringify(PLAYLIST)}`, 30000);
});
await step("G2 Studio: cut 3 + dedupe 1 by keyboard → preview → apply → read back from Spotify", async () => {
  gSkip(); if (!PL) throw new Error("G1 failed");
  const d = await invoke("load_studio_draft", { playlistUri: PL.uri });
  ORIG = d.tracks.map((t) => t.uri);
  const dupe = d.tracks.findIndex((t) => t.dupe_of);
  const plain = d.tracks.map((t, i) => i).filter((i) => i !== dupe && !d.tracks[i].dupe_of).slice(0, 3);
  if (dupe < 0 || plain.length < 3) throw new Error(`test playlist needs >= 1 duplicate and >= 3 other tracks (has ${d.tracks.length})`);
  CUT = new Set([dupe, ...plain]);
  const G = `$$("[role=radiogroup][aria-label^='Keep or cut ']")`;
  if (await js(`${G}.length`) !== d.tracks.length) throw new Error("Studio rows ≠ draft tracks");
  for (let i = 0; i < d.tracks.length; i++) {
    const want = CUT.has(i) ? "Cut" : "Keep";
    if (await js(`${G}[${i}].querySelector("[aria-checked=true]").textContent`) === want) continue;
    await tabTo(`el.getAttribute("role") === "radio" && el.closest("[role=radiogroup]") === ${G}[${i}] && el.getAttribute("aria-checked") === "true"`, { what: `row ${i + 1} keep/cut` });
    await key("ArrowRight");
    await waitFor(`${G}[${i}].querySelector("[aria-checked=true]").textContent === ${JSON.stringify(want)}`);
  }
  await waitFor(`$$("aside .rounded-control").length === 4 && $$("aside .rounded-control").some((x) => x.textContent.startsWith("Remove duplicate"))`);
  await shot("G2-studio-draft");
  await activate(byName("Apply 4 changes to Spotify"), { what: "Apply 4 changes to Spotify" });
  await waitFor(`document.querySelector("dialog[open] #dlg-t")?.textContent === ${JSON.stringify(`Apply changes to ${PLAYLIST}?`)}`, 15000);
  await shot("G3-studio-preview");
  await activate(`el.closest("dialog") && /^Apply \\d+ changes?$/.test(nm(el))`, { what: "confirm Apply" });
  await waitFor(`(() => { const t = document.querySelector(".toast")?.textContent ?? ""; return /applied to .*Undo is available for 30 days/.test(t); })()`, 30000);
  const want = ORIG.filter((_, i) => !CUT.has(i));
  let got = [];
  for (let n = 0; n < 15 && !same(got, want); n++) { got = await uris(); if (!same(got, want)) await sleep(2000); }
  if (!same(got, want)) throw new Error(`Spotify shows ${got.length} tracks, expected ${want.length}`);
});
await step("G3 undo banner → Undo last apply → Spotify has the original list again", async () => {
  gSkip(); if (!ORIG.length) throw new Error("G2 failed");
  await waitFor(`txt().includes("Undo is available until")`, 10000);
  await shot("G4-studio-undo-banner");
  await activate(byName("Undo last apply"), { what: "Undo last apply", back: true });
  await waitFor(`/restored to how it was before you applied/.test(document.querySelector(".toast")?.textContent ?? "")`, 30000);
  let got = [];
  for (let n = 0; n < 15 && !same(got, ORIG); n++) { got = await uris(); if (!same(got, ORIG)) await sleep(2000); }
  if (!same(got, ORIG)) throw new Error("the playlist isn't back to the original track list");
});
await step("G4 Check other playlists → tracks also in other playlists are marked (G-24)", async () => {
  gSkip(); if (!PL) throw new Error("G1 failed");
  await activate(byName("Check other playlists"), { what: "Check other playlists" });
  await waitFor(`/also in other playlists, marked in the list\\.|None of these tracks are in your other playlists\\./.test(txt())`, 180000);
  const rows = await invoke("find_cross_playlist_dupes", { playlistUri: PL.uri });
  if (rows.length && !(await js(`txt().includes(", also in " + ${JSON.stringify(rows[0].playlists[0].name)})`))) throw new Error("marker missing for a cross-playlist duplicate");
  await shot("G5-studio-cross-dupes");
});
for (const [n, label, read] of [["G5", "top tracks", () => invoke("get_top_items", { kind: "tracks", range: "4w" })], ["G6", "recent plays", () => invoke("get_recent_plays_api")]]) {
  await step(`${n} From Spotify ${label} → Add to backlog → a promoted record (FR-041)`, async () => {
    gSkip();
    await go("From Spotify", "#/spotify");
    await waitFor(`!!document.querySelector('section[aria-label^="Your top"]')`, 20000);
    const region = label === "top tracks" ? `el.closest('section[aria-label^="Your top"]')` : `el.closest('[aria-labelledby="fs-recent"]')`;
    const entries = await read();
    const name = await js(`$$("main button").filter((el) => ${region}).map(nm).find((x) => /^Add .+ to backlog$/.test(x)) ?? null`);
    if (!name) throw new Skip(`every ${label} entry is already in the library`);
    const e = entries.find((x) => `Add ${x.name} to backlog` === name);
    await activate(`${region} && ${byName(name)}`, { what: name });
    await qaOpen("Add to backlog");
    await activate(`el.type === "submit" && ${byName("Save")}`, { what: "Save" });
    await toast(/saved to your backlog/);
    const row = (await invoke("list_items", { filter: { limit: 1000, sort: "added_at", desc: true } })).rows.find((r) => r.spotify_uri === e?.uri);
    if (!row) throw new Error(`no record with ${e?.uri}`);
    const it = await invoke("get_item", { id: row.id });
    if (it.provenance.display_name !== "promoted") throw new Error(`display_name provenance is ${it.provenance.display_name}, not promoted`);
  });
}
await step("G7 an album not yet in your Spotify library: Saved in your Spotify library on, then off (FR-042)", async () => {
  gSkip();
  // Only an album the account does NOT already save, so a real saved album is never removed and re-saved.
  const albums = (await invoke("list_items", { filter: { type: "album", limit: 1000 } })).rows.filter((r) => r.spotify_uri && r.display_name);
  const pref = [...albums.filter((r) => r.display_name === NAME), ...albums.filter((r) => r.display_name !== NAME)].slice(0, 40);
  if (!pref.length) throw new Skip("no album with a Spotify link in the library");
  const saved = await invoke("library_contains", { uris: pref.map((r) => r.spotify_uri) });
  const al = pref.find((_, i) => !saved[i]);
  if (!al) throw new Skip("every candidate album is already saved in your Spotify library");
  try {
    await key("Ctrl+k"); await waitFor(`document.activeElement.id === "top-field"`);
    await type(al.display_name); await key("Enter");
    await waitFor(`location.hash.startsWith("#/search/")`);
    await activate(`el.tagName === "A" && el.getAttribute("href") === "#/item/${al.id}"`, { what: "album search result" });
    await waitFor(`document.querySelector("h1")?.textContent === ${JSON.stringify(al.display_name)}`);
    const box = `el.type === "checkbox" && ${byName("Saved in your Spotify library")}`;
    await waitFor(`$$("input[type=checkbox]").some((el) => ${box} && !el.disabled && !el.checked)`, 15000);
    for (const want of [true, false]) {
      await tabTo(box, { what: "Saved in your Spotify library" });
      await key(" ");
      await toast(want ? /saved in your Spotify library/ : /removed from your Spotify library/);
      let got;
      for (let n = 0; n < 10 && got !== want; n++) { got = (await invoke("library_contains", { uris: [al.spotify_uri] }))[0]; if (got !== want) await sleep(1500); }
      if (got !== want) throw new Error(`library_contains says ${got}, expected ${want}`);
      if (want) await shot("G6-saved-in-spotify");
    }
  } finally {
    // Leave the account as it was: the album was not saved before this step, so remove it whatever happened above.
    await invoke("remove_from_spotify_library", { uris: [al.spotify_uri] });
  }
});
}

/* O — offline (app launched with launch({ offline: true })): MVP + R2 screens work, R3 screens say offline/connect */
if (J.has("O")) {
const noError = `!document.querySelector("main .banner.err")`;
await step("O1 offline banner shows; Backlog and an Item page work", async () => {
  await waitFor(`txt().includes("You’re offline.")`, 20000);
  await go("Backlog", "#/backlog");
  await waitFor(`(${noError}) && ($$("main .row a").length > 0 || txt().includes("Nothing waiting"))`, 10000);
  if (await js(`$$("main .row a").length > 0`)) {
    await activate(`el.matches("main .row a")`, { what: "first backlog row" });
    await waitFor(`location.hash.startsWith("#/item/") && !!document.querySelector("main h1") && (${noError})`, 10000);
  }
  await shot("O1-offline-item");
});
await step("O2 Insights (R2) loads every card with its sources", async () => {
  await go("Insights", "#/insights");
  await waitFor(`(${noError}) && (txt().includes("Nothing to show yet") || ($$("main section.card").length > 0 && !txt().includes("Sources unknown.")))`, 20000);
  await shot("O2-offline-insights");
});
await step("O3 Recent plays and Rediscover load", async () => {
  await go("Recent plays", "#/plays");
  await waitFor(`(${noError}) && ($$("main .row").length > 0 || txt().includes("No plays yet"))`, 15000);
  await go("Rediscover", "#/rediscover");
  await waitFor(`(${noError}) && ($$("main section[aria-label]").length > 0 || txt().includes("No shelves yet"))`, 15000);
});
await step("O4 Settings loads; Spotify card and Inbox say offline/connect", async () => {
  await go("Settings", "#/settings");
  await waitFor(`txt().includes("Settings and data") && (txt().includes("Connected, but you’re offline") || txt().includes("Not connected") || txt().includes("Connect when you’re back online"))`, 15000);
  await waitFor(`!$$("button").some((b) => nm(b) === "Check now" && !b.disabled)`); // Inbox: no Check now offline
  await shot("O3-offline-settings");
});
await step("O5 R3 screens show the offline or connect state", async () => {
  await go("From Spotify", "#/spotify");
  await waitFor(`txt().includes("You're offline") || txt().includes("Spotify isn't connected")`, 15000);
  await go("Playlists", "#/playlists");
  await sleep(800);
  if (await js(`!!document.querySelector('[aria-label="Mode"]')`)) {
    await segTo("Mode", "Studio (live from Spotify)");
    await waitFor(`txt().includes("Studio needs a connection") || txt().includes("Connect Spotify to use Studio")`, 15000);
  }
  await shot("O4-offline-studio");
});
}

/* H — back up and restore */
if (J.has("H")) {
await step("H1 backup now: location and size confirmed", async () => {
  await go("Settings", "#/settings");
  await activate(byName("Back up now"), { what: "Back up now" });
  await toast(/Backup saved to .+ \(\d+\.\d MB\)/);
  await waitFor(`txt().includes("(latest)")`);
  await shot("H1-backup-done");
});
await step("H2 restore: pick latest → check verifies counts → restore", async () => {
  await activate(byName("Restore"), { what: "Restore" });
  await qaOpen("Restore a backup");
  await activate(`el.closest("dialog") && ${nameStarts("Restore the backup from")}`, { what: "Restore latest backup" });
  await waitFor(`/The check passed\\. It holds \\d+ records: \\d+ albums, \\d+ tracks, \\d+ artists\\./.test(document.querySelector("dialog[open]")?.textContent ?? "")`, 8000);
  await shot("H2-restore-check");
  await activate(`el.closest("dialog") && ${byName("Restore")}`, { what: "confirm Restore" });
  await toast(/Restored the backup from .+\. \d+ records verified\./);
  await shot("H3-restored");
});
}
if (J.has("B") && POST) await invoke("set_capture_enabled", { on: false });

/* Motion and focus checks */
await step("M2 every rendered <button> on visited routes has an accessible name", async () => {
  if (unnamed.size) throw new Error([...unnamed].join("\n  "));
});
await step("M3 every keyboard stop reached showed a focus ring", async () => {
  const bad = focusRings.filter((f) => f.style === "none");
  if (bad.length) throw new Error(JSON.stringify(bad));
});

// A journey named in RUNOUT_JOURNEYS whose steps all skipped (or that has none) proved nothing: a failure.
for (const l of NAMED) {
  const mine = results.filter((r) => r[0].startsWith(l));
  if (mine.every((r) => r[2] === "skip")) results.push([`${l} (named in RUNOUT_JOURNEYS)`, false, "every step skipped"]);
}
const failed = results.filter((r) => !r[1]);
failed.filter((r) => r[2] === "every step skipped").forEach((r) => console.log(`FAIL ${r[0]} — every step skipped`));
const skipped = results.filter((r) => r[2] === "skip").length;
console.log(`\n${results.length - failed.length - skipped} passed, ${skipped} skipped, ${failed.length} failed; ${focusRings.length} keyboard stops checked for a focus ring; ${shots.length} screenshots in ${SHOTS}`);
done(failed.length ? 1 : 0);

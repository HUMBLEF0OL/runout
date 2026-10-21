// M6 evidence: journeys A–E and H, KEYBOARD ONLY, on the INSTALLED app with its real library (not the mocks).
// 1. Launch: node -e "import('./scripts/installed/app.mjs').then((m) => m.launch())"  (WebView2 CDP on :9333)
// 2. RUNOUT_IDS=<json {album, track, artist: [22-char Spotify IDs]} kept OUT of the repo> node scripts/installed/journeys.mjs
// Needs network (prefill, embeds). Writes to the live library: use a test library, never the user's own.
// B posts to the capture listener from this PC's LAN address, exactly what the iPhone Shortcut sends (TSD §5.1).
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
async function step(name, fn) {
  current = name;
  try { await fn(); results.push([name, true]); console.log(`PASS ${name}`); }
  catch (e) { results.push([name, false, e.message]); console.log(`FAIL ${name} — ${e.message}`); }
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
const IDS = JSON.parse(readFileSync(process.env.RUNOUT_IDS));
const invoke = (cmd, args = {}) => js(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
// First catalog IDs not yet in the library, so re-runs never hit "Already saved".
const have = new Set((await invoke("list_items", { filter: { limit: 1000 } })).rows.map((r) => r.spotify_uri));
const fresh = (k, skip = 0) => IDS[k].filter((i) => !have.has(`spotify:${k}:${i}`))[skip];
const ALBUM = `https://open.spotify.com/album/${fresh("album")}?si=journey`;
const ARTIST = `https://open.spotify.com/artist/${fresh("artist")}`;
const SHARE = `https://open.spotify.com/track/${fresh("track")}?si=phone`;
await send("Page.bringToFront");
await send("Emulation.setFocusEmulationEnabled", { enabled: true }); // the real window may not be the OS foreground
await js(`(location.hash = "#/home", 1)`);
await waitFor(`!!document.querySelector(".rail")`, 15000);
await sleep(800);
const qaOpen = (title) => waitFor(`(() => { const d = document.querySelector("dialog[open]"); return d && d.querySelector("#dlg-t").textContent === ${JSON.stringify(title)}; })()`);
const closeDialog = async () => { await key("Escape"); await waitFor(`!document.querySelector("dialog[open]")`); };
let NAME = "", ARTIST_NAME = "";

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

/* B — capture from phone: a real LAN POST to the capture listener (what the iPhone Shortcut sends, TSD §5.1) */
let needBefore = 0;
await step("B1 phone share (LAN POST /capture) → backlog shows it as Name needed", async () => {
  await invoke("set_capture_enabled", { on: true });
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
  await shot("B1-share-name-needed");
});
await step("B2 name it by keyboard → badge gone", async () => {
  await activate(`el === $$('button[aria-label^="Name the link shared"]').at(-1)`, { what: "Name it (newest share)" });
  await qaOpen("Name a phone capture");
  await tabTo(`el.id === "qa-name"`, { what: "Name field" });
  const pre = await js(`document.activeElement.value`);
  if (!pre) await type("Shared from phone");
  const named = pre || "Shared from phone";
  await activate(`el.type === "submit" && ${byName("Save")}`, { what: "Save" });
  await toast(/ named/);
  await waitFor(`$$(".badge.need").length === ${needBefore}`);
  await waitFor(`$$(".row a").some((a) => a.textContent.includes(${JSON.stringify(named)}))`);
  await shot("B3-named-in-backlog");
});

/* C — explore and document an album (the one saved in A) */
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

/* D — explore an artist */
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

/* E — rediscover */
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

/* H — back up and restore */
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
await invoke("set_capture_enabled", { on: false });

/* Motion and focus checks */
await step("M2 every rendered <button> on visited routes has an accessible name", async () => {
  if (unnamed.size) throw new Error([...unnamed].join("\n  "));
});
await step("M3 every keyboard stop reached showed a focus ring", async () => {
  const bad = focusRings.filter((f) => f.style === "none");
  if (bad.length) throw new Error(JSON.stringify(bad));
});

const failed = results.filter((r) => !r[1]);
console.log(`\n${results.length - failed.length}/${results.length} steps passed; ${focusRings.length} keyboard stops checked for a focus ring; ${shots.length} screenshots in ${SHOTS}`);
done(failed.length ? 1 : 0);

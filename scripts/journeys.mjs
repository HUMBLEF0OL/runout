// M2 acceptance evidence: journeys A–H (BRD §8) driven KEYBOARD ONLY against the mock-data UI in headless Edge (CDP).
// Usage: node scripts/journeys.mjs   (needs Vite on http://localhost:1420; `npx vite --port 1420 --strictPort`)
// User actions use Input.dispatchKeyEvent / Input.insertText only. Runtime.evaluate is used for assertions, focus-ring
// reads and the external iPhone-share event. Exits non-zero on any FAIL. Screenshots → docs/design/screens/m2-journeys/.
import { spawn } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const APP = process.env.APP_URL ?? "http://localhost:1420";
const EDGE = process.env.EDGE ?? "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe";
const OUT = new URL("../docs/design/screens/m2-journeys/", import.meta.url);
const SHOTS = decodeURIComponent(OUT.pathname.replace(/^\/([A-Za-z]:)/, "$1"));
mkdirSync(SHOTS, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/* ------------------------------------------------------------------ browser */
const port = 9300 + Math.floor(Math.random() * 500);
const profile = join(tmpdir(), `runout-journeys-${port}`);
const edge = spawn(EDGE, ["--headless=new", "--disable-gpu", "--hide-scrollbars", "--window-size=1440,900", `--remote-debugging-port=${port}`,
  // Fully local: no Spotify (embed iframes are out-of-process, so CDP URL blocking alone misses them), no browser background calls.
  "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE localhost, EXCLUDE 127.0.0.1",
  `--user-data-dir=${profile}`, "about:blank"], { stdio: "ignore" });
let targets;
for (let i = 0; i < 60 && !targets; i++) { try { targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); } catch { await sleep(200); } }
const ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let seq = 0; const pending = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
const send = (method, params = {}) => new Promise((res, rej) => { pending.set(++seq, (d) => (d.error ? rej(new Error(`${method}: ${d.error.message}`)) : res(d.result))); ws.send(JSON.stringify({ id: seq, method, params })); });
const done = (code) => { try { ws.close(); edge.kill(); } catch { /* gone */ } setTimeout(() => { try { rmSync(profile, { recursive: true, force: true }); } catch { /* locked */ } process.exit(code); }, 500); };

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
async function tabTo(pred, { back = false, max = 150, what = pred } = {}) {
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

/* ------------------------------------------------------------------ run */
await send("Page.enable");
await send("Page.navigate", { url: `${APP}/#/home` });
try { await waitFor(`!!document.querySelector(".rail")`, 15000); } catch { console.log(`FAIL app not reachable at ${APP}`); done(1); }
await sleep(800);

const NEW_ALBUM = "https://open.spotify.com/album/3QJzdZJYIAcoET1GcfpNGi";
const qaOpen = (title) => waitFor(`(() => { const d = document.querySelector("dialog[open]"); return d && d.querySelector("#dlg-t").textContent === ${JSON.stringify(title)}; })()`);
const closeDialog = async () => { await key("Escape"); await waitFor(`!document.querySelector("dialog[open]")`); };

/* A — save music for later */
await step("A1 Ctrl+N opens quick add, focus in link field", async () => {
  await key("Ctrl+n"); await qaOpen("Add to backlog");
  await waitFor(`document.activeElement.id === "qa-link"`);
});
await step("A2 paste link → type detected, name prefilled, embed slot shown", async () => {
  await type(NEW_ALBUM);
  await waitFor(`txt().includes("Detected an album. Not in your library yet.")`);
  await waitFor(`document.querySelector("#qa-name")?.value === "Con Todo El Mundo"`);
  await waitFor(`!!document.querySelector('dialog iframe[src="https://open.spotify.com/embed/album/3QJzdZJYIAcoET1GcfpNGi"]')`);
  await waitFor(`txt().includes("Suggested by Spotify") && $$("dialog a").some((a) => a.textContent === "Open Spotify")`); // attribution
  await shot("A1-quick-add-link");
});
await step("A3 confirm the name, Save → toast, dialog closes", async () => {
  await tabTo(`el.id === "qa-name"`, { what: "Name field" });
  await press("Ctrl+a"); await type("Con Todo El Mundo");
  await activate(`el.type === "submit" && ${byName("Save")}`, { what: "Save" });
  await toast(/Con Todo El Mundo saved to your backlog/);
  await waitFor(`!document.querySelector("dialog[open]")`);
});
await step("A4 appears in the backlog as Planned", async () => {
  await go("Backlog", "#/backlog");
  await waitFor(`$$(".row a").some((a) => a.textContent.includes("Con Todo El Mundo"))`);
  await waitFor(`$$("button").some((b) => nm(b) === "Start Con Todo El Mundo")`); // Planned rows offer Start
  await shot("A2-saved-backlog");
});
await step("A5 item page shows the embed slot", async () => {
  await activate(`el.tagName === "A" && el.textContent.includes("Con Todo El Mundo")`, { what: "backlog row link" });
  await waitFor(`document.querySelector("h1")?.textContent === "Con Todo El Mundo"`);
  await waitFor(`!!document.querySelector('iframe[title="Spotify embed"][src*="/embed/album/3QJzdZJYIAcoET1GcfpNGi"]')`);
  await waitFor(`document.querySelector('[role=radio][aria-checked=true]')?.textContent === "Planned"`);
  await shot("A3-item-embed");
});
await step("A6 quick add: duplicate link → Already saved + Open existing", async () => {
  await key("Ctrl+n"); await qaOpen("Add to backlog"); await type(NEW_ALBUM);
  await waitFor(`txt().includes("Already saved") && txt().includes("Con Todo El Mundo is already in your library")`);
  await tabTo(`${byName("Open existing")}`, { what: "Open existing" });
  await shot("A4-quick-add-duplicate"); await closeDialog();
});
await step("A7 quick add: unsupported (podcast episode) link", async () => {
  await key("Ctrl+n"); await qaOpen("Add to backlog"); await type("https://open.spotify.com/episode/3QJzdZJYIAcoET1GcfpNGi");
  await waitFor(`txt().includes("This is a podcast episode. Runout keeps albums, tracks and artists.")`);
  await shot("A5-quick-add-unsupported"); await closeDialog();
});
await step("A8 quick add offline: short link → Save for later → queued", async () => {
  await devState("offline");
  await waitFor(`txt().includes("You’re offline.")`);
  await key("Ctrl+n"); await qaOpen("Add to backlog"); await type("https://spotify.link/aXk3P9q");
  await waitFor(`txt().includes("the short link can’t be opened yet")`);
  await shot("A6-quick-add-offline-save-later");
  await activate(byName("Save for later"), { what: "Save for later" });
  await toast(/It will be named when you’re back online/);
  await devState("normal");
  await waitFor(`!txt().includes("You’re offline.")`);
});

/* B — capture from phone (the share itself is outside the app: dev panel "iPhone share", reached by keyboard) */
let needBefore = 0;
await step("B1 iPhone share arrives → backlog shows it as Name needed", async () => {
  await go("Backlog", "#/backlog");
  needBefore = await js(`$$(".badge.need").length`);
  await devPanel(true);
  await activate(`el.matches("details.proto a") && el.textContent === "iPhone share"`, { what: "dev panel iPhone share" });
  await devPanel(false);
  await waitFor(`$$(".badge.need").length === ${needBefore + 1}`);
  await shot("B1-share-name-needed");
});
await step("B2 name it by keyboard → badge gone", async () => {
  await activate(`el === $$('button[aria-label^="Name the link shared"]').at(-1)`, { what: "Name it (newest share)" });
  await qaOpen("Name a phone capture");
  await tabTo(`el.id === "qa-name"`, { what: "Name field" });
  await type("Field Recordings Vol. 1");
  await shot("B2-quick-add-naming");
  await activate(`el.type === "submit" && ${byName("Save")}`, { what: "Save" });
  await toast(/Field Recordings Vol\. 1 named/);
  await waitFor(`$$(".badge.need").length === ${needBefore}`);
  await waitFor(`$$(".row a").some((a) => a.textContent.includes("Field Recordings Vol. 1"))`);
  await shot("B3-named-in-backlog");
});

/* C — explore and document an album */
await step("C1 Ctrl+K search finds the album; key 2 sets In progress; Open Spotify is reachable", async () => {
  await key("Ctrl+k"); await waitFor(`document.activeElement.id === "top-field"`);
  await type("Con Todo"); await key("Enter");
  await waitFor(`location.hash.startsWith("#/search/")`);
  await activate(`el.tagName === "A" && el.textContent.includes("Con Todo El Mundo")`, { what: "search result link" });
  await waitFor(`document.querySelector("h1")?.textContent === "Con Todo El Mundo"`);
  await tabTo(`${byName("Edit Con Todo El Mundo")}`, { what: "Edit link (any non-input focus)" });
  await key("2");
  await toast(/Con Todo El Mundo is now In progress/);
  await waitFor(`document.querySelector('[role=radio][aria-checked=true]')?.textContent === "In progress"`);
  await tabTo(`${byName("Open Con Todo El Mundo in Spotify")} && el.href.startsWith("https://open.spotify.com/album/")`, { back: true, what: "Open Spotify" });
  await shot("C1-in-progress");
});
await step("C2 rating by arrow keys (4 stars)", async () => {
  await tabTo(`el.getAttribute("role") === "slider"`, { what: "rating slider" });
  for (let i = 1; i <= 8; i++) { await key("ArrowRight"); await waitFor(`document.activeElement.getAttribute("aria-valuenow") === "${i / 2}"`); }
  await toast(/Rated Con Todo El Mundo 4/);
});
await step("C3 review and tag by keyboard", async () => {
  await tabTo(`el.id === "dt-review"`, { what: "Review" });
  await type("Warm, unhurried; side two is the keeper.");
  await key("Tab"); await toast(/Review saved/);
  await activate(`${byName("Add tag")}`, { back: true, what: "Add tag" });
  await waitFor(`document.activeElement.getAttribute("aria-label") === "New tag"`);
  await type("late-night"); await key("Enter");
  await waitFor(`$$("button").some((b) => nm(b) === "Remove tag late-night")`);
});
await step("C4 key 3 sets Explored; Revisit toggle by Space", async () => {
  await tabTo(byName("Edit Con Todo El Mundo"), { back: true, what: "Edit link (digits are ignored on the rating slider)" });
  await key("3");
  await toast(/is now Explored/);
  await waitFor(`document.querySelector('[role=radio][aria-checked=true]')?.textContent === "Explored"`);
  await tabTo(`el.type === "checkbox" && ${byName("Revisit later")}`, { what: "Revisit later" });
  await key(" ");
  await toast(/flagged to revisit/);
  await waitFor(`document.activeElement.checked === true`);
  await shot("C2-explored-rated-reviewed");
});

/* D — explore an artist */
await step("D1 quick add artist with why-interesting notes", async () => {
  await key("Ctrl+n"); await qaOpen("Add to backlog");
  await type("https://open.spotify.com/artist/7mEBp3V1D8Xz2Rk4Qc9TnY");
  await waitFor(`txt().includes("Detected an artist.") && document.querySelector("#qa-name")?.value === "Arooj Aftab"`);
  await tabTo(`el.id === "qa-notes"`, { what: "Why this artist" });
  await type("Heard on a friend's mix; start with Vulture Prince.");
  await shot("D1-quick-add-artist");
  await activate(`el.type === "submit" && ${byName("Save")}`, { what: "Save" });
  await toast(/Arooj Aftab saved to your backlog/);
});
await step("D2 add an album of theirs from the artist page", async () => {
  await go("Backlog", "#/backlog");
  await activate(`el.tagName === "A" && el.textContent.startsWith("Arooj Aftab")`, { what: "artist row link" });
  await waitFor(`document.querySelector("h1")?.textContent === "Arooj Aftab" && document.querySelector("#dt-notes").value.includes("Vulture Prince")`);
  await activate(byName("Add an album or track by Arooj Aftab"), { what: "Add an album by artist" });
  await qaOpen("Add without a link");
  await waitFor(`document.activeElement.id === "qa-name"`);
  await type("Vulture Prince");
  await activate(`el.type === "submit" && ${byName("Save")}`, { what: "Save" });
  await toast(/Vulture Prince saved to your backlog/);
  await waitFor(`$$("section a").some((a) => a.textContent.startsWith("Vulture Prince"))`);
});
await step("D3 update artist status and notes", async () => {
  await tabTo(byName("Edit Arooj Aftab"), { what: "Edit link" });
  await key("2"); await toast(/Arooj Aftab is now In progress/);
  await tabTo(`el.id === "dt-notes"`, { what: "Why this artist" });
  await key("End"); await type(" Then the Coke Studio sessions.");
  await key("Tab"); await toast(/Notes saved/);
  await shot("D2-artist-page");
});

/* E — rediscover */
const shelfCount = `$$("main section[aria-label]").map((s) => s.getAttribute("aria-label") + ":" + s.querySelectorAll(".tiles > *").length).join("|")`;
await step("E1 filter by tag", async () => {
  await go("Rediscover", "#/rediscover");
  await waitFor(`$$("main section[aria-label]").length > 0`);
  const before = await js(shelfCount);
  await tabTo(`el.id === "rd-tag"`, { what: "Tag filter" });
  await key("ArrowDown");
  await waitFor(`document.activeElement.value !== "" && ${JSON.stringify(before)} !== (${shelfCount})`);
  await shot("E1-filter-tag");
  await key("ArrowUp"); await waitFor(`document.activeElement.value === ""`);
});
await step("E2 filter by rating and Revisit flag", async () => {
  await tabTo(`el.id === "rd-min"`, { what: "Rating filter" });
  await key("ArrowDown"); await waitFor(`document.activeElement.value === "4"`);
  await activate(`el.getAttribute("aria-pressed") !== null && nm(el).startsWith("Revisit only")`, { what: "Revisit only" });
  await waitFor(`document.activeElement.getAttribute("aria-pressed") === "true"`);
  await waitFor(`$$("main section[aria-label]").map((s) => s.getAttribute("aria-label")).includes("Flagged to revisit") && txt().includes("Con Todo El Mundo")`);
  await shot("E2-filter-rating-revisit");
});
await step("E3 artwork set ends with Open Spotify; record opens with Open Spotify", async () => {
  await activate(byName("Show artwork"), { what: "Show artwork" });
  await waitFor(`$$("main a").some((a) => a.textContent === "Open Spotify") && $$("main img[alt=Spotify]").length > 0`);
  await shot("E3-artwork-open-spotify");
  await activate(`el.tagName === "A" && el.textContent === "Con Todo El Mundo"`, { what: "record in shelf" });
  await waitFor(`document.querySelector("h1")?.textContent === "Con Todo El Mundo"`);
  await tabTo(byName("Open Con Todo El Mundo in Spotify"), { what: "Open Spotify" });
});

/* F — review listening history */
await step("F1 import: choose → check → done", async () => {
  await activate(`el.matches(".rail-box a") && el.textContent === "Import a newer export"`, { what: "Import a newer export" });
  await waitFor(`location.hash === "#/import/1"`);
  await activate(byName("Choose folder or .zip"), { what: "Choose folder" });
  await waitFor(`txt().includes("(selected)")`);
  await shot("F1-import-choose");
  await activate(byName("Next"), { what: "Next" });
  await waitFor(`location.hash === "#/import/2" && txt().includes("What we found")`);
  await shot("F2-import-check");
  await activate(nameStarts("Import "), { what: "Import N plays" });
  await waitFor(`location.hash === "#/import/3" && /Imported [\\d,]+ plays\\./.test(txt())`);
  await shot("F3-import-done");
});
await step("F2 Insights shows freshness per source", async () => {
  await activate(byName("See your insights"), { what: "See your insights" });
  await waitFor(`location.hash.startsWith("#/insights")`);
  await waitFor(`/Desktop plays: live\\. All devices: up to your export of /.test(document.querySelector("main .freshness")?.textContent ?? "")`);
  await waitFor(`$$("main *").some((e) => e.children.length === 0 && /^From: .*Desktop log.*Spotify export/.test(e.textContent))`);
  await shot("F4-insights-freshness");
});

/* G — clean up a playlist */
await step("G1 check-up highlights duplicates and low ratings", async () => {
  await go("Playlists", "#/playlists");
  await waitFor(`txt().includes("Read-only.") && $$(".stat").some((s) => s.textContent.startsWith("Duplicates") && !s.textContent.startsWith("Duplicates0"))`);
  await waitFor(`txt().includes("Duplicate") && txt().includes("Rated low")`);
  await shot("G1-checkup");
});
await step("G2 Studio: keep/cut by keyboard, preview, confirm", async () => {
  await activate(byName("Open in Studio"), { what: "Open in Studio" });
  await waitFor(`location.hash.startsWith("#/playlists/studio") && txt().includes("Your changes")`);
  const before = await js(`$$("aside .rounded-control").length`);
  // first track currently kept → cut it with the arrow keys
  await tabTo(`el.getAttribute("role") === "radio" && el.closest("[role=radiogroup]").getAttribute("aria-label").startsWith("Keep or cut") && el.getAttribute("aria-checked") === "true" && el.textContent === "Keep"`, { what: "a Keep/Cut control" });
  await key("ArrowRight");
  await waitFor(`document.activeElement.textContent === "Cut" && document.activeElement.getAttribute("aria-checked") === "true"`);
  await waitFor(`$$("aside .rounded-control").length === ${before + 1}`);
  await key("ArrowLeft"); await waitFor(`$$("aside .rounded-control").length === ${before}`);
  await key("ArrowRight"); await waitFor(`$$("aside .rounded-control").length === ${before + 1}`);
  await shot("G2-studio-draft");
  await activate(`/^Apply \\d+ changes? to Spotify$/.test(nm(el))`, { what: "Apply N changes to Spotify" });
  await waitFor(`document.querySelector("dialog[open] #dlg-t")?.textContent.startsWith("Apply changes to")`);
  await shot("G3-studio-preview");
  await activate(`el.closest("dialog") && /^Apply \\d+ changes?$/.test(nm(el))`, { what: "confirm Apply" });
  await toast(/applied to .*Undo is available for 30 days/);
});
await step("G3 undo banner restores the playlist", async () => {
  await waitFor(`txt().includes("Undo is available until")`);
  await shot("G4-studio-undo-banner");
  await activate(byName("Undo last apply"), { what: "Undo last apply", back: true });
  await toast(/restored to how it was before you applied/);
  await waitFor(`!txt().includes("Undo is available until")`);
});

/* H — back up and restore */
await step("H1 backup now: location and size confirmed", async () => {
  await go("Settings", "#/settings");
  await activate(byName("Back up now"), { what: "Back up now" });
  await toast(/Backup saved to .+ \(\d+\.\d MB\)/);
  await waitFor(`txt().includes("(latest)")`);
  await shot("H1-backup-done");
});
await step("H2 restore: pick → check verifies counts → restore", async () => {
  await activate(byName("Restore"), { what: "Restore" });
  await qaOpen("Restore a backup");
  await activate(`el.closest("dialog") && ${nameStarts("Restore the backup from")}`, { what: "Restore latest backup" });
  await waitFor(`/The check passed\\. It holds \\d+ records: \\d+ albums, \\d+ tracks, \\d+ artists\\./.test(document.querySelector("dialog[open]")?.textContent ?? "")`);
  await shot("H2-restore-check");
  await activate(`el.closest("dialog") && ${byName("Restore")}`, { what: "confirm Restore" });
  await toast(/Restored the backup from .+\. \d+ records verified\./);
  await shot("H3-restored");
});

/* Motion and focus checks */
await step("M1 prefers-reduced-motion: transitions/animations off", async () => {
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
  await devState("loading");
  await go("Library", "#/library");
  const r = await waitFor(`(() => { const b = document.querySelector("main .btn"), s = document.querySelector(".skel"); if (!b || !s) return null;
    const bc = getComputedStyle(b), sc = getComputedStyle(s);
    return { btn: bc.transitionDuration, skelAnim: sc.animationName, skelDur: sc.animationDuration }; })()`);
  if (!r.btn.split(",").every((d) => parseFloat(d) === 0)) throw new Error(`.btn transition-duration ${r.btn}`);
  if (!(r.skelAnim === "none" || r.skelDur.split(",").every((d) => parseFloat(d) === 0))) throw new Error(`.skel animation ${r.skelAnim} ${r.skelDur}`);
  await devState("normal");
  await send("Emulation.setEmulatedMedia", { features: [] });
});
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

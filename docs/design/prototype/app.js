/* Runout design prototype: core (store, router, components, interactions).
   Screens live in screens.js. Data: mock-data.js (scripts/gen_mock_data.py). */
"use strict";
const D = window.RUNOUT_MOCK;
const R = (window.R = {});

/* ---------------------------------------------------------------- store */
R.items = D.items.map((i) => ({ ...i, tags: [...i.tags] }));
R.history = [...D.history];
R.captures = D.captures.map((c) => ({ ...c }));
R.S = {
  release: "r3",          // mvp | r2 | r3
  view: "normal",         // normal | empty | loading | offline | error
  dialog: null,           // { kind, ...args }
  toast: null,            // { text, undo }
  sel: new Set(),         // backlog selection
  draft: {},              // playlist studio: position -> "cut" | "keep"
  paused: false,
  backlogFilter: { type: "all", tag: "", sort: "order" },
  libFilter: { status: "any", rating: "any", page: 1 },
};
const TODAY = new Date(D.meta.today);
R.byId = (id) => R.items.find((i) => i.id === Number(id));
R.byName = (type, name) => R.items.find((i) => i.type === type && i.display_name === name);
R.isR2 = () => R.S.release !== "mvp";
R.isR3 = () => R.S.release === "r3";
R.offline = () => R.S.view === "offline";

/* ---------------------------------------------------------------- formatting */
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
R.esc = esc;
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
R.day = (iso) => { const d = new Date(iso); return `${d.getDate()} ${MONTHS[d.getMonth()]}`; };
R.dayY = (iso) => { const d = new Date(iso); return `${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`; };
R.time = (iso) => iso.slice(11, 16);
const midnight = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
R.daysSince = (iso) => Math.max(0, Math.round((midnight(TODAY) - midnight(new Date(iso))) / 86400000)); // calendar days
R.pl = (n, one, many) => `${R.num(n)} ${n === 1 ? one : many || one + "s"}`;
R.an = (word) => (/^[aeiou]/i.test(word) ? "an " : "a ") + word;
R.ago = (iso) => {
  const days = R.daysSince(iso);
  if (days === 0) {
    const min = Math.round((TODAY - new Date(iso)) / 60000);
    if (min < 2) return "now";
    return min < 60 ? `${min} min ago` : `${Math.round(min / 60)} h ago`;
  }
  if (days === 1) return "Yesterday";
  if (days < 7) return `${R.pl(days, "day")} ago`;
  return R.day(iso);
};
R.age = (iso) => {
  const d = R.daysSince(iso);
  if (d === 0) return "today";
  if (d < 14) return R.pl(d, "day");
  if (d < 60) return R.pl(Math.round(d / 7), "week");
  return R.pl(Math.round(d / 30), "month");
};
R.since = (it) => { const h = R.history.filter((x) => x.item_id === it.id && x.to === it.status).sort((a, b) => b.at.localeCompare(a.at))[0]; return h ? h.at : it.added_at; };
R.hm = (ms) => { const m = Math.round(ms / 60000); return m >= 60 ? (m % 60 ? `${Math.floor(m / 60)}h ${m % 60}m` : `${m / 60}h`) : `${m}m`; };
R.mmss = (ms) => { const s = Math.round(ms / 1000); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`; };
R.num = (n) => Number(n).toLocaleString("en-US");
R.stars = (r) => (r ? "★".repeat(Math.floor(r)) + (r % 1 ? "½" : "") : "");
R.STATUS = { planned: "Planned", in_progress: "In progress", explored: "Explored", not_for_me: "Not for me", abandoned: "Abandoned" };
R.TYPE = { album: "Album", track: "Track", artist: "Artist" };
R.PRIO = { high: "High priority", medium: "Medium", low: "Low" };
R.uriLink = (it) => `open.spotify.com/${it.type}/${(it.spotify_uri || "").split(":")[2] || ""}`;
R.nameOf = (it) => it.display_name || R.uriLink(it);
R.sub = (it) => (it.type === "artist" ? "Artist" : it.type === "track" ? `${it.artist}, from ${it.album}` : it.artist);

/* ---------------------------------------------------------------- router */
R.route = () => {
  const [path, query = ""] = location.hash.replace(/^#\/?/, "").split("?");
  const parts = path.split("/").filter(Boolean).map(decodeURIComponent);
  const params = Object.fromEntries(new URLSearchParams(query));
  return { name: parts[0] || "home", args: parts.slice(1), params };
};
R.go = (hash) => { location.hash = hash; };

/* ---------------------------------------------------------------- shared components */
R.nav = (active) => {
  const c = D.counts;
  const backlogCount = R.items.filter((i) => i.in_backlog).length;
  const all = [
    ["home", "Home", ""],
    ["plays", "Recent plays", "", "r2"],
    ["insights", "Insights", "", "r2"],
    ["backlog", "Backlog", backlogCount],
    ["library", "Library", R.items.length],
    ["rediscover", "Rediscover", R.items.filter((i) => i.revisit).length],
    ["playlists", "Playlists", c.playlists, "r2"],
    ["spotify", "From Spotify", "", "r3"],
    ["settings", "Settings", ""],
  ].filter((n) => !n[3] || (n[3] === "r2" ? R.isR2() : R.isR3()));
  const emptyNow = R.S.view === "empty"; // an empty Home (first run) or Library means no records, so their subsets are 0 too
  return `<nav class="nav" aria-label="Main">${all.map(([k, label, count]) =>
    `<a class="nav-item" href="#/${k}" ${k === active ? 'aria-current="page"' : ""}><span>${label}</span><span class="count">${emptyNow && count !== "" && (k === active || (["home", "library"].includes(active) && ["backlog", "library", "rediscover"].includes(k))) ? 0 : count}</span></a>`).join("")}</nav>`;
};
R.railBox = () => {
  if (R.isR2()) {
    return `<div class="rail-box"><span style="font-weight:600">Data freshness</span>
      <span class="muted" style="line-height:1.45">Desktop plays: ${R.S.paused ? "paused" : "live"}. All devices: up to your export of ${R.day(D.listening.freshness.export_until)}.</span>
      <a href="#/import/1" style="font-weight:600">Import a newer export</a></div>`;
  }
  const waiting = R.S.view === "empty" ? 0 : R.captures.length;
  return `<div class="rail-box"><span style="font-weight:600">Phone capture is on</span>
    <span class="muted" style="line-height:1.45">${waiting ? `${waiting} share${waiting > 1 ? "s" : ""} from your iPhone ${waiting > 1 ? "are" : "is"} waiting for a name.` : "Nothing waiting."}</span></div>`;
};
R.shell = (active, body) => `<div class="app has-rail">
  <aside class="rail"><a class="logo" href="#/home">Runout</a>${R.nav(active)}${R.railBox()}</aside>
  <main class="main" id="main">${R.stateBanner()}${body}</main></div>`;
R.topbar = (extra = "") => `<div class="topbar">
  <form data-form="top" role="search">
    <label class="sr" for="top-field">Search your library, or paste a Spotify link to add it</label>
    <input id="top-field" class="input surface" style="flex:1 1 auto" autocomplete="off" placeholder="Search your library, or paste a Spotify link to add it">
    <button class="btn btn-primary" id="top-btn" type="submit">Add</button>
  </form>${extra}</div>`;
R.stateBanner = () => {
  if (R.S.view === "offline") return `<div class="banner warn" role="status"><span><b>You’re offline.</b> Everything you saved still works. Artwork and short links come back when you reconnect.</span></div>`;
  return "";
};
R.seg = (label, options, current, hrefOrAct) => `<div class="seg" role="tablist" aria-label="${esc(label)}">${options.map(([v, t]) => {
  const on = v === current;
  return typeof hrefOrAct === "function"
    ? `<button type="button" role="tab" aria-selected="${on}" onclick="location.hash='${hrefOrAct(v)}'">${t}</button>`
    : `<button type="button" role="tab" aria-selected="${on}" data-act="${hrefOrAct}" data-v="${v}">${t}</button>`;
}).join("")}</div>`;
R.stat = (k, v, d, act) => `<div class="card stat hover"><span class="k">${k}</span><span class="v">${v}</span><span class="d ${act ? "act" : ""}">${d}</span></div>`;
R.embed = (h, label = "Spotify embed") => R.offline()
  ? `<div class="embed off" role="img" aria-label="Artwork unavailable offline" style="height:${h}px"><b class="text-2">Offline</b>Artwork returns when you reconnect</div>`
  : `<div class="embed" role="img" aria-label="Spotify embed placeholder" style="height:${h}px">${label}</div>`;
R.logo = () => `<span class="logo-slot" role="img" aria-label="Spotify logo placeholder">Spotify logo</span>`;
R.empty = (h, line, action = "") => `<div class="card empty"><span class="h">${h}</span><span class="text-2">${line}</span>${action}</div>`;
R.skelRows = (n = 6) => `<div class="card pad-s"><div class="list">${Array.from({ length: n }, (_, i) =>
  `<div class="row" style="grid-template-columns:36px minmax(0,1fr) 60px"><div class="skel" style="width:36px;height:36px"></div>
   <div class="stack" style="gap:6px"><div class="skel" style="height:10px;width:${60 + ((i * 13) % 30)}%"></div><div class="skel" style="height:8px;width:${30 + ((i * 7) % 20)}%"></div></div>
   <div class="skel" style="height:8px"></div></div>`).join("")}</div></div>`;
R.skelTiles = (n = 5) => `<div class="tiles five">${Array.from({ length: n }, () => `<div class="stack" style="gap:6px"><div class="skel" style="aspect-ratio:1/1;border-radius:10px"></div><div class="skel" style="height:10px;width:70%"></div><div class="skel" style="height:8px;width:45%"></div></div>`).join("")}</div>`;
R.error = (title, line, action = "") => `<div class="banner err" role="alert"><span><b style="color:var(--danger)">${title}</b> <span class="text-2">${line}</span></span>${action}</div>`;
R.openSpotify = (name) => `<a href="#" onclick="return false" style="font-size:13px;font-weight:600" ${name ? `aria-label="Open ${esc(name)} in Spotify"` : ""}>Open Spotify</a>`;
R.addAttrs = (link, name, artist, kind) => `data-act="open-add" data-link="${esc(link)}" data-name="${esc(name)}" data-artist="${esc(artist)}" data-kind="${kind}"`;
R.statusMeta = (it) => (it.rating ? `${R.STATUS[it.status]} ${R.stars(it.rating)}` : R.STATUS[it.status]);

/* ---------------------------------------------------------------- mutations (with undo) */
R.setStatus = (id, to) => {
  const it = R.byId(id); const from = it.status;
  if (from === to) return;
  it.status = to; it.in_backlog = to === "planned" || to === "in_progress";
  if (to === "explored" || to === "not_for_me") it.explored_at = D.meta.today;
  R.history.push({ item_id: it.id, from, to, at: D.meta.today, note: "" });
  R.toast(`${esc(R.nameOf(it))} is now ${R.STATUS[to]}`, () => { it.status = from; it.in_backlog = from === "planned" || from === "in_progress"; R.history.pop(); });
};
R.toast = (text, undo) => { R.S.toast = { text, undo }; R.render(); clearTimeout(R._t); R._t = setTimeout(() => { R.S.toast = null; R.render(); }, 5000); };
R.toastHtml = () => R.S.toast ? `<div class="toast overlay" role="status"><span>${R.S.toast.text}</span>${R.S.toast.undo ? `<button class="btn btn-secondary s" data-act="undo">Undo</button>` : ""}</div>` : "";

/* ---------------------------------------------------------------- render */
R.render = () => {
  const r = R.route();
  const screen = R.screens[r.name] || R.screens.home;
  const focusId = document.activeElement && document.activeElement.id;
  document.getElementById("app").innerHTML = screen(r) + (R.S.dialog ? R.dialogs[R.S.dialog.kind](R.S.dialog) : "") + R.toastHtml() + R.protoPanel(r);
  if (focusId) { const el = document.getElementById(focusId); if (el && el.tagName === "INPUT") { el.focus(); } }
  if (R.S.dialog) { const f = document.querySelector(".dialog [autofocus]"); if (f) f.focus(); }
  document.title = `Runout · ${r.name}`;
};

/* ---------------------------------------------------------------- prototype controls (outside the product) */
R.protoPanel = (r) => {
  const screens = [["home", "Home"], ["backlog", "Backlog"], ["library", "Library"], ["item/" + R.byName("album", "Promises").id, "Item detail (album)"],
    ["item/" + R.byName("artist", "Alice Coltrane").id, "Item detail (artist)"], ["item/" + R.byName("track", "Movement 6").id, "Item detail (track)"],
    ["search/coltrane", "Search results"], ["rediscover", "Rediscover"], ["plays", "Recent plays"], ["insights", "Insights"], ["import/1", "Import"],
    ["playlists", "Playlists"], ["spotify", "From Spotify"], ["settings", "Settings"], ["first-run", "First run"], ["tray", "Tray menu"], ["system", "Design system"]];
  const opts = (vals, cur) => vals.map(([v, t]) => `<option value="${v}" ${v === cur ? "selected" : ""}>${t}</option>`).join("");
  return `<details class="proto" ${R._protoOpen ? "open" : ""} ontoggle="R._protoOpen=this.open"><summary>Prototype controls</summary><div class="in">
    <label>Release <select data-proto="release">${opts([["mvp", "MVP"], ["r2", "Release 2"], ["r3", "Release 3"]], R.S.release)}</select></label>
    <label>State of this screen <select data-proto="view">${opts([["normal", "Normal"], ["empty", "Empty"], ["loading", "Loading"], ["offline", "Offline"], ["error", "Error"]], R.S.view)}</select></label>
    <span>Quick add with:</span>
    <div class="grid"><a href="#" data-act="open-add" data-link="https://open.spotify.com/album/3QJzdZJYIAcoET1GcfpNGi">new album</a>
      <a href="#" data-act="open-add" data-link="${"https://" + R.uriLink(R.byName("album", "Promises"))}">duplicate</a>
      <a href="#" data-act="open-add" data-link="https://open.spotify.com/episode/4rOoJ6Egrf8K2IrywzwOMk">episode</a>
      <a href="#" data-act="open-add" data-link="https://spotify.link/aXk3P9q">short link</a>
      <a href="#" data-act="open-add" data-link="https://open.spotify.com/artist/7mEBp3V1D8Xz2Rk4Qc9TnY">artist</a></div>
    <span>Screens:</span><div class="grid">${screens.map(([h, t]) => `<a href="#/${h}">${t}</a>`).join("")}</div>
  </div></details>`;
};

/* ---------------------------------------------------------------- events */
R.linkType = (s) => {
  const m = String(s).trim().match(/(?:open\.spotify\.com\/(?:intl-[a-z]+\/)?|spotify:)(album|track|artist|playlist|episode|show|audiobook)[/:]([A-Za-z0-9]{22})/);
  if (m) return { kind: m[1], id: m[2] };
  if (/spotify\.link\/\w+/.test(s)) return { kind: "short" };
  return null;
};
document.addEventListener("input", (e) => {
  if (e.target.id === "top-field") {
    const t = R.linkType(e.target.value);
    const btn = document.getElementById("top-btn");
    btn.textContent = t ? (t.kind === "short" ? "Add link" : `Add ${t.kind}`) : e.target.value ? "Search" : "Add";
  }
  if (e.target.id === "qa-link") { R.S.dialog.link = e.target.value; R.render(); }
});
document.addEventListener("submit", (e) => {
  e.preventDefault();
  if (e.target.dataset.form === "top") {
    const v = document.getElementById("top-field").value.trim();
    if (!v || R.linkType(v)) { R.S.dialog = { kind: "quickAdd", link: v }; R.render(); }
    else R.go(`#/search/${encodeURIComponent(v)}`);
  }
});
document.addEventListener("change", (e) => {
  const p = e.target.dataset.proto;
  if (p) { R.S[p] = e.target.value; R.S.dialog = null; R.render(); return; }
  const f = e.target.dataset.filter;
  if (f) { const [obj, key] = f.split("."); R.S[obj][key] = e.target.value; if (obj === "libFilter") R.S.libFilter.page = 1; R.render(); }
  if (e.target.dataset.act === "sel") { const id = Number(e.target.dataset.id); R.S.sel.has(id) ? R.S.sel.delete(id) : R.S.sel.add(id); R.render(); }
  if (e.target.dataset.act === "toggle") { const it = R.byId(e.target.dataset.id); it[e.target.dataset.k] = e.target.checked; R.toast(`${esc(R.nameOf(it))}: ${e.target.dataset.k === "revisit" ? (it.revisit ? "flagged to revisit" : "revisit flag removed") : it.favourite ? "favourite" : "no longer a favourite"}`); }
  if (e.target.dataset.act === "prio") { const it = R.byId(e.target.dataset.id); it.priority = e.target.value || null; R.render(); return; }
  const act = e.target.dataset.act;
  if (act && !["sel", "toggle"].includes(act) && R.actions[act]) R.actions[act](e.target, e.target.dataset.id, e.target.dataset.v);
});
document.addEventListener("click", (e) => {
  const a = e.target.closest("[data-act]");
  if (!a || a.tagName === "INPUT") return;
  const act = a.dataset.act; const id = a.dataset.id; const v = a.dataset.v;
  const H = R.actions[act];
  if (H) { e.preventDefault(); H(a, id, v); }
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && R.S.dialog) { R.S.dialog = null; R.render(); return; }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") { e.preventDefault(); R.S.dialog = { kind: "quickAdd", link: "" }; R.render(); return; }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") { e.preventDefault(); const f = document.getElementById("top-field"); if (f) f.focus(); return; }
  const r = R.route();
  if (r.name === "item" && /^[1-5]$/.test(e.key) && !/INPUT|TEXTAREA|SELECT/.test(document.activeElement.tagName)) {
    R.setStatus(r.args[0], Object.keys(R.STATUS)[Number(e.key) - 1]);
  }
});
window.addEventListener("hashchange", () => { R.S.dialog = null; R.S.view = R.S.view === "loading" ? "normal" : R.S.view; R.render(); window.scrollTo(0, 0); });

R.actions = {
  undo: () => { const u = R.S.toast && R.S.toast.undo; R.S.toast = null; if (u) u(); R.render(); },
  status: (a, id, v) => R.setStatus(id, v),
  start: (a, id) => R.setStatus(id, "in_progress"),
  rate: (a, id, v) => { const it = R.byId(id); const old = it.rating; it.rating = Number(v); R.toast(`Rated ${esc(R.nameOf(it))} ${it.rating}`, () => { it.rating = old; }); },
  "quick-rate": (a, id) => R.go(`#/item/${id}`),
  "add-tag": (a, id) => { const t = prompt("New tag"); if (t) { R.byId(id).tags.push(t.trim().toLowerCase()); R.render(); } },
  "remove-tag": (a, id, v) => { const it = R.byId(id); it.tags = it.tags.filter((t) => t !== v); R.toast(`Removed tag ${esc(v)}`, () => it.tags.push(v)); },
  "sel-clear": () => { R.S.sel.clear(); R.render(); },
  "sel-remove": () => { const ids = [...R.S.sel]; ids.forEach((i) => (R.byId(i).in_backlog = false)); R.S.sel.clear(); R.toast(`${ids.length} removed from backlog`, () => ids.forEach((i) => (R.byId(i).in_backlog = true))); },
  "sel-prio": () => { const ids = [...R.S.sel]; const old = ids.map((i) => R.byId(i).priority); ids.forEach((i) => (R.byId(i).priority = "high")); R.S.sel.clear(); R.toast(`${ids.length} set to high priority`, () => ids.forEach((i, n) => (R.byId(i).priority = old[n]))); },
  "remove-backlog": (a, id) => { const it = R.byId(id); it.in_backlog = false; R.toast(`${esc(R.nameOf(it))} removed from backlog. The record stays in your library.`, () => (it.in_backlog = true)); },
  "open-add": (a) => { R.S.dialog = { kind: "quickAdd", link: a.dataset.link || "", suggest: a.dataset.name ? [a.dataset.name, a.dataset.artist || "", a.dataset.kind || "track"] : null }; R.render(); },
  "edit-save": (a, id) => { R.go(`#/item/${id}`); R.toast("Changes saved"); },
  "name-it": (a, id) => { const c = R.captures.find((x) => x.id === Number(id)); R.S.dialog = { kind: "quickAdd", link: "https://" + c.raw_link, capture: c.id, naming: true }; R.render(); },
  close: () => { R.S.dialog = null; R.render(); },
  "save-add": () => {
    const dl = R.S.dialog; const name = document.getElementById("qa-name").value.trim(); const t = R.linkType(dl.link) || { kind: "album" };
    const it = { id: Math.max(...R.items.map((i) => i.id)) + 1, type: t.kind === "short" ? "album" : t.kind, display_name: name || null, artist: document.getElementById("qa-artist").value,
      spotify_uri: `spotify:${t.kind}:${t.id || "x"}`, status: "planned", priority: document.getElementById("qa-prio").value || null, rating: null, review: "", notes: "",
      favourite: false, revisit: false, in_backlog: true, recommended_by: document.getElementById("qa-rec").value, tags: [], added_at: D.meta.today, tint: "#3A4250", provenance: { display_name: "promoted" } };
    R.items.push(it);
    if (dl.capture) R.captures = R.captures.filter((c) => c.id !== dl.capture);
    R.S.dialog = null;
    R.toast(`${esc(name || "Link")} saved to your backlog`, () => { R.items.pop(); });
  },
  "save-later": () => { R.S.dialog = null; R.toast("Saved. It will be named when you’re back online."); },
  "pause": (a, id, v) => { R.S.paused = v !== "resume"; R.toast(R.S.paused ? (v === "hour" ? "Logging paused for 1 hour" : "Logging paused until you resume") : "Logging resumed"); },
  "backup-now": () => R.toast("Backup saved to Documents\\Runout\\Backups (14.3 MB). Oldest copy removed."),
  dialog: (a, id, v) => { R.S.dialog = { kind: v, id }; R.render(); },
  "cut": (a, id, v) => { R.S.draft[id] = v; R.render(); },
  "apply-confirm": () => { const n = R.studioChanges().length; R.S.dialog = null; R.S.draft = { applied: true }; R.toast(`${n} changes applied to Late night jazz. Undo is available for 30 days.`, () => { R.S.draft = {}; }); },
  "discard": () => { R.S.draft = {}; R.toast("Draft discarded"); },
  "add-backlog": (a, id, v) => { R.toast(`${esc(v)} added to your backlog`); },
  "new-code": () => { D.settings.capture.code = String(Math.floor(100000 + Math.random() * 900000)).replace(/(\d{3})(\d{3})/, "$1 $2"); R.toast("New pairing code. Update it in your iPhone Shortcut."); },
  "do-delete": () => { const v = document.getElementById("del-confirm").value; if (v !== "DELETE") { document.getElementById("del-help").textContent = "Type DELETE in capitals to confirm."; return; } R.S.dialog = null; R.toast("Library deleted. Your backups are untouched."); },
  "do-restore": (a, id, v) => { R.S.dialog = null; R.toast(`Restored the backup from ${esc(v)}. 65 records verified.`); },
  "do-disconnect": () => { R.S.dialog = null; R.S.release = "r2"; R.toast("Spotify disconnected. Everything Runout cached from Spotify is deleted."); },
  "lib-page": (a, id, v) => { R.S.libFilter.page = Number(v); R.render(); },
  "import-go": () => { R.go("#/import/3"); },
};

window.addEventListener("DOMContentLoaded", R.render);

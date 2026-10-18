/* Runout design prototype: dialogs and the design system page. */
"use strict";
(() => {
const { esc } = R;
const D = window.RUNOUT_MOCK;
const S = R.S;
const dlg = (title, body, foot, wide) => `<div class="scrim"><div class="overlay dialog" role="dialog" aria-modal="true" aria-labelledby="dlg-t" style="${wide ? "width:min(760px,100%)" : ""}">
  <div class="between" style="align-items:center"><h1 id="dlg-t" style="font-size:20px;font-weight:800">${title}</h1><button class="icon-btn" data-act="close" aria-label="Close">×</button></div>${body}${foot ? `<div class="row-flex" style="justify-content:flex-end">${foot}</div>` : ""}</div></div>`;
const dots = `<span aria-label="Working" style="letter-spacing:3px">•••</span>`;
// What oEmbed would suggest for the sample links (title only in the MVP; R3 autofill adds more, marked as Spotify's)
const SUGGEST = { "3QJzdZJYIAcoET1GcfpNGi": ["Con Todo El Mundo", "Khruangbin", "album", ["Cómo Me Quieres", "Lady and Man", "Maria También", "August 10", "Evan Finds the Third Room"]],
  short: ["Vulture Prince", "Arooj Aftab", "album", ["Baghon Main", "Mohabbat", "Inayaat", "Last Night", "Diya Hai"]],
  artist: ["Arooj Aftab", "", "artist"] };

R.dialogs = {
  quickAdd: (d) => {
    const t = R.linkType(d.link);
    const manual = d.manual;
    const field = `<div class="field"><label class="label" for="qa-link">Spotify link</label><input id="qa-link" class="input mono" value="${esc(d.link)}" placeholder="Paste an album, track or artist link" ${d.naming ? "readonly" : "autofocus"}>`;
    let state = "", body = "", foot = "";
    const sgEarly = d.suggest || (t && (SUGGEST[t.id] || (t.kind === "short" ? SUGGEST.short : t.kind === "artist" ? SUGGEST.artist : null)));
    const existing = (t && t.id && R.items.find((i) => (i.spotify_uri || "").endsWith(t.id)))
      || (sgEarly && R.items.find((i) => i.display_name === sgEarly[0] && i.type === (sgEarly[2] || "track") && (!sgEarly[1] || i.type === "artist" || i.artist === sgEarly[1])));
    if (!d.link && !manual) {
      state = `<span class="help">Detects albums, tracks and artists. Short links from the iPhone share sheet work too.</span></div>`;
      foot = `<button class="btn btn-ghost" data-act="qa-manual">Add without a link</button><button class="btn btn-primary" disabled>Save</button>`;
    } else if (manual) {
      return dlg("Add without a link", `<div class="field"><label class="label" for="qa-type">Type</label><select id="qa-type" class="input"><option>Album</option><option>Track</option><option>Artist</option></select></div>
        <div class="field"><label class="label" for="qa-name">Name</label><input id="qa-name" class="input" autofocus placeholder="For example, Spirit of Eden"></div>
        <div class="field"><label class="label" for="qa-artist">Artist</label><input id="qa-artist" class="input" placeholder="Optional"></div>
        <input type="hidden" id="qa-prio" value=""><input type="hidden" id="qa-rec" value="">
        <span class="help">You can add the Spotify link later from the record.</span>`, `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-primary" data-act="save-add">Save</button>`);
    } else if (!t) {
      state = `<span class="help err">That isn’t a Spotify link. Paste a link from Share, then Copy link, in Spotify.</span></div>`;
      foot = `<button class="btn btn-secondary" data-act="close">Cancel</button>`;
    } else if (["episode", "show", "audiobook"].includes(t.kind)) {
      state = `<span class="help err">This is a podcast ${t.kind}. Runout keeps albums, tracks and artists.</span></div>`;
      foot = `<button class="btn btn-secondary" data-act="close">Close</button>`;
    } else if (t.kind === "playlist") {
      state = `<span class="help">Playlists aren’t saved as records. Open Playlists to check or tidy one.</span></div>`;
      foot = `<a class="btn btn-primary" href="#/playlists">Open Playlists</a>`;
    } else if (existing) {
      state = `</div><div class="card stack" style="border-color:rgba(245,181,68,0.35)"><span class="accent" style="font-weight:700;font-size:13px">Already saved</span>
        <span>${esc(R.nameOf(existing))} is already in your library, ${R.STATUS[existing.status]} since ${R.day(R.since(existing))}.</span></div>`;
      foot = `<button class="btn btn-secondary" data-act="qa-version">Add a different version</button><a class="btn btn-light" href="#/item/${existing.id}">Open existing</a>`;
    } else if (t.kind === "short" && R.offline()) {
      state = `</div><div class="card stack"><span class="text-2" style="font-weight:700;font-size:13px">Offline</span><span>You’re offline, so the short link can’t be opened yet. It’s saved and will be filled in when you’re back online.</span></div>`;
      foot = `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-primary" data-act="save-later">Save for later</button>`;
    } else if (S.view === "loading") {
      state = `</div><div class="row-flex">${dots}<span class="muted">Getting the title from Spotify…</span></div>`;
      foot = `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-primary" disabled>Save</button>`;
    } else {
      const sg = sgEarly || [d.capture ? "Lanquidity" : "Untitled", d.capture ? "Sun Ra" : "", t.kind];
      const kind = sg[2];
      const r3 = R.isR3() && !d.naming;
      state = `<span class="help">${t.kind === "short" ? `Short link opened: ${R.an(kind)}.` : `Detected ${R.an(kind)}.`} Not in your library yet.</span></div>`;
      const nameField = `<div class="field"><label class="label" for="qa-name">Name</label><input id="qa-name" class="input" value="${esc(sg[0])}" autofocus>
        <span class="help">Suggested by Spotify. Edit it if you like; it’s saved as yours.</span></div>`;
      const artistField = kind === "artist" ? `<div class="field"><label class="label" for="qa-notes">Why this artist</label><textarea id="qa-notes" class="input" rows="2" placeholder="Who sent you here, what to start with"></textarea></div><input type="hidden" id="qa-artist" value="">`
        : `<div class="field"><label class="label" for="qa-artist">Artist</label><input id="qa-artist" class="input" ${r3 ? `value="${esc(sg[1])}"` : 'placeholder="Type the artist (optional)"'}>${r3 ? `<span class="help"><span class="badge need">From Spotify</span> Saved only if you keep it.</span>` : ""}</div>`;
      const auto = r3 && sg[3] ? `<div class="card stack" style="padding:14px"><div class="between"><span class="label">Track list from Spotify</span><span class="row-flex">${R.logo()}</span></div>
          ${sg[3].map((x, n) => `<label class="row-flex" style="font-size:13px"><input type="checkbox" ${n < 3 ? "checked" : ""}>${esc(x)}</label>`).join("")}<span class="help">Ticked tracks are added to your library with the album. The rest stays with Spotify.</span></div>` : "";
      body = `<div style="display:flex;gap:16px;align-items:flex-start"><div style="flex:0 0 152px">${R.embed(152)}</div><div class="stack" style="flex:1 1 auto;min-width:0">${nameField}${artistField}</div></div>${auto}
        <div class="tiles" style="grid-template-columns:repeat(2,minmax(0,1fr))"><div class="field"><label class="label" for="qa-prio">Priority</label><select id="qa-prio" class="input"><option value="">None</option><option value="high">High</option><option value="medium">Medium</option><option value="low">Low</option></select></div>
        <div class="field"><label class="label" for="qa-rec">Recommended by</label><input id="qa-rec" class="input" placeholder="Optional"></div></div>
        <span class="help">Saved as Planned. Only the link is required. <span class="kbd">Ctrl+N</span> opens this from anywhere.</span>`;
      foot = `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-primary" data-act="save-add">Save</button>`;
    }
    return dlg(d.naming ? "Name a phone capture" : "Add to backlog", field + state + body, foot);
  },
  restore: (d) => {
    const step = d.step || "pick";
    if (step === "verify") return dlg("Restore backup", `<div class="row-flex">${dots}<span>Checking the backup from ${esc(d.v)}…</span></div><span class="help">Nothing changes until the check passes.</span>`, "");
    if (step === "fail") return dlg("Restore backup", R.error("This backup didn’t pass the check.", "The file is damaged. Your current library is unchanged. Try an older backup."), `<button class="btn btn-secondary" data-act="dialog" data-v="restore">Pick another</button>`);
    return dlg("Restore a backup", `<span class="text-2">Your current library is kept as a copy until Runout starts again.</span>
      <div class="card pad-s"><div class="list">${D.settings.backups.map((b, i) => `<div class="row" style="grid-template-columns:minmax(0,1fr) 70px auto"><span>${R.day(b.at)}, ${R.time(b.at)}${i === 0 ? " (latest)" : ""}</span><span class="mono muted" style="font-size:12px">${b.mb} MB</span>
        <button class="btn btn-secondary s" data-act="restore-pick" data-v="${R.day(b.at)}" aria-label="Restore the backup from ${R.day(b.at)}">Restore</button></div>`).join("")}</div></div>`, `<button class="btn btn-secondary" data-act="close">Cancel</button>`);
  },
  deleteItem: (d) => { const it = R.byId(d.id); return dlg(`Delete ${esc(R.nameOf(it))}?`, `<span class="text-2">Its rating, review, notes, tags and history are deleted. Plays stay in your listening history.</span>`,
    `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-danger" data-act="do-delete-item" data-id="${it.id}">Delete</button>`); },
  deleteLib: () => dlg("Delete your whole library?", `<span class="text-2">Every record, note, tag and play on this PC is removed. Backups in Documents\\Runout\\Backups are kept, so you can restore later.</span>
    <div class="field"><label class="label" for="del-confirm">Type DELETE to confirm</label><input id="del-confirm" class="input" autofocus autocomplete="off"><span class="help err" id="del-help"></span></div>`,
    `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-danger" data-act="do-delete">Delete library</button>`),
  shortcut: () => dlg("Set up the iPhone Shortcut", `<ol style="margin:0;padding-left:18px;display:flex;flex-direction:column;gap:10px;line-height:1.5">
      <li>On your iPhone, open <b>Shortcuts</b> and create a new shortcut named <b>Save to Runout</b>. Turn on <b>Show in Share Sheet</b> and accept <b>URLs</b>.</li>
      <li>Add <b>Get Contents of URL</b>: URL <span class="mono">http://${esc(D.settings.capture.address)}/capture</span>, method <b>POST</b>, body <b>JSON</b> with <span class="mono">code</span> = <span class="mono">${esc(D.settings.capture.code)}</span> and <span class="mono">link</span> = Shortcut Input.</li>
      <li>Add <b>Show Result</b>.</li>
      <li>Test it: in Spotify, tap Share, then Save to Runout, on the same Wi-Fi as this PC. Allow <b>Local Network</b> when iOS asks.</li></ol>
    <div class="banner"><span class="text-2">Waiting for a test share…</span>${dots}</div><span class="help">If nothing arrives: check your Wi-Fi is set to Private in Windows, and that Windows Firewall allowed Runout.</span>`,
    `<button class="btn btn-secondary" data-act="close">Done</button>`, true),
  connect: (d) => {
    const step = d.step || "start";
    if (step === "wait") return dlg("Connect Spotify", `<div class="row-flex">${dots}<span>Waiting for you to approve in your browser…</span></div><span class="help">Spotify’s page lists what Runout may do. Check it says Runout, then click Agree.</span>`,
      `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-ghost" data-act="connect-step" data-v="fail">Show failure</button><button class="btn btn-primary" data-act="connect-step" data-v="done">Show success</button>`);
    if (step === "fail") return dlg("Couldn’t connect", R.error("Spotify said no.", "Developer access needs an active Premium plan on the app owner’s account, and your account added under User Management."), `<button class="btn btn-secondary" data-act="close">Close</button><button class="btn btn-primary" data-act="connect-step" data-v="wait">Try again</button>`);
    if (step === "done") return dlg("Spotify connected", `<span>Playlist Studio, autofill, Saved in Spotify and From Spotify are ready. Spotify will ask you to sign in again in 6 months.</span>`, `<button class="btn btn-primary" data-act="connect-done">Done</button>`);
    return dlg("Connect Spotify", `<div class="row-flex">${R.logo()}</div><span class="text-2" style="line-height:1.55">Runout opens Spotify’s sign-in page in your browser. It asks to read and edit your playlists, read your top items and recent plays, and save to your library.</span>
      <span class="help">Nothing Runout gets from Spotify feeds your insights. Disconnecting deletes it right away.</span>`, `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-primary" data-act="connect-step" data-v="wait">Open Spotify sign-in</button>`);
  },
  disconnect: () => dlg("Disconnect Spotify?", `<span class="text-2">Runout deletes everything it cached from Spotify and any undo snapshots for playlists. Your library, ratings and listening history stay.</span>`,
    `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-danger" data-act="do-disconnect">Disconnect</button>`),
  apply: () => { const ch = R.studioChanges(); return dlg(`Apply changes to ${esc(R.studioName || "this playlist")}?`, `<span class="text-2">Runout reads the playlist from Spotify again first. If it changed since you started, nothing is applied.</span>
    <div class="card pad-s"><div class="list">${ch.map((t) => `<div class="row" style="grid-template-columns:130px minmax(0,1fr)"><span class="accent" style="font-size:12px;font-weight:700">${t.duplicate_of ? "Remove duplicate" : "Remove"}</span><span>${esc(t.track)}, ${esc(t.artist)}</span></div>`).join("")}</div></div>
    <span class="help">You can undo this for 30 days.</span>`, `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-primary" data-act="apply-confirm">Apply ${ch.length} changes</button>`); },
  makePlaylist: (d) => {
    const shelf = R.shelves().find((s) => s[0] === d.id);
    const filt = D.saved_filters.find((f) => f.name === d.id);
    const items = (shelf ? shelf[2] : R.items.filter((i) => i.type !== "artist" && (!filt || ((filt.definition.tags || []).every((t) => i.tags.includes(t)) && (i.rating || 0) >= (filt.definition.min_rating || 0) && (!filt.definition.status || i.status === filt.definition.status))))).slice(0, 50);
    return dlg(`New Spotify playlist from “${esc(d.id)}”`, `<div class="field"><label class="label" for="mp-name">Playlist name</label><input id="mp-name" class="input" value="${esc(d.id)}" autofocus></div>
      <span class="text-2">${R.pl(items.length, "record")} from this ${shelf ? "shelf" : "filter"}. Albums add all their tracks.</span><div class="card pad-s" style="max-height:220px;overflow:auto"><div class="list">${items.map((i) => `<div class="row" style="grid-template-columns:minmax(0,1fr) 70px"><span>${esc(R.nameOf(i))}<span class="muted">, ${esc(i.artist)}</span></span><span class="muted" style="font-size:12px">${R.TYPE[i.type]}</span></div>`).join("")}</div></div>
      <label class="row-flex"><input type="checkbox" checked>Private playlist</label><span class="row-flex">${R.logo()}<span class="help">Created on your Spotify account.</span></span>`,
      `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-primary" data-act="mp-confirm">Create playlist</button>`); },
  newShelf: () => dlg("New shelf", `<div class="field"><label class="label" for="ns-name">Shelf name</label><input id="ns-name" class="input" autofocus placeholder="For example, Rainy evenings"></div>
    <div class="tiles" style="grid-template-columns:repeat(3,minmax(0,1fr))"><div class="field"><label class="label" for="ns-tag">Tag</label><select id="ns-tag" class="input"><option>late-night</option><option>jazz</option></select></div>
    <div class="field"><label class="label" for="ns-r">Rating</label><select id="ns-r" class="input"><option>Any</option><option>4 and up</option></select></div><div class="field"><label class="label" for="ns-s">Status</label><select id="ns-s" class="input"><option>Explored</option><option>Any</option></select></div></div>
    <span class="help">Shelves use only your own records. 7 records match right now.</span>`, `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-primary" data-act="close-toast" data-v="Shelf added to Rediscover">Add shelf</button>`),
  saveFilter: () => dlg("Save this filter", `<div class="field"><label class="label" for="sf-name">Name</label><input id="sf-name" class="input" autofocus value="Explored, 4 and up"></div><span class="help">Saved filters appear in Rediscover and can become a Spotify playlist${R.isR3() ? "" : " in Release 3"}.</span>`,
    `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-primary" data-act="close-toast" data-v="Filter saved">Save</button>`),
  tagRename: (d) => dlg(`Rename “${esc(d.id)}”`, `<div class="field"><label class="label" for="tr">New name</label><input id="tr" class="input" autofocus value="${esc(d.id)}"></div>`, `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-primary" data-act="close-toast" data-v="Tag renamed">Rename</button>`),
  tagMerge: (d) => { const n = R.items.filter((i) => i.tags.includes(d.id)).length; return dlg(`Merge “${esc(d.id)}” into another tag`, `<div class="field"><label class="label" for="tm">Merge into</label><select id="tm" class="input">${[...new Set(R.items.flatMap((i) => i.tags))].filter((t) => t !== d.id).map((t) => `<option>${esc(t)}</option>`).join("")}</select></div>
    <span class="help">${R.pl(n, "record")} move to the chosen tag; records that already have both keep it once.</span>`, `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-primary" data-act="close-toast" data-v="Tags merged">Merge</button>`); },
  tagDelete: (d) => { const n = R.items.filter((i) => i.tags.includes(d.id)).length; return dlg(`Delete the tag “${esc(d.id)}”?`, `<span class="text-2">It’s removed from ${R.pl(n, "record")}. The records stay.</span>`,
    `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-danger" data-act="do-tag-delete" data-id="${esc(d.id)}">Delete tag</button>`); },
  autofill: () => dlg("Fill the track list from Spotify", `<div class="row-flex">${R.logo()}<span class="text-2">From Spotify. Tick the tracks you want to keep in your library.</span></div>
    ${["Movement 1", "Movement 2", "Movement 3"].map((t) => `<label class="row-flex"><input type="checkbox" checked>${t}</label>`).join("")}<span class="help">Unticked tracks aren’t stored. Kept tracks become your records.</span>`,
    `<button class="btn btn-secondary" data-act="close">Cancel</button><button class="btn btn-primary" data-act="close-toast" data-v="Tracks added">Add tracks</button>`),
};

Object.assign(R.actions, {
  "do-tag-delete": (a, id) => { const had = R.items.filter((i) => i.tags.includes(id)); had.forEach((i) => (i.tags = i.tags.filter((t) => t !== id))); S.dialog = null; R.toast(`Tag ${esc(id)} removed from ${R.pl(had.length, "record")}`, () => had.forEach((i) => i.tags.push(id))); },
  "qa-manual": () => { S.dialog = { kind: "quickAdd", link: "", manual: true }; R.render(); },
  "qa-version": () => { const d = S.dialog; S.dialog = { kind: "quickAdd", link: "https://open.spotify.com/album/5Xq1Hx0Pu7Hn3Kp9Lm2Qa1", suggest: [(d.suggest ? d.suggest[0] : "Promises") + " (Deluxe)", d.suggest ? d.suggest[1] : "Floating Points", "album"], version: true }; R.render(); },
  "restore-pick": (a, id, v) => { S.dialog = { kind: "restore", step: "verify", v }; R.render(); setTimeout(() => { if (S.dialog && S.dialog.kind === "restore") { if (S.view === "error") { S.dialog = { kind: "restore", step: "fail" }; R.render(); } else R.actions["do-restore"](a, id, v); } }, 1400); },
  "do-delete-item": (a, id) => { const it = R.byId(id); R.items = R.items.filter((x) => x !== it); S.dialog = null; R.go("#/library"); R.toast(`${esc(R.nameOf(it))} deleted`, () => R.items.push(it)); },
  "connect-step": (a, id, v) => { S.dialog = { kind: "connect", step: v }; R.render(); },
  "connect-done": () => { S.dialog = null; S.release = "r3"; if (S.view === "empty") S.view = "normal"; R.render(); },
  "mp-confirm": () => { S.dialog = null; R.toast("Playlist created on Spotify"); },
  "close-toast": (a, id, v) => { S.dialog = null; R.toast(esc(v)); },
});
document.addEventListener("mousedown", (e) => { if (e.target.classList && e.target.classList.contains("scrim")) { S.dialog = null; R.render(); } });

/* ---------------------------------------------------------------- design system page */
R.systemScreen = () => {
  const sw = [["--bg", "#101113", "Page canvas"], ["--rail", "#0B0C0E", "Sidebar"], ["--surface", "#17191C", "Cards, inputs"], ["--surface-2", "#1F2226", "Hover, selected"], ["--line", "#23262B", "Dividers"],
    ["--line-strong", "#3A3F46", "Control borders"], ["--text", "#ECEDEF", "Primary text, 16.1:1"], ["--text-2", "#C3C7CE", "Secondary, 11.1:1"], ["--muted", "#A9AEB7", "Labels, 8.5:1"], ["--faint", "#9A9FA8", "Ranks, times, 7.1:1"],
    ["--accent", "#F5B544", "Needs you, 10.4:1"], ["--link", "#F5C46B", "Links, 11.7:1"], ["--bar", "#4A5059", "Chart bars"], ["--danger", "#FF7A6B", "Destructive, 7.4:1"], ["--accent-ink", "#101113", "Text on accent, 10.4:1"]];
  const states = ["Default", "Hover", "Pressed", "Focus", "Disabled", "Loading"];
  const btn = (cls, label) => states.map((s) => `<span class="btn ${cls}" style="${s === "Hover" ? "filter:brightness(1.1)" : s === "Pressed" ? "transform:scale(0.98);filter:brightness(0.92)" : s === "Focus" ? "outline:2px solid var(--accent);outline-offset:2px" : s === "Disabled" ? "opacity:0.4" : ""}">${s === "Loading" ? `<span style="letter-spacing:3px">•••</span>` : label}</span>`).join("");
  return `<div class="app"><main class="main" style="margin:0 auto;max-width:1240px;gap:44px;padding-top:40px">
  <header class="stack" style="gap:8px"><a href="#/home" style="font-size:13px;font-weight:600">Back to Runout</a><h1 style="font-size:40px;font-weight:800;letter-spacing:-0.03em">Runout design system</h1>
    <p class="text-2" style="margin:0;font-size:16px;max-width:720px">Direction D · Pulse, v1.1. Graphite layers, one amber accent, mono numbers. Everything on this page is live: hover, press and tab through it. Source: docs/DESIGN.md.</p></header>
  <section class="stack"><h2 class="section-title">Color</h2><div class="tiles" style="grid-template-columns:repeat(5,minmax(0,1fr))">${sw.map(([n, h, u]) => `<div class="card stack" style="padding:10px;gap:6px"><div style="height:56px;border-radius:10px;background:${h};border:1px solid rgba(255,255,255,0.08)"></div><span class="mono" style="font-size:12px;font-weight:600">${n}</span><span class="mono muted" style="font-size:12px">${h}</span><span class="text-2" style="font-size:12px">${u}</span></div>`).join("")}</div></section>
  <div class="cols"><section class="col-main"><h2 class="section-title">Type</h2><div class="card stack">${[["32 / 800", "font-size:32px;font-weight:800;letter-spacing:-0.025em", "Promises"], ["26 / 800", "font-size:26px;font-weight:800", "Backlog"], ["20 / 800", "font-size:20px;font-weight:800", "Top artists"], ["15 / 600", "font-size:15px;font-weight:600", "Journey in Satchidananda"], ["14 / 400", "color:var(--text-2)", "Recommended by Meera."], ["12 / 400", "font-size:12px;color:var(--muted)", "Desktop plays: live"], ["Mono 26 / 600", "font-family:var(--mono);font-size:26px;font-weight:600", "6,519 plays"]].map(([s, st, t]) => `<div style="display:grid;grid-template-columns:120px minmax(0,1fr);gap:16px;align-items:baseline"><span class="mono faint" style="font-size:12px">${s}</span><span style="${st}">${t}</span></div>`).join("")}</div></section>
    <section class="col-side"><h2 class="section-title">Depth and shape</h2><div class="tiles" style="grid-template-columns:repeat(3,minmax(0,1fr))"><div style="height:110px;border-radius:16px;border:1px dashed var(--line-2);padding:12px;display:flex;align-items:flex-end"><b>Level 0</b></div><div class="card hover" style="height:110px;display:flex;align-items:flex-end"><b>Level 1</b></div><div class="overlay" style="height:110px;padding:12px;display:flex;align-items:flex-end"><b>Level 2</b></div></div>
      <div class="card row-flex" style="gap:24px"><span class="muted" style="font-size:12px">Radius 6, 10, 16</span><span style="width:36px;height:36px;border-radius:6px;background:var(--line-2)"></span><span style="width:48px;height:36px;border-radius:10px;background:var(--line-2)"></span><span style="width:64px;height:48px;border-radius:16px;background:var(--line-2)"></span></div>
      <div class="card stack"><span class="label">Keyboard</span>${[["Ctrl+N", "Quick add from anywhere"], ["Ctrl+K", "Search"], ["1 to 5", "Set status on an item page"], ["Arrow keys", "Rating, by half a star"], ["Esc", "Close a dialog"]].map(([k, t]) => `<span class="between"><span>${t}</span><span class="kbd">${k}</span></span>`).join("")}</div></section></div>
  <section class="stack"><h2 class="section-title">Buttons</h2><div class="card" style="display:grid;grid-template-columns:110px repeat(6,minmax(0,1fr));gap:14px 10px;align-items:center"><span></span>${states.map((s) => `<span class="muted" style="font-size:12px;font-weight:600">${s}</span>`).join("")}
    <span class="text-2">Primary</span>${btn("btn-primary", "Save")}<span class="text-2">Secondary</span>${btn("btn-secondary", "Add a note")}<span class="text-2">Light</span>${btn("btn-light", "Rate it")}<span class="text-2">Danger</span>${btn("btn-danger", "Delete")}</div>
    <div class="card row-flex"><span class="muted" style="width:110px">Live</span><button class="btn btn-primary">Save</button><button class="btn btn-secondary">Add a note</button><button class="btn btn-light">Rate it</button><button class="btn btn-danger">Delete</button><button class="btn btn-ghost">Open Spotify</button><button class="btn btn-primary" disabled>Disabled</button></div></section>
  <div class="cols"><section class="col-main"><h2 class="section-title">Inputs</h2><div class="card stack">
      <div class="field"><label class="label" for="ds-1">Live field</label><input id="ds-1" class="input" placeholder="Search your library, or paste a Spotify link"></div>
      <div class="field"><label class="label" for="ds-2">Error</label><input id="ds-2" class="input error" value="open.spotify.com/episode/4rOoJ6Egrf8K2IrywzwOMk"><span class="help err">Podcast episodes can’t be saved.</span></div>
      <div class="field" style="opacity:0.4"><label class="label" for="ds-3">Disabled</label><input id="ds-3" class="input" disabled value="Connect Spotify to use autofill"></div></div></section>
    <section class="col-side"><h2 class="section-title">Selection</h2><div class="card stack"><span class="label">Status</span><div class="seg" role="radiogroup" aria-label="Example status">${Object.values(R.STATUS).map((t, i) => `<button role="radio" aria-checked="${i === 1}">${t}</button>`).join("")}</div>
      <span class="label">Rating</span>${`<div class="rating" role="slider" tabindex="0" aria-label="Example rating" aria-valuemin="0" aria-valuemax="5" aria-valuenow="3.5"><button tabindex="-1" class="on">★</button><button tabindex="-1" class="on">★</button><button tabindex="-1" class="on">★</button><button tabindex="-1" class="half">★</button><button tabindex="-1">★</button></div>`}
      <span class="label">Tags and badges</span><div class="row-flex"><span class="chip">late-night</span><span class="chip on">jazz (filter on)</span><span class="chip add">Add tag</span><span class="badge need">Name needed</span><span class="badge status">Revisit</span><span class="row-flex accent" style="font-size:13px;font-weight:600"><span class="live"></span>Playing now</span></div></div></section></div>
  <section class="stack"><h2 class="section-title">Data and feedback</h2><div class="cols"><div class="col-main" style="flex:1 1 320px"><div class="stats">${R.stat("Listening time", "27h 40m", "3h 10m more")}${R.stat("Unrated plays", "86", "Rate what you keep coming back to", true)}</div>
      <div class="card pad-s"><div class="list"><div class="row" style="grid-template-columns:minmax(0,1fr) auto 50px"><span class="stack" style="gap:2px"><b>Movement 6</b><span class="muted" style="font-size:12px">Floating Points</span></span><span class="stars">★★★★★</span><span class="mono faint" style="font-size:12px">18 min</span></div>
      <div class="row" style="grid-template-columns:minmax(0,1fr) auto 50px"><span class="stack" style="gap:2px"><b>Teardrop</b><span class="muted" style="font-size:12px">Massive Attack</span></span><button class="btn btn-secondary s">Rate</button><span class="mono faint" style="font-size:12px">3 h</span></div>
      <div class="row" style="grid-template-columns:minmax(0,1fr) auto 50px"><span class="stack" style="gap:2px"><b style="color:var(--faint)">Glow</b><span class="muted" style="font-size:12px">Kelly Lee Owens</span></span><span class="faint" style="font-size:12px">Skipped, not counted</span><span class="mono faint" style="font-size:12px">4 h</span></div></div></div></div>
    <div class="col-side"><div class="row-flex" style="align-items:stretch">${`<div style="flex:1 1 140px">${`<div class="embed" style="height:140px">Spotify embed, online</div>`}</div><div style="flex:1 1 140px"><div class="embed off" style="height:140px"><b class="text-2">Offline</b>Artwork returns when you reconnect</div></div>`}</div>
      <div class="card row-flex">${R.logo()}<span class="text-2" style="font-size:13px">Attribution next to Spotify content. ${R.openSpotify()}</span></div>
      <div class="overlay row-flex" style="padding:12px 14px;justify-content:space-between"><span>Thembi saved to your backlog</span><button class="btn btn-secondary s">Undo</button></div>
      ${R.empty("Nothing waiting", "Paste a Spotify link, or share one from your phone.", `<button class="btn btn-primary">Add a link</button>`)}
      ${R.skelRows(3)}</div></div></section></main></div>`;
};
})();

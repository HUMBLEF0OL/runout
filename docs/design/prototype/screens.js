/* Runout design prototype: screens SC-01..SC-13 plus first run, search, tray, uninstall.
   Every screen reads the shared mock data (R.items, window.RUNOUT_MOCK) and honours R.S.view
   (normal | empty | loading | offline | error). Artwork appears only as Spotify embed slots. */
"use strict";
(() => {
const { esc } = R;
const D = window.RUNOUT_MOCK;
const L = D.listening;
const S = R.S;
const v = () => S.view;
const backlog = () => R.items.filter((i) => i.in_backlog);
const SRC = { U: "Your records", S: "Desktop log", E: "Spotify export" };
const src = (...k) => `<span class="faint" style="font-size:12px">From: ${k.map((x) => SRC[x]).join(" + ")}.</span>`;
const pick = (arr, n) => arr.slice(0, n);
const named = (verb, it) => `${verb} ${esc(R.nameOf(it))}`;
const pl = R.pl;
S.art = S.art || false;
S.range = S.range || "7d";
const NEW_ALBUM = ["https://open.spotify.com/album/3QJzdZJYIAcoET1GcfpNGi", "Con Todo El Mundo", "Khruangbin", "album"];

/* ----- reusable bits */
const artToggle = () => `<button class="btn btn-secondary" data-act="art" aria-pressed="${S.art}">${S.art ? "Hide artwork" : "Show artwork"}</button>`;
const setFooter = (shown, total, name) => `<div class="row-flex" style="justify-content:space-between">
  <span class="row-flex">${R.logo()}${R.openSpotify(name)}</span>${total ? `<span class="mono faint" style="font-size:12px">${shown} of ${total}</span>` : ""}</div>`;
const embedTile = (it, meta, act) => `<div class="tile">
  <a href="#/item/${it.id}" style="color:inherit;display:flex;flex-direction:column;gap:6px" aria-label="Open ${esc(R.nameOf(it))}">
    ${R.offline() ? `<div class="embed off" style="aspect-ratio:1/1">Offline</div>` : `<div class="embed art" style="aspect-ratio:1/1;background:${it.tint}">Spotify embed</div>`}
    <span class="t">${esc(R.nameOf(it))}</span><span class="s">${esc(it.type === "artist" ? "Artist" : it.artist)}</span></a>
  ${meta ? `<span class="m ${act ? "act" : ""}">${meta}</span>` : ""}</div>`;
const textCard = (it, meta, act) => `<a class="card hover" href="#/item/${it.id}" style="display:flex;flex-direction:column;gap:6px;padding:14px;color:var(--text)">
  <span class="mono faint" style="font-size:11px">${R.TYPE[it.type]}</span>
  <span style="font-weight:700;line-height:1.25">${esc(R.nameOf(it))}</span>
  <span class="muted" style="font-size:12px">${esc(it.type === "artist" ? (it.notes || "Artist") : it.artist)}</span>
  ${meta ? `<span class="mono ${act ? "accent" : "muted"}" style="font-size:12px">${meta}</span>` : ""}
  ${it.revisit ? `<span class="badge status" style="align-self:flex-start">Revisit</span>` : ""}</a>`;
const unsavedCard = (name, artist, meta, link) => `<button class="card hover" ${R.addAttrs(link, name, artist, "album")} aria-label="Save ${esc(name)} to your backlog" style="display:flex;flex-direction:column;gap:6px;padding:14px;color:var(--text);text-align:left;cursor:pointer">
  <span class="mono faint" style="font-size:11px">Album, not saved</span><span style="font-weight:700;line-height:1.25">${esc(name)}</span><span class="muted" style="font-size:12px">${esc(artist)}</span>
  <span class="mono accent" style="font-size:12px">${meta}</span></button>`;
const itemSet = (list, metaFn, total, setName) => {
  if (!list.length) return "";
  const shown = pick(list, 20);
  return S.art
    ? `<div class="tiles">${shown.map((it) => embedTile(it, ...metaFn(it))).join("")}</div>${setFooter(shown.length, total ?? list.length, setName)}`
    : `<div class="tiles">${shown.map((it) => textCard(it, ...metaFn(it))).join("")}</div>${setFooter(shown.length, total > shown.length ? total : 0, setName)}`;
};
const statusSeg = (it) => `<div class="seg" role="radiogroup" aria-label="Status of ${esc(R.nameOf(it))}">${Object.entries(R.STATUS).map(([k, t]) =>
  `<button type="button" role="radio" aria-checked="${it.status === k}" data-act="status" data-id="${it.id}" data-v="${k}">${t}</button>`).join("")}</div>`;
const rating = (it, size = 26) => `<div class="rating" role="slider" tabindex="0" aria-label="Your rating for ${esc(R.nameOf(it))}" aria-valuemin="0" aria-valuemax="5" aria-valuenow="${it.rating || 0}"
  aria-valuetext="${it.rating ? it.rating + " of 5 stars" : "Not rated"}" data-rating="${it.id}">${[1, 2, 3, 4, 5].map((n) => {
  const cls = it.rating >= n ? "on" : it.rating >= n - 0.5 ? "half" : "";
  return `<button type="button" tabindex="-1" class="${cls}" style="font-size:${size}px" data-act="rate" data-id="${it.id}" data-v="${n}" aria-hidden="true">★</button>`;
}).join("")}</div>`;
const ratingOf = (e) => { const it = e.item_id ? R.byId(e.item_id) : null; return it && it.rating ? it : null; };
const stars = (it) => `<span class="stars nowrap" aria-label="${it.type === "album" ? "Album rated" : "Rated"} ${it.rating} of 5">${R.stars(it.rating)}${it.type === "album" ? `<span class="faint" style="font-size:11px;letter-spacing:0"> album</span>` : ""}</span>`;
const statusRow = (it) => `<div class="row" style="grid-template-columns:16px minmax(0,1fr) 70px 90px 80px 104px">
  <input type="checkbox" data-act="sel" data-id="${it.id}" aria-label="Select ${esc(R.nameOf(it))}" ${S.sel.has(it.id) ? "checked" : ""}>
  <a href="#/item/${it.id}" style="color:inherit;min-width:0" class="stack"><span class="ellipsis" style="font-weight:600">${esc(R.nameOf(it))}</span><span class="ellipsis muted" style="font-size:12px">${esc(R.sub(it))}</span></a>
  <span class="text-2" style="font-size:13px">${R.TYPE[it.type]}</span>
  <span class="text-2" style="font-size:13px">${esc(it.recommended_by || (it.via_phone ? "Phone" : "You"))}</span>
  <span class="mono faint" style="font-size:12px">${R.age(it.added_at)}</span>
  <span style="justify-self:end">${it.status === "in_progress" ? `<span class="badge status">In progress</span>` : `<button class="btn btn-secondary s" data-act="start" data-id="${it.id}" aria-label="${named("Start", it)}">Start</button>`}</span></div>`;
const stateSkel = (n = 6) => R.skelRows(n);
const errorFor = (what) => R.error(`Couldn’t load ${what}.`, "The library file is busy. Nothing was lost; try again in a moment.", `<button class="btn btn-secondary s" data-act="toast" data-v="Loaded">Try again</button>`);

/* ================================================================ SC-01 / SC-02 Home */
const homePulse = () => {
  const st = L.stats[S.range]; const ch = L.charts[S.range];
  const now = L.recent.find((e) => e.now_playing);
  const nowTrack = R.byName("track", now.track); const nowAlbum = R.byName("album", now.album);
  const ranges = [["7d", "7 days"], ["30d", "30 days"], ["12m", "12 months"], ["all", "All time"]];
  const rangeSeg = `<div class="seg" role="group" aria-label="Time range">${ranges.map(([k, t]) => `<button type="button" aria-pressed="${S.range === k}" data-act="range" data-v="${k}">${t}</button>`).join("")}</div>`;
  if (v() === "empty") return R.shell("home", R.topbar() + R.empty("No listening data yet",
    "Runout learns your listening from two places: the desktop logger, which records what Spotify plays on this PC, and your Spotify history export, which covers every device.",
    `<div class="row-flex"><a class="btn btn-primary" href="#/import/1">Import your Spotify history</a><button class="btn btn-secondary" data-act="pause" data-v="resume">Turn on the desktop logger</button></div>`) + homeMvpBody());
  const nowCard = v() === "loading" ? `<div class="card" style="flex:999 1 520px;height:172px"><div class="skel" style="height:100%"></div></div>`
    : v() === "error" ? `<div class="card" style="flex:999 1 520px">${R.error("Desktop logger stopped.", "Windows isn’t sharing what Spotify plays right now. Everything else works.", `<button class="btn btn-secondary s" data-act="toast" data-v="Retrying the desktop logger">Try again</button>`)}</div>`
    : S.paused ? `<div class="card" style="flex:999 1 520px;display:flex;flex-direction:column;gap:10px;justify-content:center">
        <h2 style="font-size:13px;font-weight:600" class="muted">Desktop logger paused</h2><span class="text-2">Nothing is being logged on this PC until you resume.</span>
        <div><button class="btn btn-light" data-act="pause" data-v="resume">Resume logging</button></div></div>`
    : `<div class="card now" style="flex:999 1 520px;display:flex;flex-wrap:wrap;gap:20px;align-items:center;--glow:rgba(92,70,48,0.6)">
      <div style="flex:0 0 132px">${R.embed(132)}</div>
      <div style="flex:1 1 260px;min-width:0" class="stack">
        <h2 style="font-size:13px;font-weight:600;display:flex;align-items:center;gap:8px" class="accent"><span class="live" aria-hidden="true"></span>Playing now on your PC</h2>
        <span style="font-size:26px;font-weight:800;letter-spacing:-0.02em;line-height:1.1">${esc(now.track)}</span>
        <span class="text-2">${esc(now.artist)}, from ${esc(now.album)}. ${nowTrack ? "The track is in your backlog." : "Not in your library yet."}</span>
        <span class="mono faint" style="font-size:12px">${R.mmss(now.ms)} of ${R.mmss(now.duration_ms)}, counted as a play after 0:30</span>
        <div class="row-flex">${nowTrack ? `<a class="btn btn-light" href="#/item/${nowTrack.id}">Rate it</a><a class="btn btn-secondary" href="#/item/${nowTrack.id}">Add a note</a>`
          : `<button class="btn btn-light" ${R.addAttrs("https://open.spotify.com/track/" + now.spotify_uri.split(":")[2], now.track, now.artist, "track")}>Save and rate</button>`}
          ${nowAlbum ? `<a class="btn btn-secondary" href="#/item/${nowAlbum.id}">Open ${esc(now.album)}</a>` : `<button class="btn btn-secondary" ${R.addAttrs(NEW_ALBUM[0], now.album, now.artist, "album")}>Save the album to backlog</button>`}${R.openSpotify(now.track)}</div></div></div>`;
  const statTiles = v() === "loading" ? `<div class="stats" style="flex:1 1 300px">${[1, 2, 3, 4].map(() => `<div class="card" style="height:96px"><div class="skel" style="height:100%"></div></div>`).join("")}</div>`
    : `<div class="stats" style="flex:1 1 300px;grid-template-columns:repeat(2,minmax(0,1fr))">
      ${R.stat("Plays", R.num(st.plays), `${st.plays >= st.plays_prev ? R.num(st.plays - st.plays_prev) + " more" : R.num(st.plays_prev - st.plays) + " fewer"} than the period before`)}
      ${R.stat("Listening time", R.hm(st.ms), `${R.hm(Math.abs(st.ms - st.ms_prev))} ${st.ms >= st.ms_prev ? "more" : "less"} than before`)}
      ${R.stat("New artists", st.new_artists, "First heard in this period")}
      ${R.stat("Plays you haven’t rated", R.num(st.unrated_plays), "Rate what you keep coming back to", true)}</div>`;
  const maxHours = Math.max(...ch.artists.map((a) => a.hours));
  const hrs = (x) => `${x.hours.toFixed(1)} h`; // charts rank by listening time (CAP-23)
  const artists = pick(ch.artists, 6).map((a, n) => {
    const it = R.byName("artist", a.name);
    return `<div class="rank"><span class="mono faint" style="font-size:12px">${n + 1}</span>
      <div style="min-width:0"><span style="font-weight:600">${it ? `<a href="#/item/${it.id}" style="color:inherit">${esc(a.name)}</a>` : esc(a.name)}</span><span class="muted">${it ? `, ${R.STATUS[it.status].toLowerCase()}` : ", not in your library"}</span>
      <div class="bar ${it ? "" : "hot"}" style="width:${Math.round((a.hours / maxHours) * 100)}%"></div></div>
      <span class="mono text-2 nowrap" style="font-size:12px;text-align:right">${hrs(a)}</span></div>`;
  }).join("");
  const albums = pick(ch.albums, 5).map((a) => {
    const it = R.byName("album", a.name);
    if (!it) return unsavedCard(a.name, a.by, hrs(a), NEW_ALBUM[0]);
    return S.art ? embedTile(it, R.statusMeta(it), false) : textCard(it, `${R.statusMeta(it)}, ${hrs(a)}`, it.status === "in_progress");
  }).join("");
  const recent = pick(L.recent, 6).map((e) => playRow(e)).join("");
  const takes = ["loved_not_played", "played_not_rated", "backlog_never_played"].map((k) => L.takes.find((t) => t.kind === k)).filter(Boolean).map((t) => {
    const it = R.byId(t.item_id);
    return `<a href="#/item/${it.id}" class="row" style="display:flex;flex-direction:column;align-items:flex-start;gap:3px;background:var(--surface-2)"><span style="font-weight:600">${esc(R.nameOf(it))}</span><span class="text-2" style="font-size:13px">${esc(t.line)}</span></a>`;
  }).join("");
  const order = { high: 0, medium: 1, low: 2 };
  const top3 = [...backlog()].sort((a, b) => (order[a.priority] ?? 3) - (order[b.priority] ?? 3)).slice(0, 3);
  return R.shell("home", `${R.topbar(rangeSeg)}
    <section aria-label="Now and this period" style="display:flex;flex-wrap:wrap;gap:20px;align-items:stretch">${nowCard}${statTiles}</section>
    <div class="between"><span></span>${src("S", "E")}</div>
    <section class="cols" aria-label="Charts">
      <div class="col-main" style="flex:1 1 380px"><div class="between"><h2 class="section-title">Top artists</h2>${src("S", "E")}</div>${v() === "loading" ? R.skelRows(6) : artists}</div>
      <div class="col-main" style="flex:1.3 1 460px"><div class="between"><h2 class="section-title">Top albums</h2><span class="row-flex">${src("S", "E")}${artToggle()}</span></div>
        ${v() === "loading" ? R.skelTiles(5) : `<div class="tiles five">${albums}</div>${S.art ? setFooter(5, 0, "your top albums") : ""}`}</div></section>
    <section class="cols">
      <div class="col-main" style="flex:1.4 1 480px"><div class="between"><h2 class="section-title">Recent plays</h2><a href="#/plays" style="font-size:13px;font-weight:600">See all</a></div>
        ${v() === "loading" ? R.skelRows(6) : `<div class="card pad-s"><div class="list">${recent}</div></div>`}</div>
      <div class="col-side">
        <div class="card stack"><div class="between"><h2 class="card-title">Your take vs your plays</h2>${src("U", "S", "E")}</div>${takes}</div>
        <div class="card stack"><div class="between"><h2 class="card-title">Backlog</h2><span class="mono muted" style="font-size:12px">${backlog().length} waiting</span></div>
          ${top3.map((it) => `<a href="#/item/${it.id}" class="between" style="color:var(--text);padding:4px 0"><span class="stack" style="gap:2px"><span style="font-weight:600">${esc(R.nameOf(it))}</span><span class="muted" style="font-size:12px">${esc(R.TYPE[it.type])}${it.recommended_by ? ", from " + esc(it.recommended_by) : ""}</span></span>
            <span style="font-size:12px" class="${it.priority === "high" ? "accent" : "muted"}">${it.status === "in_progress" ? "In progress" : R.PRIO[it.priority] || "Planned"}</span></a>`).join("")}</div>
      </div></section>`);
};
const playRow = (e) => {
  const it = e.item_id ? R.byId(e.item_id) : null; const rated = ratingOf(e);
  const end = e.skipped && !e.counts ? `<span class="faint nowrap" style="font-size:12px">Skipped, not counted</span>`
    : rated ? stars(rated)
    : it ? `<a class="btn btn-secondary s" href="#/item/${it.id}" aria-label="Rate ${esc(e.track)}">Rate</a>`
    : `<button class="btn btn-secondary s" ${R.addAttrs("https://open.spotify.com/track/" + e.spotify_uri.split(":")[2], e.track, e.artist, "track")} aria-label="Save ${esc(e.track)}">Save</button>`;
  return `<div class="row" style="grid-template-columns:minmax(0,1fr) auto 76px">
    <div class="stack" style="gap:2px;min-width:0"><span class="ellipsis" style="font-weight:600;${e.skipped && !e.counts ? "color:var(--faint)" : ""}">${esc(e.track)}</span><span class="ellipsis muted" style="font-size:12px">${esc(e.artist)}</span></div>
    ${end}<span class="mono faint" style="font-size:12px;text-align:right">${e.now_playing ? "now" : R.ago(e.ended_at)}</span></div>`;
};

const homeMvpBody = () => {
  const items = R.items;
  const month = D.meta.today.slice(0, 7);
  const explored = items.filter((i) => i.explored_at && i.explored_at.startsWith(month));
  const noReview = items.filter((i) => i.status === "explored" && !i.review && i.type !== "artist");
  const inProg = items.filter((i) => i.status === "in_progress");
  const order = { high: 0, medium: 1, low: 2 };
  const next = [...backlog()].filter((i) => i.status === "planned").sort((a, b) => (order[a.priority] ?? 3) - (order[b.priority] ?? 3)).slice(0, 5);
  const returns = items.filter((i) => i.revisit);
  const recentAdds = [...items].sort((a, b) => b.added_at.localeCompare(a.added_at)).slice(0, 4);
  const oldest = inProg.map((i) => R.since(i)).sort()[0];
  return `<section class="stats" aria-label="Your numbers">
      ${R.stat("In backlog", backlog().length, `${backlog().filter((i) => i.status === "planned").length} planned, ${inProg.length} in progress`)}
      ${R.stat("In progress", inProg.length, oldest ? `Oldest started ${R.age(oldest)} ago` : "Nothing started", true)}
      ${R.stat("Explored this month", explored.length, `${explored.filter((i) => i.status === "not_for_me").length} marked not for me`)}
      ${R.stat("Explored, no review", noReview.length, "Write a line while it is fresh", true)}</section>
    <div class="cols"><section class="col-main" aria-labelledby="mvp-prog">
      <h2 id="mvp-prog" class="section-title">In progress</h2>
      ${inProg.map((it) => `<div class="card" style="display:flex;flex-wrap:wrap;gap:16px;align-items:center">
        ${it.type !== "artist" ? `<div style="flex:0 0 96px">${R.embed(96)}</div>` : ""}
        <div class="stack" style="flex:1 1 220px;gap:4px;min-width:0"><a href="#/item/${it.id}" style="font-size:18px;font-weight:800;color:var(--text)">${esc(R.nameOf(it))}</a><span class="text-2">${esc(it.type === "artist" ? it.notes || "Artist" : it.artist)}</span>
          <span class="mono faint" style="font-size:12px">${R.TYPE[it.type]}, started ${R.age(R.since(it))} ago${it.recommended_by ? ", from " + esc(it.recommended_by) : ""}</span></div>
        <div class="row-flex"><button class="btn btn-light" data-act="status" data-id="${it.id}" data-v="explored" aria-label="${named("Mark explored:", it)}">Mark explored</button>${R.openSpotify(R.nameOf(it))}</div></div>`).join("")}
      <div class="between" style="margin-top:12px"><h2 class="section-title">Worth a return</h2>${artToggle()}</div>
      ${itemSet(returns, (it) => [it.rating ? `Revisit, ${R.stars(it.rating)}` : "Revisit", false], returns.length, "these records")}
    </section>
    <aside class="col-side" aria-label="Needs you">
      <section class="card stack" aria-labelledby="mvp-names"><h2 id="mvp-names" class="card-title">Waiting for a name</h2>
        ${R.captures.length ? R.captures.map((c) => `<div class="row" style="grid-template-columns:minmax(0,1fr) auto;background:var(--surface-2)">
          <span class="stack" style="gap:2px;min-width:0"><span class="mono ellipsis text-2" style="font-size:12px">${esc(c.raw_link)}</span><span class="muted" style="font-size:12px">Shared from iPhone ${R.ago(c.received_at).toLowerCase()}${c.state === "unresolved" ? ", link not opened yet" : ""}</span></span>
          <button class="btn btn-primary s" data-act="name-it" data-id="${c.id}" aria-label="Name the link shared ${R.ago(c.received_at).toLowerCase()}">Name it</button></div>`).join("") : `<span class="muted">Nothing waiting.</span>`}</section>
      <section class="card stack" aria-labelledby="mvp-next"><div class="between"><h2 id="mvp-next" class="card-title">Up next</h2><a href="#/backlog" style="font-size:13px;font-weight:600">Backlog (${backlog().length})</a></div>
        ${next.map((it) => `<a href="#/item/${it.id}" class="between" style="color:var(--text);padding:4px 0"><span class="stack" style="gap:2px"><span style="font-weight:600">${esc(R.nameOf(it))}</span><span class="muted" style="font-size:12px">${R.TYPE[it.type]}${it.recommended_by ? ", from " + esc(it.recommended_by) : ""}</span></span><span style="font-size:12px" class="${it.priority === "high" ? "accent" : "muted"}">${R.PRIO[it.priority] || "Planned"}</span></a>`).join("")}</section>
      <section class="card stack" aria-labelledby="mvp-recent"><h2 id="mvp-recent" class="card-title">Recently added</h2>
        ${recentAdds.map((it) => `<a href="#/item/${it.id}" class="between" style="color:var(--text);padding:2px 0"><span class="ellipsis">${esc(R.nameOf(it))}</span><span class="mono faint nowrap" style="font-size:12px">${R.age(it.added_at)}${R.daysSince(it.added_at) ? " ago" : ""}</span></a>`).join("")}</section>
    </aside></div>`;
};
const homeMvp = () => {
  if (v() === "empty") return firstRun();
  if (v() === "loading") return R.shell("home", R.topbar() + `<div class="stats">${[1, 2, 3, 4].map(() => `<div class="card" style="height:96px"><div class="skel" style="height:100%"></div></div>`).join("")}</div>` + R.skelRows(5));
  if (v() === "error") return R.shell("home", R.topbar() + R.error("Couldn’t open your library.", "The library file is locked by another program. Close it, or restore the latest backup.", `<a class="btn btn-secondary s" href="#/settings">Open settings</a>`) + homeMvpBody());
  return R.shell("home", R.topbar() + homeMvpBody());
};
const firstRun = () => `<div class="app"><main class="main" style="margin:0 auto;max-width:760px;padding-top:96px;gap:28px">
  <span class="logo" style="padding:0;font-size:26px">Runout</span>
  <div class="stack" style="gap:8px"><h1 style="font-size:34px;font-weight:800;letter-spacing:-0.025em">Keep the music you mean to come back to.</h1>
    <span class="text-2" style="font-size:16px">Paste a Spotify link for an album, track or artist. It goes into your backlog.</span></div>
  ${R.topbar()}
  <div class="tiles" style="grid-template-columns:repeat(3,minmax(0,1fr))">
    <div class="card stack"><b>Share from your phone</b><span class="text-2" style="font-size:13px">Set up the iPhone Shortcut so songs you hear on the go land here.</span><a href="#" data-act="dialog" data-v="shortcut" style="font-weight:600">Set it up</a></div>
    <div class="card stack"><b>Bring your history</b><span class="text-2" style="font-size:13px">Import your Spotify listening history to see what you actually play.</span>${R.isR2() ? `<a href="#/import/1" style="font-weight:600">Import</a>` : `<span class="faint" style="font-size:12px">Available in a later release</span>`}</div>
    <div class="card stack"><b>Clean up playlists</b><span class="text-2" style="font-size:13px">Connect Spotify to tidy your own playlists with your ratings.</span>${R.isR3() ? `<a href="#" data-act="dialog" data-v="connect" style="font-weight:600">Connect Spotify</a>` : `<span class="faint" style="font-size:12px">Available in a later release</span>`}</div>
  </div><span class="faint" style="font-size:12px">Everything stays on this PC. <span class="kbd">Ctrl+N</span> adds a link from anywhere.</span></main></div>`;

/* ================================================================ SC-03 Backlog */
const backlogScreen = () => {
  const f = S.backlogFilter;
  const all = backlog();
  let list = all.filter((i) => f.type === "all" || i.type === f.type).filter((i) => !f.tag || i.tags.includes(f.tag));
  if (f.sort === "added") list.sort((a, b) => b.added_at.localeCompare(a.added_at));
  const tags = [...new Set(all.flatMap((i) => i.tags))].sort();
  const groups = f.sort === "added" ? [["All, newest first", list]] : [["High priority", list.filter((i) => i.priority === "high")], ["Medium", list.filter((i) => i.priority === "medium")],
    ["Low", list.filter((i) => i.priority === "low")], ["No priority", list.filter((i) => !i.priority)]].filter((g) => g[1].length);
  const empty = v() === "empty";
  const summary = empty ? "Nothing waiting" : `${all.filter((i) => i.status === "planned").length} planned, ${all.filter((i) => i.status === "in_progress").length} in progress, oldest added ${R.age(all.map((i) => i.added_at).sort()[0])} ago`;
  const head = `<div class="page-head"><div class="stack" style="gap:4px"><h1 class="page-title">Backlog</h1><span class="muted">${summary}</span></div>
    ${empty ? "" : `<div class="row-flex">${R.seg("Type", [["all", "All"], ["album", "Albums"], ["track", "Tracks"], ["artist", "Artists"]], f.type, "bl-type")}
      <label class="sr" for="bl-tag">Tag</label><select id="bl-tag" class="input surface" data-filter="backlogFilter.tag"><option value="">All tags</option>${tags.map((t) => `<option ${t === f.tag ? "selected" : ""}>${t}</option>`).join("")}</select>
      <label class="sr" for="bl-sort">Sort</label><select id="bl-sort" class="input surface" data-filter="backlogFilter.sort"><option value="order" ${f.sort === "order" ? "selected" : ""}>Priority</option><option value="added" ${f.sort === "added" ? "selected" : ""}>Date added</option></select></div>`}</div>`;
  if (v() === "loading") return R.shell("backlog", R.topbar() + head + stateSkel(8));
  if (empty) return R.shell("backlog", R.topbar() + head + R.empty("Nothing waiting", "Paste a Spotify link above, or share one from your phone, and it lands here.", `<button class="btn btn-primary" data-act="open-add">Add a link</button>`));
  const names = R.captures.length && f.type === "all" && !f.tag ? `<section class="stack" style="gap:6px"><h2 style="font-size:13px;font-weight:700" class="accent">Waiting for a name</h2><div class="card pad-s"><div class="list">${R.captures.map((c) =>
    `<div class="row" style="grid-template-columns:minmax(0,1fr) auto auto"><span class="stack" style="gap:2px;min-width:0"><span class="mono ellipsis text-2" style="font-size:12px">${esc(c.raw_link)}</span><span class="muted" style="font-size:12px">Shared from iPhone ${R.ago(c.received_at).toLowerCase()}</span></span><span class="badge need">Name needed</span><button class="btn btn-primary s" data-act="name-it" data-id="${c.id}" aria-label="Name the link shared ${R.ago(c.received_at).toLowerCase()}">Name it</button></div>`).join("")}</div></div></section>` : "";
  const body = list.length ? groups.map(([g, items]) => `<section class="stack" style="gap:6px"><h2 style="font-size:13px;font-weight:700" class="${g === "High priority" ? "accent" : "text-2"}">${g}</h2>
    <div class="card pad-s"><div class="list">${items.map((it) => statusRow(it)).join("")}</div></div></section>`).join("")
    : R.empty("No matches", "Nothing in the backlog fits these filters.", `<button class="btn btn-secondary" data-act="bl-reset">Clear filters</button>`);
  const err = v() === "error" ? R.error("Couldn’t save that change.", "Moving Madvillainy to High priority didn’t save. Nothing else changed.", `<button class="btn btn-secondary s" data-act="toast" data-v="Saved">Try again</button>`) : "";
  const bar = S.sel.size ? `<div class="selbar" role="region" aria-label="Selection actions"><span style="font-weight:700;padding:0 6px">${S.sel.size} selected</span>
    <button class="btn" data-act="sel-prio">Set high priority</button><button class="btn" data-act="sel-remove">Remove from backlog</button><button class="btn" data-act="sel-clear">Clear</button></div>` : "";
  return R.shell("backlog", R.topbar() + head + err + names + body + `<span class="mono faint" style="font-size:12px">${list.length} of ${all.length} shown</span>` + bar);
};

/* ================================================================ SC-04 Library */
const libraryScreen = (r) => {
  const tab = r.args[0] || "album";
  const f = S.libFilter;
  const all = R.items.filter((i) => i.type === tab);
  const list = all.filter((i) => f.status === "any" || i.status === f.status).filter((i) => f.rating === "any" || (i.rating || 0) >= Number(f.rating));
  const pages = Math.max(1, Math.ceil(list.length / 20)); const page = Math.min(f.page, pages);
  const shown = list.slice((page - 1) * 20, page * 20);
  const counts = { album: R.items.filter((i) => i.type === "album").length, track: R.items.filter((i) => i.type === "track").length, artist: R.items.filter((i) => i.type === "artist").length };
  if (v() === "empty") Object.keys(counts).forEach((k) => (counts[k] = 0));
  const head = `<div class="page-head"><div class="stack" style="gap:10px"><h1 class="page-title">Library</h1>
    ${R.seg("Type", [["album", `Albums ${counts.album}`], ["track", `Tracks ${counts.track}`], ["artist", `Artists ${counts.artist}`]], tab, (x) => `#/library/${x}`)}</div>
    <div class="row-flex"><label class="label" for="lb-status">Status</label><select id="lb-status" class="input surface" data-filter="libFilter.status"><option value="any">Any</option>${Object.entries(R.STATUS).map(([k, t]) => `<option value="${k}" ${f.status === k ? "selected" : ""}>${t}</option>`).join("")}</select>
      <label class="label" for="lb-rating">Rating</label><select id="lb-rating" class="input surface" data-filter="libFilter.rating"><option value="any">Any</option>${[3, 4, 4.5, 5].map((n) => `<option value="${n}" ${String(f.rating) === String(n) ? "selected" : ""}>${n} and up</option>`).join("")}</select>
      <button class="btn btn-secondary" data-act="dialog" data-v="saveFilter">Save as filter</button>${artToggle()}<a class="btn btn-secondary" href="#/settings/tags">Manage tags</a></div></div>`;
  if (v() === "loading") return R.shell("library", R.topbar() + head + R.skelTiles(10));
  if (v() === "empty") return R.shell("library", R.topbar() + head + R.empty("Your library is empty", "Albums, tracks and artists you save show up here.", `<button class="btn btn-primary" data-act="open-add">Add a link</button>`));
  if (v() === "error") return R.shell("library", R.topbar() + head + errorFor("your library"));
  const filt = f.status !== "any" || f.rating !== "any";
  const caption = `<span class="muted" style="font-size:13px">${pl(list.length, R.TYPE[tab].toLowerCase())}${filt ? ` matching ${f.status !== "any" ? R.STATUS[f.status].toLowerCase() : "any status"}${f.rating !== "any" ? `, rated ${f.rating} and up` : ""}` : ""}. Sorted by last change.</span>`;
  const grid = list.length ? (S.art ? `<div class="tiles">${shown.map((it) => embedTile(it, R.statusMeta(it), it.status === "in_progress")).join("")}</div>${setFooter(shown.length, list.length, "these records")}`
    : `<div class="tiles">${shown.map((it) => textCard(it, R.statusMeta(it), it.status === "in_progress")).join("")}</div>`)
    : R.empty("Nothing matches", "No records fit these filters.", `<button class="btn btn-secondary" data-act="lib-reset">Clear filters</button>`);
  const pager = pages > 1 ? `<div class="row-flex">${Array.from({ length: pages }, (_, n) => `<button class="btn ${n + 1 === page ? "btn-light" : "btn-secondary"} s" data-act="lib-page" data-v="${n + 1}" aria-label="Page ${n + 1}">${n + 1}</button>`).join("")}<span class="mono faint" style="font-size:12px">${(page - 1) * 20 + 1} to ${(page - 1) * 20 + shown.length} of ${list.length}</span></div>` : "";
  return R.shell("library", R.topbar() + head + caption + grid + pager);
};

/* ================================================================ Search results */
const searchScreen = (r) => {
  const q = (r.args[0] || "").toLowerCase();
  const head = `<div class="stack" style="gap:4px"><h1 class="page-title">Results for “${esc(q)}”</h1>`;
  if (v() === "loading") return R.shell("library", R.topbar() + head + `<span class="muted">Searching…</span></div>` + stateSkel(5));
  if (v() === "error") return R.shell("library", R.topbar() + head + `</div>` + errorFor("search results"));
  const hit = (it) => {
    const fields = [["name", it.display_name], ["artist", it.artist], ["review", it.review], ["notes", it.notes], ["tag", it.tags.join(" ")], ["recommended by", it.recommended_by]];
    const f = fields.find(([, val]) => val && val.toLowerCase().includes(q));
    if (!f) return null;
    const val = f[1]; const i = val.toLowerCase().indexOf(q);
    const snip = (i > 30 ? "…" : "") + esc(val.slice(Math.max(0, i - 30), i)) + `<mark style="background:rgba(245,181,68,0.3);color:var(--text);border-radius:3px">${esc(val.slice(i, i + q.length))}</mark>` + esc(val.slice(i + q.length, i + q.length + 50));
    return { it, where: f[0], snip };
  };
  const res = v() === "empty" ? [] : R.items.map(hit).filter(Boolean);
  const groups = ["artist", "album", "track"].map((t) => [t, res.filter((x) => x.it.type === t)]).filter((g) => g[1].length);
  return R.shell("library", R.topbar() + head + `<span class="muted">${pl(res.length, "result")} in names, reviews, notes, tags and recommendations</span></div>` +
    (res.length ? groups.map(([t, list]) => `<section class="stack" style="gap:6px"><h2 class="text-2" style="font-size:13px;font-weight:700">${R.TYPE[t]}s</h2><div class="card pad-s"><div class="list">${list.map(({ it, where, snip }) =>
      `<a class="row" href="#/item/${it.id}" style="grid-template-columns:minmax(0,1fr) 130px"><span class="stack" style="gap:2px;min-width:0"><span style="font-weight:600">${esc(R.nameOf(it))}${it.type !== "artist" ? `<span class="muted" style="font-weight:400">, ${esc(it.artist)}</span>` : ""}</span><span class="text-2" style="font-size:13px">${snip}</span></span><span class="muted" style="font-size:12px;text-align:right">Matched in ${where}</span></a>`).join("")}</div></div></section>`).join("")
      : R.empty("Nothing matches", `No name, review, note or tag contains “${esc(q)}”.`, `<a class="btn btn-secondary" href="#/library">Clear search</a>`)));
};

/* ================================================================ SC-05 Item detail (album, track, artist) */
const itemScreen = (r) => {
  const it = v() === "empty" ? null : R.byId(r.args[0]);
  if (!it) return R.shell("library", R.topbar() + R.empty("This record no longer exists", "It was deleted, or the link points to something that isn’t in your library.", `<a class="btn btn-secondary" href="#/library">Back to library</a>`));
  if (v() === "loading") return R.shell("backlog", R.topbar() + `<div class="cols"><div style="flex:0 1 360px"><div class="skel" style="height:352px;border-radius:12px"></div></div><div class="col-main">${R.skelRows(4)}</div></div>`);
  const edit = r.params.edit === "1";
  const hist = R.history.filter((h) => h.item_id === it.id).sort((a, b) => b.at.localeCompare(a.at));
  const stats = L.item_stats[String(it.id)];
  const artist = it.artist_id ? R.byId(it.artist_id) : null;
  const byArtist = it.type === "artist" ? R.items.filter((x) => x.artist_id === it.id) : [];
  const head = edit ? `<div class="card stack"><h2 class="card-title">Edit record</h2>
      <div class="tiles" style="grid-template-columns:repeat(3,minmax(0,1fr))">
        <div class="field"><label class="label" for="ed-name">Name</label><input id="ed-name" class="input" value="${esc(it.display_name || "")}"></div>
        ${it.type !== "artist" ? `<div class="field"><label class="label" for="ed-artist">Artist</label><input id="ed-artist" class="input" value="${esc(it.artist || "")}"></div>` : ""}
        <div class="field"><label class="label" for="ed-type">Type</label><select id="ed-type" class="input">${["album", "track", "artist"].map((t) => `<option ${t === it.type ? "selected" : ""}>${R.TYPE[t]}</option>`).join("")}</select></div></div>
      <div class="field"><span class="label">Links to other records</span><div class="row-flex">${artist ? `<span class="chip">By ${esc(artist.display_name)}</span>` : ""}${it.album ? `<span class="chip">On ${esc(it.album)}</span>` : ""}
        <button class="chip add" data-act="toast" data-v="Pick a record to link">Link to an artist or album</button><button class="chip add" data-act="toast" data-v="Pick the other version">Mark as a version of…</button></div></div>
      <div class="row-flex"><button class="btn btn-primary" data-act="edit-save" data-id="${it.id}">Save changes</button><a class="btn btn-secondary" href="#/item/${it.id}">Cancel</a></div></div>`
    : `<div class="stack" style="gap:6px"><span class="muted" style="font-size:13px">${R.TYPE[it.type]}, added ${R.day(it.added_at)}${it.via_phone ? " from your iPhone" : ""}</span>
      <div class="row-flex"><h1 style="font-size:34px;font-weight:800;letter-spacing:-0.025em">${esc(R.nameOf(it))}</h1><a class="btn btn-secondary s" href="#/item/${it.id}?edit=1" aria-label="${named("Edit", it)}">Edit</a></div>
      ${it.type !== "artist" ? `<span class="text-2">${artist ? `<a href="#/item/${artist.id}">${esc(it.artist)}</a>` : esc(it.artist)}${it.type === "track" && it.album ? `, from ${(() => { const al = R.byName("album", it.album); return al ? `<a href="#/item/${al.id}">${esc(it.album)}</a>` : esc(it.album); })()}` : ""}</span>` : ""}
      ${it.recommended_by ? `<span class="muted" style="font-size:13px">Recommended by ${esc(it.recommended_by)}</span>` : ""}</div>`;
  const spotifySave = R.isR3() ? (R.offline() ? `<span class="faint" style="font-size:12px">Saved in Spotify can’t change while you’re offline</span>`
    : `<label class="row-flex"><input type="checkbox" data-act="spotify-save" data-id="${it.id}" ${it.spotify_saved ? "checked" : ""}>Saved in your Spotify library</label>`) : "";
  const controls = `<div class="field"><span class="label">Status</span>${statusSeg(it)}
      <div class="row-flex text-2" style="gap:16px;font-size:13px">
        <label class="row-flex"><input type="checkbox" data-act="toggle" data-k="revisit" data-id="${it.id}" ${it.revisit ? "checked" : ""}>Revisit later</label>
        <label class="row-flex"><input type="checkbox" data-act="toggle" data-k="favourite" data-id="${it.id}" ${it.favourite ? "checked" : ""}>Favourite</label>
        ${it.in_backlog ? `<label for="dt-prio">Priority</label><select id="dt-prio" class="input" style="height:34px" data-act="prio" data-id="${it.id}"><option value="">None</option>${["high", "medium", "low"].map((p) => `<option value="${p}" ${it.priority === p ? "selected" : ""}>${p[0].toUpperCase() + p.slice(1)}</option>`).join("")}</select>` : ""}
        ${spotifySave}</div>
      <div class="row-flex"><label class="sr" for="st-note">Note for the next status change</label><input id="st-note" class="input" style="height:34px;flex:1 1 260px" placeholder="Optional note for the next status change"></div></div>`;
  const journal = it.type === "artist" ? `<div class="field"><label class="label" for="dt-notes">Why this artist</label><textarea id="dt-notes" class="input" rows="3">${esc(it.notes)}</textarea></div>`
    : `<div class="row-flex" style="gap:32px;align-items:flex-start"><div class="field"><span class="label">Your rating</span>${rating(it)}<span class="kbd">Arrow keys change by half a star</span></div>
      <div class="field"><span class="label">Tags</span><div class="row-flex">${it.tags.map((t) => `<button class="chip" data-act="remove-tag" data-id="${it.id}" data-v="${esc(t)}" aria-label="Remove tag ${esc(t)}">${esc(t)} ×</button>`).join("")}<button class="chip add" data-act="add-tag" data-id="${it.id}">Add tag</button></div></div></div>
      <div class="field"><label class="label" for="dt-review">Review</label><textarea id="dt-review" class="input" rows="4" placeholder="What stayed with you?">${esc(it.review)}</textarea></div>
      <div class="field"><label class="label" for="dt-notes">Notes</label><textarea id="dt-notes" class="input" rows="2" placeholder="Context, where you heard it, what to listen for">${esc(it.notes)}</textarea></div>`;
  const unf = L.unfinished.find((u) => u.item_id === it.id);
  const tracks = it.tracklist ? `<section class="card stack"><div class="between"><h2 class="card-title">Tracks you’ve added</h2>${R.isR2() ? src("U", "S", "E") : ""}</div>
      ${it.tracklist.map((t, n) => { const played = (unf ? unf.played : it.tracklist.length) > n;
        return `<div class="between" style="padding:3px 0"><span><span class="mono faint" style="font-size:12px;margin-right:10px">${n + 1}</span>${esc(t)}</span>${R.isR2() ? `<span class="${played ? "muted" : "accent"}" style="font-size:12px">${played ? "Played" : "Not played yet"}</span>` : ""}</div>`; }).join("")}
      <button class="btn btn-secondary s" style="align-self:flex-start" data-act="toast" data-v="Add a track by name or link">Add a track</button></section>` : it.type === "album" ? `<section class="card stack"><h2 class="card-title">Tracks you’ve added</h2><span class="muted">None yet. Add tracks to see which ones you haven’t played.</span>
      <div class="row-flex"><button class="btn btn-secondary s" data-act="toast" data-v="Add a track by name or link">Add a track</button>${R.isR3() ? `<button class="btn btn-secondary s" data-act="dialog" data-v="autofill">Fill the track list from Spotify</button>` : ""}</div></section>` : "";
  const artistBody = it.type === "artist" ? `<section class="stack"><div class="between"><h2 class="section-title">In your library</h2><span class="muted" style="font-size:12px">Only what you’ve saved. This is not the artist’s full discography.</span></div>
      ${byArtist.length ? `<div class="card pad-s"><div class="list">${byArtist.map((x) => `<div class="row" style="grid-template-columns:minmax(0,1fr) 140px 130px"><a href="#/item/${x.id}" style="color:inherit" class="stack"><span style="font-weight:600">${esc(R.nameOf(x))}</span><span class="muted" style="font-size:12px">${R.TYPE[x.type]}</span></a><span class="text-2" style="font-size:13px">${R.statusMeta(x)}</span>
        <span style="justify-self:end">${x.in_backlog ? `<span class="badge status">In backlog</span>` : `<button class="btn btn-secondary s" data-act="toast" data-v="${esc(R.nameOf(x))} added to your backlog" aria-label="Add ${esc(R.nameOf(x))} to backlog">Add to backlog</button>`}</span></div>`).join("")}</div></div>` : `<span class="muted">Nothing saved by this artist yet.</span>`}
      <button class="btn btn-secondary" style="align-self:flex-start" data-act="open-add" data-link="">Add an album or track by ${esc(it.display_name)}</button></section>` : "";
  const listening = R.isR2() ? `<div class="card stack" style="gap:6px"><div class="between"><h2 class="card-title">Your listening</h2>${src("S", "E")}</div>
      ${stats ? `<span class="mono" style="font-size:22px;font-weight:600">${pl(stats.plays, "play")}</span><span class="text-2" style="font-size:13px">Last played ${R.ago(stats.last).toLowerCase()}.${it.type === "album" ? " Counts every track from this album." : it.type === "artist" ? " Counts every track by this artist." : ""}</span>` : `<span class="text-2">No plays yet.</span>`}
      <span class="freshness">Desktop live; other devices up to ${R.day(L.freshness.export_until)}.</span></div>` : "";
  const hist$ = `<div class="card stack"><h2 class="card-title">History</h2>${hist.map((h) => `<div style="display:grid;grid-template-columns:64px minmax(0,1fr);gap:10px;font-size:13px"><span class="mono faint">${R.day(h.at)}</span><span class="text-2">${h.to === "review" ? "Review written" : h.from ? `${R.STATUS[h.from]} to ${R.STATUS[h.to] || h.to}` : h.note || "Added"}${h.note && h.from ? `: ${esc(h.note)}` : ""}</span></div>`).join("") || `<span class="muted">No changes yet.</span>`}</div>`;
  const danger = `<div class="row-flex">${it.in_backlog ? `<button class="btn btn-secondary" data-act="remove-backlog" data-id="${it.id}">Remove from backlog</button>` : ""}<button class="btn btn-danger" data-act="dialog" data-v="deleteItem" data-id="${it.id}" aria-label="${named("Delete", it)}">Delete</button></div>`;
  const embedH = it.type === "track" ? 152 : 352;
  return R.shell(it.in_backlog ? "backlog" : "library", R.topbar() + `<a href="#/${it.in_backlog ? "backlog" : "library"}" style="font-size:13px;font-weight:600">Back to ${it.in_backlog ? "backlog" : "library"}</a>
    ${v() === "error" ? R.error("Couldn’t save your review.", "The text is kept here; try again in a moment.", `<button class="btn btn-secondary s" data-act="toast" data-v="Review saved">Try again</button>`) : ""}
    <section class="cols"><div style="flex:0 1 360px;min-width:280px" class="stack">${R.embed(embedH, "Spotify embed: artwork and player, shown unaltered")}<span class="row-flex">${R.logo()}${R.openSpotify(R.nameOf(it))}</span></div>
      <div class="col-main" style="gap:18px">${head}${controls}${journal}</div></section>
    ${artistBody}<div class="cols"><div class="col-main" style="flex:1.3 1 420px">${tracks}</div><div class="col-side">${listening}${hist$}${danger}</div></div>`);
};

/* ================================================================ SC-07 Rediscover */
R.shelves = () => {
  const f = S.redFilter || (S.redFilter = { tag: "", min: "any", revisit: false });
  const ok = (it) => it.type !== "artist" && (!f.tag || it.tags.includes(f.tag)) && (f.min === "any" || (it.rating || 0) >= Number(f.min)) && (!f.revisit || it.revisit);
  return [
    ["Flagged to revisit", "Everything you marked Revisit later", R.items.filter((i) => i.revisit), (it) => [`Flagged, ${it.rating ? R.stars(it.rating) : "unrated"}`, false]],
    ["Your best", "Rated 4.5 or more", R.items.filter((i) => (i.rating || 0) >= 4.5), (it) => [R.stars(it.rating), false]],
    ["Words still missing", "Explored, with no review yet", R.items.filter((i) => i.status === "explored" && !i.review && i.type !== "artist"), (it) => [it.rating ? `Rated ${it.rating}, no review` : "No rating either", true]],
    ["Waiting too long", "In the backlog for more than 60 days", backlog().filter((i) => R.daysSince(i.added_at) > 60), (it) => [`Added ${R.age(it.added_at)} ago`, true]],
    ...(R.isR2() ? [["Loved, not played lately", "Rated 4.5 or more, no plays in 90 days (your records and your plays)", L.takes.filter((t) => t.kind === "loved_not_played").map((t) => R.byId(t.item_id)), (it) => [`Last played ${R.day(L.item_stats[it.id].last)}`, false]]] : []),
  ].map((s) => [s[0], s[1], s[2].filter(ok), s[3]]);
};
const rediscoverScreen = () => {
  const f = S.redFilter || (S.redFilter = { tag: "", min: "any", revisit: false });
  const shelves = R.shelves();
  const tags = [...new Set(R.items.flatMap((i) => i.tags))].sort();
  const head = `<div class="page-head"><div class="stack" style="gap:4px"><h1 class="page-title">Rediscover</h1><span class="muted">Shelves built from your own rules. Nothing here is chosen by Spotify.</span></div>
    <div class="row-flex"><label class="sr" for="rd-tag">Tag</label><select id="rd-tag" class="input surface" data-filter="redFilter.tag"><option value="">All tags</option>${tags.map((t) => `<option ${f.tag === t ? "selected" : ""}>${t}</option>`).join("")}</select>
      <label class="sr" for="rd-min">Rating</label><select id="rd-min" class="input surface" data-filter="redFilter.min"><option value="any">Any rating</option>${[4, 4.5, 5].map((n) => `<option value="${n}" ${String(f.min) === String(n) ? "selected" : ""}>${n} and up</option>`).join("")}</select>
      <button class="btn btn-secondary" aria-pressed="${f.revisit}" data-act="red-revisit">${f.revisit ? "Revisit only: on" : "Revisit only"}</button>${artToggle()}
      <button class="btn btn-primary" data-act="dialog" data-v="newShelf">New shelf</button></div></div>
    <div class="row-flex"><span class="muted" style="font-size:13px">Saved filters:</span>${D.saved_filters.map((s) => `<button class="chip ${S.activeFilter === s.id ? "on" : ""}" data-act="apply-filter" data-id="${s.id}" aria-pressed="${S.activeFilter === s.id}">${esc(s.name)}</button>`).join("")}
      ${S.activeFilter && R.isR3() ? `<button class="btn btn-secondary s" data-act="dialog" data-v="makePlaylist" data-id="${esc(D.saved_filters.find((x) => x.id === S.activeFilter).name)}">Make a Spotify playlist from this filter</button>` : ""}</div>`;
  if (v() === "loading") return R.shell("rediscover", R.topbar() + head + R.skelTiles(6) + R.skelTiles(6));
  if (v() === "error") return R.shell("rediscover", R.topbar() + head + errorFor("your shelves"));
  const visible = v() === "empty" ? [] : shelves.filter((s) => s[2].length);
  if (!visible.length) return R.shell("rediscover", R.topbar() + head + R.empty("No shelves yet", "Shelves fill up as you rate, review and flag records. Try rating a few things you’ve explored.", `<a class="btn btn-secondary" href="#/library">Open your library</a>`));
  return R.shell("rediscover", R.topbar() + head + visible.map(([name, rule, list, meta]) => `<section class="stack"><div class="between"><div class="stack" style="gap:2px"><h2 class="section-title">${name}</h2><span class="muted" style="font-size:13px">${rule}</span></div>
      <span class="row-flex">${R.isR3() ? `<button class="btn btn-secondary s" data-act="dialog" data-v="makePlaylist" data-id="${esc(name)}" aria-label="Make a Spotify playlist from ${esc(name)}">Make a Spotify playlist</button>` : ""}<span class="mono faint" style="font-size:12px" aria-label="${pl(list.length, "record")}">${pl(list.length, "record")}</span></span></div>
      ${itemSet(list.slice(0, 6), meta, list.length, name)}</section>`).join(""));
};

/* ================================================================ SC-08 Recent plays */
const unratedPlay = (e) => e.counts && !ratingOf(e);
const playsScreen = (r) => {
  const tab = r.args[0] || "all";
  const all = L.recent.filter((e) => !e.now_playing);
  const list = tab === "unrated" ? all.filter(unratedPlay) : tab === "missing" ? all.filter((e) => !e.item_id) : all;
  const byDay = {}; list.forEach((e) => { const d = R.daysSince(e.ended_at); const k = d === 0 ? "Today" : d === 1 ? "Yesterday" : R.day(e.ended_at); (byDay[k] = byDay[k] || []).push(e); });
  const row = (e) => { const it = e.item_id ? R.byId(e.item_id) : null; const dim = e.skipped && !e.counts; const rated = ratingOf(e);
    return `<div class="row" style="grid-template-columns:44px minmax(0,1fr) 64px 50px 140px">
      <span class="mono faint" style="font-size:12px">${R.time(e.ended_at)}</span>
      <div class="stack" style="gap:2px;min-width:0"><span class="ellipsis" style="font-weight:600;${dim ? "color:var(--faint)" : ""}">${it ? `<a href="#/item/${it.id}" style="color:inherit">${esc(e.track)}</a>` : esc(e.track)}</span><span class="ellipsis muted" style="font-size:12px">${esc(e.artist)}</span></div>
      <span class="muted" style="font-size:12px">${e.platform === "windows" ? "This PC" : "Phone"}</span><span class="mono ${dim ? "faint" : "text-2"}" style="font-size:12px">${R.mmss(e.ms)}</span>
      <span style="font-size:13px;justify-self:end">${dim ? `<span class="faint">Skipped, not counted</span>` : rated ? stars(rated) : it ? `<a class="btn btn-secondary s" href="#/item/${it.id}" aria-label="Rate ${esc(e.track)}">Rate</a>` : `<button class="btn btn-secondary s" ${R.addAttrs("https://open.spotify.com/track/" + e.spotify_uri.split(":")[2], e.track, e.artist, "track")} aria-label="Save ${esc(e.track)}">Save</button>`}</span></div>`; };
  const tabs = R.seg("Which plays", [["all", "All"], ["unrated", `Not rated ${all.filter(unratedPlay).length}`], ["missing", `Not in library ${all.filter((e) => !e.item_id).length}`]], tab, (x) => `#/plays/${x}`);
  const logger = `<div class="card stack"><h2 class="card-title">Desktop logger</h2>
      ${v() === "error" ? R.error("Logger stopped.", "Windows isn’t sharing media info. Restart Spotify, then try again.", `<button class="btn btn-secondary s" data-act="toast" data-v="Logger restarted">Try again</button>`)
        : `<span class="text-2" style="line-height:1.5">${S.paused ? "Paused. Nothing is logged until you resume." : "Logging Spotify on this PC. A play counts after 30 seconds."}</span>
      <div class="row-flex">${S.paused ? `<button class="btn btn-light" data-act="pause" data-v="resume">Resume</button>` : `<button class="btn btn-light" data-act="pause" data-v="hour">Pause for 1 hour</button><button class="btn btn-secondary" data-act="pause" data-v="manual">Pause</button>`}</div>`}
      <span class="muted" style="font-size:12px;line-height:1.45">Private sessions can’t be detected on the PC. Pause before one if you don’t want it logged.</span></div>`;
  const unmatched = `<div class="card stack"><div class="between"><h2 class="card-title">Unmatched plays</h2><span class="mono accent" style="font-size:12px">${L.unmatched.length}</span></div>
      <span class="text-2" style="font-size:13px;line-height:1.5">More than one record in your library could be the track. Pick the right one.</span>
      ${L.unmatched.map((u) => `<div class="stack" style="gap:6px;padding:10px 12px;border-radius:10px;background:var(--surface-2)"><span style="font-weight:600">${esc(u.track)}, ${esc(u.artist)}</span><span class="muted" style="font-size:12px">${esc(u.detail)}. ${pl(u.plays, "play")} waiting.</span>
        <div class="row-flex"><button class="btn btn-light s" data-act="toast" data-v="Linked. ${pl(u.plays, "play")} now count for this record" aria-label="${esc(u.choice)} for ${esc(u.track)}">${esc(u.choice)}</button><button class="btn btn-secondary s" data-act="toast" data-v="Choose a record" aria-label="Choose another record for ${esc(u.track)}">Choose another</button></div></div>`).join("")}</div>`;
  const content = v() === "loading" ? R.skelRows(10) : v() === "empty" ? R.empty("No plays yet", "Turn on the desktop logger, or import your Spotify history to see plays from every device.", `<div class="row-flex"><button class="btn btn-primary" data-act="pause" data-v="resume">Turn on the logger</button><a class="btn btn-secondary" href="#/import/1">Import history</a></div>`)
    : Object.entries(byDay).map(([d, es]) => `<div class="stack" style="gap:6px"><h2 class="muted" style="font-size:13px;font-weight:700">${d}</h2><div class="card pad-s"><div class="list">${es.slice(0, 12).map(row).join("")}</div></div>${es.length > 12 ? `<span class="mono faint" style="font-size:12px">12 of ${es.length} shown</span>` : ""}</div>`).join("") +
      (tab === "all" ? `<div class="between card pad-s" style="padding:12px 16px"><span class="text-2">${R.day(L.freshness.export_until)} to ${R.day(L.recent[L.recent.length - 1].ended_at)}: ${pl(L.earlier_desktop, "more desktop play")}</span><button class="btn btn-secondary s" data-act="toast" data-v="Showing earlier days">Show earlier days</button></div>
      <div class="stack" style="gap:6px"><h2 class="muted" style="font-size:13px;font-weight:700">${R.day(L.export_sample[0].ended_at)}, phone plays from your Spotify export</h2><div class="card pad-s"><div class="list">${L.export_sample.map(row).join("")}</div></div></div>` : "");
  return R.shell("plays", R.topbar() + `<div class="cols"><section class="col-main" style="flex:1.6 1 560px"><div class="page-head"><h1 class="page-title">Recent plays</h1>${tabs}</div>
    <span class="freshness">${src("S", "E")} Desktop live; phone plays up to your export of ${R.day(L.freshness.export_until)}.</span>${content}</section><aside class="col-side">${logger}${v() === "empty" ? "" : unmatched}</aside></div>`);
};

/* ================================================================ SC-09 Insights */
const MONTH = (ym, long) => { const n = Number(ym.slice(5, 7)) - 1; return long ? ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"][n] : ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"][n]; };
const insightsScreen = (r) => {
  const tab = r.args[0] || "listening";
  const ranges = [["30d", "30 days"], ["12m", "12 months"], ["all", "All time"]];
  if (!["30d", "12m", "all"].includes(S.irange || "")) S.irange = "12m";
  const tabs = R.seg("Insight group", [["listening", "Listening"], ["collection", "Collection"], ["take", "Your take"], ["year", "Year in review"]], tab, (x) => `#/insights/${x}`);
  if (v() === "empty") return R.shell("insights", R.topbar() + `<div class="stack" style="gap:10px"><h1 class="page-title">Insights</h1>${tabs}</div>` + R.empty("Nothing to show yet", "Insights need listening data. Import your Spotify history for all devices, or let the desktop logger collect plays on this PC.", `<div class="row-flex"><a class="btn btn-primary" href="#/import/1">Import history</a><button class="btn btn-secondary" data-act="pause" data-v="resume">Turn on the logger</button></div>`));
  const head = `<div class="page-head"><div class="stack" style="gap:10px"><h1 class="page-title">Insights</h1>${tabs}</div>${tab === "listening" ? `<div class="seg" role="group" aria-label="Time range">${ranges.map(([k, t]) => `<button aria-pressed="${S.irange === k}" data-act="irange" data-v="${k}">${t}</button>`).join("")}</div>` : ""}</div>
    <p class="freshness" style="margin:0">Built from your desktop log (live) and your Spotify export (up to ${R.day(L.freshness.export_until)}). ${pl(L.private_excluded, "private-session play")} excluded. Nothing here comes from the Spotify API.</p>`;
  if (v() === "loading") return R.shell("insights", R.topbar() + head + `<div class="cols"><div class="card col-main" style="height:260px"><div class="skel" style="height:100%"></div></div><div class="col-side">${R.skelRows(4)}</div></div>`);
  const stale = R.daysSince(L.freshness.export_until) > 30;
  const warn = v() === "error" || stale ? `<div class="banner warn"><span><b class="accent">Phone data is getting old.</b> <span class="text-2">Your latest export ends ${R.day(L.freshness.export_until)}. Phone listening after that is missing.</span></span><a class="btn btn-secondary s" href="#/import/1">Import a newer export</a></div>` : "";
  let body = "";
  if (tab === "listening") {
    const ch = L.charts[S.irange]; const st = L.stats[S.irange];
    const max = Math.max(...L.monthly.map((m) => m.hours));
    const bars = L.monthly.map((m, i) => `<div class="b ${i === L.monthly.length - 1 ? "hot" : ""}"><div style="height:${Math.round((m.hours / max) * 170)}px" title="${m.hours} h"></div><span>${MONTH(m.month)}</span></div>`).join("");
    const heat = L.heatmap.map((row, d) => `<div class="r"><span class="mono faint" style="font-size:11px">${["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][d]}</span>${row.map((x) => `<div class="c" style="opacity:${Math.max(0.05, x)}"></div>`).join("")}</div>`).join("");
    const topList = (title, rows, kind) => `<div class="card stack" style="flex:1 1 260px"><div class="between"><h2 class="card-title">${title}</h2>${src("S", "E")}</div>${pick(rows, 5).map((x, n) => { const it = R.byName(kind, x.name);
      return `<div class="between" style="padding:2px 0"><span style="min-width:0"><span class="mono faint" style="font-size:12px;margin-right:8px">${n + 1}</span>${it ? `<a href="#/item/${it.id}" style="color:var(--text)">${esc(x.name)}</a>` : esc(x.name)}${x.by ? `<span class="muted">, ${esc(x.by)}</span>` : ""}</span><span class="mono text-2 nowrap" style="font-size:12px">${x.hours.toFixed(1)} h</span></span></div>`; }).join("")}</div>`;
    body = `<div class="cols"><section class="card col-main" style="flex:1.4 1 480px"><div class="between"><h2 class="card-title">Listening time by month</h2><span class="mono text-2" style="font-size:13px">${R.hm(L.stats["12m"].ms)} in the last 12 months</span></div>
        <div class="bars" role="img" aria-label="Listening hours per month for the last 12 months">${bars}</div>${src("S", "E")}</section>
      <section class="col-side" style="display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px">
        ${R.stat("Plays", R.num(st.plays), "Played 30 seconds or more")}${R.stat("Listening time", R.hm(st.ms), "In this range")}
        ${R.stat("Skip rate", L.skip_rate + "%", "Of all plays")}${R.stat("From your phone", L.phone_share + "%", "Of plays, from your export")}</section></div>
      <div class="cols"><section class="card col-main" style="flex:1.4 1 480px"><div class="between"><h2 class="card-title">When you listen</h2>${src("S", "E")}</div><div class="heat" role="img" aria-label="Listening by weekday and hour; most on weeknights after 9pm">${heat}
        <div class="r"><span></span>${Array.from({ length: 24 }, (_, h) => `<span class="mono faint" style="font-size:10px">${h % 6 === 0 ? String(h).padStart(2, "0") : ""}</span>`).join("")}</div></div></section>
        <section class="card col-side stack"><div class="between"><h2 class="card-title">Unfinished albums</h2>${src("U", "S", "E")}</div><span class="muted" style="font-size:12px;line-height:1.45">Only albums whose tracks you’ve added. ${pl(R.items.filter((i) => i.type === "album" && !i.tracklist).length, "other album")} can’t be checked.</span>
          ${L.unfinished.map((u) => `<a href="#/item/${u.item_id}" class="between" style="color:var(--text);padding:3px 0"><span style="font-weight:600">${esc(u.title)}</span><span class="mono text-2 nowrap" style="font-size:12px">${u.played} of ${u.total} played</span></a>`).join("")}</section></div>
      <div class="cols">${topList("Top artists", ch.artists, "artist")}${topList("Top albums", ch.albums, "album")}${topList("Top tracks", ch.tracks, "track")}</div>`;
  }
  if (tab === "collection") {
    const byMonth = {}; R.items.forEach((i) => { const m = i.added_at.slice(0, 7); byMonth[m] = (byMonth[m] || 0) + 1; });
    const months = L.monthly.map((m) => m.month); let run = R.items.filter((i) => i.added_at.slice(0, 7) < months[0]).length;
    const growth = months.map((m) => (run += byMonth[m] || 0));
    const gmax = Math.max(...growth);
    const done = R.items.filter((i) => i.explored_at);
    const avg = Math.round(done.reduce((a, i) => a + R.daysSince(i.added_at) - R.daysSince(i.explored_at), 0) / done.length);
    const count = (s) => R.items.filter((i) => i.status === s).length;
    const dist = [1, 1.5, 2, 2.5, 3, 3.5, 4, 4.5, 5].map((n) => [n, R.items.filter((i) => i.rating === n).length]);
    const dmax = Math.max(...dist.map((d) => d[1]));
    const tagC = {}; R.items.forEach((i) => i.tags.forEach((t) => (tagC[t] = (tagC[t] || 0) + 1)));
    const topTags = Object.entries(tagC).sort((a, b) => b[1] - a[1]).slice(0, 8);
    body = `<div class="stats">${R.stat("Records", R.items.length, `${pl(R.items.filter((i) => i.type === "album").length, "album")}, ${pl(R.items.filter((i) => i.type === "track").length, "track")}, ${pl(R.items.filter((i) => i.type === "artist").length, "artist")}`)}
        ${R.stat("Explored", count("explored"), `${count("not_for_me")} not for me, ${count("abandoned")} abandoned, ${count("in_progress")} in progress, ${count("planned")} planned`)}
        ${R.stat("Planned to explored", R.pl(avg, "day"), "Average time")}${R.stat("Reviews written", R.items.filter((i) => i.review).length, `${R.items.filter((i) => i.status === "explored" && !i.review && i.type !== "artist").length} explored without one`, true)}</div>
      <div class="cols"><section class="card col-main"><div class="between"><h2 class="card-title">Library size over 12 months</h2>${src("U")}</div><div class="bars" role="img" aria-label="Library grows from ${growth[0]} to ${pl(growth[growth.length - 1], "record")}">${growth.map((g, i) => `<div class="b ${i === growth.length - 1 ? "hot" : ""}"><div style="height:${Math.round((g / gmax) * 170)}px"></div><span>${MONTH(months[i])}</span></div>`).join("")}</div></section>
        <section class="card col-side stack"><div class="between"><h2 class="card-title">How you rate</h2>${src("U")}</div>${dist.map(([n, c]) => `<div style="display:grid;grid-template-columns:40px minmax(0,1fr) 28px;gap:10px;align-items:center"><span class="mono text-2" style="font-size:12px">${n}</span><div style="height:6px;border-radius:3px;background:var(--bar);width:${dmax ? Math.round((c / dmax) * 100) : 0}%"></div><span class="mono faint" style="font-size:12px;text-align:right">${c}</span></div>`).join("")}</section></div>
      <section class="card stack"><div class="between"><h2 class="card-title">Your most used tags</h2>${src("U")}</div><div class="row-flex">${topTags.map(([t, c]) => `<span class="chip">${esc(t)} <span class="mono faint">${c}</span></span>`).join("")}</div></section>`;
  }
  if (tab === "take") {
    const groups = [["Loved, not played lately", "loved_not_played"], ["Played a lot, never rated", "played_not_rated"], ["In your backlog, never played", "backlog_never_played"]];
    body = `<p class="text-2" style="margin:0">Where your opinions and your listening disagree. ${src("U", "S", "E")}</p><div class="cols">${groups.map(([t, k]) => `<section class="card stack" style="flex:1 1 300px"><h2 class="card-title">${t}</h2>${L.takes.filter((x) => x.kind === k).map((x) => { const it = R.byId(x.item_id);
      return `<a href="#/item/${it.id}" class="stack" style="gap:2px;padding:8px 10px;border-radius:10px;background:var(--surface-2);color:var(--text)"><span style="font-weight:600">${esc(R.nameOf(it))}</span><span class="text-2" style="font-size:13px">${esc(x.line)}</span></a>`; }).join("") || `<span class="muted">Nothing here.</span>`}</section>`).join("")}</div>`;
  }
  if (tab === "year") {
    const y = L.year_review;
    body = `<div class="row-flex"><label class="label" for="yr">Year</label><select id="yr" class="input surface"><option>2026</option><option>2025</option></select><span class="muted" style="font-size:13px">2026 so far, up to ${R.day(D.meta.today)}</span></div>
      <section class="card" style="padding:32px;display:flex;flex-direction:column;gap:18px;background:radial-gradient(120% 160% at 0% 0%, rgba(245,181,68,0.16) 0%, rgba(23,25,28,0) 55%), var(--elev-1-bg)">
        <span class="muted">Your 2026 in Runout</span><h2 style="font-size:40px;font-weight:800;letter-spacing:-0.03em">${pl(y.hours, "hour")}, ${pl(y.plays, "play")}.</h2>
        <div class="stats">${R.stat("Top artist", esc(y.top_artist.name), pl(y.top_artist.plays, "play"))}${R.stat("Top album", esc(y.top_album.name), esc(y.top_album.by))}${R.stat("Explored", y.explored, "Records you finished")}${R.stat("Reviews", y.reviews, "Written this year")}</div>
        <span class="text-2">Your busiest month was ${MONTH(y.busiest_month, true)}. ${src("U", "S", "E")}</span></section>`;
  }
  return R.shell("insights", R.topbar() + head + warn + body);
};

/* ================================================================ SC-10 Import */
const importScreen = (r) => {
  const step = Number(r.args[0] || 1); const p = L.import_preview;
  const steps = `<ol class="steps" aria-label="Steps">${["Choose files", "Check", "Import"].map((t, i) => `<li ${i + 1 === step ? 'aria-current="step"' : ""} class="${i + 1 < step ? "done" : ""}">${i + 1}. ${t}</li>`).join("")}</ol>`;
  let body = "";
  if (step === 1) {
    const chosen = S.importChosen;
    body = `<div class="card stack" style="align-items:center;text-align:center;padding:40px;border:1px dashed var(--line-strong)"><b style="font-size:17px">Drop the folder or .zip from Spotify’s email</b><span class="text-2">Or choose it from your PC. Runout reads it here; nothing is uploaded.</span>
        <button class="btn btn-secondary" data-act="import-choose">Choose folder or .zip</button>${chosen ? `<span class="mono text-2" style="font-size:12px">Documents\\spotify-export (selected)</span>` : ""}</div>
      ${v() === "error" ? R.error("This folder doesn’t look like a Spotify export.", "Choose the unzipped folder from Spotify’s email, or the .zip itself.") : ""}
      ${R.offline() ? `<span class="muted" style="font-size:12px">Importing works offline. The files are read from your PC.</span>` : ""}
      <div class="row-flex" style="justify-content:flex-end"><a class="btn btn-secondary" href="#/settings">Cancel</a><button class="btn btn-primary" ${chosen && v() !== "error" ? "" : "disabled"} onclick="location.hash='#/import/2'">Next</button></div>`;
  } else if (step === 2) {
    if (v() === "loading") body = `<div class="card stack"><div class="row-flex"><span style="letter-spacing:3px">•••</span><span>Reading the export…</span></div></div>`;
    else {
      const rows = [["Music plays", `${R.dayY(p.from)} to ${R.dayY(p.until)}`, p.music], ["Podcasts and audiobooks", "Skipped. Runout keeps music only", p.podcasts_skipped, true], ["Already on your desktop log", "Merged with the export, never counted twice", p.overlap, true],
        ["Playlists", "For the read-only playlist check-up", p.playlists], ["Private-session plays", "Kept, but hidden from insights unless you turn them on", p.private, "warn"]];
      body = `<div class="card stack"><div class="between"><h2 class="card-title">What we found</h2><span class="mono muted" style="font-size:12px">Documents\\spotify-export</span></div>
        ${rows.map(([w, n, c, m]) => `<div class="between" style="padding:6px 0"><span class="stack" style="gap:2px"><span style="font-weight:600">${w}</span><span class="muted" style="font-size:12px">${n}</span></span><span class="mono ${m === "warn" ? "accent" : m ? "muted" : ""}" style="font-size:14px">${R.num(c)}</span></div>`).join("")}</div>
      <div class="cols"><div class="card stack col-main"><h2 class="card-title">What Runout keeps</h2><span class="text-2" style="line-height:1.55">When each play ended, how long it played, the track, artist, album and link, the device type, how it started and ended, shuffle, skip, offline and private-session flags.</span></div>
        <div class="card stack col-side"><h2 class="card-title">What it drops</h2><span class="text-2" style="line-height:1.55">Your IP address, user agent and username are never stored. The original files are not copied.</span></div></div>
      <div class="row-flex" style="justify-content:flex-end"><a class="btn btn-secondary" href="#/import/1">Back</a><button class="btn btn-primary" data-act="import-go">Import ${pl(p.music, "play")}</button></div>`;
    }
  } else {
    body = v() === "loading" || S.importing ? `<div class="card stack"><b>Importing…</b><div style="height:8px;border-radius:4px;background:var(--surface-2);overflow:hidden"><div style="height:100%;width:62%;background:var(--accent);border-radius:4px"></div></div><span class="mono muted" style="font-size:12px">${R.num(Math.round(p.music * 0.62))} of ${pl(p.music, "play")}</span><span class="muted" style="font-size:12px">You can keep using Runout while this runs.</span></div>`
      : v() === "error" ? R.error("Import stopped. Nothing was changed.", "One file couldn’t be read (streaming history, part 3). Your library is as it was.", `<a class="btn btn-secondary s" href="#/import/1">Start again</a>`)
      : v() === "empty" ? R.empty("Nothing new to import", `Every play in this export is already in Runout. Your history covers up to ${R.dayY(p.until)}.`, `<a class="btn btn-secondary" href="#/insights">Open insights</a>`)
      : `<div class="card stack" style="padding:28px"><b style="font-size:20px">Imported ${pl(p.music, "play")}.</b><span class="text-2">${R.num(p.overlap)} matched your desktop log and were merged. ${R.num(p.podcasts_skipped)} podcast entries skipped. Insights now cover ${R.dayY(p.from)} to ${R.dayY(p.until)}.</span>
          <div class="row-flex"><a class="btn btn-primary" href="#/insights">See your insights</a><a class="btn btn-secondary" href="#/plays/missing">Review plays not in your library</a></div><span class="muted" style="font-size:12px">Importing a newer export later only adds what’s new.</span></div>`;
  }
  return R.shell("settings", R.topbar() + `<a href="#/settings" style="font-size:13px;font-weight:600">Back to settings</a><div class="stack" style="gap:6px"><h1 class="page-title">Import your Spotify history</h1><span class="muted">Uses the files Spotify emails you after you request your data.</span></div>${steps}${body}`);
};

/* ================================================================ SC-11 Playlists */
const playlistsScreen = (r) => {
  const tab = r.args[0] || (R.isR3() ? "studio" : "checkup");
  const idx = Number(r.params.p || 0); const plst = D.playlists[idx];
  const list = `<div class="stack" style="flex:0 0 240px;gap:6px"><h2 class="muted" style="font-size:13px;font-weight:700;padding:0 8px">${tab === "studio" ? "Your playlists on Spotify" : "From your export of " + R.day(L.freshness.export_until)}</h2>${D.playlists.map((p, i) =>
    `<a href="#/playlists/${tab}?p=${i}" class="row" style="grid-template-columns:minmax(0,1fr);${i === idx ? "background:var(--surface-2)" : ""}"><span class="stack" style="gap:2px;min-width:0"><span class="ellipsis" style="font-weight:600;color:var(--text)">${esc(p.name)}</span><span class="muted" style="font-size:12px">${pl(p.tracks.length, "track")}${[p.dupes ? pl(p.dupes, "duplicate") : "", p.low ? `${p.low} rated low` : ""].filter(Boolean).map((x) => `, <span class="accent">${x}</span>`).join("")}</span></span></a>`).join("")}</div>`;
  const tabs = R.isR3() ? R.seg("Mode", [["checkup", `Check-up (export of ${R.day(L.freshness.export_until)})`], ["studio", "Studio (live from Spotify)"]], tab, (x) => `#/playlists/${x}?p=${idx}`) : "";
  const wrap = (body) => R.shell("playlists", R.topbar() + `<div class="cols">${list}<div class="col-main" style="flex:1 1 620px"><div class="page-head"><div class="stack" style="gap:4px"><h1 class="page-title">${esc(plst.name)}</h1><span class="muted">Your playlist, ${pl(plst.tracks.length, "track")}</span></div>${tabs}</div>${body}</div></div>`);
  if (tab === "checkup") {
    if (v() === "loading") return wrap(R.skelRows(8));
    if (v() === "empty") return wrap(R.empty("No playlists in your export", "Import an Account data export from Spotify to check your playlists.", `<a class="btn btn-secondary" href="#/import/1">Import</a>`));
    if (v() === "error") return wrap(errorFor("this playlist"));
    return wrap(`<p class="text-2" style="margin:0">Read-only. Shows the playlist as it was in your export. ${src("E", "U", "S")} To change it, use Studio${R.isR3() ? "" : " (Release 3)"}.</p>
      <div class="stats">${R.stat("Duplicates", plst.dupes, "Same track twice", plst.dupes > 0)}${R.stat("Rated 2 or lower", plst.low, "By you", plst.low > 0)}${R.stat("Never played", plst.tracks.filter((t) => t.never_played).length, "In your plays")}${R.stat("Tracks", plst.tracks.length, "In the export")}</div>
      <div class="card pad-s"><div class="list">${plst.tracks.map((t) => `<div class="row" style="grid-template-columns:28px minmax(0,1fr) 80px 160px"><span class="mono faint" style="font-size:12px">${t.position}</span><span class="stack" style="gap:2px"><span style="font-weight:600">${esc(t.track)}</span><span class="muted" style="font-size:12px">${esc(t.artist)}</span></span>
        <span class="stars" aria-label="${t.rating ? "Rated " + t.rating : "Not rated"}">${R.stars(t.rating)}</span><span style="font-size:12px" class="${t.duplicate_of || (t.rating && t.rating <= 2) ? "accent" : "muted"}">${t.duplicate_of ? `Duplicate of ${t.duplicate_of}` : t.rating && t.rating <= 2 ? "Rated low" : t.never_played ? "Never played" : ""}</span></div>`).join("")}</div></div>
      ${R.isR3() ? `<a class="btn btn-primary" style="align-self:flex-start" href="#/playlists/studio?p=${idx}">Open in Studio</a>` : ""}`);
  }
  if (v() === "empty") return wrap(R.empty("Connect Spotify to use Studio", "Studio edits your own playlists on Spotify. The check-up from your export works without connecting.", `<button class="btn btn-primary" data-act="dialog" data-v="connect">Connect Spotify</button>`));
  if (v() === "loading") return wrap(`<span class="muted">Reading the playlist from Spotify…</span>` + R.skelRows(8));
  if (R.offline()) return wrap(R.empty("Studio needs a connection", "Your draft is kept. Reconnect to read the playlist from Spotify and apply changes.", ""));
  const auto = (t) => (t.duplicate_of || (t.rating && t.rating <= 2) || t.tags.includes("not-jazz") ? "cut" : "keep");
  const state = (t) => S.draft[`${idx}:${t.position}`] || auto(t);
  const changes = plst.tracks.filter((t) => state(t) === "cut");
  R.studioChanges = () => changes;
  R.studioName = plst.name;
  const applied = S.draft.applied;
  return wrap(`<div class="row-flex">${R.logo()}<span class="text-2" style="font-size:13px">Tracks from Spotify, read just now, with your own ratings and tags. No listening data is used here.</span>${R.openSpotify(plst.name)}</div>
      ${v() === "error" ? R.error("This playlist changed on Spotify.", "Someone edited it since you started. Reload to see the new version; your draft is kept.", `<button class="btn btn-secondary s" data-act="toast" data-v="Playlist reloaded">Reload</button>`) : ""}
      ${applied ? `<div class="banner"><span><b>Applied ${R.day(D.meta.today)}.</b> <span class="text-2">Undo is available until ${R.day("2026-11-17")}.</span></span><button class="btn btn-secondary s" data-act="undo-apply">Undo last apply</button></div>` : ""}
      <div class="cols"><div class="card pad-s col-main"><div class="list">${plst.tracks.map((t) => { const cut = state(t) === "cut";
        const why = t.duplicate_of ? `Duplicate of ${t.duplicate_of}` : t.rating && t.rating <= 2 ? `You rated it ${t.rating}` : t.tags.includes("not-jazz") ? "Tagged not-jazz" : "";
        return `<div class="row ${cut ? "sel" : ""}" style="grid-template-columns:24px minmax(160px,1fr) 64px auto auto"><span class="mono faint" style="font-size:12px">${t.position}</span>
          <span class="stack" style="gap:2px;min-width:0"><span style="font-weight:600;${cut ? "text-decoration:line-through;color:var(--muted)" : ""}">${esc(t.track)}</span><span class="muted" style="font-size:12px">${esc(t.artist)}${why ? ` <span class="accent">· ${why}</span>` : ""}</span></span>
          <span class="stars" aria-label="${t.rating ? "Rated " + t.rating : "Not rated"}">${R.stars(t.rating)}</span><span></span>
          <div class="seg" role="group" aria-label="Keep or cut ${esc(t.track)}"><button aria-pressed="${!cut}" data-act="cut" data-id="${idx}:${t.position}" data-v="keep">Keep</button><button aria-pressed="${cut}" data-act="cut" data-id="${idx}:${t.position}" data-v="cut">Cut</button></div></div>`; }).join("")}</div></div>
      <aside class="col-side card stack" style="flex:0 1 300px"><h2 class="card-title">Your changes</h2><span class="muted" style="font-size:13px">Draft only. Nothing changes on Spotify until you apply.</span>
        ${changes.length ? changes.map((t) => `<div class="stack" style="gap:2px;padding:10px 12px;border-radius:10px;background:var(--surface-2)"><span class="accent" style="font-size:12px;font-weight:700">${t.duplicate_of ? "Remove duplicate" : "Remove"}</span><span>${esc(t.track)}</span></div>`).join("") : `<span class="muted">No changes yet.</span>`}
        <button class="btn btn-primary l" ${changes.length && !applied ? "" : "disabled"} data-act="dialog" data-v="apply">Apply ${pl(changes.length, "change")} to Spotify</button>
        <span class="muted" style="font-size:12px;line-height:1.45">Runout checks the playlist hasn’t changed on Spotify first. You can undo for 30 days.</span>
        <button class="btn btn-secondary" data-act="discard">Discard draft</button></aside></div>`);
};

/* ================================================================ SC-12 From Spotify */
const spotifyScreen = (r) => {
  const tab = r.args[0] || "tracks"; const range = r.params.r || "4w";
  const conn = v() === "empty" ? "none" : v() === "error" ? "expired" : "ok";
  const head = `<div class="page-head"><div class="stack" style="gap:8px"><div class="row-flex">${R.logo()}<h1 class="page-title">From Spotify</h1></div><span class="muted">Shown exactly as Spotify provides it. Not used in your insights.</span></div>
    ${conn === "ok" ? `<div class="row-flex">${R.seg("Top items", [["tracks", "Top tracks"], ["artists", "Top artists"]], tab, (x) => `#/spotify/${x}?r=${range}`)}${R.seg("Spotify time range", [["4w", "4 weeks"], ["6m", "6 months"], ["1y", "1 year"]], range, (x) => `#/spotify/${tab}?r=${x}`)}</div>` : ""}</div>`;
  if (conn === "none") return R.shell("spotify", R.topbar() + head + R.empty("Spotify isn’t connected", "Connect to see your Spotify Top items and recent plays from every device. Everything else in Runout keeps working without it.", `<button class="btn btn-primary" data-act="dialog" data-v="connect">Connect Spotify</button>`));
  if (conn === "expired") return R.shell("spotify", R.topbar() + head + `<div class="banner warn"><span><b class="accent">Spotify needs you to sign in again.</b> <span class="text-2">Spotify asks every 6 months. If your Premium plan has lapsed, Spotify features stay paused until it’s active. Your data in Runout is unaffected.</span></span><button class="btn btn-primary s" data-act="dialog" data-v="connect">Reconnect Spotify</button></div>`);
  if (v() === "loading") return R.shell("spotify", R.topbar() + head + R.skelRows(10));
  if (R.offline()) return R.shell("spotify", R.topbar() + head + R.empty("You’re offline", "Top items and recent plays come straight from Spotify, so they aren’t stored on this PC. They return when you reconnect.", ""));
  const rows = tab === "tracks" ? D.spotify.top_tracks[range] : D.spotify.top_artists[range];
  const recent = D.spotify.recent.slice(0, 8);
  const addBtn = (name, artist, uri, kind) => `<button class="btn btn-secondary s" ${R.addAttrs(uri ? "https://open.spotify.com/track/" + uri.split(":")[2] : NEW_ALBUM[0], name, artist, kind)} aria-label="Add ${esc(name)} to backlog">Add to backlog</button>`;
  return R.shell("spotify", R.topbar() + head + `<div class="cols"><section class="col-main" aria-label="Your top ${tab} on Spotify"><div class="card pad-s"><div class="list">${rows.slice(0, 20).map((t) => `<div class="row" style="grid-template-columns:28px minmax(0,1fr) auto"><span class="mono faint" style="font-size:12px">${t.rank}</span>
      <span class="stack" style="gap:2px;min-width:0"><span class="ellipsis" style="font-weight:600">${esc(t.track || t.artist)}</span>${t.track ? `<span class="muted" style="font-size:12px">${esc(t.artist)}</span>` : ""}</span>
      ${t.in_library ? `<span class="muted" style="font-size:12px">In your library</span>` : addBtn(t.track || t.artist, t.track ? t.artist : "", null, t.track ? "track" : "artist")}</div>`).join("")}</div></div>
      ${setFooter(Math.min(20, rows.length), 50, "your top items")}</section>
    <aside class="col-side"><section class="card stack" aria-labelledby="fs-recent"><h2 id="fs-recent" class="card-title">Recently played on Spotify</h2><span class="muted" style="font-size:13px;line-height:1.45">Spotify’s last 50 finished plays from all your devices. Catch something you heard on the go.</span>
      ${recent.map((x) => `<div class="between" style="align-items:center;padding:4px 0"><span class="stack" style="gap:2px;min-width:0"><span class="ellipsis" style="font-weight:600">${esc(x.track)}</span><span class="muted" style="font-size:12px">${esc(x.artist)}, ${R.ago(x.at).toLowerCase()}</span></span>
        <span class="row-flex" style="flex:none">${x.in_library ? `<span class="muted" style="font-size:12px">${x.in_library === "album" ? "Album in your library" : "In your library"}</span>` : addBtn(x.track, x.artist, x.spotify_uri, "track")}</span></div>`).join("")}
      <span class="row-flex">${R.logo()}${R.openSpotify("your recent plays")}</span></section></aside></div>`);
};

/* ================================================================ SC-13 Settings */
const settingsScreen = (r) => {
  const sec = r.args[0];
  if (sec === "tags") return tagsScreen();
  const st = D.settings; const connected = R.isR3() && v() !== "empty";
  const card = (id, title, body) => `<section class="card stack" aria-labelledby="st-${id}"><h2 id="st-${id}" class="card-title">${title}</h2>${body}</section>`;
  const sw = (lbl, on) => `<label class="switch-row"><span>${lbl}</span><input type="checkbox" ${on ? "checked" : ""} data-act="toast" data-v="Setting saved"></label>`;
  if (v() === "loading") return R.shell("settings", R.topbar() + `<h1 class="page-title">Settings and data</h1>` + R.skelRows(6));
  const captureOff = v() === "error";
  return R.shell("settings", R.topbar() + `<h1 class="page-title">Settings and data</h1>
    ${v() === "error" ? R.error("Last backup was larger than your limit.", "It was 112 MB; your limit is 100 MB. The backup was still kept.", `<button class="btn btn-secondary s" data-act="toast" data-v="Limit raised to 150 MB">Raise the limit</button>`) : ""}
    <div class="tiles" style="grid-template-columns:repeat(2,minmax(0,1fr));align-items:start">
    ${card("data", "Your data", `<div class="field"><span class="label">Library location</span><span class="mono" style="font-size:12px">${esc(st.library_path)}</span><button class="btn btn-secondary s" style="align-self:flex-start" data-act="toast" data-v="Pick a new folder. A library moved elsewhere isn’t removed when you uninstall.">Change location</button></div>
      <div class="field"><span class="label">Backups, weekly, newest 3 kept</span>${st.backups.map((b, i) => `<span class="between"><span>${R.day(b.at)}, ${R.time(b.at)}${i === 0 ? " (latest)" : ""}</span><span class="mono text-2" style="font-size:12px">${b.mb} MB</span></span>`).join("")}
        <span class="mono text-2" style="font-size:12px">${esc(st.backup_dir)}</span><label class="row-flex" style="font-size:13px">Warn when a backup is over <input class="input" style="height:32px;width:70px" value="100" aria-label="Backup size limit in MB"> MB</label></div>
      <div class="row-flex"><button class="btn btn-light" data-act="backup-now">Back up now</button><button class="btn btn-secondary" data-act="dialog" data-v="restore">Restore</button><button class="btn btn-secondary" data-act="toast" data-v="Exported runout-export.json to Documents (format: docs/EXPORT_FORMAT.md)">Export as JSON</button></div>`)}
    ${card("spotify", "Spotify connection", R.isR3() ? (connected ? (R.offline() ? `<span>Connected, but you’re offline. Spotify features pause until you reconnect.</span>` : `<span>Connected. Spotify asks you to sign in again by ${R.dayY(D.spotify.renew_by)}.</span><span class="muted" style="font-size:13px;line-height:1.5">Used for Playlist Studio, autofill, Saved in Spotify and the From Spotify page. Disconnecting deletes everything Runout cached from Spotify, right away.</span><div><button class="btn btn-secondary" data-act="dialog" data-v="disconnect">Disconnect</button></div>`)
      : `<span>Not connected.</span><span class="muted" style="font-size:13px">Needs a Spotify Premium account. Runout opens Spotify’s sign-in page in your browser.</span><div><button class="btn btn-primary" data-act="dialog" data-v="connect">Connect Spotify</button></div>`)
      : `<span class="muted">Arrives in Release 3: Playlist Studio, autofill and the From Spotify page.</span>`)}
    ${R.isR2() ? card("logger", "Desktop logger", `${sw("Log what Spotify plays on this PC", st.logger.on && !S.paused)}${sw("Start with Windows, in the tray", st.logger.autostart)}${sw("Include private-session plays in insights", st.logger.include_private)}<span class="muted" style="font-size:12px">A play counts after 30 seconds. Only Spotify is logged. <a href="#/tray">See the tray menu</a></span>`) : ""}
    ${card("phone", "Phone capture", captureOff ? `${R.error("Phone capture turned itself off.", "5 wrong pairing codes in a row. Make a new code and update your Shortcut.")}<div><button class="btn btn-primary" data-act="new-code">New code and turn on</button></div>`
      : `${sw("Accept shares from my iPhone on this Wi-Fi", st.capture.on)}<div class="tiles" style="grid-template-columns:repeat(2,minmax(0,1fr))"><div class="field"><span class="label">Address</span><span class="mono">${st.capture.address}</span></div><div class="field"><span class="label">Pairing code</span><span class="mono" style="font-size:18px;letter-spacing:2px">${st.capture.code}</span></div></div>
        <span class="muted" style="font-size:12px">Windows may ask to allow Runout on private networks. Your Wi-Fi must be set to Private.</span><div class="row-flex"><button class="btn btn-light" data-act="dialog" data-v="shortcut">Shortcut steps</button><button class="btn btn-secondary" data-act="new-code">New code</button></div>`)}
    ${R.isR2() ? card("import", "Spotify history", `<span>Last import covers plays up to ${R.dayY(L.freshness.export_until)}.</span><span class="muted" style="font-size:13px;line-height:1.5">Request a fresh copy from Spotify’s privacy page, then import it here. Plays already imported are skipped.</span><div><a class="btn btn-light" href="#/import/1">Import an export</a></div>`) : ""}
    ${card("tags", "Tags", `<span class="text-2">${pl(new Set(R.items.flatMap((i) => i.tags)).size, "tag")} in use.</span><div><a class="btn btn-secondary" href="#/settings/tags">Rename, merge or delete tags</a></div>`)}
    ${card("metrics", "Is Runout working for you?", `<span class="muted" style="font-size:13px">Counted on this PC only, never sent anywhere.</span><div class="tiles" style="grid-template-columns:repeat(2,minmax(0,1fr))">
      <div class="stack" style="gap:2px"><span class="mono" style="font-size:20px;font-weight:600">${D.usage.capture_median_s} s</span><span class="muted" style="font-size:12px">Typical time to save a link</span></div>
      <div class="stack" style="gap:2px"><span class="mono" style="font-size:20px;font-weight:600">${D.usage.days_opened_this_week} of 7</span><span class="muted" style="font-size:12px">Days opened this week</span></div>
      <div class="stack" style="gap:2px"><span class="mono" style="font-size:20px;font-weight:600">${D.usage.searches_this_week}</span><span class="muted" style="font-size:12px">Searches this week</span></div>
      <div class="stack" style="gap:2px"><span class="mono" style="font-size:20px;font-weight:600">${D.usage.rediscover_visits_this_week}</span><span class="muted" style="font-size:12px">Rediscover visits this week</span></div></div>`)}
    </div>
    <section class="card danger" aria-labelledby="st-danger" style="display:flex;flex-wrap:wrap;justify-content:space-between;align-items:center;gap:12px"><div class="stack" style="gap:4px"><h2 id="st-danger" class="card-title" style="color:var(--danger)">Delete your library</h2><span class="text-2">Removes every record, note and play from this PC. Backups are kept.</span></div>
      <div class="row-flex"><a class="btn btn-secondary" href="#/uninstall">What uninstall removes</a><button class="btn btn-danger" data-act="dialog" data-v="deleteLib">Delete library</button></div></section>`);
};
const tagsScreen = () => {
  const c = {}; R.items.forEach((i) => i.tags.forEach((t) => (c[t] = (c[t] || 0) + 1)));
  const entries = v() === "empty" ? [] : Object.entries(c).sort((a, b) => b[1] - a[1]);
  return R.shell("settings", R.topbar() + `<a href="#/settings" style="font-size:13px;font-weight:600">Back to settings</a><h1 class="page-title">Tags</h1>` +
    (entries.length ? `<div class="card pad-s"><div class="list">${entries.map(([t, n]) => `<div class="row" style="grid-template-columns:minmax(0,1fr) 100px auto"><span class="chip" style="justify-self:start">${esc(t)}</span><span class="mono muted" style="font-size:12px">${pl(n, "record")}</span>
      <span class="row-flex"><button class="btn btn-secondary s" data-act="dialog" data-v="tagRename" data-id="${esc(t)}" aria-label="Rename ${esc(t)}">Rename</button><button class="btn btn-secondary s" data-act="dialog" data-v="tagMerge" data-id="${esc(t)}" aria-label="Merge ${esc(t)}">Merge into…</button><button class="btn btn-danger s" data-act="dialog" data-v="tagDelete" data-id="${esc(t)}" aria-label="Delete ${esc(t)}">Delete</button></span></div>`).join("")}</div></div>`
      : R.empty("No tags yet", "Add tags on any record to group music your way.", `<a class="btn btn-secondary" href="#/library">Open your library</a>`)));
};

/* ================================================================ Tray, uninstall */
const trayScreen = () => `<div class="app" style="background:linear-gradient(180deg,#1B2735,#0E141B)"><main class="main" style="margin:0 auto;max-width:900px;padding-top:80px">
  <h1 class="page-title">Tray menu</h1><span class="text-2">Right-click the Runout icon in the Windows notification area. The window can be closed; the logger keeps running here.</span>
  <div class="cols">${[false, true].map((paused) => `<div class="stack" style="flex:1 1 300px"><span class="muted" style="font-size:12px">${paused ? "While paused" : "While logging"}</span>
    <div class="overlay" style="padding:6px;min-width:280px" role="menu" aria-label="Runout tray menu">
      <div style="padding:10px 12px" class="stack"><b>Runout</b><span class="muted" style="font-size:12px">${paused ? "Logging paused until you resume" : "Logging Spotify. Last play 1 min ago"}</span></div>
      <div class="menu" style="position:static;padding:0">${(paused ? [["Resume logging", ""]] : [["Pause for 1 hour", ""], ["Pause until I resume", ""]]).concat([["Quick add", "Ctrl+N"], ["Open Runout", ""], ["Quit Runout", ""]]).map(([t, k]) => `<button role="menuitem" type="button">${t}<span class="kbd">${k}</span></button>`).join("")}</div></div></div>`).join("")}</div>
  <a href="#/home">Back to Runout</a></main></div>`;
const uninstallScreen = () => `<div class="app" style="background:#0A0B0C"><main class="main" style="margin:0 auto;max-width:640px;padding-top:80px">
  <div class="overlay" style="padding:24px;display:flex;flex-direction:column;gap:16px;background:#F3F3F3;color:#1B1B1B;border-radius:8px;font-family:'Segoe UI',system-ui,sans-serif">
    <b style="font-size:16px">Runout Uninstall</b><span>Runout will be removed from your computer.</span>
    <span style="font-size:13px">Always removed: the Spotify connection, everything Runout cached from Spotify, and its web data.</span>
    <label style="display:flex;gap:8px;align-items:center"><input type="checkbox">Also delete my music library</label>
    <span style="font-size:13px;color:#444">Your backups in Documents\\Runout\\Backups are never removed. A library you moved to another folder stays where it is.</span>
    <div style="display:flex;justify-content:flex-end;gap:8px"><button style="padding:6px 16px">Uninstall</button><button style="padding:6px 16px">Cancel</button></div></div>
  <span class="muted" style="font-size:12px">Windows installer dialog (NSIS), shown for reference. TSD §4.11.</span><a href="#/settings">Back to settings</a></main></div>`;

R.screens = {
  home: () => (R.isR2() ? homePulse() : homeMvp()), backlog: backlogScreen, library: libraryScreen, search: searchScreen, item: itemScreen,
  rediscover: rediscoverScreen, plays: playsScreen, insights: insightsScreen, import: importScreen, playlists: playlistsScreen,
  spotify: spotifyScreen, settings: settingsScreen, "first-run": firstRun, tray: trayScreen, uninstall: uninstallScreen,
  system: () => R.systemScreen(),
};
Object.assign(R.actions, {
  art: () => { S.art = !S.art; R.render(); },
  range: (a, id, x) => { S.range = x; R.render(); },
  irange: (a, id, x) => { S.irange = x; R.render(); },
  "bl-type": (a, id, x) => { S.backlogFilter.type = x; R.render(); },
  "bl-reset": () => { S.backlogFilter = { type: "all", tag: "", sort: "order" }; R.render(); },
  "lib-reset": () => { S.libFilter = { status: "any", rating: "any", page: 1 }; R.render(); },
  "red-revisit": () => { S.redFilter.revisit = !S.redFilter.revisit; R.render(); },
  "apply-filter": (a, id) => { const f = D.saved_filters.find((x) => x.id === Number(id)); if (S.activeFilter === f.id) { S.activeFilter = null; S.redFilter = { tag: "", min: "any", revisit: false }; } else { S.activeFilter = f.id; S.redFilter = { tag: f.definition.tags[0] || "", min: f.definition.min_rating ? String(f.definition.min_rating) : "any", revisit: false }; } R.render(); },
  "rate-open": (a, id) => R.go(`#/item/${id}`),
  toast: (a, id, x) => R.toast(esc(x)),
  "import-choose": () => { S.importChosen = true; R.render(); },
  "undo-apply": () => { S.draft = {}; R.toast("Late night jazz restored to how it was before you applied"); },
  "spotify-save": (a, id) => { const it = R.byId(id); it.spotify_saved = !it.spotify_saved; R.toast(it.spotify_saved ? `${esc(R.nameOf(it))} saved to your Spotify library` : `${esc(R.nameOf(it))} removed from your Spotify library`, () => { it.spotify_saved = !it.spotify_saved; }); },
});
R.actions["import-go"] = () => { S.importing = true; R.go("#/import/3"); setTimeout(() => { S.importing = false; R.render(); }, 1600); };
document.addEventListener("keydown", (e) => {
  const el = document.activeElement;
  if (el && el.dataset && el.dataset.rating && (e.key === "ArrowRight" || e.key === "ArrowLeft")) {
    const it = R.byId(el.dataset.rating); it.rating = Math.min(5, Math.max(0.5, (it.rating || 0) + (e.key === "ArrowRight" ? 0.5 : -0.5)));
    e.preventDefault(); R.render(); const n = document.querySelector(`[data-rating="${it.id}"]`); if (n) n.focus();
  }
});
})();

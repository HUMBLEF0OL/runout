// Regenerate docs/design/screens/*.png from the design prototype.
// Run with the Playwright MCP tool `browser_run_code_unsafe` (filename: scripts/capture_designs.js)
// while serving the prototype: python -m http.server 8765 --bind 127.0.0.1 --directory docs/design/prototype
// Fails (throws) if any screen, state or dialog renders an error or leftover text.
async (page) => {
  const OUT = "docs/design/screens/"; // relative to the workspace root
  const client = await page.context().newCDPSession(page);
  await client.send("Network.setCacheDisabled", { cacheDisabled: true });
  await page.setViewportSize({ width: 1440, height: 900 });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("http://127.0.0.1:8765/index.html?v=" + Date.now() + "#/home");
  await page.waitForFunction(() => window.R && window.R.screens);
  await page.evaluate(() => new Promise((ok) => { const l = document.querySelector('link[rel="stylesheet"][href*="styles.css"]'); l.onload = ok; l.href = "styles.css?v=" + Date.now(); }));
  const dupLink = "https://" + (await page.evaluate(() => R.uriLink(R.byName("album", "Thembi"))));

  // 1. sweep: every route x state x release, every dialog
  const problems = await page.evaluate(() => {
    const routes = ["home", "backlog", "library/album", "library/track", "library/artist", "search/coltrane", "search/zzz", "item/19", "item/1", "item/50", "item/27",
      "rediscover", "plays", "plays/unrated", "plays/missing", "insights", "insights/collection", "insights/take", "insights/year", "import/1", "import/2", "import/3",
      "playlists/checkup", "playlists/studio?p=1", "spotify", "spotify/artists?r=6m", "settings", "settings/tags", "first-run", "tray", "uninstall", "system"];
    const out = [];
    for (const rel of ["mvp", "r2", "r3"]) for (const r of routes) for (const v of ["normal", "empty", "loading", "offline", "error"]) {
      R.S.release = rel; R.S.view = v; R.S.dialog = null; history.replaceState(null, "", "#/" + r);
      try { R.render(); const t = document.getElementById("app").innerText;
        for (const bad of ["undefined", "NaN", "[object", "—", " 1 days", " 1 plays", " 1 records"]) if (t.includes(bad)) out.push(`${rel} ${r} ${v}: ${bad}`);
      } catch (e) { out.push(`${rel} ${r} ${v}: ${e.message}`); }
    }
    for (const k of Object.keys(R.dialogs)) {
      R.S.view = "normal"; R.S.release = "r3"; history.replaceState(null, "", "#/playlists/studio"); R.render();
      R.S.dialog = { kind: k, link: "https://open.spotify.com/album/3QJzdZJYIAcoET1GcfpNGi", id: k === "deleteItem" ? 19 : k === "makePlaylist" ? "Your best" : "jazz" };
      try { R.render(); } catch (e) { out.push(`dialog ${k}: ${e.message}`); }
    }
    R.S.dialog = { kind: "autofill" }; R.render();
    const cb = document.querySelector(".dialog input[type=checkbox]"); const before = cb.checked; cb.click();
    if (cb.checked === before) out.push("dialog checkbox does not toggle");
    R.S.dialog = null; R.S.view = "normal"; R.render();
    return out;
  });
  if (errors.length || problems.length) throw new Error(JSON.stringify({ errors, problems }));

  // 2. screenshots
  await page.evaluate(() => document.body.classList.add("capture"));
  const set = (rel, view, hash, dialog, extra) => page.evaluate(([rel, view, hash, dialog, extra]) => {
    R.S.release = rel; R.S.view = view; R.S.sel.clear(); R.S.art = false; R.S.draft = {}; R.S.importing = false;
    history.replaceState(null, "", hash); R.S.dialog = dialog;
    if (extra === "sel") R.items.filter((i) => i.in_backlog).slice(0, 2).forEach((i) => R.S.sel.add(i.id));
    R.S.libFilter.rating = extra === "art" ? "4.5" : "any"; if (extra === "art") R.S.art = true;
    R.render(); window.scrollTo(0, 0);
  }, [rel, view, hash, dialog, extra]);
  const A = (link, extra = {}) => ({ kind: "quickAdd", link, ...extra });
  const shots = [
    ["SC-01-home", "r3", "normal", "#/home", null, null, true], ["SC-02-home-mvp", "mvp", "normal", "#/home", null, null, true],
    ["SC-02b-first-run", "mvp", "empty", "#/home", null, null, true], ["SC-03-backlog", "mvp", "normal", "#/backlog", null, "sel", true],
    ["SC-04-library", "mvp", "normal", "#/library/album", null, null, true], ["SC-04b-library-artwork", "mvp", "normal", "#/library/album", null, "art", true],
    ["SC-04c-search", "r2", "normal", "#/search/late", null, null, true], ["SC-05-detail-album", "r2", "normal", "#/item/19", null, null, true],
    ["SC-05b-detail-artist", "r2", "normal", "#/item/1", null, null, true], ["SC-05c-detail-track", "r3", "normal", "#/item/50", null, null, true],
    ["SC-05d-detail-edit", "r2", "normal", "#/item/19?edit=1", null, null, false],
    ["SC-06-quick-add", "mvp", "normal", "#/home", A("https://open.spotify.com/album/3QJzdZJYIAcoET1GcfpNGi"), null, false],
    ["SC-06b-quick-add-autofill", "r3", "normal", "#/home", A("https://open.spotify.com/album/3QJzdZJYIAcoET1GcfpNGi"), null, false],
    ["SC-06c-quick-add-duplicate", "mvp", "normal", "#/home", A(dupLink), null, false],
    ["SC-06d-quick-add-offline", "mvp", "offline", "#/home", A("https://spotify.link/aXk3P9q"), null, false],
    ["SC-06e-quick-add-unsupported", "mvp", "normal", "#/home", A("https://open.spotify.com/episode/4rOoJ6Egrf8K2IrywzwOMk"), null, false],
    ["SC-06f-quick-add-manual", "mvp", "normal", "#/home", A("", { manual: true }), null, false],
    ["SC-06g-quick-add-artist", "mvp", "normal", "#/home", A("https://open.spotify.com/artist/7mEBp3V1D8Xz2Rk4Qc9TnY"), null, false],
    ["SC-07-rediscover", "r3", "normal", "#/rediscover", null, null, true],
    ["SC-07b-make-playlist", "r3", "normal", "#/rediscover", { kind: "makePlaylist", id: "Your best" }, null, false],
    ["SC-08-plays", "r2", "normal", "#/plays", null, null, true], ["SC-09-insights-listening", "r2", "normal", "#/insights", null, null, true],
    ["SC-09b-insights-collection", "r2", "normal", "#/insights/collection", null, null, true], ["SC-09c-insights-take", "r2", "normal", "#/insights/take", null, null, true],
    ["SC-09d-insights-year", "r2", "normal", "#/insights/year", null, null, true], ["SC-10-import-choose", "r2", "normal", "#/import/1", null, null, false],
    ["SC-10b-import-check", "r2", "normal", "#/import/2", null, null, true], ["SC-10c-import-done", "r2", "normal", "#/import/3", null, null, false],
    ["SC-11-playlists-checkup", "r3", "normal", "#/playlists/checkup?p=1", null, null, true], ["SC-11b-playlists-studio", "r3", "normal", "#/playlists/studio?p=0", null, null, true],
    ["SC-11c-playlists-apply", "r3", "normal", "#/playlists/studio?p=0", { kind: "apply" }, null, false],
    ["SC-12-from-spotify", "r3", "normal", "#/spotify", null, null, true], ["SC-13-settings", "r3", "normal", "#/settings", null, null, true],
    ["SC-13b-tags", "r3", "normal", "#/settings/tags", null, null, true],
    ["SC-13c-connect-spotify", "r3", "empty", "#/settings", { kind: "connect", step: "start" }, null, false],
    ["SC-13d-restore", "r3", "normal", "#/settings", { kind: "restore" }, null, false],
    ["SC-13e-shortcut-steps", "mvp", "normal", "#/settings", { kind: "shortcut" }, null, false],
    ["SC-13f-delete-library", "mvp", "normal", "#/settings", { kind: "deleteLib" }, null, false],
    ["X-tray-menu", "r2", "normal", "#/tray", null, null, false], ["X-uninstall", "r3", "normal", "#/uninstall", null, null, false],
    ["DS-design-system", "r3", "normal", "#/system", null, null, true],
  ];
  // animations: "disabled" jumps the 180 ms dialog fade to its end; without it dialogs were captured invisible or half-faded
  const shoot = async (path, fullPage, dialog) => {
    if (dialog && !(await page.$(".dialog"))) throw new Error(`${path}: dialog did not open`);
    await page.screenshot({ path, fullPage, animations: "disabled" });
  };
  for (const [name, rel, view, hash, dialog, extra, full] of shots) { await set(rel, view, hash, dialog, extra); await shoot(OUT + name + ".png", full, dialog); }
  const states = [["home", "r3", "#/home"], ["home-mvp", "mvp", "#/home"], ["backlog", "mvp", "#/backlog"], ["library", "mvp", "#/library/album"], ["search", "r2", "#/search/jazz"],
    ["detail", "r3", "#/item/19"], ["rediscover", "r3", "#/rediscover"], ["plays", "r2", "#/plays"], ["insights", "r2", "#/insights"], ["import", "r2", "#/import/1"],
    ["playlists-studio", "r3", "#/playlists/studio"], ["from-spotify", "r3", "#/spotify"], ["settings", "r3", "#/settings"]];
  for (const [name, rel, hash] of states) for (const view of ["loading", "empty", "offline", "error"]) {
    if (name === "home-mvp" && view === "empty") continue; // that is the first-run screen, SC-02b
    // import draws loading on step 2 and empty ("nothing new") on step 3
    const h = name !== "import" ? hash : view === "loading" ? "#/import/2" : view === "empty" ? "#/import/3" : hash;
    await set(rel, view, h, null, null); await shoot(`${OUT}states/${name}-${view}.png`, false, null);
  }
  return `sweep clean; ${shots.length} screens and ${states.length * 4 - 1} state shots written`;
}

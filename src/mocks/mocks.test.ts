import { describe, expect, it } from "vitest";
import { applyStudioDraft, createPlaylistFromFilter, getLastApply, undoLastApply, createItem, listSpotifyPlaylists, loadStudioDraft, getCounts, getDashboard, getItem, getTakeInsights, listPlays, linkItems, listItems, listSavedFilters, listTags, renameTag, saveFilter, setStatus, unlinkItems, updateItem, type AppError } from "../api";
import { setDev } from "./devState";

describe("mock commands (API.md)", () => {
  it("create_item with a URI already in the library fails with duplicate + existing_id (FR-004)", async () => {
    const [first] = (await listItems({ limit: 100 })).rows.filter((i) => i.spotify_uri);
    const err = await createItem({ type: first.type, display_name: "x", spotify_uri: first.spotify_uri, provenance: "promoted" }).catch((e: AppError) => e);
    expect(err).toMatchObject({ code: "duplicate", existing_id: first.id });
  });
  it("create_item adds to the backlog and counts follow", async () => {
    const before = await getCounts();
    const it = await createItem({ type: "album", display_name: "New one", spotify_uri: "spotify:album:AAAAAAAAAAAAAAAAAAAAAA", provenance: "promoted" });
    expect(it.in_backlog).toBe(true);
    expect((await getCounts()).library).toBe(before.library + 1);
    expect(it.history[0]).toMatchObject({ from: null, to: "planned" });
  });
  it("create_item sets provenance per field: promoted_fields promoted, the rest the record's (TSD §4.2)", async () => {
    const it = await createItem({ type: "album", display_name: "Mine", spotify_uri: "spotify:album:BBBBBBBBBBBBBBBBBBBBBB", provenance: "user",
      artist_name: "From Spotify Artist", recommended_by: "Sam", promoted_fields: ["artist"] });
    expect(it.provenance).toMatchObject({ display_name: "user", artist: "promoted", recommended_by: "user" });
  });
  it("set_status writes history and explored leaves the backlog", async () => {
    const [b] = (await listItems({ in_backlog: true, status: ["planned"] })).rows;
    const d = await setStatus(b.id, "explored");
    expect(d.status).toBe("explored");
    expect(d.in_backlog).toBe(false);
    expect(d.history[0]).toMatchObject({ from: "planned", to: "explored" });
    const back = await setStatus(b.id, "planned");
    expect(back.explored_at).toBeNull();
    expect(back.in_backlog).toBe(true);
    const orders = (await listItems({ in_backlog: true, limit: 1000 })).rows.map((r) => r.backlog_order ?? 0);
    expect(back.backlog_order).toBe(Math.max(...orders));
    await setStatus(b.id, "not_for_me");
    const prog = await setStatus(b.id, "in_progress");
    expect(prog.explored_at).toBeNull();
    expect(prog.in_backlog).toBe(false);
  });
  it("create_item with status explored sets explored_at and skips the backlog, like set_status", async () => {
    const it = await createItem({ type: "album", display_name: "Heard it", spotify_uri: null, provenance: "user", status: "explored" });
    expect(it.explored_at).not.toBeNull();
    expect(it.in_backlog).toBe(false);
    expect(it.backlog_order).toBeNull();
  });
  it("save_filter cleans tags so rename_tag rewrites them", async () => {
    const tag = (await listTags())[0];
    const f = await saveFilter("Spaced", { tags: [` ${tag.name}`, tag.name.toUpperCase(), " "] });
    expect(f.definition.tags).toEqual([tag.name]);
    await renameTag(tag.id, "mellowzz");
    expect((await listSavedFilters()).find((x) => x.id === f.id)?.definition.tags).toEqual(["mellowzz"]);
  });
  it("create_item with album_id links the new track on_album (G-3)", async () => {
    const [album] = (await listItems({ type: "album", limit: 1 })).rows;
    const t = await createItem({ type: "track", display_name: "Linked track", spotify_uri: null, provenance: "user", album_id: album.id });
    expect(t.links).toContainEqual(expect.objectContaining({ kind: "on_album", direction: "from", item: expect.objectContaining({ id: album.id }) }));
    expect(t.album_name).toBe(album.display_name);
    expect((await getItem(album.id)).links.some((l) => l.kind === "on_album" && l.direction === "to" && l.item.id === t.id)).toBe(true);
  });
  it("link_items / unlink_items add and remove version_of; update_item changes the type (G-2)", async () => {
    const [a, b] = (await listItems({ type: "album", limit: 2 })).rows;
    await linkItems(a.id, b.id, "version_of");
    expect((await getItem(a.id)).links).toContainEqual(expect.objectContaining({ kind: "version_of", direction: "from", item: expect.objectContaining({ id: b.id }) }));
    expect((await getItem(b.id)).links).toContainEqual(expect.objectContaining({ kind: "version_of", direction: "to" }));
    await unlinkItems(a.id, b.id, "version_of");
    expect((await getItem(a.id)).links.some((l) => l.kind === "version_of")).toBe(false);
    if (a.spotify_uri) await expect(updateItem(a.id, { type: "track" })).rejects.toMatchObject({ code: "validation" }); // type comes from the link
    const manual = await createItem({ type: "album", display_name: "Tape from a friend", spotify_uri: null, provenance: "user" });
    expect((await updateItem(manual.id, { type: "track" })).type).toBe("track");
  });
  it("item summaries carry notes (G-4) and the dashboard has no recent_activity (G-5)", async () => {
    const d = await getDashboard();
    expect("recent_activity" in d).toBe(false);
    expect(d.in_progress.every((i) => "notes" in i)).toBe(true);
  });
  it("dev states: empty empties lists, error rejects", async () => {
    setDev({ view: "empty" });
    expect((await listItems({})).total).toBe(0);
    setDev({ view: "error" });
    await expect(listItems({})).rejects.toMatchObject({ code: "io" });
    setDev({ view: "normal" });
  });
  it("G-9: the conflict dev state makes apply_studio_draft fail with conflict, other calls work", async () => {
    setDev({ view: "conflict" });
    const pl = (await listSpotifyPlaylists())[0];
    const d = await loadStudioDraft(pl.uri);
    await expect(applyStudioDraft(d, d.tracks.map((t) => t.uri))).rejects.toMatchObject({ code: "conflict" });
    setDev({ view: "normal" });
    await expect(applyStudioDraft(d, d.tracks.map((t) => t.uri))).resolves.toBeTruthy();
  });
  it("G-9: undo in the conflict dev state fails with conflict and drops the dead undo", async () => {
    const pl = (await listSpotifyPlaylists())[0];
    const d = await loadStudioDraft(pl.uri);
    await applyStudioDraft(d, d.tracks.map((t) => t.uri));
    expect(await getLastApply(pl.uri)).not.toBeNull();
    setDev({ view: "conflict" });
    await expect(undoLastApply(pl.uri)).rejects.toMatchObject({ code: "conflict" });
    setDev({ view: "normal" });
    expect(await getLastApply(pl.uri)).toBeNull();
  });
  it("partial dev state: a failed apply keeps its undo, a failed create carries the new playlist", async () => {
    const pl = (await listSpotifyPlaylists())[0];
    const d = await loadStudioDraft(pl.uri);
    setDev({ view: "partial" });
    await expect(applyStudioDraft(d, d.tracks.map((t) => t.uri))).rejects.toMatchObject({ code: "internal" });
    const f = (await listSavedFilters())[0];
    await expect(createPlaylistFromFilter(f.id, "Mix", false)).rejects.toMatchObject({ detail: expect.stringMatching(/^spotify:playlist:/) });
    setDev({ view: "normal" });
    expect(await getLastApply(pl.uri)).not.toBeNull();
  });
  it("G-1/G-7/G-8: track plays, album rating on plays, uncheckable albums count", async () => {
    const take = (await getTakeInsights()).data;
    expect(take.unfinished_unknown).toBeGreaterThan(0);
    const u = take.unfinished_albums[0];
    const tl = (await getItem(u.item.id)).track_list!;
    expect(tl.filter((t) => t.plays > 0)).toHaveLength(u.played);
    const rows = (await listPlays({ tab: "all", range: "all", offset: 0, limit: 500 })).rows;
    expect(rows.every((p) => !p.item || p.item.type === "track")).toBe(true);
    expect(rows.some((p) => p.album_rating != null)).toBe(true);
  });
});

describe("mock Runout Inbox, cross-playlist dupes, library URIs (API.md §5, M7)", () => {
  it("check_inbox imports the waiting add as one name-needed item, then finds nothing", async () => {
    const api = await import("../api");
    expect((await api.getInbox()).state).toBe("ready");
    const need = (await api.getCounts()).name_needed;
    expect(await api.checkInbox()).toEqual({ imported: 1, albums: 1, skipped: 0 });
    expect((await api.getCounts()).name_needed).toBe(need + 1);
    const [it] = (await api.listItems({ name_needed: true, sort: "added_at", desc: true, limit: 50 })).rows.filter((i) => i.added_via === "inbox");
    expect(it).toMatchObject({ display_name: null, in_backlog: true });
    expect(await api.checkInbox()).toEqual({ imported: 0, albums: 0, skipped: 0 });
    expect((await api.getInbox()).imported_last).toBe(0);
  });
  it("find_cross_playlist_dupes names only other playlists", async () => {
    const api = await import("../api");
    const [p] = await api.listSpotifyPlaylists();
    for (const r of await api.findCrossPlaylistDupes(p.uri)) expect(r.playlists.every((x) => x.uri !== p.uri)).toBe(true);
  });
  it("library save/contains refuse an artist URI (validation), as Rust", async () => {
    const api = await import("../api");
    await expect(api.libraryContains(["spotify:artist:AAAAAAAAAAAAAAAAAAAAAA"])).rejects.toMatchObject({ code: "validation" });
  });
});

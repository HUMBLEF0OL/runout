import { describe, expect, it } from "vitest";
import { createItem, getCounts, getDashboard, getItem, linkItems, listItems, listSavedFilters, listTags, renameTag, saveFilter, setStatus, unlinkItems, updateItem, type AppError } from "../api";
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
});

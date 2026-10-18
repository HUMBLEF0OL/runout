import { describe, expect, it } from "vitest";
import { createItem, getCounts, listItems, setStatus, type AppError } from "../api";
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
  });
  it("dev states: empty empties lists, error rejects", async () => {
    setDev({ view: "empty" });
    expect((await listItems({})).total).toBe(0);
    setDev({ view: "error" });
    await expect(listItems({})).rejects.toMatchObject({ code: "io" });
    setDev({ view: "normal" });
  });
});

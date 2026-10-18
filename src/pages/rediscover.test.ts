import { describe, expect, it } from "vitest";
import type { ItemSummary } from "../api";
import { shelfFilter } from "./Rediscover";

const it0 = (p: Partial<ItemSummary>): ItemSummary => ({
  id: 1, type: "album", display_name: "X", spotify_uri: null, status: "explored", priority: null, rating: null, favourite: false, revisit: false,
  in_backlog: false, backlog_order: null, tags: [], artist_names: [], album_name: null, added_via: "pc", recommended_by: null, added_at: "", modified_at: "", ...p,
});

describe("shelfFilter", () => {
  const none = shelfFilter({ tag: "", min: 0, revisit: false });
  it("keeps albums and tracks, never artists", () => {
    expect(none(it0({}))).toBe(true);
    expect(none(it0({ type: "artist" }))).toBe(false);
  });
  it("applies tag, minimum rating and revisit together", () => {
    const f = shelfFilter({ tag: "jazz", min: 4.5, revisit: true });
    expect(f(it0({ tags: ["jazz"], rating: 4.5, revisit: true }))).toBe(true);
    expect(f(it0({ tags: ["jazz"], rating: 4, revisit: true }))).toBe(false);
    expect(f(it0({ tags: ["rock"], rating: 5, revisit: true }))).toBe(false);
    expect(f(it0({ tags: ["jazz"], rating: 5, revisit: false }))).toBe(false);
    expect(f(it0({ tags: ["jazz"], rating: null, revisit: true }))).toBe(false);
  });
});

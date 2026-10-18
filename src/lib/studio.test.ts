import { describe, expect, it } from "vitest";
import { cutReason, keepList } from "./studio";
import type { DraftTrack } from "../api";

const t = (uri: string, rating: number | null, dupe_of: string | null = null): DraftTrack => ({ uri, name: uri, artist: "A", rating, tags: [], dupe_of });
const tracks = [t("a", 4.5), t("b", 5), t("c", 2), t("b", 5, "b"), t("d", null), t("e", 1.5), t("f", 2.5)];

describe("studio cut suggestions (your ratings and duplicates only)", () => {
  it("suggests cutting duplicates and tracks you rated 2 or lower", () => {
    expect(tracks.map((x) => cutReason(x, tracks))).toEqual([null, null, "You rated it 2", "Duplicate of 2", null, "You rated it 1.5", null]);
  });
  it("keeps unrated tracks", () => expect(cutReason(t("x", null), [])).toBeNull());
  it("keep list is the kept uris in order, duplicates counted once per row", () => {
    expect(keepList(tracks, new Set([2, 3, 5]))).toEqual(["a", "b", "d", "f"]);
  });
});

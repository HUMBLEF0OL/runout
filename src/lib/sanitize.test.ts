import { describe, expect, it } from "vitest";
import { cleanLine, cleanTag, cleanTags, cleanText, LIMITS, lineOrNull, safeUri } from "./sanitize";
import { openUrl } from "./format";
import { embedUrl } from "../components/ui";
import { createItem, getItem, updateItem } from "../api";

const ID = "3QJzdZJYIAcoET1GcfpNGi";

describe("text sanitization", () => {
  it("single line: trims, collapses whitespace, drops control and invisible characters", () => {
    expect(cleanLine("  Kind\tof\nBlue\u0000  ", 50)).toBe("Kind of Blue");
    expect(cleanLine("Ka​nd‮ot", 50)).toBe("Kandot");
  });
  it("caps length", () => expect(cleanLine("x".repeat(500), LIMITS.name)).toHaveLength(LIMITS.name));
  it("normalises to NFC so the same name dedupes", () => expect(cleanLine("Café", 50)).toBe("Café"));
  it("multi-line keeps paragraphs, normalises line endings, squeezes blank runs", () =>
    expect(cleanText("one\r\ntwo\n\n\n\nthree\u0007")).toBe("one\ntwo\n\nthree"));
  it("blank becomes null for nullable fields", () => {
    expect(lineOrNull("   ", 10)).toBeNull();
    expect(lineOrNull(null, 10)).toBeNull();
  });
  it("tags: lower case, no commas, deduped, empty dropped", () => {
    expect(cleanTag(" Late, Night ")).toBe("late night");
    expect(cleanTags(["Jazz", "jazz ", " ", "late-night"])).toEqual(["jazz", "late-night"]);
  });
});

describe("Spotify URLs only from well-formed URIs", () => {
  it("accepts item and playlist URIs", () => expect(safeUri(`spotify:album:${ID}`)).toEqual({ type: "album", id: ID }));
  it("rejects anything else", () => {
    for (const bad of [`spotify:episode:${ID}`, `spotify:album:${ID}/../x`, `spotify:album:short`, "javascript:alert(1)", "", null])
      expect(safeUri(bad)).toBeNull();
  });
  it("openUrl falls back to Spotify's home page, embedUrl to nothing", () => {
    expect(openUrl(`spotify:track:${ID}`)).toBe(`https://open.spotify.com/track/${ID}`);
    expect(openUrl("spotify:album:../../evil?x=")).toBe("https://open.spotify.com");
    expect(embedUrl(`spotify:artist:${ID}`)).toBe(`https://open.spotify.com/embed/artist/${ID}`);
    expect(embedUrl("spotify:album:bad")).toBeNull();
  });
});

describe("api.ts sanitizes writes for every caller", () => {
  it("create_item and update_item get clean text", async () => {
    const it0 = await createItem({ type: "album", display_name: "  New\u0000  Record ", spotify_uri: null, provenance: "user", tags: ["Jazz", "jazz"], recommended_by: "  " });
    expect(it0.display_name).toBe("New Record");
    expect(it0.tags).toEqual(["jazz"]);
    expect(it0.recommended_by).toBeNull();
    await updateItem(it0.id, { review: "  good\r\n\r\n\r\nvery  ", display_name: "   " });
    const d = await getItem(it0.id);
    expect(d.review).toBe("good\n\nvery");
    expect(d.display_name).toBeNull();
  });
});

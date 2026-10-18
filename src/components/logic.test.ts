import { describe, expect, it } from "vitest";
import type { Play } from "../api";
import { fieldButton, freshnessText, ratingClick, playSources, ratingKey, sourceText } from "./logic";

const A = "3QJzdZJYIAcoET1GcfpNGi";
describe("rating", () => {
  it("arrows move by half a star and clamp", () => {
    expect(ratingKey(3, "ArrowRight")).toBe(3.5);
    expect(ratingKey(3, "ArrowLeft")).toBe(2.5);
    expect(ratingKey(5, "ArrowUp")).toBe(5);
    expect(ratingKey(0, "ArrowDown")).toBe(0);
    expect(ratingKey(2, "Home")).toBe(0);
    expect(ratingKey(2, "End")).toBe(5);
    expect(ratingKey(2, "a")).toBeNull();
  });
  it("click sets full or half stars and toggles off", () => {
    expect(ratingClick(null, 3, false)).toBe(4);
    expect(ratingClick(null, 3, true)).toBe(3.5);
    expect(ratingClick(4, 3, false)).toBeNull();
  });
});

describe("combined field", () => {
  it("pasting a link switches to Add <kind>", () => {
    expect(fieldButton(`https://open.spotify.com/album/${A}?si=x`)).toEqual({ mode: "add", label: "Add album" });
    expect(fieldButton(`spotify:track:${A}`)).toEqual({ mode: "add", label: "Add track" });
    expect(fieldButton("https://spotify.link/aXk3P9q")).toEqual({ mode: "add", label: "Add link" });
  });
  it("unsupported links still go to the add dialog (it explains why)", () => expect(fieldButton(`https://open.spotify.com/episode/${A}`).mode).toBe("add"));
  it("text searches, empty adds", () => {
    expect(fieldButton("coltrane")).toEqual({ mode: "search", label: "Search" });
    expect(fieldButton("  ")).toEqual({ mode: "add", label: "Add" });
  });
});

describe("freshness note", () => {
  const f = { logger_last_event: "2026-10-18T21:20", export_latest_event: "2026-10-02T23:59" };
  it("live logger and an export", () => expect(freshnessText(f, "listening")).toBe("Desktop plays: live. All devices: up to your export of 2 Oct."));
  it("paused logger", () => expect(freshnessText(f, "paused")).toMatch(/^Desktop plays: paused\./));
  it("no export yet", () => expect(freshnessText({ ...f, export_latest_event: null })).toMatch(/import your Spotify export/));
  it("source tag", () => expect(sourceText(["U", "S", "E"])).toBe("From: Your records + Desktop log + Spotify export."));
  it("play sources follow the rows: E only when export plays are shown", () => {
    const p = (source: "logger" | "export", item: unknown = null) => ({ source, item }) as unknown as Play;
    expect(playSources([p("logger")])).toEqual(["S"]);
    expect(playSources([p("logger", { id: 1 }), p("export")])).toEqual(["U", "S", "E"]);
  });
});

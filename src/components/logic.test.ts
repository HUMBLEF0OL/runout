import { describe, expect, it } from "vitest";
import type { Play } from "../api";
import { compareOptions, fieldButton, freshnessText, periodFromSearch, periodLabel, periodLastYear, periodSearch, periodSpan, ratingClick, playSources, ratingKey, sourceText, stepPeriod, vsName } from "./logic";

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

describe("Insights period header (M10 design §5)", () => {
  const T = "2026-10-18"; // a Sunday
  const q = (s: string) => periodFromSearch(new URLSearchParams(s), T);
  it("reads a deep link and falls back to this month vs the one before", () => {
    expect(q("grain=month&at=2026-09&vs=prev")).toEqual({ grain: "month", vs: "prev", at: "2026-09" });
    expect(q("")).toEqual({ grain: "month", vs: "prev", at: null });
    expect(q("grain=nope&at=2026-13&vs=x")).toEqual({ grain: "month", vs: "prev", at: null });
    expect(q("grain=month&at=2026-13")).toEqual({ grain: "month", vs: "prev", at: null });
    expect(q("grain=year&at=2025&vs=usual")).toEqual({ grain: "year", vs: "usual", at: "2025" });
    expect(q("grain=all&vs=last_year")).toEqual({ grain: "all", vs: "last_year", at: null });
  });
  it("a week link names its Monday; a bad day is ignored", () => {
    expect(q("grain=week&at=2026-09-17").at).toBe("2026-09-14");
    expect(q("grain=week&at=2026-02-31").at).toBeNull();
  });
  it("a period that starts after today falls back to the current one", () => {
    expect(q("grain=month&at=2030-01").at).toBeNull();
    expect(q("grain=year&at=2027").at).toBeNull();
    expect(q("grain=week&at=2026-10-19").at).toBeNull();
    expect(q("grain=week&at=2026-10-18").at).toBe("2026-10-12");
  });
  it("same period last year is the year before for a year: one choice, not two", () => expect(q("grain=year&vs=last_year").vs).toBe("prev"));
  it("custom keeps a valid range, else this month", () => {
    expect(q("grain=custom&from=2026-03-01&to=2026-04-15&vs=prev")).toEqual({ grain: "custom", vs: "prev", at: null, from: "2026-03-01", to: "2026-04-15" });
    expect(q("grain=custom&from=2026-05-01&to=2026-04-15")).toEqual({ grain: "custom", vs: "prev", at: null, from: "2026-10-01", to: "2026-10-31" });
  });
  it("writes the link back in the documented order", () => {
    expect(periodSearch({ grain: "month", vs: "prev", at: "2026-09" })).toBe("grain=month&at=2026-09&vs=prev");
    expect(periodSearch({ grain: "custom", vs: "usual", at: null, from: "2026-03-01", to: "2026-04-15" })).toBe("grain=custom&from=2026-03-01&to=2026-04-15&vs=usual");
    const back = { grain: "week", vs: "last_year", at: "2026-09-14" } as const;
    expect(q(periodSearch(back))).toEqual(back);
  });
  it("steps a period and never into the future", () => {
    expect(stepPeriod({ grain: "month", vs: "prev", at: null }, -1, T)?.at).toBe("2026-09");
    expect(stepPeriod({ grain: "month", vs: "prev", at: "2026-09" }, 1, T)?.at).toBeNull(); // back to the current month
    expect(stepPeriod({ grain: "month", vs: "prev", at: null }, 1, T)).toBeNull();
    expect(stepPeriod({ grain: "year", vs: "prev", at: "2024" }, 1, T)?.at).toBe("2025");
    expect(stepPeriod({ grain: "week", vs: "prev", at: null }, -1, T)?.at).toBe("2026-10-05");
    expect(stepPeriod({ grain: "month", vs: "prev", at: "2025-12" }, 1, T)?.at).toBe("2026-01");
    expect(stepPeriod({ grain: "all", vs: "prev", at: null }, -1, T)).toBeNull();
  });
  it("spans and labels", () => {
    expect(periodSpan("month", "2024-02", T)).toEqual({ from: "2024-02-01", to: "2024-02-29" });
    expect(periodSpan("week", null, T)).toEqual({ from: "2026-10-12", to: "2026-10-18" });
    expect(periodLabel({ grain: "month", vs: "prev", at: "2026-09" }, T)).toBe("Sep 2026");
    expect(periodLabel({ grain: "week", vs: "prev", at: null }, T)).toBe("Week of 12 Oct 2026");
    expect(periodLabel({ grain: "custom", vs: "prev", at: null, from: "2025-12-20", to: "2026-01-05" }, T)).toBe("20 Dec 2025 to 5 Jan 2026");
    expect(periodLabel({ grain: "custom", vs: "prev", at: null, from: "2026-03-01", to: "2026-04-15" }, T)).toBe("1 Mar to 15 Apr 2026");
    expect(periodLabel({ grain: "all", vs: "prev", at: null }, T)).toBe("All time");
  });
  it("the period's last year comes from the query, not the data", () => {
    expect(periodLastYear({ grain: "year", vs: "prev", at: "2024" }, T)).toBe(2024);
    expect(periodLastYear({ grain: "month", vs: "prev", at: "2025-12" }, T)).toBe(2025);
    expect(periodLastYear({ grain: "custom", vs: "prev", at: null, from: "2025-12-20", to: "2026-01-05" }, T)).toBe(2026);
    expect(periodLastYear({ grain: "all", vs: "prev", at: null }, T)).toBe(+T.slice(0, 4));
  });
  it("compare choices and what the deltas name", () => {
    expect(compareOptions({ grain: "month", vs: "prev", at: "2026-09" }, T).map((o) => o[1])).toEqual(["vs the month before", "vs Sep 2025", "vs your usual month"]);
    expect(compareOptions({ grain: "year", vs: "prev", at: null }, T).map((o) => o[0])).toEqual(["prev", "usual"]);
    expect(compareOptions({ grain: "all", vs: "prev", at: null }, T)).toEqual([]);
    expect(vsName({ grain: "month", vs: "prev", at: null }, "2026-08-01")).toBe("August");
    expect(vsName({ grain: "month", vs: "last_year", at: null }, "2025-09-01")).toBe("September 2025");
    expect(vsName({ grain: "year", vs: "prev", at: null }, "2025-01-01")).toBe("2025");
    expect(vsName({ grain: "week", vs: "last_year", at: null }, null)).toBe("the same week last year");
    expect(vsName({ grain: "custom", vs: "usual", at: null }, null)).toBe("your usual stretch");
  });
});

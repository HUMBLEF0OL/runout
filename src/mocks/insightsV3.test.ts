import { afterEach, describe, expect, it } from "vitest";
import { explore, getGenresDecades, setMusicDb, updateSettings, listPlays, getDataCoverage, getListeningInsights, getPeriodReport, getRecords, getTrends, type AppError, type Compared } from "../api";
import { setDev } from "./devState";
import { likeForLike } from "../components/logic";

// Mock dataset: today 2026-10-18, export ends 2026-10-02, desktop log from 2026-07-12 (insightsV3.ts).
const comparedOf = (o: Record<string, unknown>) => Object.values(o).filter((v): v is Compared => !!v && typeof v === "object" && "before" in v);

describe("Insights v3 mocks (API.md §4.1, FR-048 coverage rule)", () => {
  afterEach(() => setDev({ view: "normal" }));

  it("a period past the export's end compares desktop plays on both sides and says so", async () => {
    const r = await getPeriodReport({ grain: "month", vs: "prev", at: "2026-10" });
    expect(r.coverage).toMatchObject({ compared_sources: ["S"], desktop_only_since: "2026-10-03", no_compare_reason: null });
    expect(r.data.tiles.ms.now).toBeLessThan(r.data.tiles.ms.value); // the 1-2 Oct phone plays are left out of the delta
    expect(r.data.tiles.ms.before).not.toBeNull();
    expect(r.data.timeline.map((d) => d.key)).toHaveLength(18); // up to today, never the future
    expect(r.data.timeline[r.data.timeline.length - 1]).toMatchObject({ key: "2026-10-18", coverage: "this_pc" });
  });

  it("a period inside the export compares everything; the stepper moves by month", async () => {
    const r = await getPeriodReport({ grain: "month", vs: "prev", at: "2026-09" });
    expect(r.coverage).toMatchObject({ compared_sources: ["S", "E"], desktop_only_since: null });
    expect(r.data.tiles.plays.now).toBe(r.data.tiles.plays.value);
    expect(r.data.period).toMatchObject({ from: "2026-09-01", to: "2026-09-30", vs_from: "2026-08-01", prev_at: "2026-08", next_at: "2026-10" });
    expect(r.data.lines.length).toBeLessThanOrEqual(4);
  });

  it("hides every delta with a reason when no source covers both periods", async () => {
    const r = await getPeriodReport({ grain: "year", vs: "prev", at: "2026" }); // 2025 has no desktop log
    expect(r.coverage.no_compare_reason).toMatch(/only logged from/);
    for (const c of [...comparedOf(r.data.tiles), ...comparedOf(r.data.ratio)]) expect([c.now, c.before]).toEqual([null, null]);
    expect(r.data.timeline.every((t) => t.before_ms === null)).toBe(true);
    expect((await getPeriodReport({ grain: "all", vs: "prev", at: null })).coverage.compared_sources).toEqual([]);
  });

  it("Home's and Insights' deltas pair like-for-like numbers, never an all-device total with a desktop-only one", async () => {
    const r = await getListeningInsights("30d"); // reaches past the export's end: desktop plays on both sides
    expect(r.coverage.compared_sources).toEqual(["S"]);
    const t = r.data.totals, vs = likeForLike(r)!;
    expect(vs).toEqual({ plays: [t.plays_cmp, t.plays_prev], ms: [t.ms_cmp, t.ms_prev], pc: true });
    expect(vs.plays[0]).toBeLessThan(t.plays); // the phone plays of 1-2 Oct stay out of the delta
    expect(likeForLike(await getListeningInsights("all"))).toBeNull();
    expect(likeForLike(await getListeningInsights("12m"))).toBeNull(); // 2025 has no desktop log
  });

  it("a period past the export's end counts days listened, streak and fingerprint on this PC (design §4.4)", async () => {
    const oct = await getPeriodReport({ grain: "month", vs: "prev", at: "2026-10" });
    expect(oct.coverage.desktop_only_since).toBe("2026-10-03");
    expect(oct.data.tiles.days_listened.value).toBe(oct.data.tiles.days_listened.now); // 1-2 Oct phone days not counted
    const streak = (r: typeof oct) => r.data.lines.find((l) => l.rule === "streak")!.parts.map((p) => p.text).join("");
    expect(streak(oct)).toMatch(/on this PC/);
    const sep = await getPeriodReport({ grain: "month", vs: "prev", at: "2026-09" });
    expect(sep.coverage.desktop_only_since).toBeNull();
    expect(streak(sep)).not.toMatch(/on this PC/);
  });

  it("list_plays with a date keeps that local day only (Trends calendar)", async () => {
    const day = (await listPlays({ tab: "all", range: "all", offset: 0, limit: 1 })).rows[0].ended_at_local.slice(0, 10);
    const r = await listPlays({ tab: "all", range: "7d", offset: 0, limit: 500, date: day });
    expect(r.total).toBeGreaterThan(0);
    expect(r.rows.every((p) => p.ended_at_local.startsWith(day))).toBe(true);
    expect(await listPlays({ tab: "all", range: "7d", offset: 0, limit: 5, date: "18 Oct" }).catch((e: AppError) => e.code)).toBe("validation");
  });

  it("rejects a malformed period", async () => {
    expect(await getPeriodReport({ grain: "month", vs: "prev", at: "Sept" }).catch((e: AppError) => e.code)).toBe("validation");
    expect(await getPeriodReport({ grain: "custom", vs: "prev", at: null, from: "2026-09-10" }).catch((e: AppError) => e.code)).toBe("validation");
  });

  it("Home's insights carry the same coverage block and a like-for-like pair (G-46)", async () => {
    const r = await getListeningInsights("30d");
    expect(r.coverage).toMatchObject({ compared_sources: ["S"], desktop_only_since: "2026-10-03" });
    expect(r.data.totals.plays_cmp).toBeLessThan(r.data.totals.plays);
    expect((await getListeningInsights("all")).coverage.no_compare_reason).not.toBeNull();
  });

  it("a no-data date is never silence: null in the calendar, outside streaks", async () => {
    const cal = (await getTrends("hours")).data.calendar.find((c) => c.year === 2026)!;
    expect(cal.minutes[19]).toBeNull(); // 20 Jan: a gap
    expect(cal.minutes[275]).toBeNull(); // 3 Oct: this PC only, not an all-device date
    const s = cal.longest_streak!;
    expect(s.from <= s.to && s.days > 0).toBe(true);
    expect((await getDataCoverage()).coverage.gaps).toContainEqual({ from: "2026-01-19", to: "2026-01-21" });
  });

  it("heavy is a long-time listener: about 180k plays since 2015", async () => {
    setDev({ view: "heavy" });
    const r = (await getRecords()).data;
    expect(r.since).toBe("2015-02-14");
    expect(r.plays_total).toBeGreaterThan(170_000);
    expect(r.plays_total).toBeLessThan(190_000);
    expect(r.next_milestone.plays % 5000).toBe(0);
    expect((await getTrends("plays")).data.months.find((m) => m.month === "2024-03")?.value).toBeGreaterThan(0);
  });

  it("explore: 1-3 series, names cleaned in api.ts, unknown names give zeros", async () => {
    const q = { from: "2026-01-01", to: "2026-09-30", grain: "month" as const, measure: "hours" as const };
    const r = await explore({ ...q, series: [{ kind: "artist", name: "Air\u0000" }, { kind: "artist", name: "Nobody At All" }] });
    expect(r.data.buckets).toHaveLength(9);
    expect(r.data.series[0].name).toBe("Air");
    expect(r.data.series[0].values).toHaveLength(9);
    expect(r.data.series[1]).toMatchObject({ total: 0, peak: null, first_played_local: null });
    const four = Array.from({ length: 4 }, () => ({ kind: "artist" as const, name: "Air" }));
    expect(await explore({ ...q, series: four }).catch((e: AppError) => e.code)).toBe("validation");
    expect((await explore({ ...q, series: [{ kind: "artist", name: "Air" }], shuffle: true })).data.export_only).toBe(true);
  });

  it("empty state keeps the shape and the coverage block", async () => {
    setDev({ view: "empty" });
    const r = await getPeriodReport({ grain: "month", vs: "prev", at: "2026-10" });
    expect(r.data.most_played.artists).toEqual([]);
    expect(r.coverage.desktop_only_since).toBe("2026-10-03");
  });

  it("genres & decades: source M with the credit; the switch off empties it and update_settings can't flip it", async () => {
    const y = { grain: "year" as const, vs: "prev" as const, at: null };
    const on = await getGenresDecades(y);
    expect(on.sources).toContain("M");
    expect(on.data.attribution).toBe("Genres from MusicBrainz, CC BY-NC-SA 3.0");
    expect(await updateSettings({ music_db: false }).catch((e: AppError) => e.code)).toBe("validation");
    expect((await setMusicDb(false)).music_db).toBe(false);
    const off = await getGenresDecades(y);
    expect(off.sources).not.toContain("M");
    expect(off.data.top_genres).toEqual([]);
    await setMusicDb(true);
  });
});

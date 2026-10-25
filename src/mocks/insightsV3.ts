// Mock of API.md §4.1 Insights v3 (M10). A deterministic synthetic history in the shape of
// docs/design/m10-insights-mockup.html: dev state "normal" = one year (the shared dataset's span),
// "heavy" = Feb 2015 to today, about 180k plays. Every number is invented; names come from the shared dataset.
// Coverage follows API.md §4.1: all devices up to the export's end, then desktop plays only (FR-048).
/* eslint-disable @typescript-eslint/no-explicit-any */
import raw from "../../docs/design/prototype/mock-data.json";
import type {
  AppError, Compare, Compared, Coverage, CoverageClass, CoverageGap, DataCoverage, Discovery, DiscoveryRow, ExploreQuery, ExploreResult,
  Fingerprint, Grain, Habits, PeriodQuery, PeriodReport, Range, Records, Sentence, Source, Status, TopRow, Trends,
} from "../api";
import { dev } from "./devState";

const D: any = raw;
const L = D.listening;
const TODAY = (D.meta.today as string).slice(0, 10);
const EXPORT_END = (L.freshness.export_until as string).slice(0, 10);
const DESKTOP_FROM = "2026-07-12"; // the desktop log's first play
const GAPS: CoverageGap[] = [{ from: "2024-03-04", to: "2024-03-12" }, { from: "2026-01-19", to: "2026-01-21" }];
const MS_PER_PLAY = 210_000;
const DESKTOP_SHARE = 0.4; // desktop plays' share of a day's listening
const heavy = () => dev.view === "heavy";
const start = () => (heavy() ? "2015-02-14" : "2025-10-18");
const fail = (code: AppError["code"], message: string): never => { throw { code, message } as AppError; };

/* dates: local 'YYYY-MM-DD' strings, computed in UTC so no time zone moves a day */
const DAY = 86_400_000;
const ms0 = (d: string) => Date.parse(d + "T00:00Z");
const iso = (n: number) => new Date(n).toISOString().slice(0, 10);
const add = (d: string, n: number) => iso(ms0(d) + n * DAY);
const diff = (a: string, b: string) => Math.round((ms0(b) - ms0(a)) / DAY);
const weekday = (d: string) => (new Date(ms0(d)).getUTCDay() + 6) % 7; // Monday = 0
const addMonths = (m: string, n: number) => new Date(Date.UTC(+m.slice(0, 4), +m.slice(5, 7) - 1 + n, 1)).toISOString().slice(0, 7);
const monthEnd = (m: string) => add(addMonths(m, 1) + "-01", -1);
const days = (from: string, to: string) => Array.from({ length: Math.max(0, diff(from, to) + 1) }, (_, i) => add(from, i));
const lo = (...a: string[]) => a.reduce((x, y) => (x < y ? x : y));
const hi = (...a: string[]) => a.reduce((x, y) => (x > y ? x : y));
const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/** One stable value in [0, 1) per string. */
const noise = (s: string) => {
  let h = 2166136261;
  for (const c of s) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  h = Math.imul(h ^ (h >>> 13), 0x5bd1e995);
  return ((h ^ (h >>> 15)) >>> 0) / 4294967296;
};
const r1 = (n: number) => Math.round(n * 10) / 10;
const fakeUri = (s: string) => "spotify:track:" + Array.from({ length: 22 }, (_, i) => "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"[Math.floor(noise(s + i) * 62)]).join("");

/* ------------------------------------------------------------------ the synthetic history */
const inGap = (d: string) => GAPS.some((g) => d >= g.from && d <= g.to);
const cls = (d: string): CoverageClass =>
  d < start() || d > TODAY || inGap(d) ? "no_data" : d <= EXPORT_END ? "all_devices" : d >= DESKTOP_FROM ? "this_pc" : "no_data";
/** A day's listening as Runout stores it: every device up to the export's end, then desktop plays only. */
function dayMs(d: string): number {
  if (cls(d) === "no_data" || noise(d) < 0.06) return 0; // ~6% of days without music
  const ms = (heavy() ? 158 : 46) * 60_000 * (0.3 + 1.4 * noise(d + "x")) * (weekday(d) >= 5 ? 1.35 : 0.86);
  return Math.round(d > EXPORT_END ? ms * DESKTOP_SHARE : ms);
}
const desktopMs = (d: string) => (d < DESKTOP_FROM ? 0 : d > EXPORT_END ? dayMs(d) : Math.round(dayMs(d) * DESKTOP_SHARE));
type Basis = "all" | "S";
const basisMs = (d: string, b: Basis) => (b === "S" ? desktopMs(d) : dayMs(d));

interface Span { from: string; to: string }
function measure(s: Span, b: Basis) {
  let ms = 0, listened = 0, covered = 0;
  for (const d of days(s.from, lo(s.to, TODAY))) {
    const v = basisMs(d, b);
    ms += v; listened += v > 0 ? 1 : 0; covered += cls(d) === "no_data" ? 0 : 1;
  }
  const plays = Math.round(ms / MS_PER_PLAY), artists = Math.round(Math.sqrt(plays) * 6);
  return { ms, plays, artists, new_artists: Math.round(artists * 0.11), days_listened: listened, covered, tracks: Math.round(artists * 3.4), albums: Math.round(artists * 1.6) };
}
type Measure = ReturnType<typeof measure>;
const median = (a: number[]) => { const s = [...a].sort((x, y) => x - y); return s.length ? (s[(s.length - 1) >> 1] + s[s.length >> 1]) / 2 : 0; };

/* ------------------------------------------------------------------ periods + the coverage rule */
const AT = { year: /^\d{4}$/, month: /^\d{4}-\d{2}$/, week: /^\d{4}-\d{2}-\d{2}$/ } as const;
const DATE = /^\d{4}-\d{2}-\d{2}$/;
const spanOf = (g: "week" | "month" | "year", at: string): Span =>
  g === "year" ? { from: `${at}-01-01`, to: `${at}-12-31` } : g === "month" ? { from: `${at}-01`, to: monthEnd(at) } : { from: add(at, -weekday(at)), to: add(at, 6 - weekday(at)) };
const shift = (g: "week" | "month" | "year", at: string, n: number) => (g === "year" ? String(+at + n) : g === "month" ? addMonths(at, n) : add(at, 7 * n));

interface Resolved { cur: Span; vs: Span[] | null; prev_at: string | null; next_at: string | null }
function resolve(q: PeriodQuery): Resolved {
  if (q.grain === "all") return { cur: { from: start(), to: TODAY }, vs: null, prev_at: null, next_at: null };
  if (q.grain === "custom") {
    if (!q.from || !q.to || !DATE.test(q.from) || !DATE.test(q.to) || q.from > q.to) fail("validation", "Choose a start and an end date.");
    const cur = { from: q.from!, to: q.to! }, len = diff(cur.from, cur.to) + 1;
    const back = (n: number) => ({ from: add(cur.from, -n), to: add(cur.to, -n) });
    const vs = q.vs === "usual" ? Array.from({ length: 12 }, (_, i) => back(len * (i + 1))) : [back(q.vs === "last_year" ? 364 : len)];
    return { cur, vs, prev_at: null, next_at: null };
  }
  const g = q.grain;
  const now = g === "year" ? TODAY.slice(0, 4) : g === "month" ? TODAY.slice(0, 7) : TODAY;
  const at = q.at ?? now;
  if (!AT[g].test(at)) fail("validation", "That period isn't a valid date.");
  const cur = spanOf(g, at);
  const yearBack = g === "year" ? 1 : g === "month" ? 12 : 52;
  const vs = q.vs === "usual" ? Array.from({ length: 12 }, (_, i) => spanOf(g, shift(g, at, -(i + 1))))
    : [spanOf(g, shift(g, at, q.vs === "last_year" ? -yearBack : -1))];
  const p = shift(g, at, -1), n = shift(g, at, 1);
  return { cur, vs, prev_at: spanOf(g, p).to >= start() ? p : null, next_at: spanOf(g, n).from <= TODAY ? n : null };
}

const fmtDay = (d: string) => `${+d.slice(8, 10)} ${MONTHS[+d.slice(5, 7) - 1].slice(0, 3)} ${d.slice(0, 4)}`;
/** FR-048: compare only over sources that cover both sides completely (API.md §4.1). */
function coverageOf(cur: Span, vs: Span[] | null): Coverage {
  const first = vs ? lo(cur.from, ...vs.map((s) => s.from)) : cur.from;
  const gaps = GAPS.filter((g) => g.from >= start() && g.from <= lo(cur.to, TODAY) && g.to >= first);
  const past = cur.to > EXPORT_END && cur.from <= TODAY;
  const desktop_only_since = past ? add(EXPORT_END, 1) : null;
  const none = (reason: string): Coverage => ({ compared_sources: [], desktop_only_since, gaps, no_compare_reason: reason });
  if (!vs) return none("All time has nothing before it to compare with.");
  const earliest = lo(...vs.map((s) => s.from));
  if (vs.every((s) => s.to < start())) return none(`There's no listening before ${fmtDay(start())}.`);
  if (past && earliest < DESKTOP_FROM) return none(`Plays on this PC are only logged from ${fmtDay(DESKTOP_FROM)}, so there's nothing like for like to compare with.`);
  const compared_sources: Source[] = past ? ["S"] : earliest >= DESKTOP_FROM ? ["S", "E"] : ["E"];
  return { compared_sources, desktop_only_since, gaps, no_compare_reason: null };
}
const basisOf = (c: Coverage): Basis => (c.compared_sources.includes("E") ? "all" : "S");

/** Range (Home, Insights' old tiles) vs the period before, for get_listening_insights. */
export function rangeCoverage(range: Range): Coverage {
  if (range === "all") return coverageOf({ from: start(), to: TODAY }, null);
  const n = { "7d": 7, "30d": 30, "12m": 365 }[range];
  const cur = { from: add(TODAY, -(n - 1)), to: TODAY };
  return coverageOf(cur, [{ from: add(cur.from, -n), to: add(cur.from, -1) }]);
}

function compare(q: PeriodQuery) {
  const r = resolve(q), coverage = coverageOf(r.cur, r.vs), hidden = coverage.no_compare_reason != null, b = basisOf(coverage);
  const all = measure(r.cur, "all"), like = measure(r.cur, b);
  const befores = hidden ? [] : r.vs!.map((s) => measure(s, b));
  const before = (k: keyof Measure) => (hidden ? null : Math.round(median(befores.map((m) => m[k]))));
  const c = (k: keyof Measure): Compared => ({ value: all[k], now: hidden ? null : like[k], before: before(k) });
  return { r, coverage, hidden, b, all, like, c, before };
}

const vsLabel = (grain: Grain, vs: Compare, s: Span | null) =>
  vs === "usual" ? `your usual ${grain === "custom" ? "stretch" : grain}`
    : !s ? "" : grain === "month" ? (vs === "prev" ? MONTHS[+s.from.slice(5, 7) - 1] : `${MONTHS[+s.from.slice(5, 7) - 1]} ${s.from.slice(0, 4)}`)
      : grain === "year" ? s.from.slice(0, 4) : vs === "prev" ? `the ${grain === "week" ? "week" : "stretch"} before` : "the same time last year";

/* ------------------------------------------------------------------ shared shapes */
type Find = (type: "artist" | "album" | "track", name: string) => { id: number; status: Status; rating: number | null } | undefined;
const em = (text: string) => ({ text, em: true });
const tx = (text: string) => ({ text, em: false });
const spark = (s: string) => Array.from({ length: 12 }, (_, i) => r1(2 + 10 * noise(s + i)));
const fp = (seed: string, base: Fingerprint): Fingerprint =>
  Object.fromEntries(Object.entries(base).map(([k, v]) => [k, Math.max(0, Math.min(100, Math.round(v + 16 * (noise(seed + k) - 0.5))))])) as unknown as Fingerprint;
const FP_NOW: Fingerprint = { regularity: 93, discovery: 31, focus: 54, variety: 62, repeat: 71 };
const FP_AVG: Fingerprint = { regularity: 80, discovery: 22, focus: 60, variety: 58, repeat: 66 };
const TAGS = [["spiritual jazz", 24], ["late night", 19], ["electronic", 14], ["indie folk", 11], ["trip-hop", 9], ["comfort", 7]] as const;
const MOVES = [2, -1, null, -2, 0, 4, null, 9, -3, -2];
const pctOf = (a: number[]) => { const s = a.reduce((x, y) => x + y, 0) || 1; return a.map((v) => r1((v / s) * 100)); };
const years = () => Array.from({ length: +TODAY.slice(0, 4) - +start().slice(0, 4) + 1 }, (_, i) => +start().slice(0, 4) + i);
const monthsBetween = (a: string, b: string) => { const out: string[] = []; for (let m = a; m <= b; m = addMonths(m, 1)) out.push(m); return out; };
const majority = (ds: string[]): CoverageClass => {
  const n = { all_devices: 0, this_pc: 0, no_data: 0 }; ds.forEach((d) => n[cls(d)]++);
  return (Object.entries(n).sort((a, b) => b[1] - a[1])[0][0]) as CoverageClass;
};

/* ------------------------------------------------------------------ commands */
export function insightsV3(find: Find, freshness: () => unknown) {
  const covered = <T,>(data: T, sources: Source[], coverage: Coverage) => ({ data, sources, freshness: freshness(), coverage });
  const k = () => (heavy() ? 3 : 1);
  const rows = (kind: "artists" | "albums" | "tracks"): TopRow[] => L.charts["30d"][kind].slice(0, 10).map((r: any, i: number) => ({
    name: r.name, by: r.by, plays: r.plays * k(), hours: r1(r.hours * k()), item_id: r.item_id ?? null, rating: r.rating ?? null, status: r.status ?? null,
    move: MOVES[i], is_new: MOVES[i] == null, spark: spark(kind + r.name),
  }));
  const rank = (type: "artist" | "album" | "track", name: string, by: string, plays: number, hours: number) => {
    const it = find(type, name);
    return { name, by, plays, hours, item_id: it?.id ?? null, rating: it?.rating ?? null, status: it?.status ?? null };
  };

  const report = ({ period: q }: { period: PeriodQuery }) => {
    const { r, coverage, hidden, b, all, like, c, before } = compare(q);
    const vs1 = r.vs && r.vs.length === 1 ? r.vs[0] : null;
    const daily = q.grain === "week" || q.grain === "month" || (q.grain === "custom" && diff(r.cur.from, r.cur.to) < 92);
    const tb: Basis = hidden ? "all" : b; // a chart with a ghost line is a comparison: like for like
    const timeline = daily
      ? days(r.cur.from, lo(r.cur.to, TODAY)).map((d, i) => ({
        key: d, ms: basisMs(d, tb), coverage: cls(d),
        before_ms: hidden ? null : Math.round(r.vs!.reduce((a, s) => a + basisMs(add(s.from, i), b), 0) / r.vs!.length),
      }))
      : monthsBetween(r.cur.from.slice(0, 7), lo(r.cur.to, TODAY).slice(0, 7)).map((m, i) => {
        const ds = days(hi(m + "-01", r.cur.from), lo(monthEnd(m), TODAY));
        const vm = vs1 && addMonths(vs1.from.slice(0, 7), i);
        return { key: m, ms: ds.reduce((a, d) => a + basisMs(d, tb), 0), coverage: majority(ds), before_ms: hidden || !vm ? null : days(vm + "-01", lo(monthEnd(vm), TODAY)).reduce((a, d) => a + basisMs(d, b), 0) };
      });
    // Design §4.4: past the export's end, days listened, streak and fingerprint count this PC for the whole period.
    const pc = coverage.desktop_only_since ? measure(r.cur, "S") : all, onPc = coverage.desktop_only_since ? " on this PC" : "";
    const lines: Sentence[] = [];
    const bm = before("ms");
    if (bm && like.ms && Math.abs(like.ms / bm - 1) >= 0.1) {
      const p = like.ms / bm - 1;
      lines.push({ rule: "time_change", parts: [tx("You listened "), em(`${Math.round(Math.abs(p) * 100)}% ${p > 0 ? "more" : "less"}`), tx(` than ${vsLabel(q.grain, q.vs, vs1)}, most of it late: `), em("22:00 to 01:00"), tx(" was "), em("38%"), tx(" of your time.")] });
    }
    if (all.ms > 0) {
      lines.push({ rule: "new_in_top5", parts: [em("Floating Points"), tx(" is new and already your "), em("#3"), tx(" artist. You first played them on "), em(fmtDay(add(r.cur.from, 3))), tx(".")] });
      if (heavy()) lines.push({ rule: "left_top10", parts: [em("Massive Attack"), tx(" left your top 10 for the first time since "), em("March 2023"), tx(".")] });
      lines.push({ rule: "streak", parts: [tx("You listened on "), em(`${pc.days_listened} of ${pc.covered} days`), tx(`${onPc}. Your current streak is `), em("19 days"), tx(".")] });
    }
    const fpNow = fp(r.cur.from, FP_NOW);
    const data: PeriodReport = {
      period: { grain: q.grain, vs: q.vs, from: r.cur.from, to: r.cur.to, vs_from: vs1?.from ?? null, vs_to: vs1?.to ?? null, prev_at: r.prev_at, next_at: r.next_at, days: all.covered },
      private_excluded: L.private_excluded,
      tiles: {
        ms: c("ms"), plays: c("plays"), artists: c("artists"), new_artists: c("new_artists"), days_listened: { ...c("days_listened"), value: pc.days_listened }, streak: all.ms ? 19 : 0,
        finished_pct: r.cur.from > EXPORT_END ? null : { value: 71, now: hidden ? null : 71, before: hidden ? null : 68 },
      },
      timeline, lines: lines.slice(0, 4),
      clock: { hours: pctOf([22, 14, 8, 3, 2, 2, 4, 12, 20, 18, 15, 16, 19, 17, 15, 16, 20, 24, 26, 28, 34, 40, 52, 61]), baseline: pctOf([20, 12, 7, 3, 2, 2, 5, 14, 22, 19, 16, 16, 18, 16, 15, 17, 20, 23, 25, 27, 31, 36, 44, 50]), weekdays: pctOf([48, 44, 52, 46, 58, 80, 66]) },
      top_new: all.ms ? { name: "Floating Points", item_id: find("artist", "Floating Points")?.id ?? null, status: find("artist", "Floating Points")?.status ?? null, first_played_local: add(r.cur.from, 3) + "T21:14", first_track: "Silhouettes (I, II & III)", ms: 17_640_000 * k(), plays: 71 * k(), rank: 3, share_pct: 12 } : null,
      also_new: all.ms ? ["Kelly Lee Owens", "Pharoah Sanders", "Sons of Kemet"] : [], also_new_more: all.ms ? 20 : 0,
      ratio: {
        tracks: c("tracks"), albums: c("albums"), artists: c("artists"),
        tracks_per_artist: { value: 3.4, now: hidden ? null : 3.4, before: hidden ? null : 3.1 }, albums_per_artist: { value: 1.6, now: hidden ? null : 1.6, before: hidden ? null : 1.6 },
      },
      fingerprint: { now: fpNow, average: FP_AVG },
      tags: { rows: TAGS.map(([tag, share_pct]) => ({ tag, share_pct })), covered_pct: 62 },
      most_played: all.ms ? { artists: rows("artists"), albums: rows("albums"), tracks: rows("tracks") } : { artists: [], albums: [], tracks: [] },
    };
    return covered(data, ["U", "S", "E"], coverage);
  };

  const trends = ({ measure: m }: { measure: "hours" | "plays" | "artists" }) => {
    const months = monthsBetween(start().slice(0, 7), TODAY.slice(0, 7)).map((month) => {
      const ds = days(hi(month + "-01", start()), lo(monthEnd(month), TODAY));
      const ms = ds.reduce((a, d) => a + dayMs(d), 0), plays = ms / MS_PER_PLAY;
      return { month, coverage: majority(ds), value: m === "hours" ? r1(ms / 3_600_000) : m === "plays" ? Math.round(plays) : Math.round(Math.sqrt(plays) * 6) };
    });
    const eras = heavy()
      ? [["2015-02", "2016-12", ["Prateek Kuhad"]], ["2017-01", "2018-07", ["Massive Attack", "Air"]], ["2018-08", "2019-06", ["Miles Davis"]], ["2019-07", "2021-03", ["John Coltrane"]], ["2021-04", "2022-10", ["Kate Bush"]], ["2022-11", TODAY.slice(0, 7), ["Alice Coltrane"]]]
      : [["2025-10", "2026-04", ["Prateek Kuhad"]], ["2026-05", TODAY.slice(0, 7), ["Alice Coltrane"]]];
    const shape = (kind: string, i: number) => kind === "rising" ? (i > 27 ? 4 + (i - 27) * 1.7 : 1) : kind === "fading" ? Math.max(1, 20 - i * 0.5) : kind === "back_again" ? (i < 8 ? 5 : i < 31 ? 0.2 : 4 + (i - 31)) : 6;
    const arc = (artist: string, kind: "rising" | "fading" | "back_again" | "constant", change_pct: number | null, since: string | null, quiet_from: string | null, related: string[]) =>
      ({ artist, item_id: find("artist", artist)?.id ?? null, kind, shares: Array.from({ length: 36 }, (_, i) => r1(shape(kind, i) + noise(artist + i))), change_pct, since, quiet_from, related });
    const ys = years();
    const data: Trends = {
      months,
      eras: eras.map(([from, to, artists]) => ({ from: from as string, to: to as string, artists: artists as string[] })),
      arcs: [
        arc("Floating Points", "rising", 210, "2026-09", null, ["Massive Attack"]),
        arc("Kelly Lee Owens", "rising", 85, "2026-07", null, []),
        arc("Massive Attack", "fading", -74, "2025-10", null, ["Floating Points", "Air"]),
        arc("Kate Bush", "back_again", null, "2026-08", heavy() ? "2021-03" : "2025-11", []),
        arc("Miles Davis", "constant", null, null, null, []),
      ],
      fingerprint_by_year: ys.map((year) => ({ year, fingerprint: fp(String(year), FP_AVG) })),
      tag_months: { months: monthsBetween(addMonths(TODAY.slice(0, 7), -11), TODAY.slice(0, 7)), rows: TAGS.map(([tag, s]) => ({ tag, shares: Array.from({ length: 12 }, (_, i) => r1(s * (0.6 + 0.8 * noise(tag + i)))) })) },
      calendar: ys.map((year) => {
        const ds = days(`${year}-01-01`, `${year}-12-31`);
        const minutes = ds.map((d) => (cls(d) === "all_devices" ? Math.round(dayMs(d) / 60_000) : null));
        let best: { date: string; ms: number } | null = null, run = 0, runFrom = "", streak: { days: number; from: string; to: string } | null = null, silent = 0;
        ds.forEach((d) => {
          if (cls(d) !== "all_devices") return; // a "no data" day breaks nothing and counts nothing (design §4.2)
          const v = dayMs(d);
          if (v === 0) { silent++; run = 0; return; }
          if (!best || v > best.ms) best = { date: d, ms: v };
          if (run++ === 0) runFrom = d;
          if (!streak || run > streak.days) streak = { days: run, from: runFrom, to: d };
        });
        return { year, minutes, biggest_day: best, longest_streak: streak, days_without_music: silent };
      }),
    };
    return covered(data, ["U", "S", "E"], { compared_sources: [], desktop_only_since: add(EXPORT_END, 1), gaps: GAPS.filter((g) => g.from >= start()), no_compare_reason: null });
  };

  const discovery = ({ period: q }: { period: PeriodQuery }) => {
    const { r, coverage, all, c } = compare(q);
    const per = monthsBetween(lo(r.cur.from.slice(0, 7), addMonths(TODAY.slice(0, 7), -23)), lo(r.cur.to, TODAY).slice(0, 7)).map((month) => {
      const coverage = monthEnd(month) < start() ? "no_data" : majority(days(hi(month + "-01", start()), lo(monthEnd(month), TODAY)));
      const n = coverage === "no_data" ? 0 : Math.round((heavy() ? 18 : 4) * (0.5 + noise(month)));
      const open = diff(month + "-01", TODAY) < 120 ? n : 0, stuck = Math.round((n - open) * 0.34);
      return { month, stuck, dropped: n - open - stuck, open, coverage };
    });
    const log: [string, string, string, number, DiscoveryRow["now"]][] = [["Floating Points", "2026-09-04T21:14", "Silhouettes (I, II & III)", 71, "in_rotation"], ["Pharoah Sanders", "2026-08-28T08:02", "Love Is Everywhere", 44, "in_rotation"],
      ["Kelly Lee Owens", "2026-07-02T23:40", "On", 63, "in_rotation"], ["Sons of Kemet", "2026-06-19T22:15", "My Queen Is Ada Eastman", 9, "dropped"],
      ...(heavy() ? [["Kate Bush", "2018-02-14T20:03", "Running Up That Hill", 612, "back_again"]] as typeof log : [])];
    const data: Discovery = {
      new_artists: c("new_artists"), days_per_new: all.new_artists ? r1(all.covered / all.new_artists) : null,
      stick_rate: { pct: 34, best_pct: heavy() ? 46 : null, best_year: heavy() ? 2019 : null }, new_time_pct: 11,
      artists_known: heavy() ? 4211 : 412, known_since: start(), per_month: per,
      starts: [{ kind: "followed", pct: 52 }, { kind: "picked", pct: 41 }, { kind: "back", pct: 4 }, { kind: "other", pct: 3 }],
      log: all.ms ? log.map(([artist, first_played_local, first_track, plays_since, now]) => ({ artist, item_id: find("artist", artist)?.id ?? null, first_played_local, first_track, plays_since: plays_since * k(), now })) : [],
    };
    return covered(data, ["S", "E"], coverage);
  };

  const habits = ({ period: q }: { period: PeriodQuery }) => {
    const { coverage, all } = compare(q);
    const ex = days(start(), EXPORT_END).reduce((a, d) => a + dayMs(d), 0);
    const it = find("album", "Kind of Blue");
    const data: Habits = {
      export_plays: Math.round(ex / MS_PER_PLAY),
      ends: [{ kind: "finished", pct: 71 }, { kind: "skipped", pct: 13 }, { kind: "back", pct: 6 }, { kind: "stopped", pct: 10 }],
      skip_rate: L.skip_rate, skips_by_hour: [10, 8, 6, 5, 5, 6, 12, 22, 34, 30, 18, 14, 15, 13, 12, 14, 16, 18, 15, 12, 10, 8, 6, 7],
      most_skipped: { name: "Radiohead", item_id: find("artist", "Radiohead")?.id ?? null, skip_pct: 38 },
      quick_bails: [["Teardrop", "Massive Attack", 14], ["Bitches Brew", "Miles Davis", 11], ["cold/mess", "Prateek Kuhad", 9], ["Hounds of Love", "Kate Bush", 8]]
        .map(([track, artist, times]) => ({ track: track as string, artist: artist as string, item_id: find("track", track as string)?.id ?? null, times: times as number })),
      shuffle: { pct: 46, by_year: years().map((year) => ({ year, pct: Math.round(40 + 25 * noise("sh" + year)) })) },
      devices: [{ kind: "phone", pct: 50 }, { kind: "this_pc", pct: 39 }, { kind: "web", pct: 6 }, { kind: "computer", pct: 2 }, { kind: "other", pct: 3 }],
      offline: { pct: 6, peak_month: "2026-06" },
      finished: { pct: 71, how_far_pct: 83, how_far_coverage_pct: 39 },
      sessions: { count: Math.round(all.plays / 15), typical_ms: 2_280_000, longest: all.ms ? { ms: 22_320_000, start_local: "2026-09-13T14:02" } : null, spread: [14, 22, 30, 24, 16, 10, 6, 3, 2, 1] },
      deep_dives: [
        { kind: "artist", name: "Alice Coltrane", by: "", start_local: "2026-09-13T14:02", end_local: "2026-09-13T18:30", tracks: 38, ms: 16_080_000, times: null },
        { kind: "artist", name: "Floating Points", by: "", start_local: "2026-09-11T21:30", end_local: "2026-09-12T01:10", tracks: 41, ms: 13_200_000, times: null },
        { kind: "album", name: "Mezzanine", by: "Massive Attack", start_local: "2026-09-07T15:10", end_local: "2026-09-07T17:17", tracks: 30, ms: 7_620_000, times: 3 },
        { kind: "repeat", name: "Silhouettes (I, II & III)", by: "Floating Points", start_local: "2026-09-04T21:14", end_local: "2026-09-04T23:03", tracks: 9, ms: 6_540_000, times: 9 },
      ],
      start_to_finish: it ? [{ item_id: it.id, name: "Kind of Blue", by: "Miles Davis", times: 2 }] : [],
    };
    return covered(data, ["S", "E"], coverage);
  };

  const records = () => {
    const ds = days(start(), TODAY);
    let total = 0, streak = { days: 0, from: "", to: "" }, run = 0, runFrom = "", best = { date: "", ms: 0 };
    const byYear = new Map<number, number>(), byMonth = new Map<string, number>();
    ds.forEach((d) => {
      const v = dayMs(d);
      total += v;
      if (cls(d) !== "all_devices") return; // all-time records use all-device dates only (design §4.4)
      byYear.set(+d.slice(0, 4), (byYear.get(+d.slice(0, 4)) ?? 0) + v);
      byMonth.set(d.slice(0, 7), (byMonth.get(d.slice(0, 7)) ?? 0) + v);
      if (v > best.ms) best = { date: d, ms: v };
      if (v === 0) { run = 0; return; }
      if (run++ === 0) runFrom = d;
      if (run > streak.days) streak = { days: run, from: runFrom, to: d };
    });
    const plays_total = Math.round(total / MS_PER_PLAY);
    const pace = days(add(TODAY, -89), TODAY).reduce((a, d) => a + dayMs(d), 0) / MS_PER_PLAY / 90;
    const next = Math.ceil((plays_total + 1) / 5000) * 5000;
    const [by, bms] = [...byYear].sort((a, b) => b[1] - a[1])[0] ?? [0, 0];
    const [bm, bmms] = [...byMonth].sort((a, b) => b[1] - a[1])[0] ?? ["", 0];
    const otd = years().slice(0, -1).reverse().map((year, i) => ({ year, d: `${year}${TODAY.slice(4)}`, i })).filter(({ d }) => cls(d) === "all_devices")
      .map(({ year, d, i }) => ({ year, plays: Math.round(dayMs(d) / MS_PER_PLAY), ms: dayMs(d), top: [{ kind: "album" as const, name: "Mezzanine", by: "Massive Attack" }, { kind: "artist" as const, name: "Kate Bush", by: "" }, null][i % 3], first_artists: i === 3 ? ["Air"] : [] }));
    const data: Records = {
      since: start(), plays_total,
      first_play: { track: "Hope", artist: "Prateek Kuhad", at_local: start() + "T19:02" },
      top_track: rank("track", "Teardrop", "Massive Attack", heavy() ? 1204 : 61, heavy() ? 108.4 : 5.5),
      top_artist: rank("artist", "Alice Coltrane", "", heavy() ? 9120 : 877, heavy() ? 742.3 : 69.4),
      top_album: rank("album", "Journey in Satchidananda", "Alice Coltrane", heavy() ? 3450 : 310, heavy() ? 266.1 : 24.6),
      longest_streak: streak.days ? streak : null, biggest_day: best.ms ? best : null, biggest_year: bms ? { year: by, ms: bms } : null,
      most_artists_month: bmms ? { month: bm, artists: Math.round(Math.sqrt(bmms / MS_PER_PLAY) * 6) } : null,
      most_loyal: { name: "Miles Davis", item_id: find("artist", "Miles Davis")?.id ?? null, years_in_top20: Math.max(1, years().length - 2), years: years().length },
      next_milestone: { plays: next, eta_days: pace ? Math.ceil((next - plays_total) / pace) : null },
      on_this_day: otd,
      forgotten: heavy() ? [["Massive Attack", 2023, "2025-01-14T22:10"], ["Bon Iver", 2017, "2022-03-02T08:41"], ["The National", 2016, "2021-08-19T19:05"], ["Sufjan Stevens", 2019, "2023-02-11T23:30"]]
        .map(([name, top10_year, last_played_local]) => ({ name: name as string, item_id: find("artist", name as string)?.id ?? null, top10_year: top10_year as number, last_played_local: last_played_local as string, play_uri: fakeUri(name as string) })) : [],
    };
    return covered(data, ["S", "E"], { compared_sources: [], desktop_only_since: add(EXPORT_END, 1), gaps: GAPS.filter((g) => g.from >= start()), no_compare_reason: null });
  };

  const KNOWN = new Set<string>([...["artists", "albums", "tracks"].flatMap((kk) => Object.values(L.charts).flatMap((ch: any) => ch[kk].map((x: any) => x.name.toLowerCase()))),
    "floating points", "kelly lee owens", "pharoah sanders", "sons of kemet"]);
  const exploreQ = ({ query: q }: { query: ExploreQuery }) => {
    if (q.series.length < 1 || q.series.length > 3) fail("validation", "Choose one to three artists, albums or tracks.");
    if (!DATE.test(q.from) || !DATE.test(q.to) || q.from > q.to) fail("validation", "Choose a start and an end date.");
    if (q.hours?.some((h) => h < 0 || h > 23) || q.weekdays?.some((w) => w < 1 || w > 7)) fail("validation", "That hour or weekday is out of range.");
    const export_only = !!q.devices?.length || q.shuffle != null;
    const end = lo(q.to, TODAY, export_only ? EXPORT_END : TODAY);
    const key = (d: string) => (q.grain === "year" ? d.slice(0, 4) : q.grain === "month" ? d.slice(0, 7) : add(d, -weekday(d)));
    const buckets = [...new Set(days(q.from, end).map(key))];
    const filt = ((q.hours?.length ?? 24) / 24) * ((q.weekdays?.length ?? 7) / 7) * (export_only ? 0.5 : 1);
    const shareOf = (name: string, d: string) => {
      if (!KNOWN.has(name.trim().toLowerCase())) return 0; // no plays: a series of zeros, not an error
      const peak = ms0(start()) + noise(name.toLowerCase()) * (ms0(TODAY) - ms0(start())), w = 400 * DAY;
      return 0.01 + 0.1 * Math.exp(-(((ms0(d) - peak) / w) ** 2));
    };
    const totals = new Map<string, number>();
    days(q.from, end).forEach((d) => totals.set(key(d), (totals.get(key(d)) ?? 0) + dayMs(d) * filt));
    const series = q.series.map((sr) => {
      const ms = new Map<string, number>();
      days(q.from, end).forEach((d) => ms.set(key(d), (ms.get(key(d)) ?? 0) + dayMs(d) * filt * shareOf(sr.name, d)));
      const values = buckets.map((bk) => { const v = ms.get(bk) ?? 0; return q.measure === "hours" ? r1(v / 3_600_000) : q.measure === "plays" ? Math.round(v / MS_PER_PLAY) : r1((v / (totals.get(bk) || 1)) * 100); });
      const pi = values.indexOf(Math.max(...values)), fi = values.findIndex((v) => v > 0);
      return { kind: sr.kind, name: sr.name, by: sr.by ?? "", values, total: q.measure === "share" ? r1(values.reduce((a, v) => a + v, 0) / (values.length || 1)) : r1(values.reduce((a, v) => a + v, 0)),
        peak: values[pi] > 0 ? { bucket: buckets[pi], value: values[pi] } : null, first_played_local: fi < 0 ? null : (q.grain === "week" ? buckets[fi] : (buckets[fi] + "-01-01").slice(0, 10)) + "T20:00" };
    });
    const data: ExploreResult = { buckets, series, export_only };
    return covered(data, ["S", "E"], { compared_sources: [], desktop_only_since: q.to > EXPORT_END ? add(EXPORT_END, 1) : null, gaps: GAPS.filter((g) => g.from >= start() && g.from <= end && g.to >= q.from), no_compare_reason: null });
  };

  const dataCoverage = () => {
    const sum = (a: string, b: string, f: (d: string) => number) => days(a, b).reduce((x, d) => x + f(d), 0) / MS_PER_PLAY;
    const exportPlays = Math.round(sum(start(), EXPORT_END, dayMs));
    const data: DataCoverage = {
      export: { from: start(), to: EXPORT_END, plays: exportPlays },
      desktop: { from: DESKTOP_FROM, to: TODAY, plays: Math.round(sum(DESKTOP_FROM, TODAY, desktopMs)), matched: Math.round(sum(DESKTOP_FROM, EXPORT_END, desktopMs)) },
      logger_runs_since: null,
      imports: [{ id: 1, imported_at: "2026-10-05T10:12", from: start(), to: EXPORT_END, added: exportPlays, already_had: 0 }],
      private_excluded: L.private_excluded,
    };
    return covered(data, ["S", "E"], { compared_sources: [], desktop_only_since: add(EXPORT_END, 1), gaps: GAPS.filter((g) => g.from >= start()), no_compare_reason: null });
  };

  return {
    get_period_report: report, get_trends: trends, get_discovery: discovery, get_listening_habits: habits,
    get_records: records, explore: exploreQ, get_data_coverage: dataCoverage,
  };
}

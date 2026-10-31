// Pure logic behind components (tested in logic.test.ts).
import type { Compare, Covered, Freshness, Grain, ListeningInsights, PeriodQuery, Play, Range, Source, Sourced } from "../api";
import { linkKind, parseLink } from "../lib/link";
import { day, dayY } from "../lib/format";

/** Rating keyboard (DESIGN §5): arrows ±0.5, Home/End, clamp 0–5. Returns null for keys it doesn't handle. */
export function ratingKey(value: number, key: string): number | null {
  const v = value ?? 0;
  switch (key) {
    case "ArrowRight": case "ArrowUp": return Math.min(5, v + 0.5);
    case "ArrowLeft": case "ArrowDown": return Math.max(0, v - 0.5);
    case "Home": return 0;
    case "End": return 5;
    default: return null;
  }
}
/** Click on star `index` (0-based); left half gives a half star. Clicking the current value clears it. */
export function ratingClick(current: number | null, index: number, leftHalf: boolean): number | null {
  const v = index + (leftHalf ? 0.5 : 1);
  return v === current ? null : v;
}

/** Combined search/add field (DESIGN §5): a Spotify link switches the button to Add. */
export function fieldButton(value: string): { mode: "add" | "search"; label: string } {
  const v = value.trim();
  if (!v) return { mode: "add", label: "Add" };
  const p = parseLink(v);
  if (p.state === "short") return { mode: "add", label: "Add link" };
  if (p.state === "ok" || p.state === "unsupported") return { mode: "add", label: `Add ${linkKind(v)}` };
  return { mode: "search", label: "Search" };
}

/** Freshness note (DESIGN §1.3, FR-032): "Desktop: live. All devices: up to 2 Oct." */
export function freshnessText(f: Freshness | undefined, loggerState?: string): string {
  const desktop = loggerState === "paused" ? "paused" : loggerState === "off" ? "off" : loggerState === "error" ? "not available" : f?.logger_last_event ? "live" : "no plays yet";
  const all = f?.export_latest_event ? `up to your export of ${day(f.export_latest_event)}` : "import your Spotify export to include them";
  return `Desktop plays: ${desktop}. All devices: ${all}.`;
}

const SRC = { U: "Your records", S: "Desktop log", E: "Spotify export", M: "Music database" } as const;
/** Source tag (FR-032): "From: Your records + Desktop log." */
export const sourceText = (sources: (keyof typeof SRC)[]) => `From: ${sources.map((s) => SRC[s]).join(" + ")}.`;
/** Insight card tag (FR-032): the response's sources, each listening source with how current it is. A response
 *  without both never reads as sourced. */
export function sourcedText(r: Partial<Pick<Sourced<unknown>, "sources" | "freshness">>): string {
  const f = r.freshness;
  if (!r.sources?.length || !f) return "Sources unknown.";
  const to = (iso: string | null) => ` (${iso ? `to ${dayY(iso)}` : "none yet"})`;
  return `From: ${r.sources.map((s) => SRC[s] + (s === "S" ? to(f.logger_last_event) : s === "E" ? to(f.export_latest_event) : "")).join(" + ")}.`;
}

/** Source tag for a play list (list_plays has no `sources`): U when any play is linked to a record, then each play source present. */
export const playSources = (rows: Play[]): Source[] => [
  ...(rows.some((p) => p.item) ? ["U" as const] : []),
  ...(rows.some((p) => p.source === "logger") ? ["S" as const] : []),
  ...(rows.some((p) => p.source === "export") ? ["E" as const] : []),
];

/** FR-048: Home's strip and the Insights tiles compare like for like (`*_cmp` vs `*_prev`, API.md §4.1); null = no delta. */
export function likeForLike(res: Covered<ListeningInsights>): { plays: [number, number]; ms: [number, number]; pc: boolean } | null {
  const t = res.data.totals;
  // ponytail: no `coverage` = a pre-M10 Rust core (G-46): show no delta rather than an unlike one.
  if (!res.coverage || res.coverage.no_compare_reason != null || !t.plays_prev) return null;
  return { plays: [t.plays_cmp, t.plays_prev], ms: [t.ms_cmp, t.ms_prev], pc: res.coverage.desktop_only_since != null };
}

/** FR-048 short note beside those deltas (M10 design §4.4): the switch to this-PC plays, or why there's no delta. */
export function compareNote(res: Covered<ListeningInsights>, range: Range): string | null {
  const c = res.coverage;
  if (!c || range === "all") return null; // All time: nothing before it, the tiles say so already
  if (c.no_compare_reason) return c.no_compare_reason;
  return c.desktop_only_since && likeForLike(res) ? `From ${day(c.desktop_only_since)}, only plays on this PC are counted in the comparison.` : null;
}

/* ------------------------------------------------------------------ Insights page header (M10 design §5): the period ↔ the URL */
// Dates are local 'YYYY-MM-DD' strings, computed in UTC so no time zone moves a day (as the mock and Rust do).
const GRAINS: Grain[] = ["week", "month", "year", "all", "custom"];
const VS: Compare[] = ["prev", "last_year", "usual"];
const MON = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const MONTH = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const d0 = (d: string) => Date.parse(d + "T00:00Z");
const isoDay = (n: number) => new Date(n).toISOString().slice(0, 10);
const addDays = (d: string, n: number) => isoDay(d0(d) + n * 86_400_000);
const weekday = (d: string) => (new Date(d0(d)).getUTCDay() + 6) % 7; // Monday = 0
const addMonths = (m: string, n: number) => new Date(Date.UTC(+m.slice(0, 4), +m.slice(5, 7) - 1 + n, 1)).toISOString().slice(0, 7);
const isDay = (s: string | null): s is string => !!s && /^\d{4}-\d{2}-\d{2}$/.test(s) && !isNaN(d0(s)) && isoDay(d0(s)) === s; // 2026-02-31, 2026-13-01 are not days
const dm = (d: string) => `${+d.slice(8, 10)} ${MON[+d.slice(5, 7) - 1]}`;
type Step = "week" | "month" | "year";
const stepped = (g: Grain): g is Step => g === "week" || g === "month" || g === "year";
const current = (g: Step, today: string) => (g === "year" ? today.slice(0, 4) : g === "month" ? today.slice(0, 7) : addDays(today, -weekday(today)));

/** Today as a local date ('YYYY-MM-DD'). */
export const localDay = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

/** First and last day of a week (Mon–Sun), month or year; `at` null = the current one. */
export function periodSpan(g: Step, at: string | null, today: string) {
  const a = at ?? current(g, today);
  return g === "year" ? { from: `${a}-01-01`, to: `${a}-12-31` }
    : g === "month" ? { from: `${a}-01`, to: addDays(addMonths(a, 1) + "-01", -1) }
    : { from: addDays(a, -weekday(a)), to: addDays(a, 6 - weekday(a)) };
}

/** The period a deep link names (`#/insights/<tab>?grain=&at=&vs=`, custom `from`/`to`). Anything unreadable falls back:
 *  grain to month, vs to the period before, `at` to the current period, a custom range to this month. */
export function periodFromSearch(sp: URLSearchParams, today: string): PeriodQuery {
  const grain = GRAINS.find((g) => g === sp.get("grain")) ?? "month";
  let vs = VS.find((v) => v === sp.get("vs")) ?? "prev";
  if (grain === "year" && vs === "last_year") vs = "prev"; // the same year last year is the year before: one choice
  if (grain === "custom") {
    const from = sp.get("from"), to = sp.get("to");
    return isDay(from) && isDay(to) && from <= to ? { grain, vs, at: null, from, to } : { grain, vs, at: null, ...periodSpan("month", null, today) };
  }
  if (!stepped(grain)) return { grain, vs, at: null };
  const at = sp.get("at");
  const ok = grain === "year" ? !!at && /^\d{4}$/.test(at) : grain === "month" ? !!at && /^\d{4}-\d{2}$/.test(at) && isDay(at + "-01") : isDay(at);
  const a = !ok ? null : grain === "week" ? addDays(at!, -weekday(at!)) : at;
  return { grain, vs, at: a && periodSpan(grain, a, today).from <= today ? a : null }; // a future period falls back to now
}

/** The deep link's query, in the documented order (grain, at | from + to, vs). */
export function periodSearch(q: PeriodQuery): string {
  const p = new URLSearchParams({ grain: q.grain });
  if (q.at) p.set("at", q.at);
  if (q.grain === "custom" && q.from && q.to) { p.set("from", q.from); p.set("to", q.to); }
  p.set("vs", q.vs);
  return p.toString();
}

/** The period `n` steps away (the ‹ › stepper); null for All time, Custom, or a period that would start after today. */
export function stepPeriod(q: PeriodQuery, n: number, today: string): PeriodQuery | null {
  if (!stepped(q.grain)) return null;
  const g = q.grain, a = q.at ?? current(g, today);
  const b = g === "year" ? String(+a + n) : g === "month" ? addMonths(a, n) : addDays(a, 7 * n);
  if (periodSpan(g, b, today).from > today) return null;
  return { ...q, at: b === current(g, today) ? null : b }; // the current period links as "now", so the link stays current
}

/** The stepper's label: "Sep 2026", "2026", "Week of 12 Oct 2026", "1 Mar to 15 Apr 2026", "All time". */
export function periodLabel(q: PeriodQuery, today: string): string {
  if (q.grain === "all") return "All time";
  if (q.grain === "custom") {
    const f = q.from ?? today, t = q.to ?? today;
    return `${dm(f)}${f.slice(0, 4) === t.slice(0, 4) ? "" : " " + f.slice(0, 4)} to ${dm(t)} ${t.slice(0, 4)}`;
  }
  const s = periodSpan(q.grain, q.at, today);
  return q.grain === "year" ? s.from.slice(0, 4) : q.grain === "month" ? `${MON[+s.from.slice(5, 7) - 1]} ${s.from.slice(0, 4)}` : `Week of ${dm(s.from)} ${s.from.slice(0, 4)}`;
}

/** The period's last calendar year (API.md GenresDecades "the period's last year"); All time = today's year. */
export function periodLastYear(q: PeriodQuery, today: string): number {
  if (q.grain === "all") return +today.slice(0, 4);
  const to = q.grain === "custom" ? (q.to ?? today) : periodSpan(q.grain, q.at, today).to;
  return +(to < today ? to : today).slice(0, 4); // Rust: span end clipped to today
}

/** "Compare with" choices for the grain (none for All time; a year's "same period last year" is its "before"). */
export function compareOptions(q: PeriodQuery, today: string): [Compare, string][] {
  if (q.grain === "all") return [];
  const unit = q.grain === "custom" ? "stretch" : q.grain;
  const usual: [Compare, string] = ["usual", `vs your usual ${unit}`];
  if (q.grain === "year") return [["prev", "vs the year before"], usual];
  const ly = q.grain === "month" ? (() => { const f = periodSpan("month", q.at, today).from; return `vs ${MON[+f.slice(5, 7) - 1]} ${+f.slice(0, 4) - 1}`; })()
    : q.grain === "week" ? "vs the same week last year" : "vs the same dates last year";
  return [["prev", `vs the ${unit} before`], ["last_year", ly], usual];
}

/** What a delta is "than": "August", "September 2025", "2025", "the week before", "your usual month". `vsFrom` = PeriodInfo.vs_from. */
export function vsName(q: PeriodQuery, vsFrom: string | null): string {
  const unit = q.grain === "custom" ? "stretch" : q.grain;
  if (q.vs === "usual") return `your usual ${unit}`;
  if (vsFrom && q.grain === "month") return MONTH[+vsFrom.slice(5, 7) - 1] + (q.vs === "last_year" ? ` ${vsFrom.slice(0, 4)}` : "");
  if (vsFrom && q.grain === "year") return vsFrom.slice(0, 4);
  if (q.vs === "last_year") return q.grain === "week" ? "the same week last year" : "the same dates last year";
  return `the ${unit} before`;
}

/** Export reminder (M10, Settings.export_reminder): due every 30 days from the last export's end. Returns the due date
 *  when it's due and not yet shown (`shown` = the due date shown last), else null. Local only, never network.
 *  No export yet: due on the first app open with the switch on, then every 30 days from the last one shown. */
export function exportReminderDue(exportEnd: string | null, today: string, shown: string | null): string | null {
  if (!exportEnd || !isDay(exportEnd.slice(0, 10))) return !isDay(shown) ? today : addDays(shown, 30) <= today ? today : null;
  const end = exportEnd.slice(0, 10), n = Math.floor((Date.parse(today + "T00:00Z") - Date.parse(end + "T00:00Z")) / 86_400_000 / 30);
  if (n < 1) return null;
  const due = addDays(end, 30 * n);
  return due === shown ? null : due;
}

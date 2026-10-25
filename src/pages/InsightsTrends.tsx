// SC-09 Insights v3, Trends tab (M10 WI 7), per docs/design/M10-insights-v3.md §5 and mockup PNG 2. Your whole history:
// lifetime months you can brush to set the Report's custom period, eras, artist arcs, the fingerprint by year, tags month
// by month and a calendar per year whose days open Recent plays for that date. Sources from the response (FR-032).
// Heuristics (eras, arcs, "may have taken time from") say so. Fingerprint scores stay on screen: SVG, not selectable,
// no copy or export (design §2). The calendar shows all-device dates only; other dates are hatched, never silence (§4.4).
import { useRef, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { m, useReducedMotion } from "motion/react";
import * as api from "../api";
import type { Arc, Covered, Fingerprint, PeriodQuery, Trends as TrendsData } from "../api";
import { Empty, ErrorBanner, InsightCard as Card, Seg, useSlide } from "../components/ui";
import { monthLong, SPRING } from "../components/charts";
import { periodSearch } from "../components/logic";
import { useApi } from "../hooks/useApi";
import { day, hm, hrs, monthName, num, pl } from "../lib/format";
import { EASE } from "../lib/motion";

type Measure = "hours" | "plays" | "artists";
type Res = Covered<TrendsData>;
const MEASURES: [Measure, string][] = [["hours", "Hours"], ["plays", "Plays"], ["artists", "Artists"]];
const DAY = 86_400_000;
const utc = (d: string) => Date.parse(d + "T00:00Z");
/** Local date `i` days after 1 Jan of `year` (calendar[].minutes[0] = 1 Jan). */
export const calendarDay = (year: number, i: number) => new Date(Date.UTC(year, 0, 1) + i * DAY).toISOString().slice(0, 10);
/** Empty cells before 1 Jan so each column runs Monday to Sunday. */
export const calendarLead = (year: number) => (new Date(Date.UTC(year, 0, 1)).getUTCDay() + 6) % 7;
const monthEnd = (ym: string) => new Date(Date.UTC(+ym.slice(0, 4), +ym.slice(5, 7), 0)).toISOString().slice(0, 10);
/** Months a..b (either order) as the Report's custom period, whole months, never past today. */
export function brushPeriod(a: string, b: string, today: string): PeriodQuery {
  const [lo, hi] = a <= b ? [a, b] : [b, a];
  const to = monthEnd(hi);
  return { grain: "custom", vs: "prev", at: null, from: lo + "-01", to: to < today ? to : today };
}
const WD = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const longDay = (d: string) => `${WD[new Date(utc(d)).getUTCDay()]} ${day(d + "T00:00")} ${d.slice(0, 4)}`;
const shortMonth = (ym: string) => `${monthName(ym)} ${ym.slice(0, 4)}`;
const span = (a: string, b: string) => (a === b ? shortMonth(a) : `${shortMonth(a)} to ${shortMonth(b)}`);
/** ES2020 has no Array.findLastIndex. */
const lastIdx = <T,>(a: T[], p: (x: T) => boolean) => { for (let i = a.length - 1; i >= 0; i--) if (p(a[i])) return i; return -1; };
const pct1 = (v: number) => Math.round(v * 10) / 10; // spoken shares: one decimal
const fmtOf = (k: Measure) => (v: number) => (k === "hours" ? hrs(v) : pl(v, k === "plays" ? "play" : "artist"));

export function Trends({ today }: { today: string }) {
  const reduce = useReducedMotion();
  const [measure, setMeasure] = useState<Measure>("hours");
  const tr = useApi(() => api.getTrends(measure), [measure]);
  if (tr.loading && !tr.data) return <TrendsSkel />;
  if (tr.error) return <ErrorBanner error={tr.error} title="Couldn’t load your trends." onRetry={tr.reload} />;
  const res = tr.data!, t = res.data;
  if (!t.months.some((x) => x.value > 0)) return (
    <Empty h="No listening history yet" line="Import your Spotify history to see this.">
      <Link className="btn btn-primary" to="/import/1">Import history</Link>
    </Empty>);
  return (
    <m.div className="flex flex-col gap-6" initial={reduce ? false : { opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.35, ease: EASE }}>
      <Lifetime res={res} measure={measure} setMeasure={setMeasure} today={today} />
      <Arcs res={res} />
      <ByYear res={res} />
      <TagMonths res={res} />
      <Calendar res={res} today={today} />
    </m.div>
  );
}

/* ------------------------------------------------------------------ lifetime months + eras, a brush for the Report */
function Lifetime({ res, measure, setMeasure, today }: { res: Res; measure: Measure; setMeasure: (m: Measure) => void; today: string }) {
  const reduce = useReducedMotion();
  const t = res.data, ms = t.months, n = ms.length;
  const [cur, setCur] = useState(n - 1);
  const [anchor, setAnchor] = useState<number | null>(null);
  const drag = useRef(false);
  const c = Math.min(cur, n - 1), a = anchor == null ? c : Math.min(anchor, n - 1);
  const [lo, hi] = [Math.min(a, c), Math.max(a, c)];
  const fmt = fmtOf(measure);
  const max = Math.max(1e-9, ...ms.map((x) => x.value));
  const sel = ms.slice(lo, hi + 1);
  const total = measure === "artists" ? `up to ${fmt(Math.max(...sel.map((x) => x.value)))} a month` : fmt(sel.reduce((s, x) => s + x.value, 0));
  const range = span(ms[lo].month, ms[hi].month);
  const to = `/insights/report?${periodSearch(brushPeriod(ms[lo].month, ms[hi].month, today))}`;
  const at = (e: React.PointerEvent) => { const r = e.currentTarget.getBoundingClientRect(); return Math.max(0, Math.min(n - 1, Math.floor(((e.clientX - r.left) / r.width) * n))); };
  const keys = (e: React.KeyboardEvent) => {
    const step: Record<string, number> = { ArrowLeft: -1, ArrowDown: -1, ArrowRight: 1, ArrowUp: 1, PageDown: -12, PageUp: 12 };
    const k = e.key in step ? c + step[e.key] : e.key === "Home" ? 0 : e.key === "End" ? n - 1 : null;
    if (k === null) return;
    e.preventDefault();
    setAnchor(e.shiftKey ? a : null); // Shift+arrows stretch the pick from where it started; plain arrows pick one month
    setCur(Math.max(0, Math.min(n - 1, k)));
  };
  const cols = { gridTemplateColumns: `repeat(${n}, minmax(0, 1fr))` };
  const every = n > 72 ? 2 : 1;
  const tick = (ym: string) => (ym.endsWith("-01") && +ym.slice(0, 4) % every === 0 ? ym.slice(0, 4) : "");
  const idx = (ym: string) => Math.max(0, ms.findIndex((x) => x.month >= ym));
  const pc = res.coverage.desktop_only_since;
  const gap = ms.some((x) => x.coverage === "no_data");
  return (
    <Card title={`Your listening since ${ms[0].month.slice(0, 4)}`} res={res}>
      <div className="flex flex-wrap gap-3 justify-between items-center">
        <span className="text-text-2 text-sm">{MEASURES.find((x) => x[0] === measure)![1]} per month. Drag across any stretch, or use Shift with the arrow keys, to report on it.</span>
        <Seg label="Measure" options={MEASURES} value={measure} onChange={setMeasure} />
      </div>
      <div className="hm-plot in3-life" role="slider" tabIndex={0} aria-label={`${MEASURES.find((x) => x[0] === measure)![1]} per month since ${ms[0].month.slice(0, 4)}. Arrow keys pick a month, Shift with arrow keys a stretch`}
        aria-valuemin={0} aria-valuemax={n - 1} aria-valuenow={c} aria-valuetext={`${range}: ${total}`} onKeyDown={keys}
        onPointerDown={(e) => { const i = at(e); drag.current = true; e.currentTarget.setPointerCapture?.(e.pointerId); setAnchor(i); setCur(i); }}
        onPointerMove={(e) => { if (drag.current) setCur(at(e)); }}
        onPointerUp={() => { drag.current = false; }} onPointerCancel={() => { drag.current = false; }}>
        <div className="hm-cols dense xs" style={cols} aria-hidden>
          {ms.map((x, i) => (
            <span key={x.month} className={`hm-col${i >= lo && i <= hi ? " on" : ""}${x.coverage === "no_data" ? " gap" : ""}`}>
              <m.i style={{ height: x.coverage === "no_data" ? "100%" : `${Math.max(2, (x.value / max) * 100)}%`, originY: 1 }}
                initial={reduce ? false : { scaleY: 0 }} animate={{ scaleY: 1 }} transition={{ ...SPRING, delay: 0.1 + (i / n) * 0.6 }} />
            </span>))}
        </div>
      </div>
      {t.eras.length > 0 && <div className="in3-eras" style={cols}>
        {t.eras.map((e) => {
          const s = idx(e.from), f = Math.max(s, lastIdx(ms, (x) => x.month <= e.to));
          return <button key={e.from} type="button" className="in3-era" style={{ gridColumn: `${s + 1} / ${f + 2}` }} title={`${e.artists.join(" & ")}, ${span(ms[s].month, ms[f].month)}`}
            aria-label={`${e.artists.join(" and ")} era, ${span(ms[s].month, ms[f].month)} (estimate). Pick this stretch`} onClick={() => { setAnchor(s); setCur(f); }}>
            <b>{e.artists[0]}</b>{e.artists.length > 1 && <span> &amp; {e.artists.slice(1).join(", ")}</span>}</button>;
        })}
      </div>}
      <div className="hm-months dense xs" style={cols} aria-hidden>{ms.map((x) => <span key={x.month}>{tick(x.month)}</span>)}</div>
      <div className="flex flex-wrap gap-3 justify-between items-center">
        <span className="text-sm" aria-live="polite"><b className="font-mono">{range}</b> <span className="text-text-2">{total}</span></span>
        <Link className="btn btn-secondary s" to={to}>Report on {range}</Link>
      </div>
      <span className="text-muted text-xs">Eras are an estimate: stretches where one or two artists held an unusually large share, named after them.
        {gap ? " Hatched months are gaps with no data from any source, never silence." : ""}
        {pc ? ` From ${day(pc + "T00:00")}, months count plays on this PC only, until your next export.` : ""}</span>
    </Card>
  );
}

/* ------------------------------------------------------------------ artist arcs */
type Kind = Arc["kind"];
const KINDS: [Kind, string][] = [["rising", "Rising"], ["fading", "Fading"], ["back_again", "Back again"], ["constant", "Constants"]];
function arcLine(x: Arc) {
  if (x.kind === "rising") return `rising${x.since ? ` since ${monthName(x.since)}` : ""}`;
  if (x.kind === "fading") return `fading${x.change_pct != null ? `, ${Math.round(x.change_pct)}% in a year` : ""}`;
  if (x.kind === "back_again") return `back again${x.since ? ` since ${shortMonth(x.since)}` : ""}${x.quiet_from ? `, quiet from ${x.quiet_from.slice(0, 4)}` : ""}`;
  return "in your top 20 most years";
}
const related = (x: Arc) => !x.related.length ? null
  : x.kind === "fading" ? `may have lost time to ${x.related.join(", ")}` : `may have taken time from ${x.related.join(", ")}`;
function Arcs({ res }: { res: Res }) {
  const [kind, setKind] = useState<Kind>("rising");
  const [, slide] = useSlide(KINDS.map((k) => k[0]), kind);
  const all = res.data.arcs;
  const rows = all.filter((x) => x.kind === kind);
  const max = Math.max(1e-9, ...all.flatMap((x) => x.shares));
  return (
    <Card title="Artist arcs" res={res}>
      <div className="flex flex-wrap gap-3 justify-between items-center">
        <span className="text-text-2 text-sm">Each line is an artist’s share of your time, month by month, over three years.</span>
        <Seg label="Artist arcs" options={KINDS} value={kind} onChange={(k) => { slide(k); setKind(k); }} />
      </div>
      {!rows.length ? <span className="text-muted">No artists {kind === "constant" ? "have stayed constant" : `are ${KINDS.find((k) => k[0] === kind)![1].toLowerCase()}`} right now.</span>
        : <ul className="flex flex-col">{rows.map((x) => (
          <li key={x.artist} className="in3-arc">
            <span className="flex flex-col min-w-0">{x.item_id ? <Link to={`/item/${x.item_id}`} className="font-semibold text-text truncate">{x.artist}</Link> : <b className="truncate">{x.artist}</b>}
              <span className="text-muted text-xs">{arcLine(x)}</span></span>
            <svg viewBox="0 0 100 100" preserveAspectRatio="none" className={x.kind === "fading" ? "fade" : undefined} role="img"
              aria-label={`${x.artist}: ${x.shares.length} months` + (x.shares.length ? `, from ${pct1(x.shares[0])}% to ${pct1(x.shares[x.shares.length - 1])}% of your time` : "")}>
              <polyline points={x.shares.map((v, i) => `${((i / Math.max(1, x.shares.length - 1)) * 100).toFixed(1)},${(96 - (v / max) * 92).toFixed(1)}`).join(" ")} vectorEffect="non-scaling-stroke" />
            </svg>
            <span className="text-text-2 text-sm">{related(x) ?? ""}</span>
          </li>))}</ul>}
      <span className="text-muted text-xs">Arcs and “may have taken time from” are estimates: they compare the share each artist lost and gained in the same months. Artists are matched by name.</span>
    </Card>
  );
}

/* ------------------------------------------------------------------ fingerprint by year */
const AXES: [keyof Fingerprint, string, string][] = [["regularity", "Regularity", "share of days with music"], ["discovery", "Discovery", "time on artists new that year"],
  ["focus", "Focus", "time on your top 10 artists"], ["variety", "Variety", "how spread out your listening is"], ["repeat", "Repeat", "plays of tracks you’d played before"]];
function ByYear({ res }: { res: Res }) {
  const ys = res.data.fingerprint_by_year;
  if (!ys.length) return null;
  const first = ys[0].year, last = ys[ys.length - 1].year;
  const x = (i: number) => (ys.length < 2 ? 50 : (i / (ys.length - 1)) * 100);
  return (
    <Card title="Your listening, year by year" res={res}>
      <span className="text-text-2 text-sm">Five measures, each 0 to 100. The dot is {last}; the line is every year since {first}.</span>
      <div className="in3-years">{AXES.map(([k, name, what]) => {
        const v = ys.map((y) => y.fingerprint[k]);
        return (
          <div key={k} className="card in3-year" role="img" aria-label={`${name}, ${what}: ${ys.map((y, i) => `${y.year} ${Math.round(v[i])}`).join(", ")}`}>
            <span className="font-semibold text-sm">{name}</span><span className="text-muted text-xs">{what}</span>
            <span className="hm-big">{Math.round(v[v.length - 1])}</span>
            <svg viewBox="-4 -4 108 48" preserveAspectRatio="none" aria-hidden>
              <polyline points={v.map((y, i) => `${x(i).toFixed(1)},${(40 - y * 0.4).toFixed(1)}`).join(" ")} vectorEffect="non-scaling-stroke" />
              <circle cx={x(v.length - 1)} cy={40 - v[v.length - 1] * 0.4} r="2.5" />
            </svg>
            <span className="flex justify-between font-mono text-faint text-xs"><span>{first}</span><span>{last}</span></span>
          </div>);
      })}</div>
      <span className="text-muted text-xs">Measured against your own past only. Shown here, never exported or shared.</span>
    </Card>
  );
}

/* ------------------------------------------------------------------ tags month by month */
function TagMonths({ res }: { res: Res }) {
  const { months, rows } = res.data.tag_months;
  if (!months.length) return null;
  const max = Math.max(1e-9, ...rows.flatMap((r) => r.shares));
  return (
    <Card title="Your tags, month by month" res={res}>
      <span className="text-text-2 text-sm">Share of tagged listening per tag, the last {months.length} months.</span>
      {!rows.length ? <span className="text-muted">No tagged records played lately. <Link to="/library">Tag your records</Link></span> : (
        <div className="overflow-x-auto"><table className="in3-heat">
          <thead><tr><th scope="col"><span className="sr-only">Tag</span></th>{months.map((mo) => <th key={mo} scope="col" title={monthLong(mo)}>{monthName(mo).slice(0, 1)}<span className="sr-only">{monthName(mo).slice(1)} {mo.slice(0, 4)}</span></th>)}</tr></thead>
          <tbody>{rows.map((r) => (
            <tr key={r.tag}><th scope="row">{r.tag}</th>
              {r.shares.map((s, i) => <td key={i} title={`${r.tag}, ${monthLong(months[i])}: ${Math.round(s)}%`}><i className={`l${Math.ceil((s / max) * 4)}`} /><span className="sr-only">{Math.round(s)}%</span></td>)}</tr>))}
          </tbody>
        </table></div>)}
    </Card>
  );
}

/* ------------------------------------------------------------------ calendar per year */
function Calendar({ res, today }: { res: Res; today: string }) {
  const nav = useNavigate();
  const cal = res.data.calendar;
  const [year, setYear] = useState(cal[cal.length - 1]?.year ?? 0);
  const y = cal.find((c) => c.year === year) ?? cal[cal.length - 1];
  const lastOn = y ? Math.max(0, lastIdx(y.minutes, (v) => v != null)) : 0;
  const [focus, setFocus] = useState<{ year: number; i: number }>();
  const box = useRef<HTMLDivElement>(null);
  if (!y) return null;
  const f = focus?.year === y.year ? focus.i : lastOn;
  const max = Math.max(1, ...y.minutes.map((v) => v ?? 0));
  const keys = (e: React.KeyboardEvent) => {
    const step: Record<string, number> = { ArrowUp: -1, ArrowDown: 1, ArrowLeft: -7, ArrowRight: 7 };
    const k = e.key in step ? f + step[e.key] : e.key === "Home" ? 0 : e.key === "End" ? y.minutes.length - 1 : null;
    if (k === null || k < 0 || k >= y.minutes.length) return;
    e.preventDefault();
    setFocus({ year: y.year, i: k });
    box.current?.querySelector<HTMLElement>(`[data-i="${k}"]`)?.focus();
  };
  const years: [string, string][] = cal.map((c) => [String(c.year), String(c.year)]);
  const s = y.longest_streak;
  return (
    <Card title={`Every day of ${y.year}`} res={res}>
      <div className="flex flex-wrap gap-3 justify-between items-center">
        <span className="text-text-2 text-sm">Minutes per day. Choose a day to see everything you played on it.</span>
        {years.length <= 4 ? <Seg radio label="Calendar year" options={years} value={String(y.year)} onChange={(v) => setYear(+v)} />
          : <select className="input surface h-10" aria-label="Calendar year" value={y.year} onChange={(e) => setYear(+e.target.value)}>{years.map(([v]) => <option key={v} value={v}>{v}</option>)}</select>}
      </div>
      <div ref={box} className="in3-cal" role="group" aria-label={`Minutes per day in ${y.year}. Arrow keys move by day and week, Enter opens that day’s plays`} onKeyDown={keys}>
        {Array.from({ length: calendarLead(y.year) }, (_, i) => <span key={`l${i}`} aria-hidden />)}
        {y.minutes.map((v, i) => {
          const d = calendarDay(y.year, i);
          const what = d > today ? "not yet" : v == null ? "no data" : v === 0 ? "no music" : hm(v * 60_000);
          return <button key={d} type="button" data-i={i} tabIndex={i === f ? 0 : -1} aria-disabled={v == null || d > today || undefined}
            className={d > today ? "fut" : v == null ? "nd" : `l${v ? Math.max(1, Math.ceil((v / max) * 4)) : 0}`} aria-label={`${longDay(d)}: ${what}`} title={`${longDay(d)}: ${what}`}
            onFocus={() => setFocus({ year: y.year, i })} onClick={() => { if (v != null && d <= today) nav(`/plays/all?date=${d}`); }} />;
        })}
      </div>
      <span className="flex flex-wrap gap-x-6 gap-y-1 text-sm text-text-2">
        {y.biggest_day && <span>Biggest day <b className="font-mono text-text">{longDay(y.biggest_day.date)}, {hm(y.biggest_day.ms)}</b></span>}
        {s && <span>Longest streak <b className="font-mono text-text">{pl(s.days, "day")}</b> ({span(s.from.slice(0, 7), s.to.slice(0, 7))})</span>}
        <span>Days without music <b className="font-mono text-text">{num(y.days_without_music)}</b></span>
      </span>
      <span className="text-muted text-xs">Only days every device is counted for: from your first export play to its last. Hatched days have no data and break no streak.</span>
    </Card>
  );
}

const TrendsSkel = () => (
  <div className="flex flex-col gap-6" aria-busy="true" aria-label="Loading">
    <div className="card h-[300px]"><div className="skel h-full" /></div>
    <div className="card h-[260px]"><div className="skel h-full" /></div>
  </div>
);

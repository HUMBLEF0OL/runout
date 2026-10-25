// SC-09 Insights v3 (M10 WI 6): the page header (grain, ‹ period ›, compare with) and the Report tab, per
// docs/design/M10-insights-v3.md §5 and mockup PNG 1. Sources U, S, E only (FR-031); every card carries the response's
// source tag (FR-032). Every comparison is like for like (FR-048): bars and deltas use `now` vs `before`, never `value`,
// and the coverage banner says when that means this PC only. Fingerprint scores stay on screen: drawn as SVG that can't
// be selected, no copy or export action (design §2 anti-goals).
import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { AnimatePresence, m, useReducedMotion } from "motion/react";
import { ArrowDownRightIcon, ArrowUpRightIcon, CaretLeftIcon, CaretRightIcon } from "@phosphor-icons/react";
import * as api from "../api";
import type { Compared, Covered, Fingerprint, Grain, PeriodInfo, PeriodQuery, PeriodReport, TopRow } from "../api";
import { CountUp, Empty, ErrorBanner, InsightCard as Card, Seg, SkelRows, Stat, Stars, useSlide } from "../components/ui";
import { Grow, MonthBars, monthLong, SPRING, Versus } from "../components/charts";
import { compareOptions, periodLabel, periodSpan, stepPeriod, vsName } from "../components/logic";
import { useQuickAdd } from "../components/QuickAddContext";
import { useApi } from "../hooks/useApi";
import { day, hm, hmBig, hrs, monthName, num, pl, STATUS, time } from "../lib/format";
import { EASE, RISE, swap } from "../lib/motion";

type Dir = "from-left" | "from-right";
const GRAINS: [Grain, string][] = [["week", "Week"], ["month", "Month"], ["year", "Year"], ["all", "All time"], ["custom", "Custom…"]];
const hh = (h: number) => `${String(h).padStart(2, "0")}:00`;
// Our own formats, not toLocale*: WebView2 and Edge versions disagree ("Sep" vs "Sept").
const WD = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const longDay = (d: string) => `${WD[new Date(d + "T00:00").getDay()]} ${day(d + "T00:00")}`;
const monthOf = (d: string) => monthLong(d.slice(0, 7)).split(" ")[0];
/** "month" / "week" / "year" / "stretch"; null for All time. */
const unitOf = (g: Grain) => (g === "all" ? null : g === "custom" ? "stretch" : g);

/* ------------------------------------------------------------------ page header */
/** Grain · ‹ period › · Compare with (design §5). Replaces the range control on the listening tabs only. */
/** `bounds` = the loaded report's PeriodInfo for this `q`: ‹ stops before your first play, › after the current period. */
export function PeriodHeader({ q, today, bounds, onChange }: { q: PeriodQuery; today: string; bounds?: Pick<PeriodInfo, "prev_at" | "next_at">; onChange: (q: PeriodQuery, dir: Dir) => void }) {
  const prev = bounds && bounds.prev_at == null ? null : stepPeriod(q, -1, today);
  const next = bounds && bounds.next_at == null ? null : stepPeriod(q, 1, today);
  const opts = compareOptions(q, today);
  const unit = unitOf(q.grain) ?? "period";
  const order = GRAINS.map((g) => g[0]);
  const grain = (g: Grain) => {
    const dir: Dir = order.indexOf(g) >= order.indexOf(q.grain) ? "from-right" : "from-left";
    // Keep where you were: Sep 2026 as a year is 2026, as a week its first week; Custom starts as the period shown.
    const from = q.grain === "custom" ? q.from! : q.grain === "all" ? null : periodSpan(q.grain, q.at, today).from;
    if (g === "custom") return onChange({ grain: g, vs: q.vs, at: null, ...(q.grain === "week" || q.grain === "month" || q.grain === "year" ? periodSpan(q.grain, q.at, today) : periodSpan("month", null, today)) }, dir);
    const vs = g === "year" && q.vs === "last_year" ? "prev" : q.vs;
    if (g === "all" || !from) return onChange({ grain: g, vs, at: null }, dir);
    const at = g === "year" ? from.slice(0, 4) : g === "month" ? from.slice(0, 7) : periodSpan("week", from, today).from;
    const cand: PeriodQuery = { grain: g, vs, at };
    onChange(stepPeriod(cand, 0, today) ?? { ...cand, at: null }, dir);
  };
  const date = (k: "from" | "to", v: string) => {
    const r = { from: q.from ?? today, to: q.to ?? today, [k]: v };
    if (v && r.from <= r.to && r.to <= today) onChange({ ...q, ...r }, "from-right");
  };
  return (
    <div className="in3-head">
      <Seg radio label="Period length" options={GRAINS} value={q.grain} onChange={grain} />
      {(q.grain === "week" || q.grain === "month" || q.grain === "year") && (
        <div className="in3-step" role="group" aria-label={`Choose the ${unit}`}>
          <button type="button" className="icon-btn" aria-label={`Previous ${unit}`} aria-disabled={!prev} onClick={() => prev && onChange(prev, "from-left")}><CaretLeftIcon size={16} weight="bold" aria-hidden /></button>
          <output aria-live="polite">{periodLabel(q, today)}</output>
          <button type="button" className="icon-btn" aria-label={`Next ${unit}`} aria-disabled={!next} onClick={() => next && onChange(next, "from-right")}><CaretRightIcon size={16} weight="bold" aria-hidden /></button>
        </div>)}
      {q.grain === "custom" && <span className="flex flex-wrap gap-2 items-center">
        <input type="date" className="input surface h-10" aria-label="From" value={q.from ?? ""} max={q.to ?? today} onChange={(e) => date("from", e.target.value)} />
        <span className="text-muted text-sm">to</span>
        <input type="date" className="input surface h-10" aria-label="To" value={q.to ?? ""} min={q.from} max={today} onChange={(e) => date("to", e.target.value)} />
      </span>}
      {opts.length > 0 && <select className="input surface h-10" aria-label="Compare with" value={q.vs} onChange={(e) => onChange({ ...q, vs: e.target.value as PeriodQuery["vs"] }, "from-right")}>
        {opts.map(([v, t]) => <option key={v} value={v}>{t}</option>)}
      </select>}
    </div>
  );
}

/* ------------------------------------------------------------------ small parts */
/** "4h 25m more than August" from a like-for-like pair; null when there is nothing to compare. */
function change(c: Compared, fmt: (n: number) => string, more: string, less: string, name: string, pc: boolean) {
  if (c.now == null || c.before == null) return null;
  const on = pc ? " on this PC" : ""; // FR-048: past the export's end both sides count this PC only
  if (Math.round(c.now) === Math.round(c.before)) return { up: null, text: `Same${on} as ${name}` };
  return { up: c.now > c.before, text: `${fmt(Math.abs(c.now - c.before))} ${c.now > c.before ? more : less}${on} than ${name}` };
}
export function Delta({ c, fmt, more, less, name, pc, rest }: { c: Compared; fmt: (n: number) => string; more: string; less: string; name: string; pc: boolean; rest: string }) {
  const d = change(c, fmt, more, less, name, pc);
  if (!d) return <>{rest}</>;
  const Icon = d.up ? ArrowUpRightIcon : ArrowDownRightIcon;
  return <Versus now={c.now!} prev={c.before!}><span className="hm-delta">{d.up != null && <Icon size={12} weight="bold" aria-hidden />}{d.text}</span></Versus>;
}
const Opinion = ({ r }: { r: Pick<TopRow, "status" | "rating"> }) => (
  <span className="text-muted text-xs whitespace-nowrap">{r.status ? STATUS[r.status] : "Not saved"}{r.rating ? " " : ""}<Stars r={r.rating} /></span>
);
function Ring({ pct, label }: { pct: number; label: string }) {
  const reduce = useReducedMotion();
  return (
    <div className="hm-ring" role="img" aria-label={label}>
      <svg viewBox="0 0 64 64" aria-hidden><circle cx="32" cy="32" r="28" className="track" />
        <m.circle cx="32" cy="32" r="28" className="fill" initial={reduce ? false : { pathLength: 0 }} animate={{ pathLength: Math.min(1, Math.max(0.01, pct / 100)) }} transition={{ duration: 0.9, ease: EASE, delay: 0.2 }} /></svg>
      <span aria-hidden>{Math.round(pct)}%</span>
    </div>
  );
}

/* ------------------------------------------------------------------ the Report */
type Res = Covered<PeriodReport>;

export function Report({ q, today, onPeriod }: { q: PeriodQuery; today: string; onPeriod?: (p: PeriodInfo) => void }) {
  const reduce = useReducedMotion();
  const rep = useApi(() => api.getPeriodReport(q), [q.grain, q.vs, q.at, q.from, q.to]);
  const info = rep.data?.data.period;
  useEffect(() => { if (info) onPeriod?.(info); }, [info, onPeriod]);
  if (rep.loading) return <ReportSkel />;
  if (rep.error) return <ErrorBanner error={rep.error} title="Couldn’t load the report." onRetry={rep.reload} />;
  const res = rep.data!, r = res.data;
  const pc = !!res.coverage.desktop_only_since;
  const name = vsName(q, r.period.vs_from);
  const unit = unitOf(q.grain);
  if (!r.tiles.plays.value && !r.tiles.ms.value) return <>
    <CoverageNotes res={res} q={q} />
    <Empty h={`No listening in ${unit ? periodLabel(q, today) : "your history"}`} line="Import your Spotify history to see this, or pick another period.">
      <Link className="btn btn-primary" to="/import/1">Import history</Link>
    </Empty>
  </>;
  const t = r.tiles;
  const fin = t.finished_pct;
  return (
    <m.div className="flex flex-col gap-6" initial={reduce ? false : "hidden"} animate="show">
      <m.div variants={RISE} custom={0} className="flex flex-col gap-3 empty:hidden"><CoverageNotes res={res} q={q} /></m.div>
      <m.section variants={RISE} custom={1} className="stats in3-tiles" aria-label={`${periodLabel(q, today)} in numbers`}>
        <Stat k="Listening time" v={hmBig(t.ms.value)} d={<Delta c={t.ms} fmt={hmBig} more="more" less="less" name={name} pc={pc} rest="In this period" />} />
        <Stat k="Plays" v={num(t.plays.value)} d={<Delta c={t.plays} fmt={num} more="more" less="fewer" name={name} pc={pc} rest="Played 30 seconds or more" />} />
        <Stat k="Artists" v={num(t.artists.value)} d={<Delta c={t.artists} fmt={num} more="more" less="fewer" name={name} pc={pc} rest="With at least one play" />} />
        <Stat k="New artists" v={num(t.new_artists.value)} d={<Delta c={t.new_artists} fmt={num} more="more" less="fewer" name={name} pc={pc} rest="First heard in this period" />} />
        <Stat k={pc ? "Days listened on this PC" : "Days listened"} v={`${num(t.days_listened.value)} / ${num(r.period.days)}`}
          d={<span className="flex flex-col gap-2.5"><Delta c={t.days_listened} fmt={num} more="more" less="fewer" name={name} pc={pc} rest="" /><span>Current streak {pl(t.streak, "day")}</span></span>} />
        <Stat k="Tracks finished" v={fin ? `${Math.round(fin.value)}%` : "None"}
          d={!fin ? "Export plays only, and none in this period" : <Delta c={fin} fmt={(n) => pl(Math.round(n), "point")} more="higher" less="lower" name={name} pc={false} rest="Of export plays, played to the end" />} />
      </m.section>
      <m.div variants={RISE} custom={2} className="flex flex-wrap gap-7 items-stretch">
        <Timeline r={r} q={q} name={name} res={res} pc={pc} />
        <Lines r={r} unit={unit} res={res} />
      </m.div>
      <m.div variants={RISE} custom={3} className="flex flex-wrap gap-7 items-stretch">
        <Clock r={r} res={res} />
        <TopNew r={r} unit={unit} res={res} />
      </m.div>
      <m.div variants={RISE} custom={4} className="flex flex-wrap gap-7 items-stretch">
        <Ratio r={r} name={name} res={res} />
        <Print r={r} label={periodLabel(q, today)} res={res} pc={pc} />
        <Tags r={r} unit={unit} res={res} />
      </m.div>
      <m.div variants={RISE} custom={5}><MostPlayed r={r} unit={unit} name={name} res={res} /></m.div>
    </m.div>
  );
}

/** Coverage banner (FR-048, design §4.4) when the period runs past the last export; why there's no delta; no-data stretches. */
export function CoverageNotes({ res, q }: { res: Pick<Res, "coverage">; q: PeriodQuery }) {
  const c = res.coverage;
  return <>
    {c.desktop_only_since && (
      <div className="banner warn" role="note">
        <span className="flex flex-col gap-0.5"><b className="text-sm">From {day(c.desktop_only_since + "T00:00")}, only plays on this PC are counted</b>
          <span className="text-xs text-text-2">Phone and other devices arrive with your next Spotify export. Until then, comparisons use desktop plays on both sides so the change is like for like.</span></span>
        <Link className="btn btn-secondary s" to="/import/1">Import a newer export</Link>
      </div>)}
    {c.no_compare_reason && q.grain !== "all" && <p className="text-muted text-sm m-0">{c.no_compare_reason}</p>}
    {c.gaps.length > 0 && <p className="text-muted text-sm m-0">
      No data for {c.gaps.map((g) => `${day(g.from + "T00:00")} to ${day(g.to + "T00:00")}`).join(", ")}: the logger was off and no export covers {c.gaps.length === 1 ? "it" : "them"}. Hatched in the chart, never counted as silence.
    </p>}
  </>;
}

function Timeline({ r, q, name, res, pc }: { r: PeriodReport; q: PeriodQuery; name: string; res: Res; pc: boolean }) {
  const t = r.timeline;
  const daily = t[0]?.key.length === 10;
  const unit = daily ? "day" : "month";
  const label = (k: string) => (daily ? longDay(k) : monthLong(k));
  const total = t.reduce((a, x) => a + x.ms, 0);
  const peak = t.reduce((p, x, i) => (x.ms > t[p].ms ? i : p), 0);
  const ghost = t.some((x) => x.before_ms != null) ? t.map((x) => x.before_ms) : undefined;
  const gap = (i: number) => t[i].coverage === "no_data";
  const tick = (i: number, on: boolean) => {
    const k = t[i].key;
    if (q.grain === "week") return longDay(k).slice(0, 3);
    if (daily) return on || i === 0 || i === t.length - 1 || i % 7 === 0 ? String(+k.slice(8, 10)) : "";
    if (t.length <= 24) return on || i % (t.length > 12 ? 3 : 1) === 0 ? monthName(k) : "";
    return on || (k.endsWith("-01") && +k.slice(0, 4) % (t.length > 72 ? 2 : 1) === 0) ? k.slice(0, 4) : ""; // All time: years
  };
  const title = q.grain === "month" ? `Every day of ${monthOf(r.period.from)}` : q.grain === "week" ? "Every day of the week"
    : daily ? "Every day of this stretch" : q.grain === "year" ? `Every month of ${r.period.from.slice(0, 4)}` : q.grain === "all" ? "Every month you’ve listened" : "Every month of this stretch";
  return (
    <Card title={title} res={res} className="flex-[2_1_520px] min-w-0">
      <span className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1"><span className="hm-big"><CountUp text={hmBig(total)} /></span>
        {t.length > 0 && <span className="text-muted text-sm">listened{pc && ghost ? " on this PC" : ""}, busiest {daily ? "on" : "in"} <b className="text-text">{label(t[peak].key)}</b> ({hm(t[peak].ms)}).</span>}</span>
      {t.length < 2 ? <span className="text-muted">Not enough of the period yet to draw.</span>
        : <MonthBars months={t.map((x) => ({ month: x.key, value: x.ms }))} fmt={hm} hot={peak} tick={tick} ghost={ghost} gap={gap}
          label={`Listening time per ${unit}${ghost ? `, with ${name} as a line` : ""}`}
          valuetext={(i) => `${label(t[i].key)}: ${gap(i) ? "no data" : hm(t[i].ms)}${t[i].before_ms != null ? `; ${name}: ${hm(t[i].before_ms!)}` : ""}`}
          caption={(i) => i === null ? (ghost ? `The faint line is ${name}, ${unit} by ${unit}.` : `Point at a ${unit} or use the arrow keys.`)
            : `${label(t[i].key)}: ${gap(i) ? "no data" : hm(t[i].ms)}${t[i].before_ms != null ? `, ${name} ${hm(t[i].before_ms!)}` : ""}.`} />}
    </Card>
  );
}

/** "The month in a few lines" (design §5.1): sentences Rust picked by fixed rules; emphasised parts in bold. */
function Lines({ r, unit, res }: { r: PeriodReport; unit: string | null; res: Res }) {
  const reduce = useReducedMotion();
  return (
    <Card title={unit ? `The ${unit} in a few lines` : "All time in a few lines"} res={res} side>
      {r.lines.length ? r.lines.map((s, i) => (
        <m.p key={s.rule} className="in3-line m-0" initial={reduce ? false : { opacity: 0, x: 10 }} animate={{ opacity: 1, x: 0 }} transition={{ duration: 0.35, ease: EASE, delay: 0.2 + i * 0.08 }}>
          <span>{s.parts.map((p, k) => (p.em ? <b key={k}>{p.text}</b> : <span key={k}>{p.text}</span>))}</span>
        </m.p>)) : <span className="text-muted">Nothing stands out{unit ? ` this ${unit}` : ""}.</span>}
      <span className="text-faint text-xs">Picked from the numbers on this page by fixed rules. Nothing leaves this PC.</span>
    </Card>
  );
}

const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
function Clock({ r, res }: { r: PeriodReport; res: Res }) {
  const { hours, baseline, weekdays } = r.clock;
  if (hours.length !== 24) return null;
  const best = hours.reduce((p, x, i) => (x > hours[p] ? i : p), 0);
  const wmax = Math.max(1e-9, ...weekdays), wbest = weekdays.indexOf(Math.max(...weekdays));
  const base = baseline.length === 24;
  return (
    <Card title="When you listened" res={res} className="flex-[1_1_420px] min-w-0">
      <span className="text-text-2 text-sm"><b className="text-text font-mono">{hh(best)}</b> was your busiest hour.{base ? " The faint line is your usual day over the last 6 months." : ""}</span>
      <MonthBars months={hours.map((v, i) => ({ month: String(i), value: v }))} fmt={(v) => `${Math.round(v)}%`} hot={best} ghost={base ? baseline : undefined}
        tick={(i, on) => (on || i % 6 === 0 || i === 23 ? String(i).padStart(2, "0") : "")} label="Share of your listening by hour of day"
        valuetext={(i) => `${hh(i)}: ${Math.round(hours[i])}% of your listening${base ? `, usually ${Math.round(baseline[i])}%` : ""}`}
        caption={(i) => (i === null ? "" : `${hh(i)}: ${Math.round(hours[i])}%${base ? `, usually ${Math.round(baseline[i])}%` : ""}.`)} />
      {weekdays.length === 7 && <div className="in3-week" role="img" aria-label={`Busiest weekday: ${DAYS[wbest]}`}>
        {weekdays.map((v, i) => <span key={i} className={i === wbest ? "on" : undefined}><Grow pct={(v / wmax) * 100} i={i} hot={i === wbest} />{DAYS[i]}</span>)}
      </div>}
    </Card>
  );
}

function TopNew({ r, unit, res }: { r: PeriodReport; unit: string | null; res: Res }) {
  const add = useQuickAdd();
  const n = r.top_new;
  return (
    <Card title={unit ? `Top new this ${unit}` : "Top new artist"} res={res} className="flex-[1_1_420px] min-w-0">
      {!n ? <span className="text-muted">No new artists in this period.</span> : <>
        <div className="hm-lead flex-none">
          <div className="flex justify-between items-start gap-4">
            <div className="flex flex-col gap-1.5 min-w-0">
              <span className="text-muted text-xs">First played {day(n.first_played_local)}, {time(n.first_played_local)}, with “{n.first_track}”</span>
              {n.item_id ? <Link to={`/item/${n.item_id}`} className="hm-lead-name text-text">{n.name}</Link> : <span className="hm-lead-name">{n.name}</span>}
            </div>
            <Ring pct={n.share_pct} label={`${Math.round(n.share_pct)}% of your listening time in this period`} />
          </div>
          <div className="flex flex-wrap gap-7">
            <span className="flex flex-col gap-0.5"><span className="hm-big"><CountUp text={hmBig(n.ms)} /></span><span className="text-muted text-xs">since then</span></span>
            <span className="flex flex-col gap-0.5"><span className="hm-big"><CountUp text={num(n.plays)} /></span><span className="text-muted text-xs">plays</span></span>
            <span className="flex flex-col gap-0.5"><span className="hm-big">#{n.rank}</span><span className="text-muted text-xs">{unit ? `this ${unit}` : "of all time"}</span></span>
          </div>
          <div className="flex flex-wrap gap-2 items-center">
            {n.item_id ? <><span className="badge status">{n.status ? STATUS[n.status] : "In your library"}</span><Link to={`/item/${n.item_id}`} className="btn btn-secondary s">Open</Link></>
              : <><span className="badge status">Not in your library</span>
                <button type="button" className="btn btn-light s" onClick={() => add({ suggest: { name: n.name, type: "artist", from: "plays" } })}>Save to backlog</button></>}
          </div>
        </div>
        {r.also_new.length > 0 && <span className="flex flex-wrap gap-1.5 items-center text-muted text-xs">Also new:
          {r.also_new.map((a) => <span key={a} className="chip">{a}</span>)}{r.also_new_more > 0 && <span className="chip">+{num(r.also_new_more)}</span>}</span>}
      </>}
    </Card>
  );
}

/** FR-048: beside a "before", the like-for-like `now`; `value` (every source) only when nothing is compared. */
export const shown = (c: Compared) => (c.before != null && c.now != null ? c.now : c.value);
function Ratio({ r, name, res }: { r: PeriodReport; name: string; res: Res }) {
  const rows: [string, Compared][] = [["Tracks", r.ratio.tracks], ["Albums", r.ratio.albums], ["Artists", r.ratio.artists]];
  const per = (c: Compared, what: string) => <span><b className="font-mono text-text">{shown(c).toFixed(1)}</b> {what}{c.before != null && <span className="text-faint"> ({name} {c.before.toFixed(1)})</span>}</span>;
  return (
    <Card title="Music ratio" res={res} className="flex-[1_1_300px] min-w-0">
      <span className="text-text-2 text-sm">How many different things you played{r.ratio.tracks.before != null ? `, vs ${name}` : ""}.</span>
      {rows.map(([k, c]) => {
        const d = c.now != null && c.before != null ? Math.round(c.now - c.before) : null;
        return (
          <div key={k} className="flex flex-col gap-1.5">
            <span className="flex justify-between items-baseline gap-3 text-sm"><span>{k}</span>
              <span className="font-mono"><CountUp text={num(c.value)} />{d != null && d !== 0 && <><span className={`text-xs ${d > 0 ? "text-text-2" : "text-faint"}`} aria-hidden> {d > 0 ? "▲" : "▼"} {num(Math.abs(d))}</span>
                <span className="sr-only">, {num(Math.abs(d))} {d > 0 ? "more" : "fewer"} than {name}</span></>}</span></span>
            {c.now != null && c.before != null && <Versus now={c.now} prev={c.before}>{null}</Versus>}
          </div>);
      })}
      <span className="flex flex-wrap gap-x-5 gap-y-1 text-sm text-text-2 pt-2 border-t border-line">{per(r.ratio.tracks_per_artist, "tracks per artist")}{per(r.ratio.albums_per_artist, "albums per artist")}</span>
    </Card>
  );
}

/** Listening fingerprint (design §4.2): five 0–100 measures against your own 12-month average. On screen only. */
const AXES: [keyof Fingerprint, string][] = [["regularity", "Regularity"], ["discovery", "Discovery"], ["focus", "Focus"], ["variety", "Variety"], ["repeat", "Repeat"]];
function Print({ r, label: period, res, pc }: { r: PeriodReport; label: string; res: Res; pc: boolean }) {
  const label = pc ? `${period}, on this PC` : period; // API.md §4: fingerprint.now counts source S while desktop_only_since is set
  const reduce = useReducedMotion();
  const { now, average } = r.fingerprint;
  const [cx, cy, R] = [160, 128, 84];
  const pt = (i: number, v: number) => { const a = ((-90 + i * 72) * Math.PI) / 180; return [cx + Math.cos(a) * R * (v / 100), cy + Math.sin(a) * R * (v / 100)]; };
  const poly = (f: Fingerprint | Record<string, number>) => AXES.map(([k], i) => pt(i, (f as Record<string, number>)[k]).map((x) => x.toFixed(1)).join(",")).join(" ");
  const ring = (v: number) => poly(Object.fromEntries(AXES.map(([k]) => [k, v])));
  const aria = AXES.map(([k, l]) => `${l} ${Math.round(now[k])}${average ? `, your average ${Math.round(average[k])}` : ""}`).join("; ");
  return (
    <Card title="Listening fingerprint" res={res} className="flex-[1_1_300px] min-w-0">
      <svg className="in3-radar" viewBox="-30 0 380 256" role="img" aria-label={`Listening fingerprint for ${label}, 0 to 100: ${aria}`}>
        {[25, 50, 75, 100].map((v) => <polygon key={v} className="web" points={ring(v)} />)}
        {AXES.map((_, i) => { const [x, y] = pt(i, 100); return <line key={i} className="web" x1={cx} y1={cy} x2={x} y2={y} />; })}
        {average && <polygon className="avg" points={poly(average)} />}
        <m.polygon className="now" points={poly(now)} initial={reduce ? false : { scale: 0.3, opacity: 0 }} animate={{ scale: 1, opacity: 1 }} transition={{ ...SPRING, delay: 0.2 }} />
        {AXES.map(([k, l], i) => {
          const [x, y] = pt(i, 122), c = Math.cos(((-90 + i * 72) * Math.PI) / 180);
          return <text key={k} x={x} y={y + 4} textAnchor={c > 0.3 ? "start" : c < -0.3 ? "end" : "middle"}>{l} <tspan>{Math.round(now[k])}</tspan></text>;
        })}
      </svg>
      <span className="in3-key"><span><i />{label}</span>{average && <span><i className="avg" />Your 12-month average</span>}</span>
      <span className="text-faint text-xs">Measured against your own past only. Shown here, never exported or shared.</span>
    </Card>
  );
}

function Tags({ r, unit, res }: { r: PeriodReport; unit: string | null; res: Res }) {
  const rows = r.tags.rows;
  const max = Math.max(1e-9, ...rows.map((x) => x.share_pct));
  return (
    <Card title="Top tags" res={res} className="flex-[1_1_300px] min-w-0">
      <span className="text-text-2 text-sm">From the tags on your records, weighted by listening time.</span>
      {rows.length ? <div className="flex flex-col gap-1">{rows.map((x, i) => (
        <div key={x.tag} className="in3-tag"><span className="chip justify-self-start truncate max-w-full">{x.tag}</span><Grow pct={(x.share_pct / max) * 100} i={i} hot={i === 0} />
          <span className="font-mono text-text-2 text-xs text-right">{Math.round(x.share_pct)}%</span></div>))}</div>
        : <span className="text-muted">No tagged records played in this period.</span>}
      <span className="text-muted text-xs">Covers {Math.round(r.tags.covered_pct)}% of {unit ? `this ${unit}’s` : "your"} listening: plays of things in your library. <Link to="/library">Tag the rest</Link></span>
    </Card>
  );
}

type Kind = "artists" | "albums" | "tracks";
const KINDS: [Kind, string][] = [["artists", "Artists"], ["albums", "Albums"], ["tracks", "Tracks"]];
function Spark({ v }: { v: number[] }) {
  if (v.length < 2) return <span />;
  const max = Math.max(1e-9, ...v);
  return <svg className="in3-spark" viewBox="0 0 64 20" preserveAspectRatio="none" aria-hidden>
    <polyline points={v.map((x, i) => `${((i / (v.length - 1)) * 64).toFixed(1)},${(19 - (x / max) * 18).toFixed(1)}`).join(" ")} vectorEffect="non-scaling-stroke" /></svg>;
}
function Move({ x, name }: { x: TopRow; name: string }) {
  if (x.is_new) return <span className="in3-move new">New</span>;
  if (x.move == null) return <span />;
  const t = x.move > 0 ? `▲${x.move}` : x.move < 0 ? `▼${-x.move}` : "=";
  return <span className={`in3-move${x.move > 0 ? " up" : ""}`}><span aria-hidden>{t}</span>
    <span className="sr-only">{x.move ? `${x.move > 0 ? "up" : "down"} ${pl(Math.abs(x.move), "place")} vs ${name}` : `same place as ${name}`}</span></span>;
}
function MostPlayed({ r, unit, name, res }: { r: PeriodReport; unit: string | null; name: string; res: Res }) {
  const reduce = useReducedMotion();
  const [kind, setKind] = useState<Kind>("artists");
  const [dir, slide] = useSlide(KINDS.map((k) => k[0]), kind);
  const rows = r.most_played[kind].slice(0, 10);
  const max = Math.max(1e-9, ...rows.map((x) => x.hours));
  return (
    <Card title={unit ? `Most played this ${unit}` : "Most played of all time"} res={res}>
      <Seg label="Most played" options={KINDS} value={kind} onChange={(k) => { slide(k); setKind(k); }} />
      <AnimatePresence mode="wait" initial={false}>
        <m.div key={kind} role="tabpanel" aria-label={KINDS.find((k) => k[0] === kind)![1]} className="min-w-0"
          initial={{ opacity: 0, x: dir === "from-right" ? 18 : -18 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, transition: swap(reduce, { duration: 0.12 }) }} transition={swap(reduce, { duration: 0.32, ease: EASE })}>
          {!rows.length ? <span className="text-muted">No plays in this period.</span> : <ol className="hm-ranks in3-ranks">{rows.map((x, i) => (
            <li key={x.name + x.by} className="hm-rank in3-rank">
              <span className="hm-rank-n">{i + 1}</span>
              <div className="min-w-0 flex flex-col gap-1">
                <span className="truncate" title={kind === "artists" ? x.name : `${x.name}, ${x.by}`}>{x.item_id ? <Link to={`/item/${x.item_id}`} className="hm-stretch text-text font-semibold">{x.name}</Link> : <b>{x.name}</b>}
                  {kind !== "artists" && x.by && <span className="text-muted">, {x.by}</span>}</span>
                <m.span className={`hm-bar${i === 0 ? " hot" : ""}`} style={{ width: `${Math.max(2, (x.hours / max) * 100)}%`, originX: 0 }}
                  initial={reduce ? false : { scaleX: 0 }} animate={{ scaleX: 1 }} transition={{ ...SPRING, delay: 0.08 + i * 0.035 }} />
              </div>
              <Spark v={x.spark} />
              <span className="flex flex-col items-end gap-0.5"><span className="font-mono text-text-2 text-xs whitespace-nowrap">{hrs(x.hours)}</span><Opinion r={x} /></span>
              <Move x={x} name={name} />
            </li>))}</ol>}
        </m.div>
      </AnimatePresence>
      <span className="flex flex-wrap gap-x-4 gap-y-1 text-muted text-xs"><span>▲▼ places vs {name}</span><span><span className="text-accent font-semibold">New</span>: not in the top 50 before</span><span>Line: hours per month, the last 12 months</span></span>
    </Card>
  );
}

const ReportSkel = () => (
  <div className="flex flex-col gap-6" aria-busy="true" aria-label="Loading">
    <div className="stats">{Array.from({ length: 6 }, (_, i) => <div key={i} className="card stat h-[132px]"><div className="skel h-full" /></div>)}</div>
    <div className="flex flex-wrap gap-7 items-start">
      <div className="card flex-[2_1_520px] min-w-0 h-[300px]"><div className="skel h-full" /></div>
      <div className="flex-[1_1_320px] min-w-0"><SkelRows n={4} /></div>
    </div>
  </div>
);

// SC-09 Insights v3, How you listen tab (M10 WI 7), per docs/design/M10-insights-v3.md §5 and mockup PNG 4. For the period in
// the page header: how plays ended, skips by hour, quick bails, shuffle by year, where you listen (+ offline), finished
// tracks (+ "how far in" with its coverage), sessions and deep dives. Ends, skips, bails and finished come from export plays
// only (API.md §4.1), and the banner says so; devices and sessions count plays on this PC too. Sessions, deep dives and
// "start to finish" are estimates and say so (design §4.2). Sources and freshness from the response on every card (FR-032).
import { Link } from "react-router-dom";
import { m, useReducedMotion } from "motion/react";
import * as api from "../api";
import type { Covered, DeviceClass, Habits as Data, PeriodQuery } from "../api";
import { CountUp, Empty, ErrorBanner, InsightCard as Card } from "../components/ui";
import { Grow, MonthBars, monthLong, SPRING } from "../components/charts";
import { periodLabel } from "../components/logic";
import { useApi } from "../hooks/useApi";
import { day, dayY, hm, num, pl, time } from "../lib/format";
import { RISE } from "../lib/motion";
import { CoverageNotes } from "./InsightsReport";

type Res = Covered<Data>;
type Dive = Data["deep_dives"][number];
const hh = (h: number) => `${String(h).padStart(2, "0")}:00`;
const WD = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const wday = (local: string) => `${WD[new Date(local).getDay()]} ${day(local)}`;
/** reason_end groups; the class reuses the .in3-split colours (finished text-2, skipped accent, back bar grey, stopped line). */
const ENDS: [Data["ends"][number]["kind"], string, string][] = [["finished", "Played to the end", "back"], ["skipped", "Skipped forward", "picked"], ["back", "Went back", "followed"], ["stopped", "Stopped or closed", "other"]];
const DEVICE: Record<DeviceClass, string> = { phone: "Phone", this_pc: "This PC", computer: "Other computers", web: "Web player", other: "Other" };
/** Session length buckets (API.md: edges 5, 15, 30, 45, 60, 90, 120, 180, 240 min). */
export const BUCKETS = ["Under 5 minutes", "5 to 15 minutes", "15 to 30 minutes", "30 to 45 minutes", "45 minutes to 1 hour", "1 to 1.5 hours", "1.5 to 2 hours", "2 to 3 hours", "3 to 4 hours", "4 hours or more"];
const TICKS = ["<5m", "5m", "15m", "30m", "45m", "1h", "1.5h", "2h", "3h", "4h+"];
/** A deep dive's name line: the artist, the album and how many times through, or the track on repeat. */
export const diveTitle = (d: Dive) => d.kind === "repeat" ? `“${d.name}” on repeat` : d.kind === "album" ? (d.times && d.times > 1 ? `${d.name}, ${pl(d.times, "time")} through` : d.name) : d.name;
/** Its when line (one middle dot per line, DESIGN copy rules). */
export const diveWhen = (d: Dive) => d.kind === "repeat" ? `${wday(d.start_local)} · ${pl(d.times ?? d.tracks, "play")} that day`
  : `${wday(d.start_local)}, ${time(d.start_local)} to ${time(d.end_local)} · ${pl(d.tracks, "track")}`;
const unitOf = (q: PeriodQuery) => (q.grain === "all" ? null : q.grain === "custom" ? "stretch" : q.grain);
const NO_EXPORT = "No Spotify export plays in this period. Plays logged on this PC don’t record this.";

export function Habits({ q, today }: { q: PeriodQuery; today: string }) {
  const reduce = useReducedMotion();
  const h = useApi(() => api.getListeningHabits(q), [q.grain, q.vs, q.at, q.from, q.to]);
  if (h.loading) return <Skel />;
  if (h.error) return <ErrorBanner error={h.error} title="Couldn’t load how you listen." onRetry={h.reload} />;
  const res = h.data!, d = res.data;
  // Nothing is compared on this tab: only the gaps note applies; the export banner below says where the export ends.
  const notes = <CoverageNotes res={{ coverage: { ...res.coverage, no_compare_reason: null, desktop_only_since: null } }} q={q} />;
  if (!d.export_plays && !d.sessions.count) return <>
    {notes}
    <Empty h="No listening in this period" line="Import your Spotify history to see this.">
      <Link className="btn btn-primary" to="/import/1">Import history</Link>
    </Empty>
  </>;
  const until = res.freshness.export_latest_event;
  // Export-only cards name only the export (and the library, if linked), not the desktop log (FR-032, API.md §4.1).
  const exp = { ...res, sources: res.sources.filter((x) => x !== "S") };
  return (
    <m.div className="flex flex-col gap-6" initial={reduce ? false : "hidden"} animate="show">
      <m.div variants={RISE} custom={0} className="flex flex-col gap-3">
        <div className="banner" role="note">
          <span className="text-sm text-text-2">Skips, why plays ended and finished tracks come from your Spotify export. Plays on this PC don’t record them, so those cards cover your export plays{until ? ` (to ${dayY(until)})` : ""}: <b className="text-text">{pl(d.export_plays, "play")}</b>. Where you listen and sessions count plays on this PC too.</span>
          {res.coverage.desktop_only_since && <Link className="btn btn-secondary s" to="/import/1">Import a newer export</Link>}
        </div>
        {notes}
      </m.div>
      <m.section variants={RISE} custom={1} className="flex flex-wrap gap-7 items-stretch" aria-label={`How you listen in ${periodLabel(q, today)}`}>
        <Ends res={exp} />
        <SkipHours res={exp} />
        <Bails res={exp} />
      </m.section>
      <m.div variants={RISE} custom={2} className="flex flex-wrap gap-7 items-stretch">
        <Shuffle res={res} />
        <Devices res={res} />
        <Finished res={exp} />
      </m.div>
      <m.div variants={RISE} custom={3} className="flex flex-wrap gap-7 items-stretch">
        <Sessions res={res} unit={unitOf(q)} />
        <Dives res={res} />
      </m.div>
    </m.div>
  );
}

const third = "flex-[1_1_300px] min-w-0";
const pct = (v: number) => `${Math.round(v)}%`;

function Ends({ res }: { res: Res }) {
  const reduce = useReducedMotion();
  const d = res.data, of = (k: string) => d.ends.find((x) => x.kind === k)?.pct ?? 0;
  return (
    <Card title="How plays ended" res={res} className={third}>
      {!d.ends.length ? <span className="text-muted">{NO_EXPORT}</span> : <>
        <div className="in3-split" role="img" aria-label={ENDS.map(([k, l]) => `${l} ${pct(of(k))}`).join(", ")}>
          {ENDS.map(([k, , c], i) => of(k) > 0 && <m.i key={k} className={c} style={{ width: `${of(k)}%`, originX: 0 }}
            initial={reduce ? false : { scaleX: 0 }} animate={{ scaleX: 1 }} transition={{ ...SPRING, delay: 0.2 + i * 0.06 }} />)}
        </div>
        <span className="in3-key flex-col" aria-hidden>{ENDS.map(([k, l, c]) => <span key={k}><i className={c} />{l} <b className="font-mono text-text">{pct(of(k))}</b></span>)}</span>
      </>}
      {d.most_skipped && <span className="text-text-2 text-sm">Most skipped of your top 20 artists:{" "}
        {d.most_skipped.item_id ? <Link to={`/item/${d.most_skipped.item_id}`} className="font-semibold text-text">{d.most_skipped.name}</Link> : <b className="text-text">{d.most_skipped.name}</b>}, with {pct(d.most_skipped.skip_pct)} of tracks skipped.</span>}
    </Card>
  );
}

function SkipHours({ res }: { res: Res }) {
  const s = res.data.skips_by_hour;
  const most = s.indexOf(Math.max(...s));
  // ponytail: no "least at" clause: a 0% hour may have had no plays at all; add it when the payload carries per-hour plays.
  return (
    <Card title="Skips by hour" res={res} className={third}>
      {!res.data.export_plays || s.length !== 24 ? <span className="text-muted">{NO_EXPORT}</span> : !s.some((v) => v > 0) ? <span className="text-muted">No skips in this period.</span> : <>
        <span className="text-text-2 text-sm"><b className="font-mono text-text">{pct(res.data.skip_rate)}</b> of plays skipped. You skip most at <b className="font-mono text-text">{hh(most)}</b>.</span>
        <MonthBars months={s.map((v, i) => ({ month: String(i), value: v }))} fmt={pct} hot={most} label="Share of plays skipped by hour of day"
          tick={(i, on) => (on || i % 6 === 0 || i === 23 ? String(i).padStart(2, "0") : "")}
          valuetext={(i) => `${hh(i)}: ${pct(s[i])} of plays skipped`} caption={(i) => (i === null ? "" : `${hh(i)}: ${pct(s[i])} skipped.`)} />
      </>}
      <span className="text-muted text-xs">Only plays the export flags as skipped or not count here.</span>
    </Card>
  );
}

function Bails({ res }: { res: Res }) {
  const b = res.data.quick_bails;
  return (
    <Card title="Tracks you bail on" res={res} className={third}>
      <span className="text-text-2 text-sm">Skipped within 10 seconds, 3 times or more:</span>
      {!b.length ? <span className="text-muted">None in this period.</span> : (
        <ol className="in-ranks">{b.map((x) => (
          <li key={x.track + x.artist} className="in-rank grid-cols-[minmax(0,1fr)_auto]">
            <span className="flex flex-col min-w-0">
              {x.item_id ? <Link to={`/item/${x.item_id}`} className="in-stretch font-semibold text-text truncate">{x.track}</Link> : <span className="font-semibold truncate">{x.track}</span>}
              <span className="text-muted text-xs truncate">{x.artist}</span>
            </span>
            <span className="font-mono text-sm">{pl(x.times, "time")}</span>
          </li>))}</ol>)}
    </Card>
  );
}

function Shuffle({ res }: { res: Res }) {
  const s = res.data.shuffle, ys = s.by_year;
  const best = ys.length ? ys.reduce((p, x) => (x.pct > p.pct ? x : p), ys[0]) : null;
  return (
    <Card title="Shuffle" res={res} className={third}>
      {s.pct == null ? <span className="text-muted">No plays in this period say whether shuffle was on.</span> : (
        <span className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1"><span className="hm-big"><CountUp text={pct(s.pct)} /></span>
          <span className="text-text-2 text-sm">of plays on shuffle{best && Math.round(best.pct) !== Math.round(s.pct) ? `; your most was ${pct(best.pct)} in ${best.year}` : ""}.</span></span>)}
      {ys.length >= 2 && <MonthBars months={ys.map((x) => ({ month: String(x.year), value: x.pct }))} fmt={pct} label="Share of plays on shuffle by year"
        tick={(i, on) => (on || i === 0 || i === ys.length - 1 || ys.length <= 8 ? String(ys[i].year) : "")}
        valuetext={(i) => `${ys[i].year}: ${pct(ys[i].pct)} on shuffle`} caption={(i) => (i === null ? "" : `${ys[i].year}: ${pct(ys[i].pct)} on shuffle.`)} />}
      <span className="text-muted text-xs">Export plays, and plays on this PC since the logger began recording shuffle.</span>
    </Card>
  );
}

function Devices({ res }: { res: Res }) {
  const { devices, offline } = res.data;
  const top = devices.reduce((p, x, i) => (x.pct > devices[p].pct ? i : p), 0);
  return (
    <Card title="Where you listen" res={res} className={third}>
      {!devices.length ? <span className="text-muted">No plays in this period.</span> : devices.map((x, i) => (
        <div key={x.kind} className="in3-tag">
          <span className="text-sm">{DEVICE[x.kind]}</span><Grow pct={x.pct} i={i} hot={i === top} /><span className="font-mono text-xs text-right">{pct(x.pct)}</span>
        </div>))}
      {offline.pct > 0 && <span className="text-text-2 text-sm">Offline: <b className="font-mono text-text">{pct(offline.pct)}</b> of export plays{offline.peak_month ? `, mostly in ${monthLong(offline.peak_month)}` : ""}.</span>}
      <span className="text-muted text-xs">The export names each device in its own words; Runout groups them into Phone, Other computers, Web player and Other. Plays logged here count as This PC.</span>
    </Card>
  );
}

function Finished({ res }: { res: Res }) {
  const f = res.data.finished;
  return (
    <Card title="Finished tracks" res={res} className={third}>
      {!res.data.export_plays ? <span className="text-muted">{NO_EXPORT}</span> : (
        <span className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1"><span className="hm-big"><CountUp text={pct(f.pct)} /></span>
          <span className="text-text-2 text-sm">ended with the track finishing on its own</span></span>)}
      {f.how_far_pct != null ? <>
        <span className="text-text-2 text-sm">How far into each track you got, on average: <b className="font-mono text-text">{pct(f.how_far_pct)}</b></span>
        <span className="text-muted text-xs">Measured on {pct(f.how_far_coverage_pct)} of plays: only tracks whose length the logger saw on this PC.</span>
      </> : <span className="text-muted text-xs">How far into each track you got needs its length, which only plays on this PC record.</span>}
    </Card>
  );
}

function Sessions({ res, unit }: { res: Res; unit: string | null }) {
  const s = res.data.sessions;
  const most = s.spread.indexOf(Math.max(...s.spread));
  const fig = (k: string, v: string, sub?: string) => <span className="flex flex-col gap-0.5 min-w-0"><span className="text-muted text-xs">{k}</span><span className="hm-big"><CountUp text={v} /></span>{sub && <span className="text-text-2 text-xs">{sub}</span>}</span>;
  return (
    <Card title="Sessions" res={res} className="flex-[1_1_440px] min-w-0">
      <div className="grid grid-cols-3 gap-4">
        {fig(unit ? `This ${unit}` : "In all", num(s.count))}
        {fig("Typical length", hm(s.typical_ms))}
        {s.longest ? fig("Longest", hm(s.longest.ms), `${wday(s.longest.start_local)} from ${time(s.longest.start_local)}`) : fig("Longest", "None")}
      </div>
      {s.spread.length === 10 && s.count > 0 && <MonthBars months={s.spread.map((v, i) => ({ month: TICKS[i], value: v }))} fmt={num} hot={most}
        label="Sessions by length" tick={(i, on) => (on || i % 3 === 0 || i === 9 ? TICKS[i] : "")}
        valuetext={(i) => `${BUCKETS[i]}: ${pl(s.spread[i], "session")}`} caption={(i) => (i === null ? "" : `${BUCKETS[i]}: ${pl(s.spread[i], "session")}.`)} />}
      <span className="text-muted text-xs">An estimate. A session ends after 30 minutes of silence. Start times are estimated as end time minus time played, so pauses stretch them a little.</span>
    </Card>
  );
}

function Dives({ res }: { res: Res }) {
  const { deep_dives: dv, start_to_finish: sf } = res.data;
  return (
    <Card title="Deep dives" res={res} className="flex-[1_1_440px] min-w-0">
      <span className="text-text-2 text-sm">Long stretches with one artist or album, and tracks on repeat:</span>
      {!dv.length ? <span className="text-muted">None in this period.</span> : (
        <ol className="in-ranks">{dv.map((x) => (
          <li key={x.kind + x.name + x.start_local} className="in-rank grid-cols-[minmax(0,1fr)_auto]">
            <span className="flex flex-col min-w-0"><span className="font-semibold truncate">{diveTitle(x)}</span><span className="text-muted text-xs truncate">{x.by ? `${x.by}, ` : ""}{diveWhen(x)}</span></span>
            <span className="font-mono text-sm">{hm(x.ms)}</span>
          </li>))}</ol>)}
      {sf.length > 0 && <>
        <h3 className="text-sm font-semibold m-0 pt-1">Albums played start to finish</h3>
        <ol className="in-ranks">{sf.map((x) => (
          <li key={x.item_id} className="in-rank grid-cols-[minmax(0,1fr)_auto]">
            <span className="flex flex-col min-w-0"><Link to={`/item/${x.item_id}`} className="in-stretch font-semibold text-text truncate">{x.name}</Link><span className="text-muted text-xs truncate">{x.by}</span></span>
            <span className="font-mono text-sm">{pl(x.times, "time")}</span>
          </li>))}</ol>
      </>}
      <span className="text-muted text-xs">An estimate: at least an hour, 80% one artist or album, within one session. Start to finish counts only albums whose track list you’ve added.</span>
    </Card>
  );
}

const Skel = () => (
  <div className="flex flex-col gap-6" aria-busy="true" aria-label="Loading">
    <div className="card h-[64px]"><div className="skel h-full" /></div>
    <div className="flex flex-wrap gap-7">{Array.from({ length: 3 }, (_, i) => <div key={i} className={`card h-[260px] ${third}`}><div className="skel h-full" /></div>)}</div>
  </div>
);

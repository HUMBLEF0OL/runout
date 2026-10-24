// SC-09 Insights (CAP-23–27). Built only from your records, the desktop log and the Spotify export:
// never Spotify Top items or API recent plays (FR-031). Every card carries its source tag (FR-032).
// M9 (24 Oct 2026, user: "refactor insights using taste + motion"): charts you can point at (month bars shared with Home,
// a heatmap with a crosshair readout), bars that grow in, rings for shares; sizes fixed by the data, not history length.
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { m, useReducedMotion } from "motion/react";
import * as api from "../api";
import type { Freshness, Range, RankRow as Rank, Sourced } from "../api";
import { CountUp, Empty, ErrorBanner, FreshnessNote, InsightCard as Card, Seg, SkelBlock, SkelRows, SourcedTag, spot, Stat, TabPanel, useSlide } from "../components/ui";
import { Topbar } from "../components/Shell";
import { Grow, MonthBars, monthLong, SPRING, Versus } from "../components/charts";
import { useApi, type ApiState } from "../hooks/useApi";
import { day, daysSince, hmBig, hrs, nameOf, num, pl, STATUS, today } from "../lib/format";
import { EASE } from "../lib/motion";

type Tab = "listening" | "collection" | "take" | "year";
const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const DAYS_LONG = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const hh = (h: number) => `${String(h).padStart(2, "0")}:00`;
/** "1,369 more than the range before" / "Same as the range before". */
const delta = (now: number, prev: number, fmt: (n: number) => string, more: string, less: string) =>
  now === prev ? "Same as the range before" : `${fmt(Math.abs(now - prev))} ${now > prev ? more : less} than the range before`;

/** A share as a ring that fills once (skip rate, phone share). */
function Ring({ share }: { share: number }) {
  const reduce = useReducedMotion();
  return (
    <svg viewBox="0 0 64 64" className="in-ring" aria-hidden><circle cx="32" cy="32" r="26" className="track" />
      <m.circle cx="32" cy="32" r="26" className="fill" initial={reduce ? false : { pathLength: 0 }} animate={{ pathLength: Math.min(1, Math.max(0.005, share)) }} transition={{ duration: 0.9, ease: EASE, delay: 0.2 }} /></svg>
  );
}
function RingStat({ k, v, share, d }: { k: string; v: string; share: number; d: string }) {
  return <div className="card stat in-ringstat"><span className="flex flex-col gap-1.5 min-w-0"><span className="k">{k}</span><span className="v"><CountUp text={v} /></span><span className="d">{d}</span></span><Ring share={share} /></div>;
}

/** Ranked rows: number, name (whole row opens the record), a bar that grows in, hours. */
function TopList({ title, rows, kind, src }: { title: string; rows: Rank[]; kind: string; src: Sourced<unknown> }) {
  const max = Math.max(1e-9, ...rows.map((r) => r.hours));
  return (
    <Card title={title} res={src} className="flex-[1_1_280px] min-w-0">
      {rows.length ? <ol className="in-ranks">{rows.slice(0, 5).map((x, n) => (
        <li key={x.name + x.by} className="in-rank">
          <span className="font-mono text-faint text-xs">{n + 1}</span>
          <span className="min-w-0 flex flex-col gap-1">
            <span className="truncate" title={kind === "artist" ? x.name : `${x.name}, ${x.by}`}>
              {x.item_id ? <Link to={`/item/${x.item_id}`} className="in-stretch text-text font-semibold">{x.name}</Link> : <b>{x.name}</b>}
              {x.by && kind !== "artist" && <span className="text-muted">, {x.by}</span>}</span>
            <Grow pct={(x.hours / max) * 100} i={n} hot={!x.item_id} />
          </span>
          <span className="flex flex-col items-end gap-0.5"><span className="font-mono text-text-2 whitespace-nowrap text-xs">{hrs(x.hours)}</span>
            <span className="text-muted text-xs whitespace-nowrap">{x.status ? STATUS[x.status] : "Not saved"}</span></span>
        </li>))}</ol> : <span className="text-muted">No plays in this range.</span>}
    </Card>
  );
}

/** Weekday × hour, darker = less. Point at a cell: its row and column light up and the readout names it. */
function Heatmap({ rows }: { rows: number[][] }) {
  const reduce = useReducedMotion();
  const [at, setAt] = useState<[number, number] | null>(null);
  let best: [number, number] = [0, 0];
  rows.forEach((r, d) => r.forEach((x, h) => { if (x > rows[best[0]][best[1]]) best = [d, h]; }));
  const pick = (e: React.PointerEvent) => {
    const c = (e.target as HTMLElement).closest<HTMLElement>("[data-c]")?.dataset.c;
    if (!c) return; // the 3 px gaps between cells keep the last cell; leaving the map resets it
    const v = c.split(",").map(Number) as [number, number];
    if (v[0] !== at?.[0] || v[1] !== at?.[1]) setAt(v);
  };
  const [d, h] = at ?? best;
  const x = rows[d]?.[h] ?? 0;
  return (
    <>
      <span className="text-sm min-h-[20px]" aria-hidden>
        <b>{DAYS_LONG[d]}s at {hh(h)}</b>
        <span className="text-muted">{at === null || (d === best[0] && h === best[1]) ? ", your busiest hour" : x < 0.05 ? ", almost never" : `, ${Math.round(x * 100)}% of your busiest hour`}</span>
      </span>
      <m.div className="heat in-heat" role="img" aria-label={`Listening by weekday and hour; busiest on ${DAYS_LONG[best[0]]}s at ${hh(best[1])}`}
        onPointerMove={pick} onPointerLeave={() => setAt(null)}
        initial={reduce ? false : { clipPath: "inset(0 100% 0 0)" }} animate={{ clipPath: "inset(0 0% 0 0)" }} transition={{ duration: 0.8, ease: EASE }}>
        {rows.map((row, di) => (
          <div key={di} className={`r${at && at[0] === di ? " on" : ""}`}><span className="font-mono text-faint text-[11px]">{DAYS[di]}</span>
            {row.map((v, hi) => <div key={hi} data-c={`${di},${hi}`} className={`c${at && at[1] === hi ? " col" : ""}${at && at[0] === di && at[1] === hi ? " here" : ""}`} style={{ opacity: Math.max(0.05, v) }} />)}</div>
        ))}
        <div className="r"><span />{Array.from({ length: 24 }, (_, hi) => <span key={hi} className={`font-mono text-[10px] ${at && at[1] === hi ? "text-text" : "text-faint"}`}>{hi % 6 === 0 || (at && at[1] === hi) ? String(hi).padStart(2, "0") : ""}</span>)}</div>
      </m.div>
    </>
  );
}

function Listening({ range }: { range: Range }) {
  const li = useApi(() => api.getListeningInsights(range), [range]);
  const take = useApi(api.getTakeInsights);
  if (li.loading) return <Loading />;
  if (li.error) return <ErrorBanner error={li.error} title="Couldn't load insights." onRetry={li.reload} />;
  const d = li.data!.data; const t = d.totals; const src = li.data!;
  const months = d.monthly.map((x) => ({ month: x.month, value: x.hours }));
  const sum = months.reduce((a, x) => a + x.value, 0);
  const peak = months.reduce((p, x, i) => (x.value > months[p].value ? i : p), 0);
  const vs = range !== "all" && t.plays_prev > 0; // nothing before All time
  return <>
    <div className="flex flex-wrap gap-7 items-stretch">
      <Card title="Listening time by month" res={src} className="flex-[1.4_1_480px] min-w-0">
        <span className="flex flex-wrap items-baseline gap-x-2.5"><span className="hm-big">{hrs(sum)}</span><span className="text-muted text-sm">in the last 12 months, busiest in {monthLong(months[peak].month)}</span></span>
        <MonthBars months={months} fmt={hrs} label="Listening hours per month for the last 12 months" avg
          caption={(i) => i === null ? `Average ${hrs(sum / months.length)} a month, the faint line.` : `${monthLong(months[i].month)}${i === months.length - 1 ? " so far" : ""}${i === peak ? ", your busiest month" : ""}.`} />
      </Card>
      <section className="flex-[1_1_320px] min-w-0 grid grid-cols-[repeat(2,minmax(0,1fr))] auto-rows-fr gap-3" aria-label="This range in numbers">
        <Stat k="Plays" v={num(t.plays)} d={vs ? <Versus now={t.plays} prev={t.plays_prev}>{delta(t.plays, t.plays_prev, num, "more", "fewer")}</Versus> : "Played 30 seconds or more"} />
        <Stat k="Listening time" v={hmBig(t.ms)} d={vs ? <Versus now={t.ms} prev={t.ms_prev}>{delta(t.ms, t.ms_prev, hmBig, "more", "less")}</Versus>
          : t.ms >= 172_800_000 ? `About ${pl(Math.round(t.ms / 86_400_000), "day")} of music` : "In this range"} />
        <RingStat k="Skip rate" v={`${d.skip_rate}%`} share={d.skip_rate / 100} d="Of all plays" />
        <RingStat k="From your phone" v={`${d.phone_share}%`} share={d.phone_share / 100} d="Of plays, from your export" />
      </section>
    </div>
    <div className="flex flex-wrap gap-7 items-start">
      <Card title="When you listen" res={src} className="flex-[1.4_1_480px] min-w-0"><Heatmap rows={d.heatmap} /></Card>
      <Card title="Unfinished albums" res={take.data} side>
        <span className="text-muted text-xs leading-[1.45]">Only albums whose tracks you've added.{take.data?.data.unfinished_unknown ? ` ${pl(take.data.data.unfinished_unknown, "other album")} can't be checked.` : ""}</span>
        {take.loading ? <SkelRows n={3} /> : take.error ? <ErrorBanner error={take.error} title="Couldn't load unfinished albums." onRetry={take.reload} />
          : take.data?.data.unfinished_albums.length ? take.data.data.unfinished_albums.map((u, i) => (
          <Link key={u.item.id} to={`/item/${u.item.id}`} className="lrow flex flex-col gap-1.5 text-text py-1.5">
            <span className="flex justify-between items-baseline gap-3"><span className="font-semibold truncate">{nameOf(u.item)}</span><span className="font-mono text-text-2 whitespace-nowrap text-xs">{u.played} of {u.total} played</span></span>
            <Grow pct={(u.played / Math.max(1, u.total)) * 100} i={i} />
          </Link>)) : <span className="text-muted">Nothing here.</span>}
      </Card>
    </div>
    <div className="flex flex-wrap gap-7 items-start">
      <TopList title="Top artists" rows={d.top_artists} kind="artist" src={src} />
      <TopList title="Top albums" rows={d.top_albums} kind="album" src={src} />
      <TopList title="Top tracks" rows={d.top_tracks} kind="track" src={src} />
    </div>
  </>;
}

function Collection() {
  const ci = useApi(() => api.getCollectionInsights("12m"));
  const counts = useApi(api.getCounts);
  if (ci.loading) return <Loading />;
  if (ci.error) return <ErrorBanner error={ci.error} title="Couldn't load insights." onRetry={ci.reload} />;
  const d = ci.data!.data; const c = counts.data; const src = ci.data!;
  const st = (s: api.Status) => d.completion.find((x) => x.status === s)?.count ?? 0;
  // Library size at the end of each month, walking back from today's total.
  const total = c?.library ?? 0; let after = 0;
  const growth = [...d.backlog_growth].reverse().map((x) => { const v = total - after; after += x.added; return v; }).reverse();
  const dmax = Math.max(1, ...d.rating_spread.map((r) => r.count));
  const done = d.completion.reduce((a, x) => a + x.count, 0);
  const months = d.backlog_growth.map((x, i) => ({ month: x.month, value: growth[i] }));
  return <>
    <div className="stats">
      <Stat k="Records" v={num(total)} d={c ? `${pl(c.albums, "album")}, ${pl(c.tracks, "track")}, ${pl(c.artists, "artist")}` : ""} />
      <RingStat k="Explored" v={num(st("explored"))} share={done ? st("explored") / done : 0} d={`${st("not_for_me")} not for me, ${st("abandoned")} abandoned, ${st("in_progress")} in progress, ${st("planned")} planned`} />
      <Stat k="Planned to explored" v={d.avg_days_to_explored == null ? "None yet" : pl(d.avg_days_to_explored, "day")} d="Average time" />
      <Stat k="Reviews written" v={num(d.reviews)} d={`${d.explored_without_review} explored without one`} act={d.explored_without_review > 0} />
    </div>
    <SourcedTag res={src} />
    <div className="flex flex-wrap gap-7 items-stretch">
      <Card title="Library size over 12 months" res={src} className="flex-[1.5_1_520px] min-w-0">
        <MonthBars months={months} fmt={(v) => pl(v, "record")} label={`Library size per month, from ${growth[0] ?? 0} to ${pl(growth[growth.length - 1] ?? 0, "record")}`}
          caption={(i) => { const k = i ?? months.length - 1; const add = d.backlog_growth[k]; return `${monthLong(add.month)}: ${pl(add.added, "added", "added")}, ${num(add.finished)} finished.`; }} />
      </Card>
      <Card title="How you rate" res={src} side>
        {d.rating_spread.filter((r) => r.rating >= 1).map((r, i) => (
          <div key={r.rating} className="in-spread">
            <span className="font-mono text-text-2 text-xs">{r.rating}</span>
            <Grow pct={(r.count / dmax) * 100} i={i} hot={r.count === dmax} />
            <span className="font-mono text-faint text-xs text-right">{r.count}</span>
          </div>
        ))}
      </Card>
    </div>
    <Card title="Your most used tags" res={src}>
      {d.tag_trends.length ? <div className="flex flex-wrap gap-2 items-center">{d.tag_trends.slice(0, 10).map((t, i) => (
        <m.span key={t.tag} className="chip" initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={{ ...SPRING, delay: i * 0.03 }}>{t.tag} <span className="font-mono text-faint">{t.count}</span></m.span>))}</div>
        : <span className="text-muted">No tags yet.</span>}
    </Card>
  </>;
}

function Take() {
  const ti = useApi(api.getTakeInsights);
  if (ti.loading) return <Loading />;
  if (ti.error) return <ErrorBanner error={ti.error} title="Couldn't load insights." onRetry={ti.reload} />;
  const d = ti.data!.data; const src = ti.data!;
  const groups: [string, api.Take[]][] = [["Loved, not played lately", d.loved_but_forgotten], ["Played a lot, never rated", d.rated_vs_played], ["In your backlog, never played", d.never_played_backlog]];
  return <>
    <p className="text-text-2 m-0">Where your opinions and your listening disagree.</p>
    <div className="flex flex-wrap gap-7 items-start">
      {groups.map(([title, rows], g) => (
        <Card key={title} title={title} res={src} className="flex-[1_1_300px] min-w-0">
          {rows.length ? rows.map((x, i) => (
            <m.div key={x.item.id} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.3, ease: EASE, delay: g * 0.06 + i * 0.04 }}>
              <Link to={`/item/${x.item.id}`} className="flex flex-col gap-0.5 px-2.5 py-2 rounded-control bg-surface-2 text-text transition hover:bg-surface-hover active:scale-[0.98]">
                <span className="font-semibold">{nameOf(x.item)}</span><span className="text-text-2 text-sm">{x.line}</span>
              </Link>
            </m.div>)) : <span className="text-muted">Nothing here.</span>}
        </Card>
      ))}
    </div>
  </>;
}

function Year() {
  const now = today().getFullYear();
  const [year, setYear] = useState(now);
  const yr = useApi(() => api.getYearReview(year), [year]);
  const y = yr.data?.data;
  const years: [string, string][] = [[String(now), String(now)], [String(now - 1), String(now - 1)]];
  return <>
    <div className="flex flex-wrap gap-3 items-center">
      <Seg radio label="Year" value={String(year)} onChange={(v) => setYear(Number(v))} options={years} />
      {year === now && <span className="text-muted text-sm">{now} so far, up to {day(today().toISOString())}</span>}
    </div>
    {yr.loading ? <SkelBlock h={280} /> : yr.error ? <ErrorBanner error={yr.error} title="Couldn't load your year." onRetry={yr.reload} />
      : !y?.plays ? <Empty h={`No listening in ${year}`} line="Import your Spotify history to see this year."><Link className="btn btn-primary" to="/import/1">Import history</Link></Empty>
      : (
        <section className="card year p-8 flex flex-col gap-[18px]" aria-label={`Your ${y.year} in Runout`}>
          <span className="text-muted">Your {y.year} in Runout</span>
          <h2 className="text-3xl font-extrabold tracking-[-0.03em]"><CountUp text={pl(y.hours, "hour")} />, <CountUp text={pl(y.plays, "play")} />.</h2>
          <div className="stats in-quad">
            <Stat txt k="Top artist" v={y.top_artist.name} d={pl(y.top_artist.plays, "play")} />
            <Stat txt k="Top album" v={y.top_album.name} d={y.top_album.by} />
            <Stat k="Explored" v={y.explored} d="Records you finished" />
            <Stat k="Reviews" v={y.reviews} d="Written this year" />
          </div>
          <span className="text-text-2">Your busiest month was {monthLong(y.busiest_month).split(" ")[0]}.</span>
          <SourcedTag res={yr.data!} />
        </section>
      )}
  </>;
}

const Loading = () => <div className="flex flex-wrap gap-7 items-start"><div className="card flex-[1.5_1_520px] min-w-0 flex flex-col gap-3.5 h-[260px]"><div className="skel h-full" /></div><div className="flex-[1_1_320px] min-w-0 flex flex-col gap-5"><SkelRows n={4} /></div></div>;

function StaleBanner({ f }: { f: Freshness | undefined }) {
  if (!f?.export_latest_event || daysSince(f.export_latest_event) <= 30) return null;
  return (
    <div className="banner warn">
      <span><b className="text-accent">Phone data is getting old.</b> <span className="text-text-2">Your latest export ends {day(f.export_latest_event)}. Phone listening after that is missing.</span></span>
      <Link className="btn btn-secondary s" to="/import/1">Import a newer export</Link>
    </div>
  );
}

export default function Insights() {
  const tab = (useParams().tab ?? "listening") as Tab;
  const nav = useNavigate();
  const [range, setRange] = useState<Range>("12m");
  // Probe for any listening data: empty → one sentence + one action for the whole page.
  const probe: ApiState<api.Sourced<api.ListeningInsights>> = useApi(() => api.getListeningInsights("all"));
  const fresh = useApi(api.getFreshness);
  const logger = useApi(api.getLoggerStatus);
  const empty = probe.data && probe.data.data.totals.plays === 0;
  const groups: [Tab, string][] = [["listening", "Listening"], ["collection", "Collection"], ["take", "Your take"], ["year", "Year in review"]];
  const [dir, slide] = useSlide(groups.map((g) => g[0]), tab);
  const tabs = <Seg label="Insight group" value={tab} onChange={(t) => { slide(t); setRanged(false); nav(`/insights/${t}`); }} options={groups} />;
  // Time range tabs slide the listening panel too, but only after a range pick (the group panel already slid in).
  const ranges: [Range, string][] = [["30d", "30 days"], ["12m", "12 months"], ["all", "All time"]];
  const [rdir, rslide] = useSlide(ranges.map((r) => r[0]), range);
  const [ranged, setRanged] = useState(false);
  return (
    <>
      <Topbar />
      <div className="page-head">
        <div className="flex flex-col gap-2.5"><h1 className="page-title">Insights</h1>{tabs}</div>
        {tab === "listening" && !empty && <Seg label="Time range" value={range} onChange={(r) => { rslide(r); setRanged(true); setRange(r); }} options={ranges} />}
      </div>
      {empty ? (
        <Empty h="Nothing to show yet" line="Insights need listening data. Import your Spotify history for all devices, or let the desktop logger collect plays on this PC.">
          <div className="flex flex-wrap gap-2 items-center"><Link className="btn btn-primary" to="/import/1">Import history</Link></div>
        </Empty>
      ) : <>
        <p className="freshness m-0">
          <FreshnessNote freshness={fresh.data} loggerState={logger.data?.state} />
          {probe.data ? ` ${pl(probe.data.data.totals.private_excluded, "private-session play")} excluded. Nothing here comes from the Spotify API.` : ""}
        </p>
        <StaleBanner f={fresh.data} />
        <TabPanel key={tab} dir={dir} label={groups.find((g) => g[0] === tab)?.[1]} className="lit flex flex-col gap-6" onPointerMove={spot}>
          {tab === "listening" && (
            <TabPanel key={range} dir={rdir} still={!ranged} label={ranges.find((r) => r[0] === range)?.[1]} className="flex flex-col gap-6">
              <Listening range={range} />
            </TabPanel>)}
          {tab === "collection" && <Collection />}
          {tab === "take" && <Take />}
          {tab === "year" && <Year />}
        </TabPanel>
      </>}
    </>
  );
}

// SC-09 Insights (CAP-23–27). Built only from your records, the desktop log and the Spotify export:
// never Spotify Top items or API recent plays (FR-031). Every card carries its source tag (FR-032).
// M9 (24 Oct 2026, user: "refactor insights using taste + motion"): charts you can point at (month bars shared with Home),
// bars that grow in, rings for shares; sizes fixed by the data, not history length.
// M10 (Insights v3, design/M10-insights-v3.md §5): line tabs Report to Your data, then the existing tabs; the period header
// (grain, ‹ period ›, compare with) on the listening tabs that take a period; deep links #/insights/<tab>?grain=&at=&vs=.
import { useCallback, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { m, useReducedMotion } from "motion/react";
import * as api from "../api";
import type { Freshness, PeriodInfo, PeriodQuery } from "../api";
import { CountUp, Empty, ErrorBanner, FreshnessNote, InsightCard as Card, Seg, SkelBlock, SkelRows, SourcedTag, spot, Stat, TabPanel, useSlide } from "../components/ui";
import { Topbar } from "../components/Shell";
import { Grow, MonthBars, monthLong, SPRING } from "../components/charts";
import { localDay, periodFromSearch, periodSearch } from "../components/logic";
import { useApi, type ApiState } from "../hooks/useApi";
import { day, daysSince, nameOf, num, pl, today } from "../lib/format";
import { PeriodHeader, Report } from "./InsightsReport";
import { Trends } from "./InsightsTrends";
import { Discovery } from "./InsightsDiscovery";
import { Habits } from "./InsightsHabits";
import { Records } from "./InsightsRecords";
import { Explore } from "./InsightsExplore";
import { YourData } from "./InsightsData";
import { EASE } from "../lib/motion";

type Tab = "report" | "trends" | "discovery" | "habits" | "records" | "explore" | "data" | "collection" | "take" | "year";
/** Line tabs in the design's order (§8): the listening tabs, then the existing ones. Genres & decades waits on U-11: no tab. */
const TABS: [Tab, string][] = [["report", "Report"], ["trends", "Trends"], ["discovery", "Discovery"], ["habits", "How you listen"], ["records", "Records"],
  ["explore", "Explore"], ["data", "Your data"], ["collection", "Collection"], ["take", "Your take"], ["year", "Year in review"]];
/** Tabs that read a period, so they show the header; Trends, Records and Explore cover your whole history or set their own. */
const PERIOD_TABS = new Set<Tab>(["report", "discovery", "habits"]);
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

function Collection() {
  const reduce = useReducedMotion();
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
          valuetext={(i) => `${monthLong(months[i].month)}: ${pl(months[i].value, "record")}, ${num(d.backlog_growth[i].added)} added, ${num(d.backlog_growth[i].finished)} finished`}
          caption={(i) => { const k = i ?? months.length - 1; const add = d.backlog_growth[k]; return `${monthLong(add.month)}: ${pl(add.added, "added", "added")}, ${num(add.finished)} finished.`; }} />
      </Card>
      <Card title="How you rate" res={src} side>
        {d.rating_spread.filter((r) => r.rating >= 1).map((r, i) => (
          <div key={r.rating} className="in-spread">
            <span className="font-mono text-text-2 text-xs">{r.rating}</span>
            <Grow pct={(r.count / dmax) * 100} i={i} />
            <span className="font-mono text-faint text-xs text-right">{r.count}</span>
          </div>
        ))}
      </Card>
    </div>
    <Card title="Your most used tags" res={src}>
      {d.tag_trends.length ? <div className="flex flex-wrap gap-2 items-center">{d.tag_trends.slice(0, 10).map((t, i) => (
        <m.span key={t.tag} className="chip" initial={reduce ? false : { opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={{ ...SPRING, delay: i * 0.03 }}>{t.tag} <span className="font-mono text-faint">{t.count}</span></m.span>))}</div>
        : <span className="text-muted">No tags yet.</span>}
    </Card>
  </>;
}

function Take() {
  const reduce = useReducedMotion();
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
            <m.div key={x.item.id} initial={reduce ? false : { opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.3, ease: EASE, delay: g * 0.06 + i * 0.04 }}>
              <Link to={`/item/${x.item.id}`} className="flex flex-col gap-0.5 px-2.5 py-2 rounded-control bg-surface-2 text-text transition hover:bg-surface-hover active:scale-[0.98]">
                <span className="font-semibold">{nameOf(x.item)}</span><span className="text-text-2 text-sm">{x.line}</span>
              </Link>
            </m.div>)) : <span className="text-muted">Nothing here.</span>}
        </Card>
      ))}
      <Card title="Unfinished albums" res={src} className="flex-[1_1_300px] min-w-0">
        <span className="text-muted text-xs leading-[1.45]">Only albums whose tracks you've added.{d.unfinished_unknown ? ` ${pl(d.unfinished_unknown, "other album")} can't be checked.` : ""}</span>
        {d.unfinished_albums.length ? d.unfinished_albums.map((u, i) => (
          <Link key={u.item.id} to={`/item/${u.item.id}`} className="lrow flex flex-col gap-1.5 text-text py-1.5">
            <span className="flex justify-between items-baseline gap-3"><span className="font-semibold truncate">{nameOf(u.item)}</span><span className="font-mono text-text-2 whitespace-nowrap text-xs">{u.played} of {u.total} played</span></span>
            <Grow pct={(u.played / Math.max(1, u.total)) * 100} i={i} />
          </Link>)) : <span className="text-muted">Nothing here.</span>}
      </Card>
    </div>
  </>;
}

function Year() {
  const now = today().getFullYear(), day0 = localDay(today());
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
    {y?.plays ? <Report key={year} q={{ grain: "year", vs: "prev", at: year === now ? null : String(year) }} today={day0} /> : null}
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
  const param = useParams().tab;
  const tab: Tab = TABS.some((t) => t[0] === param) ? (param as Tab) : "report"; // old #/insights/listening lands on Report
  const nav = useNavigate();
  const [sp] = useSearchParams();
  const day0 = localDay(today());
  const q = periodFromSearch(sp, day0);
  const search = periodSearch(q);
  // Probe for any listening data: empty → one sentence + one action for the whole page.
  const probe: ApiState<api.Sourced<api.ListeningInsights>> = useApi(() => api.getListeningInsights("all"));
  const fresh = useApi(api.getFreshness);
  const logger = useApi(api.getLoggerStatus);
  const empty = probe.data && probe.data.data.totals.plays === 0;
  const [dir, slide] = useSlide(TABS.map((g) => g[0]), tab);
  // A new period slides the tab's panel too (‹ from the left, › from the right), but only after a pick: the tab already slid in.
  const [pdir, setPdir] = useState<"from-left" | "from-right">("from-right");
  const [moved, setMoved] = useState(false);
  const setPeriod = (p: PeriodQuery, d: "from-left" | "from-right") => { setPdir(d); setMoved(true); nav(`/insights/${tab}?${periodSearch(p)}`, { replace: true }); };
  const label = TABS.find((g) => g[0] === tab)![1];
  // The loaded report's PeriodInfo bounds the ‹ › stepper; kept per link so a stale one never applies.
  const [info, setInfo] = useState<{ search: string; p: PeriodInfo }>();
  const onPeriod = useCallback((p: PeriodInfo) => setInfo({ search, p }), [search]);
  return (
    <>
      <Topbar />
      <div className="page-head">
        <h1 className="page-title">Insights</h1>
        {PERIOD_TABS.has(tab) && !empty && <PeriodHeader q={q} today={day0} bounds={tab === "report" && info?.search === search ? info.p : undefined} onChange={setPeriod} />}
      </div>
      <Seg label="Insight group" value={tab} onChange={(t) => { slide(t); setMoved(false); nav(`/insights/${t}?${search}`); }} options={TABS} />
      {empty ? (
        <Empty h="Nothing to show yet" line="Insights need listening data. Import your Spotify history for all devices, or let the desktop logger collect plays on this PC.">
          <div className="flex flex-wrap gap-2 items-center"><Link className="btn btn-primary" to="/import/1">Import history</Link></div>
        </Empty>
      ) : <>
        <p className="freshness m-0">
          <FreshnessNote freshness={fresh.data} loggerState={logger.data?.state} />
          {probe.data ? ` ${pl(probe.data.data.totals.private_excluded, "private-session play")} excluded. Nothing here comes from the Spotify API.` : ""}
        </p>
        {tab !== "report" && <StaleBanner f={fresh.data} />}
        <TabPanel key={tab} dir={dir} label={label} className="lit flex flex-col gap-6" onPointerMove={spot}>
          {tab === "report" && (
            <TabPanel key={search} dir={pdir} still={!moved} label={label} className="flex flex-col gap-6">
              <Report q={q} today={day0} onPeriod={onPeriod} />
            </TabPanel>)}
          {tab === "trends" && <Trends today={day0} />}
          {tab === "discovery" && <Discovery key={search} q={q} today={day0} />}
          {tab === "habits" && <Habits key={search} q={q} today={day0} />}
          {tab === "records" && <Records today={day0} />}
          {tab === "explore" && <Explore today={day0} />}
          {tab === "data" && <YourData today={day0} />}
          {tab === "collection" && <Collection />}
          {tab === "take" && <Take />}
          {tab === "year" && <Year />}
        </TabPanel>
      </>}
    </>
  );
}

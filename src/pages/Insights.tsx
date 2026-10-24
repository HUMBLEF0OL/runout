// SC-09 Insights (CAP-23–27). Built only from your records, the desktop log and the Spotify export:
// never Spotify Top items or API recent plays (FR-031). Every card carries its source tag (FR-032).
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import * as api from "../api";
import type { Freshness, Range, RankRow as Rank, Sourced } from "../api";
import { Empty, ErrorBanner, FreshnessNote, InsightCard as Card, Seg, SkelBlock, SkelRows, SourcedTag, Stat, useSlide } from "../components/ui";
import { Topbar } from "../components/Shell";
import { useApi, type ApiState } from "../hooks/useApi";
import { day, daysSince, hm, monthName, nameOf, num, pl, STATUS, today } from "../lib/format";

type Tab = "listening" | "collection" | "take" | "year";
const LONG = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const initial = (ym: string) => monthName(ym)[0];

/** CSS bars (prototype .bars); the last bar is the one that needs attention. */
function Bars({ values, labels, label }: { values: number[]; labels: string[]; label: string }) {
  const max = Math.max(1, ...values);
  return (
    <div className="bars" role="img" aria-label={label} style={{ gridTemplateColumns: `repeat(${values.length}, minmax(0,1fr))` }}>
      {values.map((v, i) => <div key={i} className={`b${i === values.length - 1 ? " hot" : ""}`}><div style={{ height: Math.round((v / max) * 170) }} title={num(v)} /><span>{labels[i]}</span></div>)}
    </div>
  );
}

function TopList({ title, rows, kind, src }: { title: string; rows: Rank[]; kind: string; src: Sourced<unknown> }) {
  return (
    <Card title={title} res={src} className="flex-[1_1_260px]">
      {rows.length ? rows.slice(0, 5).map((x, n) => (
        <div key={x.name + x.by} className="flex justify-between items-baseline gap-3 py-0.5">
          <span className="min-w-0"><span className="font-mono text-faint text-xs mr-2">{n + 1}</span>
            {x.item_id ? <Link to={`/item/${x.item_id}`} className="text-text">{x.name}</Link> : x.name}
            {x.by && kind !== "artist" && <span className="text-muted">, {x.by}</span>}
            {x.status && <span className="text-muted text-xs"> {STATUS[x.status]}</span>}</span>
          <span className="font-mono text-text-2 whitespace-nowrap text-xs">{x.hours.toFixed(1)} h</span>
        </div>
      )) : <span className="text-muted">No plays in this range.</span>}
    </Card>
  );
}

function Listening({ range }: { range: Range }) {
  const li = useApi(() => api.getListeningInsights(range), [range]);
  const take = useApi(api.getTakeInsights);
  const year = useApi(() => (range === "12m" ? Promise.resolve(null) : api.getListeningInsights("12m")), [range]); // 12m: reuse li
  const yearMs = range === "12m" ? li.data?.data.totals.ms : year.data?.data.totals.ms;
  if (li.loading) return <Loading />;
  if (li.error) return <ErrorBanner error={li.error} title="Couldn't load insights." onRetry={li.reload} />;
  const d = li.data!.data; const t = d.totals; const src = li.data!;
  return <>
    <div className="flex flex-wrap gap-7 items-start">
      <Card title="Listening time by month" res={src} className="flex-[1.4_1_480px]">
        <span className="font-mono text-text-2 text-sm">{yearMs != null ? `${hm(yearMs)} in the last 12 months` : ""}</span>
        <Bars values={d.monthly.map((m) => m.hours)} labels={d.monthly.map((m) => initial(m.month))} label="Listening hours per month for the last 12 months" />
      </Card>
      <section className="flex-[1_1_320px] min-w-0 grid grid-cols-[repeat(2,minmax(0,1fr))] gap-3">
        <Stat k="Plays" v={num(t.plays)} d="Played 30 seconds or more" />
        <Stat k="Listening time" v={hm(t.ms)} d="In this range" />
        <Stat k="Skip rate" v={`${d.skip_rate}%`} d="Of all plays" />
        <Stat k="From your phone" v={`${d.phone_share}%`} d="Of plays, from your export" />
        <span className="col-span-full"><SourcedTag res={src} /></span>
      </section>
    </div>
    <div className="flex flex-wrap gap-7 items-start">
      <Card title="When you listen" res={src} className="flex-[1.4_1_480px]">
        <div className="heat" role="img" aria-label="Listening by weekday and hour">
          {d.heatmap.map((row, i) => (
            <div key={i} className="r"><span className="font-mono text-faint text-[11px]">{["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][i]}</span>
              {row.map((x, h) => <div key={h} className="c" style={{ opacity: Math.max(0.05, x) }} />)}</div>
          ))}
          <div className="r"><span />{Array.from({ length: 24 }, (_, h) => <span key={h} className="font-mono text-faint text-[10px]">{h % 6 === 0 ? String(h).padStart(2, "0") : ""}</span>)}</div>
        </div>
      </Card>
      <Card title="Unfinished albums" res={take.data} side>
        <span className="text-muted text-xs leading-[1.45]">Only albums whose tracks you've added.{take.data?.data.unfinished_unknown ? ` ${pl(take.data.data.unfinished_unknown, "other album")} can't be checked.` : ""}</span>
        {take.loading ? <SkelRows n={3} /> : take.error ? <ErrorBanner error={take.error} title="Couldn't load unfinished albums." onRetry={take.reload} />
          : take.data?.data.unfinished_albums.length ? take.data.data.unfinished_albums.map((u) => (
          <Link key={u.item.id} to={`/item/${u.item.id}`} className="lrow flex justify-between items-baseline gap-3 text-text py-[3px]">
            <span className="font-semibold">{nameOf(u.item)}</span><span className="font-mono text-text-2 whitespace-nowrap text-xs">{u.played} of {u.total} played</span>
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
  const growth = [...d.backlog_growth].reverse().map((m) => { const v = total - after; after += m.added; return v; }).reverse();
  const dmax = Math.max(1, ...d.rating_spread.map((r) => r.count));
  return <>
    <div className="stats">
      <Stat k="Records" v={num(total)} d={c ? `${pl(c.albums, "album")}, ${pl(c.tracks, "track")}, ${pl(c.artists, "artist")}` : ""} />
      <Stat k="Explored" v={num(st("explored"))} d={`${st("not_for_me")} not for me, ${st("abandoned")} abandoned, ${st("in_progress")} in progress, ${st("planned")} planned`} />
      <Stat k="Planned to explored" v={d.avg_days_to_explored == null ? "None yet" : pl(d.avg_days_to_explored, "day")} d="Average time" />
      <Stat k="Reviews written" v={num(d.reviews)} d={`${d.explored_without_review} explored without one`} act={d.explored_without_review > 0} />
    </div>
    <SourcedTag res={src} />
    <div className="flex flex-wrap gap-7 items-start">
      <Card title="Library size over 12 months" res={src} className="flex-[1.5_1_520px]">
        <Bars values={growth} labels={d.backlog_growth.map((m) => initial(m.month))} label={`Library grows from ${growth[0] ?? 0} to ${pl(growth[growth.length - 1] ?? 0, "record")}`} />
      </Card>
      <Card title="How you rate" res={src} side>
        {d.rating_spread.filter((r) => r.rating >= 1).map((r) => (
          <div key={r.rating} className="grid grid-cols-[40px_minmax(0,1fr)_28px] gap-2.5 items-center">
            <span className="font-mono text-text-2 text-xs">{r.rating}</span>
            <div className="h-1.5 rounded-bar bg-bar" style={{ width: `${Math.round((r.count / dmax) * 100)}%` }} />
            <span className="font-mono text-faint text-xs text-right">{r.count}</span>
          </div>
        ))}
      </Card>
    </div>
    <Card title="Your most used tags" res={src}>
      {d.tag_trends.length ? <div className="flex flex-wrap gap-2 items-center">{d.tag_trends.slice(0, 8).map((t) => <span key={t.tag} className="chip">{t.tag} <span className="font-mono text-faint">{t.count}</span></span>)}</div>
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
      {groups.map(([title, rows]) => (
        <Card key={title} title={title} res={src} className="flex-[1_1_300px]">
          {rows.length ? rows.map((x) => (
            <Link key={x.item.id} to={`/item/${x.item.id}`} className="flex flex-col gap-0.5 px-2.5 py-2 rounded-control bg-surface-2 text-text transition hover:bg-surface-hover active:scale-[0.98]">
              <span className="font-semibold">{nameOf(x.item)}</span><span className="text-text-2 text-sm">{x.line}</span>
            </Link>)) : <span className="text-muted">Nothing here.</span>}
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
  return <>
    <div className="flex flex-wrap gap-2 items-center">
      <label className="label" htmlFor="yr">Year</label>
      <select id="yr" className="input surface" value={year} onChange={(e) => setYear(Number(e.target.value))}>
        {[now, now - 1].map((n) => <option key={n} value={n}>{n}</option>)}
      </select>
      {year === now && <span className="text-muted text-sm">{now} so far, up to {day(today().toISOString())}</span>}
    </div>
    {yr.loading ? <SkelBlock h={280} /> : yr.error ? <ErrorBanner error={yr.error} title="Couldn't load your year." onRetry={yr.reload} />
      : !y?.plays ? <Empty h={`No listening in ${year}`} line="Import your Spotify history to see this year." ><Link className="btn btn-secondary" to="/import/1">Import history</Link></Empty>
      : (
        <section className="card year p-8 flex flex-col gap-[18px]">
          <span className="text-muted">Your {y.year} in Runout</span>
          <h2 className="text-3xl font-extrabold tracking-[-0.03em]">{pl(y.hours, "hour")}, {pl(y.plays, "play")}.</h2>
          <div className="stats">
            <Stat txt k="Top artist" v={y.top_artist.name} d={pl(y.top_artist.plays, "play")} />
            <Stat txt k="Top album" v={y.top_album.name} d={y.top_album.by} />
            <Stat k="Explored" v={y.explored} d="Records you finished" />
            <Stat k="Reviews" v={y.reviews} d="Written this year" />
          </div>
          <span className="text-text-2">Your busiest month was {LONG[Number(y.busiest_month.slice(5, 7)) - 1]}.</span>
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
  const tabs = <Seg label="Insight group" value={tab} onChange={(t) => { slide(t); nav(`/insights/${t}`); }} options={groups} />;
  return (
    <>
      <Topbar />
      <div className="page-head">
        <div className="flex flex-col gap-2.5"><h1 className="page-title">Insights</h1>{tabs}</div>
        {tab === "listening" && !empty && <Seg label="Time range" value={range} onChange={setRange} options={[["30d", "30 days"], ["12m", "12 months"], ["all", "All time"]]} />}
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
        <div key={tab} role="tabpanel" aria-label={groups.find((g) => g[0] === tab)?.[1]} className={`tab-panel ${dir} flex flex-col gap-6`}>
          {tab === "listening" && <Listening range={range} />}
          {tab === "collection" && <Collection />}
          {tab === "take" && <Take />}
          {tab === "year" && <Year />}
        </div>
      </>}
    </>
  );
}

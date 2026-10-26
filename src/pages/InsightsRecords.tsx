// SC-09 Insights v3, Records tab (M10 WI 7), per docs/design/M10-insights-v3.md §4.2/§5 and mockup PNG 5. Your whole
// history: eight all-time records, today's date in earlier years (each opens Recent plays for that date) and forgotten
// favourites with "Play again", a link that opens Spotify like every other Open Spotify action. All-time figures count
// all-device dates only (§4.4), so the coverage note says where they stop. The milestone ETA is an estimate and says so.
import type { ReactNode } from "react";
import { Link } from "react-router-dom";
import { m, useReducedMotion } from "motion/react";
import * as api from "../api";
import type { Covered, Records as Data } from "../api";
import { Empty, ErrorBanner, InsightCard as Card, SourcedTag, Stat } from "../components/ui";
import { Grow, monthLong } from "../components/charts";
import { useApi } from "../hooks/useApi";
import { day, dayY, hm, hrs, monthName, num, openUrl, pl, time } from "../lib/format";
import { RISE } from "../lib/motion";
import { longDay } from "./InsightsTrends";

type Res = Covered<Data>;
type Otd = Data["on_this_day"][number];
const STEP = 5_000; // design §4.2: next multiple of 5,000 plays
const monthYear = (iso: string) => `${monthName(iso.slice(0, 7))} ${iso.slice(0, 4)}`;

/** Headline and detail for one earlier year of "on this day": a first play wins, then the day's top, then the time. */
export function onThisDayLine(x: Otd): [string, string] {
  if (!x.plays && !x.ms) return ["No music that day", "A day off"];
  const rest = `${pl(x.plays, "play")}, ${hm(x.ms)}`;
  const f = x.first_artists;
  if (f.length) return [`First play of ${f[0]}${f.length > 1 ? ` and ${pl(f.length - 1, "other")}` : ""}`, rest];
  if (x.top) return [`Mostly ${x.top.name}${x.top.by ? ` by ${x.top.by}` : ""}`, rest];
  return [`${hm(x.ms)} of music`, pl(x.plays, "play")];
}
/** How far (0–100) the play count is from the last multiple of 5,000 to `next`. */
export const milestoneSoFar = (total: number, next: number) => Math.max(0, Math.min(100, Math.round(((total - (next - STEP)) / STEP) * 100)));

export function Records({ today }: { today: string }) {
  const reduce = useReducedMotion();
  const r = useApi(api.getRecords);
  if (r.loading) return <Skel />;
  if (r.error) return <ErrorBanner error={r.error} title="Couldn’t load your records." onRetry={r.reload} />;
  const res = r.data!, d = res.data;
  if (!d.plays_total) return (
    <Empty h="No listening history yet" line="Import your Spotify history to see this.">
      <Link className="btn btn-primary" to="/import/1">Import history</Link>
    </Empty>);
  const c = res.coverage;
  return (
    <m.div className="flex flex-col gap-6" initial={reduce ? false : "hidden"} animate="show">
      <m.section variants={RISE} custom={0} className="flex flex-col gap-3" aria-labelledby="rec-h">
        <div className="flex flex-col gap-1">
          <h2 id="rec-h" className="card-title">All-time records</h2>
          <span className="text-text-2 text-sm">Across everything Runout has{d.since ? `: ${monthYear(d.since)} to today` : ""}.</span>
        </div>
        <Eight d={d} today={today} />
        <span className="text-muted text-xs">
          Records count only days every device covers{c.desktop_only_since ? `, so plays from ${dayY(c.desktop_only_since + "T00:00")} on join them with your next export` : ""}.
          {c.gaps.length > 0 ? ` ${pl(c.gaps.length, "stretch", "stretches")} with no data ${c.gaps.length === 1 ? "is" : "are"} skipped, never counted as silence or as a break in a streak.` : ""}
          {" "}Artists, albums and tracks are matched by name.
        </span>
        <SourcedTag res={res} />
      </m.section>
      <m.div variants={RISE} custom={1}><OnThisDay res={res} today={today} /></m.div>
      <m.div variants={RISE} custom={2}><Forgotten res={res} /></m.div>
    </m.div>
  );
}

/* ------------------------------------------------------------------ the eight all-time records */
/** A record card; a link when it has somewhere to go (the record's page, or Recent plays for a day). */
const Rec = ({ to, label, ...p }: { to?: string; label?: string } & Parameters<typeof Stat>[0]) =>
  to ? <Link to={to} aria-label={label} className="in3-rec"><Stat {...p} /></Link> : <Stat {...p} />;

function Eight({ d, today }: { d: Data; today: string }) {
  const t = d.top_track, s = d.longest_streak, by = d.biggest_year, bd = d.biggest_day, lo = d.most_loyal, ma = d.most_artists_month, nx = d.next_milestone;
  const yearDays = by ? (by.year === +today.slice(0, 4) ? (Date.parse(today + "T00:00Z") - Date.UTC(by.year, 0, 1)) / 86_400_000 + 1 : by.year % 4 ? 365 : 366) : 1;
  const others: ReactNode = (d.top_artist || d.top_album) && <span className="flex flex-col">
    {d.top_artist && <span>Artist: {d.top_artist.name}, {pl(d.top_artist.plays, "play")}</span>}
    {d.top_album && <span>Album: {d.top_album.name}, {pl(d.top_album.plays, "play")}</span>}
  </span>;
  return (
    <div className="in3-recs">
      <Stat k="First play in your history" txt v={d.first_play?.track ?? "None yet"}
        d={d.first_play && `${d.first_play.artist} · ${dayY(d.first_play.at_local)}, ${time(d.first_play.at_local)}`} />
      <Rec k="Most played track ever" txt v={t?.name ?? "None yet"} to={t?.item_id ? `/item/${t.item_id}` : undefined} label={t ? `Most played track ever: ${t.name}${t.by ? ` by ${t.by}` : ""}, ${pl(t.plays, "play")}` : undefined}
        d={t && <span className="flex flex-col gap-1"><span>{t.by ? `${t.by} · ` : ""}{pl(t.plays, "play")}, {hrs(t.hours)}</span>{others}</span>} />
      <Stat k="Longest streak" v={s ? pl(s.days, "day") : "None yet"} txt={!s} d={s && `${dayY(s.from + "T00:00")} to ${dayY(s.to + "T00:00")}`} />
      <Stat k="Biggest year" v={by ? `${by.year} · ${hrs(by.ms / 3_600_000)}` : "None yet"} txt={!by} d={by && `${hrs(by.ms / 3_600_000 / yearDays)} a day`} />
      <Rec k="Biggest day" v={bd ? hm(bd.ms) : "None yet"} txt={!bd} d={bd && longDay(bd.date)}
        to={bd ? `/plays/all?date=${bd.date}` : undefined} label={bd ? `Biggest day: ${longDay(bd.date)}, ${hm(bd.ms)}. Open its plays` : undefined} />
      <Rec k="Most loyal" txt v={lo?.name ?? "None yet"} to={lo?.item_id ? `/item/${lo.item_id}` : undefined} label={lo ? `Most loyal: ${lo.name}, in your top 20 in ${lo.years_in_top20} of ${lo.years} years` : undefined}
        d={lo && `In your top 20 in ${lo.years_in_top20} of ${pl(lo.years, "year")}`} />
      <Stat k="Next milestone" v={`${num(nx.plays)}th play`}
        d={<span className="flex flex-col gap-2">
          <span role="img" aria-label={`${num(d.plays_total)} plays so far, ${milestoneSoFar(d.plays_total, nx.plays)}% of the way`}><Grow pct={milestoneSoFar(d.plays_total, nx.plays)} hot /></span>
          <span>{nx.eta_days == null ? "Too few recent plays to guess when" : `In about ${pl(nx.eta_days, "day")} at your recent pace, an estimate`}</span>
        </span>} />
      <Stat k="Most artists in a month" v={ma ? num(ma.artists) : "None yet"} txt={!ma} d={ma && monthLong(ma.month)} />
    </div>
  );
}

/* ------------------------------------------------------------------ on this day, earlier years */
function OnThisDay({ res, today }: { res: Res; today: string }) {
  const ys = res.data.on_this_day, md = today.slice(4);
  return (
    <Card title="On this day" res={res}>
      <span className="text-text-2 text-sm">{day(today + "T00:00")} in earlier years. Choose one to see everything you played that day.</span>
      {!ys.length ? <span className="text-muted">No earlier {day(today + "T00:00")} with data yet.</span> : (
        <ul className="in3-otd">{ys.map((x) => {
          const [h, s] = onThisDayLine(x), date = `${x.year}${md}`;
          return <li key={x.year}><Link to={`/plays/all?date=${date}`} aria-label={`Plays on ${dayY(date + "T00:00")}: ${h}, ${s}`}>
            <span className="font-mono text-faint text-xs">{x.year}</span><b>{h}</b><span className="text-text-2 text-xs">{s}</span></Link></li>;
        })}</ul>)}
      <span className="text-muted text-xs">Only years every device covers on this date.</span>
    </Card>
  );
}

/* ------------------------------------------------------------------ forgotten favourites */
function Forgotten({ res }: { res: Res }) {
  const rows = res.data.forgotten;
  return (
    <Card title="Forgotten favourites" res={res}>
      <span className="text-text-2 text-sm">Once in your top 10 for a year; not played for 2 years or more.</span>
      {!rows.length ? <span className="text-muted">No forgotten favourites: every artist that made a year’s top 10 has played in the last 2 years.</span> : (
        <ul className="in3-favs">{rows.map((x) => (
          <li key={x.name} className="in3-fav">
            <span className="flex flex-col min-w-0">
              {x.item_id ? <Link to={`/item/${x.item_id}`} className="font-bold text-text truncate">{x.name}</Link> : <b className="truncate">{x.name}</b>}
              <span className="text-text-2 text-xs">top 10 in {x.top10_year} · last played {monthYear(x.last_played_local)}</span>
            </span>
            {x.play_uri && <a className="btn btn-secondary s" href={openUrl(x.play_uri)} target="_blank" rel="noreferrer" aria-label={`Play ${x.name} again in Spotify`}>Play again</a>}
          </li>))}</ul>)}
      <span className="text-muted text-xs">Play again opens their most played track in Spotify. Artists are matched by name.</span>
    </Card>
  );
}

const Skel = () => (
  <div className="flex flex-col gap-6" aria-busy="true" aria-label="Loading">
    <div className="in3-recs">{Array.from({ length: 8 }, (_, i) => <div key={i} className="card stat h-[120px]"><div className="skel h-full" /></div>)}</div>
    <div className="card h-[160px]"><div className="skel h-full" /></div>
  </div>
);

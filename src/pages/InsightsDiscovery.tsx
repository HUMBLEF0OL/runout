// SC-09 Insights v3, Discovery tab (M10 WI 7), per docs/design/M10-insights-v3.md §5 and mockup PNG 3. For the period in
// the page header: new artists (like for like, FR-048), the stick rate (an estimate), time on new artists, artists known;
// new artists per month split into still playing / dropped / too soon to tell; how you started tracks (export plays only);
// the discovery log. Sources and freshness from the response on every card (FR-032).
import { useState } from "react";
import { Link } from "react-router-dom";
import { m, useReducedMotion } from "motion/react";
import * as api from "../api";
import type { Covered, Discovery as Data, DiscoveryRow, PeriodQuery } from "../api";
import { Empty, ErrorBanner, InsightCard as Card, Stat } from "../components/ui";
import { monthLong, SPRING } from "../components/charts";
import { periodLabel, vsName } from "../components/logic";
import { useApi } from "../hooks/useApi";
import { dayY, monthName, num, pl, time } from "../lib/format";
import { RISE } from "../lib/motion";
import { CoverageNotes, Delta } from "./InsightsReport";

type Res = Covered<Data>;
type Month = Data["per_month"][number];
const unitOf = (q: PeriodQuery) => (q.grain === "all" ? null : q.grain === "custom" ? "stretch" : q.grain);
const START: [Data["starts"][number]["kind"], string][] = [["followed", "Followed on"], ["picked", "You picked it"], ["back", "Back button"], ["other", "Other"]];
const NOW: Record<DiscoveryRow["now"], string> = { in_rotation: "In rotation", dropped: "Dropped", back_again: "Back again" };
const total = (x: Month) => x.stuck + x.dropped + x.open;
/** What a screen reader hears for one month of the per-month chart. */
export const monthSays = (x: Month) => x.coverage === "no_data" ? `${monthLong(x.month)}: no data` : `${monthLong(x.month)}: ${pl(total(x), "new artist")}` +
  (total(x) ? `, ${num(x.stuck)} still playing, ${num(x.dropped)} dropped${x.open ? `, ${num(x.open)} too soon to tell` : ""}` : "");

export function Discovery({ q, today }: { q: PeriodQuery; today: string }) {
  const reduce = useReducedMotion();
  const api_ = useApi(() => api.getDiscovery(q), [q.grain, q.vs, q.at, q.from, q.to]);
  if (api_.loading) return <Skel />;
  if (api_.error) return <ErrorBanner error={api_.error} title="Couldn’t load your discoveries." onRetry={api_.reload} />;
  const res = api_.data!, d = res.data;
  const unit = unitOf(q), label = periodLabel(q, today);
  if (!d.artists_known) return <>
    <CoverageNotes res={res} q={q} />
    <Empty h="No listening history yet" line="Import your Spotify history to see this.">
      <Link className="btn btn-primary" to="/import/1">Import history</Link>
    </Empty>
  </>;
  const pc = !!res.coverage.desktop_only_since;
  const s = d.stick_rate;
  return (
    <m.div className="flex flex-col gap-6" initial={reduce ? false : "hidden"} animate="show">
      <m.div variants={RISE} custom={0} className="flex flex-col gap-3 empty:hidden"><CoverageNotes res={res} q={q} /></m.div>
      <m.section variants={RISE} custom={1} className="stats" aria-label={`Discovery in ${label}`}>
        <Stat k={unit ? `New artists this ${unit}` : "Artists discovered"} v={num(d.new_artists.value)}
          d={<span className="flex flex-col gap-2.5">{d.days_per_new != null && <span>One every {pl(d.days_per_new, "day")}</span>}
            <Delta c={d.new_artists} fmt={num} more="more" less="fewer" name={vsName(q, null)} pc={pc} rest="" /></span>} />
        <Stat k="Still playing them 3 months on" v={s.pct == null ? "Too soon" : `${Math.round(s.pct)}%`} txt={s.pct == null}
          d={`${s.best_pct != null && s.best_year != null ? `Your best: ${Math.round(s.best_pct)}% in ${s.best_year}. ` : ""}An estimate`} />
        <Stat k="Time on new artists" v={`${Math.round(d.new_time_pct)}%`} d={`Of ${unit ? `this ${unit}’s` : "your"} listening`} />
        <Stat k="Artists you know" v={num(d.artists_known)} d={d.known_since ? `Since ${d.known_since.slice(0, 4)}` : "In all your history"} />
      </m.section>
      <m.div variants={RISE} custom={2} className="flex flex-wrap gap-7 items-stretch">
        <PerMonth res={res} />
        <Starts res={res} />
      </m.div>
      <m.div variants={RISE} custom={3}><Log res={res} unit={unit} /></m.div>
    </m.div>
  );
}

/* ------------------------------------------------------------------ new artists per month, stacked; a slider like MonthBars */
function PerMonth({ res }: { res: Res }) {
  const reduce = useReducedMotion();
  const ms = res.data.per_month, n = ms.length;
  const [at, setAt] = useState<number | null>(null);
  if (!n) return null;
  const cur = Math.min(at ?? n - 1, n - 1);
  const max = Math.max(1, ...ms.map(total));
  const gap = ms.some((x) => x.coverage === "no_data");
  const keys = (e: React.KeyboardEvent) => {
    const step: Record<string, number> = { ArrowLeft: -1, ArrowDown: -1, ArrowRight: 1, ArrowUp: 1, PageDown: -12, PageUp: 12 };
    const k = e.key in step ? cur + step[e.key] : e.key === "Home" ? 0 : e.key === "End" ? n - 1 : null;
    if (k === null) return;
    e.preventDefault();
    setAt(Math.max(0, Math.min(n - 1, k)));
  };
  const dense = n > 48 ? " dense xs" : n > 16 ? " dense" : "";
  const cols = { gridTemplateColumns: `repeat(${n}, minmax(0, 1fr))` };
  const tick = (i: number) => {
    const ym = ms[i].month;
    if (n <= 24) return i === cur || n <= 12 || i % 3 === 0 ? monthName(ym) : "";
    return ym.endsWith("-01") && +ym.slice(0, 4) % (n > 72 ? 2 : 1) === 0 ? ym.slice(0, 4) : "";
  };
  return (
    <Card title="New artists per month" res={res} className="flex-[2_1_520px] min-w-0">
      <div className="hm-plot in3-stack" role="slider" tabIndex={0} aria-label="New artists per month, by whether you kept playing them"
        aria-valuemin={0} aria-valuemax={n - 1} aria-valuenow={cur} aria-valuetext={monthSays(ms[cur])} onKeyDown={keys}
        onPointerMove={(e) => { const r = e.currentTarget.getBoundingClientRect(); setAt(Math.max(0, Math.min(n - 1, Math.floor(((e.clientX - r.left) / r.width) * n)))); }}
        onPointerLeave={() => setAt(null)} onBlur={() => setAt(null)}>
        <div className={`hm-cols${dense}`} style={cols} aria-hidden>
          {ms.map((x, i) => (
            <m.span key={x.month} className={`${i === cur ? "on" : ""}${x.coverage === "no_data" ? " gap" : ""}` || undefined}
              initial={reduce ? false : { scaleY: 0 }} animate={{ scaleY: 1 }} transition={{ ...SPRING, delay: 0.1 + (i / n) * 0.6 }}>
              {x.stuck > 0 && <i className="kept" style={{ height: `${(x.stuck / max) * 100}%` }} />}
              {x.dropped > 0 && <i className="drop" style={{ height: `${(x.dropped / max) * 100}%` }} />}
              {x.open > 0 && <i className="open" style={{ height: `${(x.open / max) * 100}%` }} />}
            </m.span>))}
        </div>
      </div>
      <div className={`hm-months${dense}`} style={cols} aria-hidden>{ms.map((x, i) => <span key={x.month} className={i === cur ? "on" : undefined}>{tick(i)}</span>)}</div>
      <span className="text-sm min-h-[20px]" aria-hidden>{at == null ? "" : monthSays(ms[cur]) + "."}</span>
      <span className="in3-key"><span><i />Still playing 3 months later</span><span><i className="drop" />Tried, then dropped</span><span><i className="open" />Too soon to tell</span></span>
      <span className="text-muted text-xs">“Still playing” is an estimate: 3 or more plays in the 90 days that start a month after the first play. Artists are matched by name.{gap ? " Hatched months are gaps with no data, never silence." : ""}</span>
    </Card>
  );
}

/* ------------------------------------------------------------------ how you started tracks (export plays only) */
function Starts({ res }: { res: Res }) {
  const reduce = useReducedMotion();
  const st = res.data.starts;
  const pct = (k: string) => Math.round(st.find((x) => x.kind === k)?.pct ?? 0);
  return (
    <Card title="How you started tracks" res={res} side>
      {!st.length ? <span className="text-muted">No Spotify export plays in this period. Plays logged on this PC don’t say how a track started.</span> : <>
        <div className="in3-split" role="img" aria-label={START.map(([k, l]) => `${l} ${pct(k)}%`).join(", ")}>
          {START.map(([k], i) => pct(k) > 0 && <m.i key={k} className={k} style={{ width: `${pct(k)}%`, originX: 0 }}
            initial={reduce ? false : { scaleX: 0 }} animate={{ scaleX: 1 }} transition={{ ...SPRING, delay: 0.2 + i * 0.06 }} />)}
        </div>
        <span className="in3-key" aria-hidden>{START.map(([k, l]) => <span key={k}><i className={k} />{l} <b className="font-mono text-text">{pct(k)}%</b></span>)}</span>
        <span className="text-text-2 text-sm">You picked {pct("picked")}% of tracks yourself; {pct("followed")}% followed on from the one before.</span>
      </>}
      <span className="text-muted text-xs">Spotify export plays only. The export says how a track started (you clicked it, or it followed on), not where from (a playlist, radio or search).</span>
    </Card>
  );
}

/* ------------------------------------------------------------------ the discovery log */
function Log({ res, unit }: { res: Res; unit: string | null }) {
  const rows = res.data.log, all = Math.max(rows.length, res.data.new_artists.value);
  return (
    <Card title="Your discovery log" res={res}>
      <span className="text-text-2 text-sm">{all > rows.length ? `Your latest ${num(rows.length)} of ${pl(all, "new artist")}` : "Artists you first played"} {unit ? `this ${unit}` : "in your history"}, newest first.</span>
      {!rows.length ? <span className="text-muted">No new artists in this period.</span> : (
        <div className="overflow-x-auto"><table className="in3-log">
          <thead><tr><th scope="col">Artist</th><th scope="col">First played</th><th scope="col">With</th><th scope="col" className="num">Plays since</th><th scope="col">Now</th></tr></thead>
          <tbody>{rows.map((x) => (
            <tr key={x.artist}>
              <th scope="row">{x.item_id ? <Link to={`/item/${x.item_id}`} className="text-text">{x.artist}</Link> : x.artist}</th>
              <td className="font-mono">{dayY(x.first_played_local)}, {time(x.first_played_local)}</td>
              <td>{x.first_track}</td>
              <td className="num font-mono">{num(x.plays_since)}</td>
              <td>{x.now === "dropped" ? <span className="text-muted">{NOW[x.now]}</span> : <span className="badge status" title={x.now === "back_again" ? "An estimate" : undefined}>{NOW[x.now]}</span>}</td>
            </tr>))}</tbody>
        </table></div>)}
      <span className="text-muted text-xs">Now: back again is an estimate, after a long quiet spell, dropped after 90 days without a play, otherwise in rotation. Artists are matched by name.</span>
    </Card>
  );
}

const Skel = () => (
  <div className="flex flex-col gap-6" aria-busy="true" aria-label="Loading">
    <div className="stats">{Array.from({ length: 4 }, (_, i) => <div key={i} className="card stat h-[132px]"><div className="skel h-full" /></div>)}</div>
    <div className="card h-[280px]"><div className="skel h-full" /></div>
  </div>
);

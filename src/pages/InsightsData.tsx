// SC-09 Insights v3, Your data tab (M10 WI 8), per docs/design/M10-insights-v3.md §4.4/§5 and mockup PNG 8: what Runout
// knows per source (a coverage strip with its no-data gaps), the import history, "Import a newer export" (the existing
// 3-step import), and Keep it fresh: the monthly export reminder (off by default, a local note on app open, never network)
// and the private-session switch, moved here from Settings (same setting: set_logger include_private); the music database
// switch (M10 WI 9, set_music_db: off stops lookups and deletes every looked-up label).
import { Link } from "react-router-dom";
import { m, useReducedMotion } from "motion/react";
import * as api from "../api";
import type { Covered, DataCoverage } from "../api";
import { Empty, ErrorBanner, InsightCard as Card, SourcedTag, Switch, useToast } from "../components/ui";
import { useApi } from "../hooks/useApi";
import { dayY, monthName, num, pl } from "../lib/format";
import { EASE, RISE } from "../lib/motion";

const DAY = 86_400_000;
const t0 = (d: string) => Date.parse(d.slice(0, 10) + "T00:00Z");
const long = (d: string) => dayY(d.slice(0, 10) + "T00:00");

export function YourData({ today }: { today: string }) {
  const reduce = useReducedMotion();
  const cov = useApi(api.getDataCoverage);
  if (cov.loading) return <Skel />;
  if (cov.error) return <ErrorBanner error={cov.error} title="Couldn’t load what Runout knows." onRetry={cov.reload} />;
  const res = cov.data!, d = res.data;
  if (!d.export && !d.desktop) return (
    <Empty h="No listening history yet" line="Import your Spotify history to see this, or let the desktop logger collect plays on this PC.">
      <Link className="btn btn-primary" to="/import/1">Import history</Link>
    </Empty>);
  return (
    <m.div className="flex flex-col gap-6" initial={reduce ? false : "hidden"} animate="show">
      <m.section variants={RISE} custom={0} className="card flex flex-col gap-4" aria-labelledby="yd-h">
        <div className="flex flex-wrap gap-3 justify-between items-start">
          <div className="flex flex-col gap-0.5">
            <h2 id="yd-h" className="card-title">What Runout knows</h2>
            <span className="text-text-2 text-sm">Where every insight comes from, and the holes.</span>
          </div>
          <Link className="btn btn-light" to="/import/1">Import a newer export</Link>
        </div>
        <Strip res={res} today={today} />
        <span className="text-muted text-xs">
          {d.logger_runs_since
            ? `Since ${long(d.logger_runs_since)} the logger records when it runs, so a day it ran without plays means no listening on this PC. `
            : "A day without desktop plays and no export is shown as no data, never as silence. "}
          A newer export adds the plays it covers; desktop plays it also covers are matched and counted once.
        </span>
        <SourcedTag res={res} />
      </m.section>
      <div className="flex flex-wrap gap-7 items-start">
        <m.div variants={RISE} custom={1} className="flex-[1.3_1_440px] min-w-0"><Imports res={res} /></m.div>
        <m.div variants={RISE} custom={2} className="flex-[1_1_340px] min-w-0"><Fresh d={d} /></m.div>
      </div>
    </m.div>
  );
}

/* ------------------------------------------------------------------ coverage strip */
function Strip({ res, today }: { res: Covered<DataCoverage>; today: string }) {
  const reduce = useReducedMotion();
  const d = res.data, gaps = res.coverage.gaps;
  const first = [d.export?.from, d.desktop?.from].filter(Boolean).map((x) => x!.slice(0, 10)).sort()[0];
  const a = t0(first), span = Math.max(DAY, t0(today) + DAY - a);
  const pos = (from: string, to: string) => ({ left: `${((t0(from) - a) / span) * 100}%`, width: `${Math.max(0.4, ((t0(to) + DAY - t0(from)) / span) * 100)}%` });
  const y0 = +first.slice(0, 4), y1 = +today.slice(0, 4), step = Math.max(1, Math.ceil((y1 - y0) / 6));
  const at = (y: number) => ((Date.UTC(y, 0, 1) - a) / span) * 100;
  const years = Array.from({ length: y1 - y0 }, (_, i) => y0 + 1 + i).filter((y) => (y - y0) % step === 0 && at(y) > 10 && at(y) < 90); // clear of the end labels
  const gapText = gaps.map((g) => `${long(g.from)} to ${long(g.to)}`).join(", ");
  const aria = [d.export && `Spotify export from ${long(d.export.from)} to ${long(d.export.to)}`, d.desktop && `desktop log from ${long(d.desktop.from)} to ${long(d.desktop.to)}`,
    gaps.length && `no data for ${gapText}`].filter(Boolean).join("; ");
  const grow = (i: number) => (reduce ? {} : { initial: { scaleX: 0 }, animate: { scaleX: 1 }, transition: { duration: 0.7, ease: EASE, delay: 0.15 + i * 0.1 } });
  return <>
    <div className="in3-cov" role="img" aria-label={`Coverage: ${aria}.`}>
      {d.export && <m.i className="exp" style={{ ...pos(d.export.from, d.export.to), originX: 0 }} {...grow(0)} />}
      {d.desktop && <m.i className="pc" style={{ ...pos(d.desktop.from, d.desktop.to), originX: 0 }} {...grow(1)} />}
      {gaps.map((g) => <i key={g.from} className="gap" style={pos(g.from, g.to)} />)}
    </div>
    <div className="in3-xticks" aria-hidden>
      <span className="first">{monthName(first)} {first.slice(0, 4)}</span>
      {years.map((y) => { const p = at(y); return <span key={y} style={{ left: `${p}%`, transform: `translateX(-${p}%)` }}>{y}</span>; })}
      <span className="last">today</span>
    </div>
    <ul className="in3-covkey">
      {d.export && <li><i className="exp" /><span><b>Spotify export</b>: {long(d.export.from)} to {long(d.export.to)} · {pl(d.export.plays, "play")} · all devices</span></li>}
      {d.desktop && <li><i className="pc" /><span><b>Desktop log</b>: since {long(d.desktop.from)} · {pl(d.desktop.plays, "play")}{d.desktop.matched ? ` · ${num(d.desktop.matched)} matched to export plays and counted once` : ""}</span></li>}
      {gaps.length > 0 && <li><i className="gap" /><span><b>{gaps.length === 1 ? "Gap" : "Gaps"}</b>: {gapText}, no data from any source</span></li>}
    </ul>
  </>;
}

/* ------------------------------------------------------------------ imports */
function Imports({ res }: { res: Covered<DataCoverage> }) {
  const rows = [...res.data.imports].sort((x, y) => y.imported_at.localeCompare(x.imported_at));
  return (
    <Card title="Imports" res={res}>
      {!rows.length ? <span className="text-muted">No export imported yet. Plays so far come from the desktop logger on this PC.</span> : (
        <div className="overflow-x-auto"><table className="in3-log in3-imports">
          <thead><tr><th scope="col">Imported</th><th scope="col">Covers</th><th scope="col" className="num">New plays</th><th scope="col" className="num">Already had</th></tr></thead>
          <tbody>{rows.map((x) => (
            <tr key={x.id}>
              <th scope="row" className="font-mono">{long(x.imported_at)}</th>
              <td className="font-mono">{x.from && x.to ? `${long(x.from)} to ${long(x.to)}` : "No music plays"}</td>
              <td className="num font-mono">{num(x.added)}</td>
              <td className="num font-mono">{num(x.already_had)}</td>
            </tr>))}</tbody>
        </table></div>)}
      <span className="text-text-2 text-sm">Import the whole new export each time. Plays you already have are recognised and skipped, and desktop plays are matched so nothing counts twice.</span>
    </Card>
  );
}

/* ------------------------------------------------------------------ keep it fresh */
function Fresh({ d }: { d: DataCoverage }) {
  const toast = useToast();
  const settings = useApi(api.getSettings);
  const logger = useApi(api.getLoggerStatus);
  /** Resolves false on a failed write, so the Switch flips back (ui.tsx). */
  const write = (fn: () => Promise<unknown>) => fn().then(() => true, (e) => { toast(api.toAppError(e).message); return false; });
  const s = settings.data, lg = logger.data;
  return (
    <section className="card flex flex-col gap-3" aria-labelledby="yd-fresh">
      <h2 id="yd-fresh" className="card-title">Keep it fresh</h2>
      {!s || !lg ? (settings.error || logger.error ? <ErrorBanner error={settings.error ?? logger.error} title="Couldn’t load these settings." onRetry={() => { settings.reload(); logger.reload(); }} />
        : <div className="skel h-[120px]" aria-busy="true" aria-label="Loading" />) : <>
        <div className="flex flex-col gap-0.5">
          <Switch label="Remind me to request a new export" on={s.export_reminder} onChange={(v) => write(() => api.updateSettings({ export_reminder: v }))} />
          <span className="text-muted text-xs">Every month, from the day the last one ends: a note when you open Runout. Nothing goes online.</span>
        </div>
        <div className="flex flex-col gap-0.5">
          <Switch label="Leave out private-session plays" on={!lg.include_private} onChange={(v) => write(() => api.setLogger({ include_private: !v }))} />
          <span className="text-muted text-xs">{lg.include_private ? "Private-session plays count in every insight." : `${pl(d.private_excluded, "play")} left out of every insight. On by default.`}</span>
        </div>
        <div className="flex flex-col gap-0.5">
          <Switch label="Look up genres and release years" on={s.music_db} onChange={(v) => write(() => api.setMusicDb(v))} />
          <span className="text-muted text-xs">{s.music_db
            ? "Artist and album names only go to MusicBrainz (Wikidata when it has nothing), about one a second. Turning this off stops lookups and deletes every genre, year and country looked up."
            : "Off: nothing is looked up, and nothing looked up is kept. Genres & decades stays empty."}</span>
        </div>
      </>}
      <span className="text-text-2 text-sm">Spotify takes from a few days up to 30 to prepare an Extended streaming history. You request it on spotify.com, in your account’s privacy settings; Runout can’t do it for you.</span>
    </section>
  );
}

const Skel = () => (
  <div className="flex flex-col gap-6" aria-busy="true" aria-label="Loading">
    <div className="card h-[220px]"><div className="skel h-full" /></div>
    <div className="card h-[200px]"><div className="skel h-full" /></div>
  </div>
);

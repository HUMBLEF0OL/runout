// SC-09 Insights v3, Genres & decades tab (M10 WI 9), per docs/design/M10-insights-v3.md §5/§7 and mockup PNG 6. Listening
// time (S, E) labelled from the open music database (source M): top genres, genres over the last 24 months, music by
// decade, the average age of your music, where your artists are from, and how much of it is covered. States: lookups off
// (nothing looked up, nothing stored), no project contact yet (nothing sent), still looking up (coverage so far). The
// MusicBrainz credit shows wherever genres show; the switch that stops lookups and deletes them is here and in Your data.
import { useState } from "react";
import { Link } from "react-router-dom";
import { m, useReducedMotion } from "motion/react";
import * as api from "../api";
import type { Covered, GenresDecades, PeriodQuery } from "../api";
import { Empty, ErrorBanner, InsightCard as Card, Stat, useToast } from "../components/ui";
import { Grow, monthLong, SPRING } from "../components/charts";
import { periodLabel, periodLastYear } from "../components/logic";
import { useApi } from "../hooks/useApi";
import { dayY, monthName, num, pl } from "../lib/format";
import { RISE } from "../lib/motion";
import { CoverageNotes } from "./InsightsReport";

type Res = Covered<GenresDecades>;
const unitOf = (q: PeriodQuery) => (q.grain === "all" ? null : q.grain === "custom" ? "stretch" : q.grain);
const pct = (n: number) => `${Math.round(n)}%`;
const REGION = (() => { try { return new Intl.DisplayNames(["en"], { type: "region" }); } catch { return null; } })();
/** ISO 3166-1 alpha-2 → "United Kingdom" (native Intl; the code itself when unknown). */
export const countryName = (code: string) => { try { return REGION?.of(code) ?? code; } catch { return code; } };

export function Genres({ q, today }: { q: PeriodQuery; today: string }) {
  const reduce = useReducedMotion();
  const toast = useToast();
  const g = useApi(() => api.getGenresDecades(q), [q.grain, q.vs, q.at, q.from, q.to]);
  const [busy, setBusy] = useState(false);
  const turn = (on: boolean) => { setBusy(true); api.setMusicDb(on).then(() => { toast(on ? "Lookups on. New artists and albums are looked up in the background." : "Lookups off. Every looked-up genre, year and country is deleted."); g.reload(); }, (e) => toast(api.toAppError(e).message)).finally(() => setBusy(false)); };
  if (g.loading) return <Skel />;
  if (g.error) return <ErrorBanner error={g.error} title="Couldn’t load genres and decades." onRetry={g.reload} />;
  const res = g.data!, d = res.data, lk = d.lookups;
  if (!lk.on) return (
    <Empty h="Genres and decades are off" line="Turn lookups on and Runout looks up each artist and album name once in MusicBrainz (Wikidata when MusicBrainz has nothing). Only names leave your PC: no account, nothing about you, about one a second.">
      <div className="flex flex-wrap gap-2 items-center">
        <button className="btn btn-primary" disabled={busy} onClick={() => turn(true)}>Turn on lookups</button>
        <Link className="btn btn-light" to="/insights/data">Your data</Link>
      </div>
    </Empty>);
  const unit = unitOf(q), label = periodLabel(q, today);
  const none = !d.top_genres.length && !d.decades.length && !d.countries.length;
  return (
    <m.div className="flex flex-col gap-6" initial={reduce ? false : "hidden"} animate="show">
      <m.div variants={RISE} custom={0} className="flex flex-col gap-3 empty:hidden">
        <CoverageNotes res={res} q={q} />
        {!lk.contact_set ? (
          <div className="banner" role="note"><span className="flex flex-col gap-0.5"><b className="text-sm">Lookups haven’t started</b>
            <span className="text-xs text-text-2">MusicBrainz asks every app to name a contact, and this build doesn’t have one yet. Nothing has left your PC.</span></span></div>
        ) : lk.pending > 0 && (
          <div className="banner" role="note"><span className="flex flex-col gap-0.5"><b className="text-sm">Still looking up {pl(lk.pending, "name")}</b>
            <span className="text-xs text-text-2">About one a second, in the background. So far {pct(d.coverage.artists_genre_pct)} of {unit ? `this ${unit}’s` : "your"} listening has a genre and {pct(d.coverage.albums_year_pct)} a release year.</span></span></div>
        )}
      </m.div>
      {none && !d.unknown_year_pct ? (
        // No decades and 0% unknown = the period has no S/E time at all (Rust pct(_, 0) = 0).
        <Empty h={`No listening in ${unit ? label : "your history"}`} line="Import your Spotify history to see this, or pick another period.">
          <Link className="btn btn-primary" to="/import/1">Import history</Link>
        </Empty>
      ) : none ? (
        <Empty h={`No genres for ${unit ? label : "your history"} yet`} line="Nothing in this period has been matched in the music database yet. Pick another period, or come back once lookups have run." />
      ) : <>
        <m.section variants={RISE} custom={1} className="stats" aria-label={`Genres and decades in ${label}`}>
          <Stat k={unit ? `Genres this ${unit}` : "Genres"} v={num(d.genre_count)} d="Your artists’ genres" />
          <Stat k="Top genre" v={d.top_genres[0]?.name ?? "None yet"} txt d={d.top_genres[0] ? `${pct(d.top_genres[0].share_pct)} of your time` : "No genre matched yet"} />
          <Stat k="Average age of your music" v={d.average_age ? pl(d.average_age.years, "yr") : "Unknown"} txt={!d.average_age} d={d.average_age ? `Released around ${d.average_age.year}` : "No release years yet"} />
          <Stat k={`Released in ${periodLastYear(q, today)}`} v={d.released_this_year_pct == null ? "Unknown" : pct(d.released_this_year_pct)}
            txt={d.released_this_year_pct == null} d="Of your listening with a year" />
        </m.section>
        <m.div variants={RISE} custom={2} className="flex flex-wrap gap-7 items-stretch">
          <Top res={res} />
          <Months res={res} />
        </m.div>
        <m.div variants={RISE} custom={3}><Decades res={res} label={label} /></m.div>
        <m.div variants={RISE} custom={4} className="flex flex-wrap gap-7 items-stretch">
          <Countries res={res} />
          <Cover res={res} unit={unit} busy={busy} off={() => turn(false)} />
        </m.div>
      </>}
    </m.div>
  );
}

/** The MusicBrainz credit (CC BY-NC-SA 3.0), from the response: wherever genres show. */
const Credit = ({ d }: { d: GenresDecades }) => <span className="text-muted text-xs">{d.attribution}. Community tags; an artist’s genre is its most voted tag.</span>;

/* ------------------------------------------------------------------ top genres */
function Top({ res }: { res: Res }) {
  const rows = res.data.top_genres, max = Math.max(1e-9, ...rows.map((x) => x.share_pct));
  return (
    <Card title="Top genres" res={res} className="flex-[1_1_320px] min-w-0">
      {!rows.length ? <span className="text-muted">No genres matched in this period yet.</span> : (
        <ol className="flex flex-col gap-1 m-0 p-0 list-none">{rows.map((x, i) => (
          <li key={x.name} className="in3-tag in3-genre-row"><span className="font-mono text-faint text-xs">{i + 1}</span>
            <span className="font-semibold truncate">{x.name}</span><Grow pct={(x.share_pct / max) * 100} i={i} hot={i === 0} />
            <span className="font-mono text-text-2 text-xs text-right">{pct(x.share_pct)}</span></li>))}</ol>)}
      <Credit d={res.data} />
    </Card>
  );
}

/* ------------------------------------------------------------------ genres over two years: stacked months, a slider */
function Months({ res }: { res: Res }) {
  const reduce = useReducedMotion();
  const { months, rows } = res.data.genre_months, n = months.length;
  const [at, setAt] = useState<number | null>(null);
  if (!n || !rows.length) return null;
  const cur = Math.min(at ?? n - 1, n - 1);
  const says = (i: number) => `${monthLong(months[i])}: ${rows.map((r) => `${r.genre} ${pct(r.shares[i] ?? 0)}`).join(", ")}`;
  const keys = (e: React.KeyboardEvent) => {
    const step: Record<string, number> = { ArrowLeft: -1, ArrowDown: -1, ArrowRight: 1, ArrowUp: 1, PageDown: -12, PageUp: 12 };
    const k = e.key in step ? cur + step[e.key] : e.key === "Home" ? 0 : e.key === "End" ? n - 1 : null;
    if (k === null) return;
    e.preventDefault();
    setAt(Math.max(0, Math.min(n - 1, k)));
  };
  const cols = { gridTemplateColumns: `repeat(${n}, minmax(0, 1fr))` };
  return (
    <Card title="Genres over two years" res={res} className="flex-[2_1_520px] min-w-0">
      <span className="text-text-2 text-sm">Share of each month’s listening, your top {rows.length} genres.</span>
      <div className="hm-plot in3-stack in3-gm" role="slider" tabIndex={0} aria-label="Top genres by month"
        aria-valuemin={0} aria-valuemax={n - 1} aria-valuenow={cur} aria-valuetext={says(cur)} onKeyDown={keys}
        onPointerMove={(e) => { const r = e.currentTarget.getBoundingClientRect(); setAt(Math.max(0, Math.min(n - 1, Math.floor(((e.clientX - r.left) / r.width) * n)))); }}
        onPointerLeave={() => setAt(null)} onBlur={() => setAt(null)}>
        <div className="hm-cols dense" style={cols} aria-hidden>
          {months.map((ym, i) => (
            <m.span key={ym} className={i === cur ? "on" : undefined}
              initial={reduce ? false : { scaleY: 0 }} animate={{ scaleY: 1 }} transition={{ ...SPRING, delay: 0.1 + (i / n) * 0.6 }}>
              {rows.map((r, g) => (r.shares[i] ?? 0) > 0 && <i key={r.genre} className={`g${g}`} style={{ height: `${Math.min(100, r.shares[i])}%` }} />)}
            </m.span>))}
        </div>
      </div>
      <div className="hm-months dense" style={cols} aria-hidden>{months.map((ym, i) => <span key={ym} className={i === cur ? "on" : undefined}>{i === cur || i % 3 === 0 ? monthName(ym) : ""}</span>)}</div>
      <span className="text-sm min-h-[20px]" aria-hidden>{at == null ? "" : says(cur) + "."}</span>
      <span className="in3-key" aria-hidden>{rows.map((r, g) => <span key={r.genre}><i className={`g${g}`} />{r.genre}</span>)}</span>
      <Credit d={res.data} />
    </Card>
  );
}

/* ------------------------------------------------------------------ music by decade */
function Decades({ res, label }: { res: Res; label: string }) {
  const reduce = useReducedMotion();
  const ds = res.data.decades, max = Math.max(1e-9, ...ds.map((x) => x.share_pct));
  const hot = ds.reduce((b, x, i) => (x.share_pct > ds[b].share_pct ? i : b), 0);
  return (
    <Card title="Music by decade" res={res}>
      <span className="text-text-2 text-sm">When the music you played in {label} was released, with your most played album from each decade.</span>
      {!ds.length ? <span className="text-muted">No release years matched in this period yet.</span> : (
        <div className="in3-dec" style={{ gridTemplateColumns: `repeat(${ds.length}, minmax(0, 1fr))` }} role="img"
          aria-label={`By decade: ${ds.map((x) => `${x.decade}s ${pct(x.share_pct)}${x.top_album ? `, most played ${x.top_album.name} by ${x.top_album.by}` : ""}`).join("; ")}`}>
          {ds.map((x, i) => (
            <div key={x.decade} className={i === hot ? "hot" : undefined}>
              <span className="bar"><span className="font-mono text-faint text-xs">{pct(x.share_pct)}</span><m.i style={{ height: `calc((100% - 20px) * ${(x.share_pct / max).toFixed(3)})`, originY: 1 }} initial={reduce ? false : { scaleY: 0 }} animate={{ scaleY: 1 }} transition={{ ...SPRING, delay: 0.1 + i * 0.05 }} /></span>
              <b>{x.decade}s</b>
              <span className="text-text-2 text-xs truncate" title={x.top_album ? `${x.top_album.name} · ${x.top_album.by}` : undefined}>{x.top_album?.name ?? ""}</span>
            </div>))}
        </div>)}
      <span className="text-muted text-xs">The release year is the earliest release of that album in MusicBrainz, so remasters count by their original year. {pct(res.data.unknown_year_pct)} of the time has no year yet (not matched, no album, or not looked up yet) and is left out rather than guessed.</span>
    </Card>
  );
}

/* ------------------------------------------------------------------ where your artists are from */
function Countries({ res }: { res: Res }) {
  const rows = res.data.countries, max = Math.max(1e-9, ...rows.map((x) => x.share_pct));
  return (
    <Card title="Where your artists are from" res={res} className="flex-[1_1_360px] min-w-0">
      {!rows.length ? <span className="text-muted">No artist countries matched in this period yet.</span> : (
        <div className="flex flex-col gap-1">{rows.map((x, i) => (
          <div key={x.name} className="in3-tag"><span className="truncate">{countryName(x.name)}</span><Grow pct={(x.share_pct / max) * 100} i={i} hot={i === 0} />
            <span className="font-mono text-text-2 text-xs text-right">{pct(x.share_pct)}</span></div>))}</div>)}
      <span className="text-muted text-xs">The artist’s country from MusicBrainz or Wikidata, as a share of the time whose artist has one. It says nothing about where you were: your own location is dropped on import.</span>
    </Card>
  );
}

/* ------------------------------------------------------------------ coverage, and the switch */
function Cover({ res, unit, busy, off }: { res: Res; unit: string | null; busy: boolean; off: () => void }) {
  const d = res.data, c = d.coverage;
  const rows: [string, number][] = [["Artists with a genre", c.artists_genre_pct], ["Albums with a year", c.albums_year_pct], ["Artists with a country", c.artists_country_pct]];
  return (
    <Card title="Coverage" res={res} className="flex-[1_1_360px] min-w-0">
      <span className="text-text-2 text-sm">Share of {unit ? `this ${unit}’s` : "your"} listening time the music database could label.</span>
      <div className="flex flex-col gap-1">{rows.map(([k, v], i) => (
        <div key={k} className="in3-tag"><span className="truncate">{k}</span><Grow pct={v} i={i} />
          <span className="font-mono text-text-2 text-xs text-right">{pct(v)}</span></div>))}</div>
      <span className="text-text-2 text-sm">{d.lookups.last_lookup ? `Last looked up ${dayY(d.lookups.last_lookup)}. ` : "Nothing looked up yet. "}New artists and albums are looked up once, the first time you play them.</span>
      {c.same_name_pct > 0 && <span className="text-muted text-xs">{pct(c.same_name_pct)} of the time is by artists who share their name with another; their labels are probably right, from the closest match.</span>}
      <div><button className="btn btn-danger" disabled={busy} onClick={off}>Turn off lookups and delete them</button></div>
      <Credit d={d} />
    </Card>
  );
}

const Skel = () => (
  <div className="flex flex-col gap-6" aria-busy="true" aria-label="Loading">
    <div className="stats">{[0, 1, 2, 3].map((i) => <div key={i} className="card stat"><div className="skel h-[60px]" /></div>)}</div>
    <div className="card h-[320px]"><div className="skel h-full" /></div>
    <div className="card h-[260px]"><div className="skel h-full" /></div>
  </div>
);

// SC-01 Home, full (R2+): now playing beside your year in listening, the period's numbers, most played (artists,
// albums, tracks), recent plays, your take vs your plays, backlog. Built to stay readable for years of history.
// Listening data only from your records + desktop log + export (sources U/S/E); Spotify Top items never appear here.
// Motion (M9, user 24 Oct 2026): Motion for React for data arriving and changing; CSS keeps hover/press states.
import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { AnimatePresence, m, useMotionValue, useReducedMotion, useSpring, useTransform } from "motion/react";
import { ArrowDownRightIcon, ArrowUpRightIcon, PlusIcon } from "@phosphor-icons/react";
import {
  getDashboard, getFreshness, getListeningInsights, getLoggerStatus, getTakeInsights, listItems, listPlays, resumeLogger, search, setLogger,
  type AppError, type ItemSummary, type ItemType, type ListeningInsights, type LoggerStatus, type Play, type Range, type RankRow as Rank, type Source,
} from "../api";
import { Topbar } from "../components/Shell";
import { playSources } from "../components/logic";
import { useQuickAdd } from "../components/QuickAddContext";
import {
  ArtTiles, EmbedSlot, Empty, ErrorBanner, FreshnessNote, LiveDot, OpenSpotify, PlayRow, Seg, SkelRows, SkelTiles, SourceTag, spot, Stat, Stars, Switch, useSlide, useToast,
} from "../components/ui";
import { useApi } from "../hooks/useApi";
import { hm, hmBig, hrs, mmss, monthName, nameOf, num, pl, PRIO, STATUS, statusMeta, TYPE } from "../lib/format";
import { byPriority } from "../lib/backlog";
import { MvpBody, StatSkel } from "./HomeMvp";

const RANGES: [Range, string][] = [["7d", "7 days"], ["30d", "30 days"], ["12m", "12 months"], ["all", "All time"]];
const PERIOD: Record<Range, string> = { "7d": "The last 7 days", "30d": "The last 30 days", "12m": "The last 12 months", all: "All time" };
const EASE = [0.16, 1, 0.3, 1] as const; // DESIGN §3 --ease-out
const SPRING = { type: "spring", bounce: 0.15, visualDuration: 0.45 } as const;
const monthLong = (ym: string) => new Date(`${ym}-01T00:00`).toLocaleString("en-GB", { month: "long", year: "numeric" });

/** Change against the period before; nothing to compare for All time or a first period. */
function Delta({ now, prev, fmt, more, less }: { now: number; prev: number; fmt: (n: number) => string; more: string; less: string }) {
  if (now === prev) return <span>Same as the period before</span>;
  const Icon = now > prev ? ArrowUpRightIcon : ArrowDownRightIcon;
  return <span className="hm-delta"><Icon size={12} weight="bold" aria-hidden />{fmt(Math.abs(now - prev))} {now > prev ? more : less} than the period before</span>;
}

/** Arrival (DESIGN §2.4 "arrival of data"): the page's sections rise in once, 70 ms apart. */
const PAGE = { show: { transition: { staggerChildren: 0.07 } } };
const RISE = { hidden: { opacity: 0, y: 14 }, show: { opacity: 1, y: 0, transition: { duration: 0.5, ease: EASE } } };

/** This period against the one before as two bars (no track), drawn under the delta line. */
function Versus({ now, prev, children }: { now: number; prev: number; children: React.ReactNode }) {
  const reduce = useReducedMotion();
  const max = Math.max(now, prev, 1);
  const bar = (v: number, cls: string, delay: number) => <m.i className={cls} style={{ width: `${Math.max(2, (v / max) * 100)}%`, originX: 0 }}
    initial={reduce ? false : { scaleX: 0 }} animate={{ scaleX: 1 }} transition={{ ...SPRING, delay }} />;
  return (
    <span className="flex flex-col gap-2.5">
      <span className="hm-vs" aria-hidden><span>Now</span>{bar(now, "now", 0.35)}<span>Before</span>{bar(prev, "", 0.42)}</span>
      {children}
    </span>
  );
}

/** Reduced motion: Motion's MotionConfig stops transforms but still fades, and `mode="wait"` still waits; swaps go instant. */
const swap = (reduce: boolean | null, t: object) => (reduce ? { duration: 0 } : t);

export default function HomeFull() {
  const reduce = useReducedMotion();
  const [range, setRange] = useState<Range>("7d");
  const ins = useApi(() => getListeningInsights(range), [range]);
  const plays = useApi(() => listPlays({ tab: "all", range: "all", offset: 0, limit: 6 }));
  const logger = useApi(getLoggerStatus);
  const fresh = useApi(getFreshness);
  const seg = <Seg label="Time range" options={RANGES} value={range} onChange={setRange} />;

  const noData = ins.data && ins.data.data.totals.plays === 0 && plays.data?.total === 0;
  if (noData) return <><Topbar /><h1 className="sr-only">Home</h1><EmptyListening /></>;
  if (ins.error) return <><Topbar>{seg}</Topbar><h1 className="sr-only">Home</h1><ErrorBanner title="Couldn’t load your listening." error={ins.error} onRetry={ins.reload} /></>;

  const t = ins.data?.data.totals;
  const src = ins.data?.sources ?? []; // no tag until the response says which sources it used (FR-032)
  const compare = range !== "all" && !!t?.plays_prev;
  return (
    <>
      <Topbar>{seg}</Topbar>
      <h1 className="sr-only">Home</h1>
      <m.div className="hm-page" variants={PAGE} initial={reduce ? false : "hidden"} animate="show" onPointerMove={spot}>
      <m.section variants={RISE} aria-label="Now playing and your year" className="flex flex-wrap gap-5 items-stretch">
        <NowCard logger={logger.data} loading={logger.loading} error={!!logger.error} retry={logger.reload} />
        <Pulse months={ins.data?.data.monthly} sources={src} />
      </m.section>

      <m.section variants={RISE} aria-labelledby="hm-period" className="flex flex-col gap-3.5">
        <div className="flex flex-wrap justify-between items-baseline gap-x-4 gap-y-1">
          <h2 id="hm-period" className="section-title hm-swap">
            <AnimatePresence mode="wait" initial={false}>
              <m.span key={range} className="inline-block" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }} transition={swap(reduce, { duration: 0.18, ease: EASE })}>{PERIOD[range]}</m.span>
            </AnimatePresence>
          </h2>
          <span className="flex flex-wrap gap-x-2 items-baseline"><FreshnessNote freshness={fresh.data} loggerState={logger.data?.state} /><SourceTag sources={src} /></span>
        </div>
        {!t ? <StatSkel />
          : <div className="stats">
              <Stat k="Plays" v={num(t.plays)} d={compare ? <Versus now={t.plays} prev={t.plays_prev}><Delta now={t.plays} prev={t.plays_prev} fmt={num} more="more" less="fewer" /></Versus> : "Played 30 seconds or more"} />
              <Stat k="Listening time" v={hmBig(t.ms)} d={compare ? <Versus now={t.ms} prev={t.ms_prev}><Delta now={t.ms} prev={t.ms_prev} fmt={hm} more="more" less="less" /></Versus>
                : t.ms >= 172_800_000 ? `About ${pl(Math.round(t.ms / 86_400_000), "day")} of music` : "In this period"} />
              <Stat k="New artists" v={num(t.new_artists)} d={range === "all" ? `${pl(t.artists_total, "artist")} in your history` : "First heard in this period"} />
              <Link to="/plays/unrated" className="text-inherit grid" aria-label={`${num(t.unrated_plays)} plays you haven’t rated. Rate what you keep coming back to`}>
                <Stat k="Plays you haven’t rated" v={num(t.unrated_plays)} d="Rate what you keep coming back to" act />
              </Link>
            </div>}
      </m.section>

      <m.div variants={RISE}><Charts range={range} loading={ins.loading} data={ins.data?.data} sources={src} /></m.div>

      <m.section variants={RISE} className="flex flex-wrap gap-7 items-start">
        <div className="flex-[1.4_1_480px] flex min-w-0 flex-col gap-3.5">
          <div className="flex justify-between items-baseline gap-3"><h2 className="section-title">Recent plays</h2><span className="flex flex-wrap gap-2 items-center">{!!plays.data?.rows.length && <SourceTag sources={playSources(plays.data.rows)} />}<Link to="/plays" className="text-sm font-semibold">See all</Link></span></div>
          <RecentPlays loading={plays.loading} rows={plays.data?.rows ?? []} error={plays.error} retry={plays.reload} />
        </div>
        <div className="flex-[1_1_320px] min-w-0 flex flex-col gap-5">
          <TakeCard />
          <BacklogCard />
        </div>
      </m.section>
      </m.div>
    </>
  );
}

/* ------------------------------------------------------------------ empty: no listening yet, the MVP home below */
function EmptyListening() {
  const toast = useToast();
  const dash = useApi(getDashboard);
  return (
    <>
      <Empty h="No listening data yet" line="Runout learns your listening from two places: the desktop logger, which records what Spotify plays on this PC, and your Spotify history export, which covers every device.">
        <div className="flex flex-wrap gap-2 items-center">
          <Link className="btn btn-primary" to="/import/1">Import history</Link>
          <button className="btn btn-secondary" onClick={() => setLogger({ on: true }).then(() => toast("Desktop logger is on"))}>Turn on the desktop logger</button>
        </div>
      </Empty>
      {dash.data && <MvpBody dash={dash.data} />}
      {dash.loading && <StatSkel />}
    </>
  );
}

/* ------------------------------------------------------------------ now playing */
const NOW = "flex-[1.6_1_480px]";
function NowCard({ logger, loading, error, retry }: { logger: LoggerStatus | undefined; loading: boolean; error: boolean; retry: () => void }) {
  const reduce = useReducedMotion();
  const add = useQuickAdd();
  const toast = useToast();
  const np = logger?.now_playing;
  // The album's own record, if saved (NowPlaying only links the track).
  const album = useApi(() => (np ? search(np.album_name, 10) : Promise.resolve([])), [np?.album_name]).data
    ?.find((h) => h.item.type === "album" && h.item.display_name === np?.album_name)?.item;
  if (loading) return <div className={`card ${NOW} min-h-[200px]`}><div className="skel h-full" /></div>;
  if (error || logger?.state === "error") return (
    <div className={`card ${NOW}`}>
      <div className="banner err" role="alert"><span><b className="text-danger">Desktop logger stopped.</b> <span className="text-text-2">Windows isn’t sharing what Spotify plays right now. Everything else works.</span></span>
        <button className="btn btn-secondary s" onClick={retry}>Try again</button></div>
    </div>);
  const quiet = (title: string, line: string, action?: React.ReactNode) => (
    <div className={`card ${NOW} flex flex-col gap-2.5 justify-center`}>
      <h2 className="text-sm font-semibold text-muted">{title}</h2><span className="text-text-2">{line}</span>{action && <div>{action}</div>}
    </div>);
  if (logger?.state === "paused") return quiet("Desktop logger paused", "Nothing is being logged on this PC until you resume.",
    <button className="btn btn-light" onClick={() => resumeLogger().then(() => toast("Logging resumed"))}>Resume logging</button>);
  if (logger?.state === "off") return quiet("Desktop logger is off", "Turn it on to log what Spotify plays on this PC.",
    <button className="btn btn-light" onClick={() => setLogger({ on: true }).then(() => toast("Desktop logger is on"))}>Turn on the desktop logger</button>);
  if (!np) return quiet("Nothing playing on your PC", "Play something in Spotify and it shows up here. Every play is a prompt for an opinion.");
  const track = np.item;
  return (
    <div className={`card now ${NOW} flex flex-wrap gap-6 items-center`}>
      <div className="flex-[1_1_260px] min-w-0 flex flex-col gap-3">
        <h2 className="text-sm font-semibold flex items-center gap-2 text-accent"><LiveDot />Playing now on your PC</h2>
        {/* A new track slides in (state change); the card itself stays put */}
        <AnimatePresence mode="popLayout" initial={false}>
          <m.div key={np.track_name + np.artist_name} className="flex flex-col gap-3 min-w-0" initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -10 }} transition={swap(reduce, { duration: 0.28, ease: EASE })}>
            <span className="text-2xl font-extrabold tracking-[-0.02em] leading-[1.1]">{np.track_name}</span>
            <span className="text-text-2">{np.artist_name}, from {np.album_name}. {track ? (track.in_backlog ? "The track is in your backlog." : `You have it as ${STATUS[track.status]}.`) : "Not in your library yet."}</span>
          </m.div>
        </AnimatePresence>
        <span className="font-mono text-faint text-xs">{mmss(np.ms_played)} of {mmss(np.duration_ms)}, counted as a play after 0:30</span>
        <div className="flex flex-wrap gap-2 items-center">
          {track ? <>
              <Link className="btn btn-light" to={`/item/${track.id}`}>{track.rating ? "Change rating" : "Rate it"}</Link>
              <Link className="btn btn-secondary" to={`/item/${track.id}`}>Add a note</Link>
            </>
            : <button className="btn btn-light" onClick={() => add({ suggest: { name: np.track_name, artist: np.artist_name, type: "track", uri: np.spotify_uri, from: "plays" } })}>Save and rate</button>}
          {album ? <Link className="btn btn-secondary" to={`/item/${album.id}`}>Open {np.album_name}</Link>
            : <button className="btn btn-secondary" onClick={() => add({ suggest: { name: np.album_name, artist: np.artist_name, type: "album", from: "plays" } })}>Save the album to backlog</button>}
          <OpenSpotify uri={np.spotify_uri} name={np.track_name} />
        </div>
      </div>
      {/* Spotify's compact player lays out from ~250 px wide; narrower it scrolls (DESIGN §5: never clip the embed) */}
      <div className="flex-[1_1_260px] min-w-[250px] max-w-[360px]"><EmbedSlot uri={np.spotify_uri} compact /></div>
    </div>
  );
}

/* ------------------------------------------------------------------ your year: 12 months as bars, scrubbable */
function Pulse({ months, sources }: { months: { month: string; hours: number }[] | undefined; sources: Source[] }) {
  const reduce = useReducedMotion();
  const [at, setAt] = useState<number | null>(null);
  // The value label rides a spring to the month in focus (pointer or keys); it rests on the latest month.
  const n = months?.length ?? 0;
  const tx = useMotionValue(1);
  const sx = useSpring(tx, SPRING);
  const x = useTransform(sx, [0, 1], ["0%", "100%"]);
  useEffect(() => { if (n) { const k = (n - 0.5) / n; tx.jump(k); sx.jump(k); } }, [n, tx, sx]);
  if (!months) return <div className="card hm-pulse"><div className="skel h-full min-h-[220px]" /></div>;
  if (n < 2) return null;
  const max = Math.max(1, ...months.map((m_) => m_.hours));
  const sum = months.reduce((a, m_) => a + m_.hours, 0);
  const peak = months.reduce((p, m_, i) => (m_.hours > months[p].hours ? i : p), 0);
  const cur = at ?? n - 1;
  const pick = (i: number | null) => {
    const k = ((i ?? n - 1) + 0.5) / n;
    setAt(i);
    tx.set(k);
    if (reduce) sx.jump(k); // useSpring ignores MotionConfig reducedMotion
  };
  const keys = (e: React.KeyboardEvent) => {
    const step: Record<string, number> = { ArrowLeft: -1, ArrowDown: -1, ArrowRight: 1, ArrowUp: 1, PageDown: -3, PageUp: 3 };
    const k = e.key in step ? cur + step[e.key] : e.key === "Home" ? 0 : e.key === "End" ? n - 1 : null;
    if (k === null) return;
    e.preventDefault();
    pick(Math.max(0, Math.min(n - 1, k)));
  };
  const mo = months[cur];
  const soFar = cur === n - 1 ? " so far" : "";
  return (
    <div className="card hm-pulse">
      <h2 className="card-title">Your last 12 months</h2>
      <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1">
        <span className="hm-big">{hrs(sum)}</span>
        <span className="text-muted text-sm">listened, busiest in {monthLong(months[peak].month)}</span>
      </div>
      <div className="hm-plot" role="slider" tabIndex={0} aria-label="Listening by month" aria-valuemin={0} aria-valuemax={n - 1} aria-valuenow={cur}
        aria-valuetext={`${monthLong(mo.month)}${soFar}: ${hrs(mo.hours)}${cur === peak ? ", your busiest month" : ""}`}
        onPointerMove={(e) => { const r = e.currentTarget.getBoundingClientRect(); const i = Math.max(0, Math.min(n - 1, Math.floor(((e.clientX - r.left) / r.width) * n))); if (i !== at) pick(i); }}
        onPointerLeave={() => pick(null)} onKeyDown={keys} onBlur={() => pick(null)}>
        <m.div className="hm-tip-track" style={{ x }} aria-hidden><span className={`hm-tip${at === null ? "" : " on"}`}>{hrs(mo.hours)}</span></m.div>
        <div className="hm-cols" aria-hidden>
          <span className="hm-avg" style={{ bottom: `${(sum / n / max) * 100}%` }} />
          {months.map((m_, i) => (
            <span key={m_.month} className={`hm-col${i === cur ? " on" : ""}`}>
              <m.i style={{ height: `${Math.max(2, (m_.hours / max) * 100)}%`, originY: 1 }}
                initial={reduce ? false : { scaleY: 0 }} animate={{ scaleY: 1 }} transition={{ ...SPRING, delay: 0.15 + i * 0.03 }} />
            </span>))}
        </div>
      </div>
      <div className="hm-months" aria-hidden>{months.map((m_, i) => <span key={m_.month} className={i === cur ? "on" : undefined}>{monthName(m_.month)}</span>)}</div>
      <span className="text-muted text-xs min-h-[17px]" aria-hidden>{at === null ? `Average ${hrs(sum / n)} a month, the faint line.` : `${monthLong(mo.month)}${soFar}${cur === peak ? ", your busiest month" : ""}.`}</span>
      <SourceTag sources={sources} />
    </div>
  );
}

/* ------------------------------------------------------------------ most played: one card, three tabs, top 10 */
type Kind = "artists" | "albums" | "tracks";
const KINDS: [Kind, string][] = [["artists", "Artists"], ["albums", "Albums"], ["tracks", "Tracks"]];
const ONE: Record<Kind, ItemType> = { artists: "artist", albums: "album", tracks: "track" };

function Charts({ range, loading, data, sources }: { range: Range; loading: boolean; data: ListeningInsights | undefined; sources: Source[] }) {
  const add = useQuickAdd();
  const reduce = useReducedMotion();
  const [kind, setKind] = useState<Kind>("artists");
  const [dir, slide] = useSlide(KINDS.map((k) => k[0]), kind);
  const [art, setArt] = useState(false);
  const artOn = art && kind === "albums";
  // Album records give the embed URI for artwork mode; fetched only once artwork is on.
  const albums_ = useApi(() => (artOn ? listItems({ type: "album", limit: 1000 }) : Promise.resolve(null)), [artOn]);
  const rows = data?.[`top_${kind}`] ?? [];
  const periodH = (data?.totals.ms ?? 0) / 3_600_000;
  const max = Math.max(1, ...rows.map((r) => r.hours));
  const save = (r: Rank) => add({ suggest: { name: r.name, artist: kind === "artists" ? undefined : r.by, type: ONE[kind], from: "plays" } });
  const saved = rows.slice(0, 10).map((a) => albums_.data?.rows.find((i) => i.id === a.item_id)).filter((i): i is ItemSummary => !!i);
  const label = KINDS.find((k) => k[0] === kind)![1];
  const body = loading || (artOn && albums_.loading) ? (artOn ? <SkelTiles n={5} /> : <SkelRows n={6} />)
    : !rows.length ? <span className="text-muted">No plays in this period.</span>
    : artOn ? <ArtTiles items={saved} meta={(it) => statusMeta(it)} />
    : <div className="flex flex-wrap gap-x-8 gap-y-5 items-stretch">
        <Lead r={rows[0]} kind={kind} share={periodH ? rows[0].hours / periodH : 0} onSave={save} />
        <ol className="hm-ranks" start={2}>{rows.slice(1, 10).map((r, i) => (
          <li key={r.name + r.by} className="hm-rank">
            <span className="hm-rank-n">{i + 2}</span>
            <div className="min-w-0 flex flex-col gap-1">
              <span className="truncate" title={kind === "artists" ? r.name : `${r.name}, ${r.by}`}>{r.item_id ? <Link to={`/item/${r.item_id}`} className="hm-stretch text-text font-semibold">{r.name}</Link> : <b>{r.name}</b>}
                {kind !== "artists" && r.by && <span className="text-muted">, {r.by}</span>}</span>
              <m.span className={`hm-bar${r.item_id ? "" : " hot"}`} style={{ width: `${Math.max(2, (r.hours / max) * 100)}%`, originX: 0 }}
                initial={reduce ? false : { scaleX: 0 }} animate={{ scaleX: 1 }} transition={{ ...SPRING, delay: 0.08 + i * 0.035 }} />
            </div>
            <span className="flex flex-col items-end gap-0.5"><span className="font-mono text-text-2 text-xs whitespace-nowrap">{hrs(r.hours)}</span><Opinion r={r} /></span>
            {r.item_id ? <span aria-hidden /> : <button className="icon-btn hm-save" onClick={() => save(r)} aria-label={`Save ${r.name} to your backlog`}><PlusIcon size={14} weight="bold" /></button>}
          </li>))}
        </ol>
      </div>;
  return (
    <section className="card hm-charts" aria-labelledby="hm-top">
      <div className="flex flex-wrap justify-between items-baseline gap-x-4 gap-y-1"><h2 id="hm-top" className="section-title">Most played</h2><SourceTag sources={sources} /></div>
      <div className="flex flex-wrap justify-between items-center gap-3">
        <Seg label="Most played" options={KINDS} value={kind} onChange={(k) => { slide(k); setKind(k); }} />
        {kind === "albums" && <Switch label="Artwork" on={art} onChange={async (v) => { setArt(v); return true; }} />}
      </div>
      <AnimatePresence mode="wait" initial={false}>
        <m.div key={`${kind}-${range}-${artOn}`} role="tabpanel" aria-label={label} className="min-w-0"
          initial={{ opacity: 0, x: dir === "from-right" ? 18 : -18 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, transition: swap(reduce, { duration: 0.12 }) }} transition={swap(reduce, { duration: 0.32, ease: EASE })}>
          {body}
        </m.div>
      </AnimatePresence>
    </section>
  );
}

/** Your opinion next to every chart row (DESIGN §1.1): status and stars, or that it isn't saved. */
const Opinion = ({ r }: { r: Rank }) => (
  <span className="text-muted text-xs whitespace-nowrap">{r.status ? STATUS[r.status] : "Not saved"}{r.rating ? " " : ""}<Stars r={r.rating} /></span>
);

function Lead({ r, kind, share, onSave }: { r: Rank; kind: Kind; share: number; onSave: (r: Rank) => void }) {
  const reduce = useReducedMotion();
  const pct = share >= 0.005 ? `${Math.round(share * 100)}%` : "<1%";
  return (
    <div className="hm-lead">
      <div className="flex justify-between items-start gap-4">
        <div className="flex flex-col gap-1.5 min-w-0">
          <span className="text-muted text-xs font-semibold">Your top {ONE[kind]}</span>
          {r.item_id ? <Link to={`/item/${r.item_id}`} className="hm-lead-name text-text">{r.name}</Link> : <span className="hm-lead-name">{r.name}</span>}
          {kind !== "artists" && r.by && <span className="text-text-2">{r.by}</span>}
        </div>
        {/* Share of your listening time in the period, drawn once as the ring fills */}
        <div className="hm-ring" role="img" aria-label={`${pct} of your listening time`}>
          <svg viewBox="0 0 64 64" aria-hidden><circle cx="32" cy="32" r="28" className="track" />
            <m.circle cx="32" cy="32" r="28" className="fill" initial={reduce ? false : { pathLength: 0 }} animate={{ pathLength: Math.min(1, Math.max(0.01, share)) }} transition={{ duration: 0.9, ease: EASE, delay: 0.2 }} /></svg>
          <span aria-hidden>{pct}</span>
        </div>
      </div>
      <div className="flex gap-7">
        <span className="flex flex-col gap-0.5"><span className="hm-big">{hrs(r.hours)}</span><span className="text-muted text-xs">listened</span></span>
        <span className="flex flex-col gap-0.5"><span className="hm-big">{num(r.plays)}</span><span className="text-muted text-xs">plays</span></span>
      </div>
      <div className="flex flex-wrap gap-2 items-center mt-auto">
        {r.item_id ? <><Opinion r={r} /><Link to={`/item/${r.item_id}`} className="btn btn-secondary s">{r.rating ? "Open" : "Rate it"}</Link></>
          : <button className="btn btn-light" onClick={() => onSave(r)}>Save to backlog</button>}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ recent plays: every play is a prompt for an opinion */
function RecentPlays({ loading, rows, error, retry }: { loading: boolean; rows: Play[]; error: AppError | undefined; retry: () => void }) {
  const nav = useNavigate();
  const add = useQuickAdd();
  if (loading) return <SkelRows n={6} />;
  if (error) return <ErrorBanner title="Couldn’t load recent plays." error={error} onRetry={retry} />;
  if (!rows.length) return <span className="text-muted">No plays yet.</span>;
  return (
    <div className="card pad-s"><div className="list">{rows.map((p) => (
      <PlayRow key={p.id} p={p} onRate={(x) => x.item && nav(`/item/${x.item.id}`)}
        onSave={(x) => add({ suggest: { name: x.track_name, artist: x.artist_name, type: "track", uri: x.spotify_uri, from: "plays" } })} />
    ))}</div></div>
  );
}

function TakeCard() {
  const take = useApi(getTakeInsights);
  const d = take.data?.data;
  const rows = d ? ([["Loved, then quiet", d.loved_but_forgotten[0]], ["Played, not rated", d.rated_vs_played[0]], ["Waiting, never played", d.never_played_backlog[0]]] as const).filter(([, x]) => x) : [];
  return (
    <div className="card flex flex-col gap-3">
      <div className="flex justify-between items-baseline gap-3"><h2 className="card-title">Your take vs your plays</h2>{take.data && <SourceTag sources={take.data.sources} />}</div>
      {take.loading ? <div className="skel h-[120px]" /> : take.error ? <ErrorBanner error={take.error} title="Couldn’t load this." onRetry={take.reload} />
        : !rows.length ? <span className="text-muted">Nothing stands out yet. Rate a few records you play.</span>
        : rows.map(([k, t]) => (
          <Link key={t!.item.id} to={`/item/${t!.item.id}`} className="row flex flex-col items-start gap-[3px] bg-surface-2">
            <span className="text-faint text-xs font-semibold">{k}</span>
            <span className="font-semibold text-text">{nameOf(t!.item)}</span><span className="text-text-2 text-sm">{t!.line}</span>
          </Link>))}
    </div>
  );
}

function BacklogCard() {
  const b = useApi(() => listItems({ in_backlog: true, sort: "backlog_order", limit: 200 }));
  const top3 = [...(b.data?.rows ?? [])].sort(byPriority).slice(0, 3);
  return (
    <div className="card flex flex-col gap-3">
      <div className="flex justify-between items-baseline gap-3"><h2 className="card-title">Backlog</h2>{b.data && <Link to="/backlog" className="text-sm font-semibold">{num(b.data.total)} waiting</Link>}</div>
      {b.data && !top3.length && <span className="text-muted">Nothing waiting. Paste a link to add one.</span>}
      {top3.map((it) => (
        <Link key={it.id} to={`/item/${it.id}`} className="lrow flex justify-between items-baseline gap-3 text-text py-1">
          <span className="flex flex-col gap-0.5 min-w-0"><span className="font-semibold truncate">{nameOf(it)}</span>
            <span className="text-muted text-xs">{TYPE[it.type]}{it.recommended_by ? `, from ${it.recommended_by}` : ""}</span></span>
          <span className={`text-xs whitespace-nowrap ${it.priority === "high" ? "text-accent" : "text-muted"}`}>{it.status === "in_progress" ? "In progress" : it.priority ? PRIO[it.priority] : "Planned"}</span>
        </Link>
      ))}
    </div>
  );
}

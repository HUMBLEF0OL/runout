// SC-01 Home, full (R2+): now playing beside your year in listening, the period's numbers, most played (artists,
// albums, tracks), recent plays, your take vs your plays, backlog. Built to stay readable for years of history.
// Listening data only from your records + desktop log + export (sources U/S/E); Spotify Top items never appear here.
// Motion (M9, user 24 Oct 2026): Motion for React for data arriving and changing; CSS keeps hover/press states.
import { useEffect, useId, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { AnimatePresence, m as motion, useMotionValue, useReducedMotion, useSpring, useTransform } from "motion/react";
import { ArrowDownRightIcon, ArrowUpRightIcon, PlusIcon } from "@phosphor-icons/react";
import {
  getDashboard, getFreshness, getListeningInsights, getLoggerStatus, getTakeInsights, listItems, listPlays, resumeLogger, search, setLogger,
  type AppError, type ItemSummary, type ItemType, type ListeningInsights, type LoggerStatus, type Play, type Range, type RankRow as Rank, type Source,
} from "../api";
import { Topbar } from "../components/Shell";
import { playSources } from "../components/logic";
import { useQuickAdd } from "../components/QuickAddContext";
import {
  ArtTiles, EmbedSlot, Empty, ErrorBanner, FreshnessNote, LiveDot, OpenSpotify, PlayRow, Seg, SkelRows, SkelTiles, SourceTag, Stat, Stars, Switch, useSlide, useToast,
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
      <section aria-label="Now playing and your year" className="flex flex-wrap gap-5 items-stretch">
        <NowCard logger={logger.data} loading={logger.loading} error={!!logger.error} retry={logger.reload} />
        <Pulse months={ins.data?.data.monthly} sources={src} />
      </section>

      <section aria-labelledby="hm-period" className="flex flex-col gap-3.5">
        <div className="flex flex-wrap justify-between items-baseline gap-x-4 gap-y-1">
          <h2 id="hm-period" className="section-title hm-swap">
            <AnimatePresence mode="wait" initial={false}>
              <motion.span key={range} className="inline-block" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }} transition={swap(reduce, { duration: 0.18, ease: EASE })}>{PERIOD[range]}</motion.span>
            </AnimatePresence>
          </h2>
          <span className="flex flex-wrap gap-x-2 items-baseline"><FreshnessNote freshness={fresh.data} loggerState={logger.data?.state} /><SourceTag sources={src} /></span>
        </div>
        {!t ? <StatSkel />
          : <div className="stats">
              <Stat k="Plays" v={num(t.plays)} d={compare ? <Delta now={t.plays} prev={t.plays_prev} fmt={num} more="more" less="fewer" /> : "Played 30 seconds or more"} />
              <Stat k="Listening time" v={hmBig(t.ms)} d={compare ? <Delta now={t.ms} prev={t.ms_prev} fmt={hm} more="more" less="less" />
                : t.ms >= 172_800_000 ? `About ${pl(Math.round(t.ms / 86_400_000), "day")} of music` : "In this period"} />
              <Stat k="New artists" v={num(t.new_artists)} d={range === "all" ? `${pl(t.artists_total, "artist")} in your history` : "First heard in this period"} />
              <Link to="/plays/unrated" className="text-inherit grid" aria-label={`${num(t.unrated_plays)} plays you haven’t rated. Rate what you keep coming back to`}>
                <Stat k="Plays you haven’t rated" v={num(t.unrated_plays)} d="Rate what you keep coming back to" act />
              </Link>
            </div>}
      </section>

      <Charts range={range} loading={ins.loading} data={ins.data?.data} sources={src} />

      <section className="flex flex-wrap gap-7 items-start">
        <div className="flex-[1.4_1_480px] flex min-w-0 flex-col gap-3.5">
          <div className="flex justify-between items-baseline gap-3"><h2 className="section-title">Recent plays</h2><span className="flex flex-wrap gap-2 items-center">{!!plays.data?.rows.length && <SourceTag sources={playSources(plays.data.rows)} />}<Link to="/plays" className="text-sm font-semibold">See all</Link></span></div>
          <RecentPlays loading={plays.loading} rows={plays.data?.rows ?? []} error={plays.error} retry={plays.reload} />
        </div>
        <div className="flex-[1_1_320px] min-w-0 flex flex-col gap-5">
          <TakeCard />
          <BacklogCard />
        </div>
      </section>
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
    <div className={`card now ${NOW} flex flex-wrap gap-5 items-center`}>
      <div className="flex-[0_0_152px]"><EmbedSlot uri={np.spotify_uri} compact /></div>
      <div className="flex-[1_1_260px] min-w-0 flex flex-col gap-3">
        <h2 className="text-sm font-semibold flex items-center gap-2 text-accent"><LiveDot />Playing now on your PC</h2>
        {/* A new track slides in (state change); the card itself stays put */}
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.div key={np.track_name + np.artist_name} className="flex flex-col gap-3 min-w-0" initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -10 }} transition={swap(reduce, { duration: 0.28, ease: EASE })}>
            <span className="text-2xl font-extrabold tracking-[-0.02em] leading-[1.1]">{np.track_name}</span>
            <span className="text-text-2">{np.artist_name}, from {np.album_name}. {track ? (track.in_backlog ? "The track is in your backlog." : `You have it as ${STATUS[track.status]}.`) : "Not in your library yet."}</span>
          </motion.div>
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
    </div>
  );
}

/* ------------------------------------------------------------------ your year: 12 months of listening, scrubbable */
function Pulse({ months, sources }: { months: { month: string; hours: number }[] | undefined; sources: Source[] }) {
  const reduce = useReducedMotion();
  const gid = useId();
  const [at, setAt] = useState<number | null>(null);
  // The marker rides a spring between months (pointer or arrow keys); it rests on the latest month.
  const mx = useMotionValue(1), my = useMotionValue(0.5);
  const sx = useSpring(mx, SPRING), sy = useSpring(my, SPRING);
  const x = useTransform(sx, [0, 1], ["0%", "100%"]), y = useTransform(sy, [0, 1], ["0%", "100%"]);
  const last = months?.length ? months[months.length - 1].hours / Math.max(1, ...months.map((m) => m.hours)) : 0;
  useEffect(() => { const k = 0.92 - last * 0.84; mx.jump(1); my.jump(k); sx.jump(1); sy.jump(k); }, [last, mx, my, sx, sy]); // rest on the latest month, no glide in
  if (!months) return <div className="card hm-pulse"><div className="skel h-full min-h-[200px]" /></div>;
  const n = months.length;
  if (n < 2) return null;
  const max = Math.max(1, ...months.map((m) => m.hours));
  const sum = months.reduce((a, m) => a + m.hours, 0);
  const peak = months.reduce((p, m, i) => (m.hours > months[p].hours ? i : p), 0);
  const fy = (h: number) => 0.92 - (h / max) * 0.84; // 0 = top of the plot
  const line = months.map((m, i) => `${i ? "L" : "M"}${(i / (n - 1)) * 100},${fy(m.hours) * 100}`).join("");
  const avgY = fy(sum / n) * 100;
  const cur = at ?? n - 1;
  const pick = (i: number | null) => {
    const k = i ?? n - 1;
    setAt(i);
    mx.set(k / (n - 1));
    my.set(fy(months[k].hours));
    if (reduce) { sx.jump(k / (n - 1)); sy.jump(fy(months[k].hours)); } // useSpring ignores MotionConfig reducedMotion
  };
  const keys = (e: React.KeyboardEvent) => {
    const step: Record<string, number> = { ArrowLeft: -1, ArrowDown: -1, ArrowRight: 1, ArrowUp: 1, PageDown: -3, PageUp: 3 };
    const k = e.key in step ? cur + step[e.key] : e.key === "Home" ? 0 : e.key === "End" ? n - 1 : null;
    if (k === null) return;
    e.preventDefault();
    pick(Math.max(0, Math.min(n - 1, k)));
  };
  const m = months[cur];
  return (
    <div className="card hm-pulse">
      <h2 className="card-title">Your last 12 months</h2>
      <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1 min-h-[34px]" aria-hidden={at !== null}>
        <span className="hm-big">{at === null ? hrs(sum) : hrs(m.hours)}</span>
        <span className="text-muted text-sm">{at === null ? `listened, busiest in ${monthLong(months[peak].month)}` : `in ${monthLong(m.month)}${cur === n - 1 ? " so far" : ""}${cur === peak ? ", your busiest month" : ""}`}</span>
      </div>
      <div className="hm-plot" role="slider" tabIndex={0} aria-label="Listening by month" aria-valuemin={0} aria-valuemax={n - 1} aria-valuenow={cur}
        aria-valuetext={`${monthLong(m.month)}${cur === n - 1 ? " so far" : ""}: ${hrs(m.hours)}${cur === peak ? ", your busiest month" : ""}`}
        onPointerMove={(e) => { const r = e.currentTarget.getBoundingClientRect(); const i = Math.max(0, Math.min(n - 1, Math.round(((e.clientX - r.left) / r.width) * (n - 1)))); if (i !== at) pick(i); }}
        onPointerLeave={() => pick(null)} onKeyDown={keys} onBlur={() => pick(null)}>
        <motion.svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden
          initial={reduce ? false : { clipPath: "inset(0 100% 0 0)" }} animate={{ clipPath: "inset(0 0% 0 0)" }} transition={{ duration: 0.9, ease: EASE }}>
          <defs><linearGradient id={gid} x1="0" x2="0" y1="0" y2="1"><stop offset="0" className="hm-stop" stopOpacity="0.1" /><stop offset="1" className="hm-stop" stopOpacity="0" /></linearGradient></defs>
          <line x1="0" x2="100" y1={avgY} y2={avgY} className="hm-avg" vectorEffect="non-scaling-stroke" />
          <path d={`${line}L100,100L0,100Z`} fill={`url(#${gid})`} />
          <path d={line} className="hm-line" vectorEffect="non-scaling-stroke" />
        </motion.svg>
        <motion.div className={`hm-mark${at === null ? "" : " on"}`} style={{ x }} aria-hidden><motion.div className="absolute inset-0" style={{ y }}><span className="hm-dot" /></motion.div></motion.div>
      </div>
      <div className="hm-months" aria-hidden>{months.map((mo, i) => <span key={mo.month} className={i === cur ? "on" : undefined}>{monthName(mo.month)}</span>)}</div>
      <span className="text-muted text-xs">Average {hrs(sum / n)} a month, the faint line.</span>
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
    : <div className="flex flex-wrap gap-x-8 gap-y-5 items-start">
        <Lead r={rows[0]} kind={kind} share={periodH ? rows[0].hours / periodH : 0} onSave={save} />
        <ol className="hm-ranks" start={2}>{rows.slice(1, 10).map((r, i) => (
          <li key={r.name + r.by} className="hm-rank">
            <span className="font-mono text-faint text-xs">{i + 2}</span>
            <div className="min-w-0 flex flex-col gap-1">
              <span className="truncate" title={kind === "artists" ? r.name : `${r.name}, ${r.by}`}>{r.item_id ? <Link to={`/item/${r.item_id}`} className="text-text font-semibold">{r.name}</Link> : <b>{r.name}</b>}
                {kind !== "artists" && r.by && <span className="text-muted">, {r.by}</span>}</span>
              <motion.span className={`hm-bar${r.item_id ? "" : " hot"}`} style={{ width: `${Math.max(2, (r.hours / max) * 100)}%`, originX: 0 }}
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
        <motion.div key={`${kind}-${range}-${artOn}`} role="tabpanel" aria-label={label} className="min-w-0"
          initial={{ opacity: 0, x: dir === "from-right" ? 18 : -18 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, transition: swap(reduce, { duration: 0.12 }) }} transition={swap(reduce, { duration: 0.32, ease: EASE })}>
          {body}
        </motion.div>
      </AnimatePresence>
    </section>
  );
}

/** Your opinion next to every chart row (DESIGN §1.1): status and stars, or that it isn't saved. */
const Opinion = ({ r }: { r: Rank }) => (
  <span className="text-muted text-xs whitespace-nowrap">{r.status ? STATUS[r.status] : "Not saved"}{r.rating ? " " : ""}<Stars r={r.rating} /></span>
);

function Lead({ r, kind, share, onSave }: { r: Rank; kind: Kind; share: number; onSave: (r: Rank) => void }) {
  return (
    <div className="hm-lead">
      <span className="text-muted text-xs font-semibold">Your top {ONE[kind]}</span>
      <div className="flex flex-col gap-1 min-w-0">
        {r.item_id ? <Link to={`/item/${r.item_id}`} className="hm-lead-name text-text">{r.name}</Link> : <span className="hm-lead-name">{r.name}</span>}
        {kind !== "artists" && r.by && <span className="text-text-2">{r.by}</span>}
      </div>
      <div className="grid grid-cols-[repeat(3,minmax(0,auto))] justify-start gap-x-6 gap-y-1">
        <span className="hm-big">{hrs(r.hours)}</span><span className="hm-big">{num(r.plays)}</span><span className="hm-big">{share >= 0.005 ? `${Math.round(share * 100)}%` : "<1%"}</span>
        <span className="text-muted text-xs">listened</span><span className="text-muted text-xs">plays</span><span className="text-muted text-xs">of your time</span>
      </div>
      <div className="flex flex-wrap gap-2 items-center">
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

// SC-01 Home, full (R2+): now playing, period stats, top artists/albums, recent plays, your take vs your plays, backlog.
// Listening data only from your records + desktop log + export (sources U/S/E); Spotify Top items never appear here.
import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import {
  getDashboard, getFreshness, getListeningInsights, getLoggerStatus, getTakeInsights, listItems, listPlays, resumeLogger, search, setLogger,
  type AppError, type ItemSummary, type LoggerStatus, type Play, type Range, type RankRow as Rank,
} from "../api";
import { Topbar } from "../components/Shell";
import { playSources } from "../components/logic";
import { useQuickAdd } from "../components/QuickAddContext";
import {
  ArtTiles, EmbedSlot, Empty, ErrorBanner, FreshnessNote, LiveDot, OpenSpotify, PlayRow, Seg, SkelRows, SkelTiles, SourceTag, Stat, useToast,
} from "../components/ui";
import { useApi } from "../hooks/useApi";
import { hm, mmss, nameOf, num, PRIO, stars, STATUS, statusMeta, TYPE } from "../lib/format";
import { MvpBody, StatSkel } from "./HomeMvp";

const RANGES: [Range, string][] = [["7d", "7 days"], ["30d", "30 days"], ["12m", "12 months"], ["all", "All time"]];
const PRIO_ORDER = { high: 0, medium: 1, low: 2 } as const;
const hrs = (r: Rank) => `${r.hours.toFixed(1)} h`; // charts rank by listening time (CAP-23)
/** "37 more than the period before" / "3h 59m more than before". */
const delta = (now: number, prev: number, fmt: (n: number) => string, more: string, less: string) =>
  `${fmt(Math.abs(now - prev))} ${now >= prev ? more : less}`;

export default function HomeFull() {
  const [range, setRange] = useState<Range>("7d");
  const ins = useApi(() => getListeningInsights(range), [range]);
  const plays = useApi(() => listPlays({ tab: "all", range: "all", offset: 0, limit: 6 }));
  const logger = useApi(getLoggerStatus);
  const fresh = useApi(getFreshness);
  const seg = <Seg label="Time range" options={RANGES} value={range} onChange={setRange} />;

  const noData = ins.data && ins.data.data.totals.plays === 0 && plays.data?.total === 0;
  if (noData) return <><Topbar /><EmptyListening /></>;
  if (ins.error) return <><Topbar>{seg}</Topbar><ErrorBanner title="Couldn’t load your listening." error={ins.error} onRetry={ins.reload} /></>;

  const t = ins.data?.data.totals;
  const src = ins.data?.sources ?? []; // no tag until the response says which sources it used (FR-032)
  return (
    <>
      <Topbar>{seg}</Topbar>
      <section aria-label="Now and this period" className="flex flex-wrap gap-5 items-stretch">
        <NowCard logger={logger.data} loading={logger.loading} error={!!logger.error} retry={logger.reload} />
        {!t ? <div className="stats flex-[1_1_300px] grid-cols-2">{[1, 2, 3, 4].map((i) => <div key={i} className="card h-24"><div className="skel h-full" /></div>)}</div>
          : <div className="stats flex-[1_1_300px] grid-cols-2">
              <Stat k="Plays" v={num(t.plays)} d={`${delta(t.plays, t.plays_prev, num, "more", "fewer")} than the period before`} />
              <Stat k="Listening time" v={hm(t.ms)} d={`${delta(t.ms, t.ms_prev, hm, "more", "less")} than before`} />
              <Stat k="New artists" v={t.new_artists} d="First heard in this period" />
              <Link to="/plays/unrated" className="text-inherit grid" aria-label={`${num(t.unrated_plays)} plays you haven’t rated. Rate what you keep coming back to`}>
                <Stat k="Plays you haven’t rated" v={num(t.unrated_plays)} d="Rate what you keep coming back to" act />
              </Link>
            </div>}
      </section>
      <div className="flex flex-wrap gap-2 items-center justify-end"><FreshnessNote freshness={fresh.data} loggerState={logger.data?.state} /><SourceTag sources={src} /></div>
      <Charts loading={ins.loading} artists={ins.data?.data.top_artists ?? []} albums={ins.data?.data.top_albums ?? []} sources={src} />
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
          <Link className="btn btn-primary" to="/import/1">Import your Spotify history</Link>
          <button className="btn btn-secondary" onClick={() => setLogger({ on: true }).then(() => toast("Desktop logger is on"))}>Turn on the desktop logger</button>
        </div>
      </Empty>
      {dash.data && <MvpBody dash={dash.data} />}
      {dash.loading && <StatSkel />}
    </>
  );
}

/* ------------------------------------------------------------------ now playing */
function NowCard({ logger, loading, error, retry }: { logger: LoggerStatus | undefined; loading: boolean; error: boolean; retry: () => void }) {
  const add = useQuickAdd();
  const toast = useToast();
  const np = logger?.now_playing;
  // The album's own record, if saved (NowPlaying only links the track).
  const album = useApi(() => (np ? search(np.album_name, 10) : Promise.resolve([])), [np?.album_name]).data
    ?.find((h) => h.item.type === "album" && h.item.display_name === np?.album_name)?.item;
  if (loading) return <div className="card flex-[999_1_520px] h-[172px]"><div className="skel h-full" /></div>;
  if (error || logger?.state === "error") return (
    <div className="card flex-[999_1_520px]">
      <div className="banner err" role="alert"><span><b className="text-danger">Desktop logger stopped.</b> <span className="text-text-2">Windows isn’t sharing what Spotify plays right now. Everything else works.</span></span>
        <button className="btn btn-secondary s" onClick={retry}>Try again</button></div>
    </div>);
  const quiet = (title: string, line: string, action?: React.ReactNode) => (
    <div className="card flex-[999_1_520px] flex flex-col gap-2.5 justify-center">
      <h2 className="text-sm font-semibold text-muted">{title}</h2><span className="text-text-2">{line}</span>{action && <div>{action}</div>}
    </div>);
  if (logger?.state === "paused") return quiet("Desktop logger paused", "Nothing is being logged on this PC until you resume.",
    <button className="btn btn-light" onClick={() => resumeLogger().then(() => toast("Logging resumed"))}>Resume logging</button>);
  if (logger?.state === "off") return quiet("Desktop logger is off", "Turn it on to log what Spotify plays on this PC.",
    <button className="btn btn-light" onClick={() => setLogger({ on: true }).then(() => toast("Desktop logger is on"))}>Turn on the desktop logger</button>);
  if (!np) return quiet("Nothing playing on your PC", "Play something in Spotify and it shows up here. Every play is a prompt for an opinion.");
  const track = np.item;
  return (
    <div className="card now flex-[999_1_520px] flex flex-wrap gap-5 items-center">
      <div className="flex-[0_0_152px]"><EmbedSlot uri={np.spotify_uri} compact /></div>
      <div className="flex-[1_1_260px] min-w-0 flex flex-col gap-3">
        <h2 className="text-sm font-semibold flex items-center gap-2 text-accent"><LiveDot />Playing now on your PC</h2>
        <span className="text-2xl font-extrabold tracking-[-0.02em] leading-[1.1]">{np.track_name}</span>
        <span className="text-text-2">{np.artist_name}, from {np.album_name}. {track ? (track.in_backlog ? "The track is in your backlog." : `You have it as ${STATUS[track.status]}.`) : "Not in your library yet."}</span>
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

/* ------------------------------------------------------------------ charts */
function Charts({ loading, artists, albums, sources }: { loading: boolean; artists: Rank[]; albums: Rank[]; sources: ("U" | "S" | "E")[] }) {
  const add = useQuickAdd();
  const [art, setArt] = useState(false);
  // Album records give the embed URI for artwork mode.
  const albumItems = useApi(() => listItems({ type: "album", limit: 1000 })).data?.rows ?? [];
  const max = Math.max(1, ...artists.map((a) => a.hours));
  const top5 = albums.slice(0, 5);
  const saved = top5.map((a) => albumItems.find((i) => i.id === a.item_id)).filter((i): i is ItemSummary => !!i);
  return (
    <section className="flex flex-wrap gap-7 items-start" aria-label="Charts">
      <div className="flex-[1_1_380px] flex min-w-0 flex-col gap-3.5">
        <div className="flex justify-between items-baseline gap-3"><h2 className="section-title">Top artists</h2><SourceTag sources={sources} /></div>
        {loading ? <SkelRows n={6} /> : !artists.length ? <span className="text-muted">No plays in this period.</span> : artists.slice(0, 6).map((a, n) => (
          <div key={a.name} className="rank">
            <span className="font-mono text-faint text-xs">{n + 1}</span>
            <div className="min-w-0">
              <span className="font-semibold">{a.item_id ? <Link to={`/item/${a.item_id}`} className="text-inherit">{a.name}</Link> : a.name}</span>
              <span className="text-muted">{a.status ? `, ${STATUS[a.status].toLowerCase()}` : ", not in your library"}</span>
              <div className={`bar${a.item_id ? "" : " hot"}`} style={{ width: `${Math.round((a.hours / max) * 100)}%` }} />
            </div>
            <span className="font-mono text-text-2 whitespace-nowrap text-xs text-right">{hrs(a)}</span>
          </div>
        ))}
      </div>
      <div className="flex-[1.3_1_460px] flex min-w-0 flex-col gap-3.5">
        <div className="flex justify-between items-baseline gap-3"><h2 className="section-title">Top albums</h2>
          <span className="flex flex-wrap gap-2 items-center"><SourceTag sources={sources} /><button className="btn btn-secondary" aria-pressed={art} onClick={() => setArt(!art)}>{art ? "Hide artwork" : "Show artwork"}</button></span></div>
        {loading ? <SkelTiles n={5} /> : art ? <ArtTiles items={saved} meta={(it) => statusMeta(it)} />
          : <div className="tiles five">{top5.map((a) => {
              const meta = `${a.status ? STATUS[a.status] : ""}${a.rating ? ` ${stars(a.rating)}` : ""}, ${hrs(a)}`;
              return a.item_id
                ? <Link key={a.name} to={`/item/${a.item_id}`} className="card hover flex flex-col gap-1 text-inherit p-3.5">
                    <span className="font-mono text-faint text-[11px]">Album</span><span className="font-bold leading-[1.25]">{a.name}</span>
                    <span className="text-muted text-xs">{a.by}</span>
                    <span className={`font-mono text-xs ${a.status === "in_progress" ? "text-accent" : "text-muted"}`}>{meta}</span>
                  </Link>
                : <button key={a.name} className="card hover flex flex-col gap-1 text-text p-3.5 text-left cursor-pointer"
                    onClick={() => add({ suggest: { name: a.name, artist: a.by, type: "album", from: "plays" } })} aria-label={`Save ${a.name} to your backlog`}>
                    <span className="font-mono text-faint text-[11px]">Album, not saved</span><span className="font-bold leading-[1.25]">{a.name}</span>
                    <span className="text-muted text-xs">{a.by}</span><span className="font-mono text-accent text-xs">{hrs(a)}</span>
                  </button>;
            })}</div>}
      </div>
    </section>
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
  const rows = d ? [d.loved_but_forgotten[0], d.rated_vs_played[0], d.never_played_backlog[0]].filter(Boolean) : [];
  return (
    <div className="card flex flex-col gap-3">
      <div className="flex justify-between items-baseline gap-3"><h2 className="card-title">Your take vs your plays</h2>{take.data && <SourceTag sources={take.data.sources} />}</div>
      {take.loading ? <div className="skel h-[120px]" /> : take.error ? <ErrorBanner error={take.error} title="Couldn’t load this." onRetry={take.reload} />
        : !rows.length ? <span className="text-muted">Nothing stands out yet. Rate a few records you play.</span>
        : rows.map((t) => (
          <Link key={t.item.id} to={`/item/${t.item.id}`} className="row flex flex-col items-start gap-[3px] bg-surface-2">
            <span className="font-semibold text-text">{nameOf(t.item)}</span><span className="text-text-2 text-sm">{t.line}</span>
          </Link>))}
    </div>
  );
}

function BacklogCard() {
  const b = useApi(() => listItems({ in_backlog: true, sort: "backlog_order", limit: 200 }));
  const top3 = [...(b.data?.rows ?? [])].sort((x, y) => (x.priority ? PRIO_ORDER[x.priority] : 3) - (y.priority ? PRIO_ORDER[y.priority] : 3)).slice(0, 3);
  return (
    <div className="card flex flex-col gap-3">
      <div className="flex justify-between items-baseline gap-3"><h2 className="card-title">Backlog</h2><span className="font-mono text-muted text-xs">{b.data ? `${num(b.data.total)} waiting` : ""}</span></div>
      {b.data && !top3.length && <span className="text-muted">Nothing waiting. Paste a link to add one.</span>}
      {top3.map((it) => (
        <Link key={it.id} to={`/item/${it.id}`} className="flex justify-between items-baseline gap-3 text-text py-1">
          <span className="flex flex-col gap-0.5"><span className="font-semibold">{nameOf(it)}</span>
            <span className="text-muted text-xs">{TYPE[it.type]}{it.recommended_by ? `, from ${it.recommended_by}` : ""}</span></span>
          <span className={`text-xs ${it.priority === "high" ? "text-accent" : "text-muted"}`}>{it.status === "in_progress" ? "In progress" : it.priority ? PRIO[it.priority] : "Planned"}</span>
        </Link>
      ))}
    </div>
  );
}

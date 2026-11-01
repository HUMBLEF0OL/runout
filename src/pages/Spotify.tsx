// SC-12 From Spotify (R3, CAP-32, 33, 34): Spotify's Top items and recent plays, shown as Spotify provides them,
// with attribution. Source A: never mixed into insights, and none of your own listening stats here (D27).
// M9 (24 Oct 2026, user: "refactor the playlists and from spotify feature using taste + motion"): Spotify's #1 leads,
// 2-20 flow down two columns, then 21-40 and 41-50 open on request as their own sets (S10: each ends with Open Spotify);
// each row is marked in your library (check) or not (+); the set slides when the kind or range changes, rows rise in staggered, recent plays show Spotify's last 20.
import { useEffect, useRef, useState } from "react";
import { Link, Navigate, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { AnimatePresence, m, useReducedMotion } from "motion/react";
import { CheckIcon, PlusIcon } from "@phosphor-icons/react";
import * as api from "../api";
import type { SpotifyEntry } from "../api";
import { Attribution, Empty, ErrorBanner, Seg, SkelRows, spot, SpotifyLogo, useSlide } from "../components/ui";
import { Topbar } from "../components/Shell";
import { useQuickAdd } from "../components/QuickAddContext";
import { useApi, useOnline } from "../hooks/useApi";
import { useRelease } from "../hooks/useRelease";
import { ago } from "../lib/format";
import { ARRIVE, EASE, swap } from "../lib/motion";

type Kind = "tracks" | "artists";
type SRange = "4w" | "6m" | "1y";
const MAX = 20; // S10: at most 20 per set, the set ends with "Open Spotify"
const RANGES: [SRange, string][] = [["4w", "4 weeks"], ["6m", "6 months"], ["1y", "1 year"]];
const SPAN: Record<SRange, string> = { "4w": "the last 4 weeks", "6m": "the last 6 months", "1y": "the last year" };
/** A row rising in: capped so a 20-row set is in place within ~0.5 s. */
const rise = (reduce: boolean | null, i: number) => ({
  initial: reduce ? false : { opacity: 0, y: 8 }, animate: { opacity: 1, y: 0 },
  transition: { duration: 0.32, ease: EASE, delay: Math.min(i, 12) * 0.03 },
} as const);

function useAdd(kind: Kind) {
  const quickAdd = useQuickAdd();
  return (e: SpotifyEntry) => quickAdd({ suggest: { name: e.name, artist: kind === "tracks" ? e.artist : undefined, type: kind === "tracks" ? "track" : "artist", uri: e.uri, from: "spotify" } });
}

/** In the library: a check that opens the record. Not yet: a + that opens Quick add with the entry filled in. */
function Mark({ e, kind }: { e: SpotifyEntry; kind: Kind }) {
  const add = useAdd(kind);
  return e.in_library_id
    ? <span className="fs-in" title="In your library"><CheckIcon size={14} weight="bold" aria-hidden /><span className="sr-only">In your library</span></span>
    : <button className="icon-btn hm-save" onClick={() => add(e)} aria-label={`Add ${e.name} to backlog`}><PlusIcon size={14} weight="bold" /></button>;
}

/** Name of an entry: a whole-row link to the record when it's in the library. */
const Name = ({ e, className = "" }: { e: SpotifyEntry; className?: string }) =>
  e.in_library_id ? <Link to={`/item/${e.in_library_id}`} className={`hm-stretch text-text ${className}`}>{e.name}</Link> : <span className={className}>{e.name}</span>;

function Lead({ e, kind, range }: { e: SpotifyEntry; kind: Kind; range: SRange }) {
  const add = useAdd(kind);
  return (
    <div className="hm-lead fs-lead">
      <span className="text-muted text-xs font-semibold">Spotify's #1 {kind === "tracks" ? "track" : "artist"} for you, {SPAN[range]}</span>
      <div className="flex flex-col gap-1.5 min-w-0">
        <Name e={e} className="hm-lead-name" />
        {kind === "tracks" && <span className="text-text-2">{e.artist}</span>}
      </div>
      <div className="flex flex-wrap gap-2 items-center mt-auto">
        {e.in_library_id
          ? <><span className="fs-in" aria-hidden><CheckIcon size={14} weight="bold" /></span><span className="text-muted text-xs">In your library</span></>
          : <button className="btn btn-light" onClick={() => add(e)} aria-label={`Add ${e.name} to backlog`}>Add to backlog</button>}
      </div>
    </div>
  );
}

/** Ranked rows from `from` (1-based); each rank is read once, from the list (the visible number is hidden). */
function Ranks({ rows, from, kind }: { rows: SpotifyEntry[]; from: number; kind: Kind }) {
  const reduce = useReducedMotion();
  return (
    <ol className="hm-ranks" start={from}>{rows.map((e, i) => (
      <m.li key={e.uri} className="hm-rank fs-rank" {...rise(reduce, i)}>
        <span className="hm-rank-n" aria-hidden>{from + i}</span>
        <span className="min-w-0 flex flex-col gap-0.5">
          <Name e={e} className="truncate font-semibold" />
          {kind === "tracks" && <span className="text-muted text-xs truncate">{e.artist}</span>}
        </span>
        <span aria-hidden />
        <Mark e={e} kind={kind} />
      </m.li>))}
    </ol>
  );
}

/** Spotify's top items in sets of 20 (S10: each set ends with Open Spotify). The first set leads with #1; later sets
 *  open on request, and focus moves to the new set's heading. Keyed by kind and range, so a change starts over. */
function TopSets({ all, kind, range }: { all: SpotifyEntry[]; kind: Kind; range: SRange }) {
  const [sets, setSets] = useState(1);
  const heads = useRef<(HTMLHeadingElement | null)[]>([]);
  useEffect(() => { if (sets > 1) heads.current[sets - 1]?.focus(); }, [sets]);
  if (!all.length) return <Empty h="Nothing from Spotify yet" line="Spotify has no top items for this range. Try a longer one." />;
  const shown = Math.min(all.length, sets * MAX);
  const next = all.slice(shown, shown + MAX).length;
  const span = (s: number) => `${s * MAX + 1} to ${Math.min(all.length, (s + 1) * MAX)}`;
  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-wrap gap-x-8 gap-y-5 items-stretch">
        <Lead e={all[0]} kind={kind} range={range} />
        <Ranks rows={all.slice(1, MAX)} from={2} kind={kind} />
      </div>
      <Attribution name="your top items" />
      {Array.from({ length: sets - 1 }, (_, k) => k + 1).map((s) => (
        <section key={s} className="fs-set" aria-labelledby={`fs-set-${s}`}>
          <h3 id={`fs-set-${s}`} ref={(el) => { heads.current[s] = el; }} tabIndex={-1} className="fs-set-h">Ranks {span(s)}</h3>
          <Ranks rows={all.slice(s * MAX, (s + 1) * MAX)} from={s * MAX + 1} kind={kind} />
          <Attribution name="your top items" />
        </section>
      ))}
      <div className="flex flex-wrap gap-3 items-center justify-between">
        {next > 0 ? <button className="btn btn-secondary" onClick={() => setSets(sets + 1)}>Show {span(sets)}</button> : <span />}
        <span className="font-mono text-faint text-xs">{shown} of {all.length}</span>
      </div>
    </div>
  );
}

export default function Spotify() {
  const { r3 } = useRelease();
  const reduce = useReducedMotion();
  const kind = (useParams().tab ?? "tracks") as Kind;
  const [sp] = useSearchParams();
  const range = (sp.get("r") ?? "4w") as SRange;
  const nav = useNavigate();
  const online = useOnline();
  const status = useApi(api.getSpotifyStatus);
  const top = useApi(() => api.getTopItems(kind, range), [kind, range]);
  const recent = useApi(api.getRecentPlaysApi);
  // One direction for both controls: the kind (tracks, artists) and then the range, left to right.
  const order = ["tracks", "artists"].flatMap((k) => RANGES.map(([r]) => `${k}-${r}`));
  const [dir, slide] = useSlide(order, `${kind}-${range}`);
  if (!r3) return <Navigate to="/" replace />;
  const go = (k: Kind, r: SRange) => { slide(`${k}-${r}`); nav(`/spotify/${k}?r=${r}`); };

  const state = status.data?.state;
  const head = (
    <div className="page-head">
      <div className="flex flex-col gap-2">
        <div className="flex flex-wrap gap-2 items-center"><SpotifyLogo /><h1 className="page-title">From Spotify</h1></div>
        <span className="text-muted">Shown exactly as Spotify provides it. Not used in your insights.</span>
      </div>
    </div>
  );

  let body;
  if (status.loading) body = <SkelRows n={10} />;
  else if (state === "disconnected" || top.error?.code === "spotify_auth") body = (
    <Empty h="Spotify isn't connected" line="Connect to see your Spotify Top items and recent plays from every device. Everything else in Runout keeps working without it.">
      <Link className="btn btn-primary" to="/settings?tab=spotify">Connect Spotify in Settings</Link>
    </Empty>);
  else if (state === "reconnect") body = (
    <div className="banner warn">
      <span><b className="text-accent">Spotify needs you to sign in again.</b> <span className="text-text-2">Spotify asks every 6 months. Your data in Runout is unaffected.</span></span>
      <Link className="btn btn-primary s" to="/settings?tab=spotify">Reconnect in Settings</Link>
    </div>);
  else if (!online || top.error?.code === "offline") body = <Empty h="You're offline" line="Top items and recent plays come straight from Spotify, so they aren't stored on this PC. They return when you reconnect." />;
  else if (top.error && !top.loading) body = <ErrorBanner error={top.error} title="Couldn't read from Spotify." onRetry={() => { top.reload(); recent.reload(); }} />;
  else {
    const label = `Your top ${kind} on Spotify`;
    body = (
      <div className="fs-page lit flex flex-wrap gap-6 items-start" onPointerMove={spot}>
        <section className="card hm-charts flex-[2_1_620px] min-w-0" aria-labelledby="fs-top">
          <div className="flex flex-wrap justify-between items-center gap-4">
            <h2 id="fs-top" className="section-title">{label}</h2>
          </div>
          <div className="flex flex-wrap justify-between items-center gap-3">
            <Seg label="Top items" value={kind} onChange={(k) => go(k, range)} options={[["tracks", "Top tracks"], ["artists", "Top artists"]]} />
            <Seg label="Spotify time range" value={range} onChange={(r) => go(kind, r)} options={RANGES} />
          </div>
          <AnimatePresence mode="wait" initial={false}>
            <m.div key={`${kind}-${range}`} role="tabpanel" aria-label={label} className="min-w-0"
              initial={{ opacity: 0, x: dir === "from-right" ? 18 : -18 }} animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, transition: swap(reduce, { duration: 0.12 }) }} transition={swap(reduce, ARRIVE)}>
              {top.loading ? <SkelRows n={8} /> : <TopSets all={top.data ?? []} kind={kind} range={range} />}
            </m.div>
          </AnimatePresence>
        </section>
        <aside className="fs-side flex-[1_1_300px] min-w-0">
          <section className="card flex flex-col gap-3" aria-labelledby="fs-recent">
            <div className="flex flex-col gap-1">
              <h2 id="fs-recent" className="card-title">Recently played on Spotify</h2>
              <span className="text-muted text-sm leading-[1.45]">Your last finished plays on any device. Catch something you heard on the go.</span>
            </div>
            {recent.error ? <ErrorBanner error={recent.error} onRetry={recent.reload} />
              : recent.loading ? <SkelRows n={4} />
              : recent.data?.length ? <ol className="fs-recent">{recent.data.slice(0, MAX).map((x, i) => (
                <m.li key={x.uri + x.played_at} className="hm-rank fs-rank" {...rise(reduce, i)}>
                  <span className="min-w-0 flex flex-col gap-0.5">
                    <Name e={x} className="truncate font-semibold" />
                    <span className="text-muted text-xs truncate">{x.artist}</span>
                  </span>
                  <span className="font-mono text-faint text-xs whitespace-nowrap">{ago(x.played_at).toLowerCase()}</span>
                  <Mark e={x} kind="tracks" />
                </m.li>))}
              </ol> : <span className="text-muted">No recent plays on Spotify.</span>}
            <Attribution name="your recent plays" />
          </section>
        </aside>
      </div>
    );
  }
  return <><Topbar />{head}{body}</>;
}

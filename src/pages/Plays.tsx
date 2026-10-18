// SC-08 Recent plays: plays by day, desktop logger control, unmatched plays (CAP-18, 20, 21).
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import * as api from "../api";
import { playSources } from "../components/logic";
import type { Play, Range, Unmatched } from "../api";
import { Dialog, Dots, Empty, ErrorBanner, FreshnessNote, PageHead, PlayRow, Seg, SkelRows, SourceTag, useToast } from "../components/ui";
import { Topbar } from "../components/Shell";
import { useQuickAdd } from "../components/QuickAddContext";
import { useApi } from "../hooks/useApi";
import { daysSince, day, nameOf, pl, sub } from "../lib/format";

type Tab = "all" | "unrated" | "missing";
const RANGES: Range[] = ["30d", "12m", "all"];
const PER_DAY = 12;

const dayLabel = (p: Play) => {
  const d = daysSince(p.ended_at_local);
  const base = d === 0 ? "Today" : d === 1 ? "Yesterday" : day(p.ended_at_local);
  return p.source === "export" ? `${base}, phone plays from your Spotify export` : base;
};

function DayGroup({ label, plays }: { label: string; plays: Play[] }) {
  const [all, setAll] = useState(false);
  const nav = useNavigate();
  const quickAdd = useQuickAdd();
  const shown = all ? plays : plays.slice(0, PER_DAY);
  return (
    <div className="flex flex-col gap-1.5">
      <h2 className="text-muted text-sm font-bold">{label}</h2>
      <div className="card pad-s"><div className="list">
        {shown.map((p) => <PlayRow key={p.id} p={p} wide onRate={(x) => nav(`/item/${x.item!.id}`)}
          onSave={(x) => quickAdd({ suggest: { name: x.track_name, artist: x.artist_name, type: "track", uri: x.spotify_uri, from: "plays" } })} />)}
      </div></div>
      {plays.length > PER_DAY && (
        <div className="flex flex-wrap gap-2 items-center justify-between">
          <span className="font-mono text-faint text-xs">{shown.length} of {plays.length} shown</span>
          <button className="btn btn-secondary s" onClick={() => setAll(!all)}>{all ? "Show fewer" : "Show all"}</button>
        </div>
      )}
    </div>
  );
}

function Logger() {
  const st = useApi(api.getLoggerStatus);
  const [busy, setBusy] = useState(false);
  const act = (fn: () => Promise<unknown>) => { setBusy(true); fn().finally(() => setBusy(false)); };
  const s = st.data?.state;
  return (
    <div className="card flex flex-col gap-3">
      <h2 className="card-title">Desktop logger</h2>
      {st.error || s === "error"
        ? <ErrorBanner error={st.error ?? { code: "internal", message: "Windows isn't sharing media info. Restart Spotify, then try again." }} title="Logger stopped." onRetry={() => act(api.resumeLogger)} />
        : <>
          <span className="text-text-2 leading-[1.5]">
            {s === "paused" ? "Paused. Nothing is logged until you resume." : s === "off" ? "The logger is off. Nothing on this PC is logged." : "Logging Spotify on this PC. A play counts after 30 seconds."}
          </span>
          <div className="flex flex-wrap gap-2 items-center">
            {busy ? <Dots /> : s === "paused" ? <button className="btn btn-light" onClick={() => act(api.resumeLogger)}>Resume</button>
              : s === "off" ? <button className="btn btn-light" onClick={() => act(() => api.setLogger({ on: true }))}>Turn on the logger</button>
              : <><button className="btn btn-light" onClick={() => act(() => api.pauseLogger(60))}>Pause for 1 hour</button>
                <button className="btn btn-secondary" onClick={() => act(() => api.pauseLogger(null))}>Pause</button></>}
          </div>
        </>}
      <span className="text-muted text-xs leading-[1.45]">Private sessions can't be detected on the PC. Pause before one if you don't want it logged.</span>
    </div>
  );
}

function UnmatchedCard() {
  const um = useApi(api.listUnmatched);
  const toast = useToast();
  const [choosing, setChoosing] = useState<Unmatched | null>(null);
  const link = (u: Unmatched, itemId: number) => api.linkPlay(u.event_id, itemId).then(
    () => { setChoosing(null); toast(`Linked. ${pl(u.plays, "play")} now count for this record`); },
    (e) => toast(api.toAppError(e).message));
  if (um.error) return <ErrorBanner error={um.error} title="Couldn't load unmatched plays." onRetry={um.reload} />;
  if (!um.data?.length) return null;
  return (
    <div className="card flex flex-col gap-3">
      <div className="flex justify-between items-baseline gap-3"><h2 className="card-title">Unmatched plays</h2><span className="font-mono text-accent text-xs">{um.data.length}</span></div>
      <span className="text-text-2 text-sm leading-[1.5]">More than one record in your library could be the track. Pick the right one.</span>
      {um.data.map((u) => (
        <div key={u.norm_key} className="flex flex-col gap-1.5 px-3 py-2.5 rounded-control bg-surface-2">
          <span className="font-semibold">{u.track_name}, {u.artist_name}</span>
          <span className="text-muted text-xs">{u.detail}. {pl(u.plays, "play")} waiting.</span>
          <div className="flex flex-wrap gap-2 items-center">
            {u.candidates[0] && <button className="btn btn-light s" onClick={() => link(u, u.candidates[0].id)} aria-label={`${u.suggestion} for ${u.track_name}`}>{u.suggestion}</button>}
            <button className="btn btn-secondary s" onClick={() => setChoosing(u)} aria-label={`Choose another record for ${u.track_name}`}>Choose another</button>
          </div>
        </div>
      ))}
      {choosing && (
        <Dialog title={`Which record is ${choosing.track_name}?`} onClose={() => setChoosing(null)}>
          <div className="list">
            {choosing.candidates.map((it) => (
              <button key={it.id} className="row hover grid-cols-[minmax(0,1fr)_auto] text-left bg-transparent border-0 text-inherit cursor-pointer" onClick={() => link(choosing, it.id)}>
                <span className="flex flex-col gap-0.5"><b>{nameOf(it)}</b><span className="text-muted text-xs">{it.type === "album" ? "Album" : "Track"}, {sub(it)}</span></span>
                <span className="text-text-2 text-sm">Link</span>
              </button>
            ))}
          </div>
        </Dialog>
      )}
    </div>
  );
}

export default function Plays() {
  const tab = (useParams().tab ?? "all") as Tab;
  const nav = useNavigate();
  const [ri, setRi] = useState(0);
  const range = RANGES[ri];
  const plays = useApi(() => api.listPlays({ tab, range, offset: 0, limit: 500 }), [tab, range]);
  const unrated = useApi(() => api.listPlays({ tab: "unrated", range: "all", offset: 0, limit: 0 }));
  const missing = useApi(() => api.listPlays({ tab: "missing", range: "all", offset: 0, limit: 0 }));
  const fresh = useApi(api.getFreshness);
  const logger = useApi(api.getLoggerStatus);

  const groups = new Map<string, Play[]>();
  plays.data?.rows.forEach((p) => { const k = dayLabel(p); const g = groups.get(k); if (g) g.push(p); else groups.set(k, [p]); });
  const n = (s: typeof unrated) => (s.data ? ` ${s.data.total}` : "");

  let content;
  if (plays.loading) content = <SkelRows n={10} />;
  else if (plays.error) content = <ErrorBanner error={plays.error} title="Couldn't load your plays." onRetry={plays.reload} />;
  else if (!plays.data?.rows.length) content = tab === "all"
    ? <Empty h="No plays yet" line="Turn on the desktop logger, or import your Spotify history to see plays from every device.">
        <div className="flex flex-wrap gap-2 items-center"><button className="btn btn-primary" onClick={() => api.setLogger({ on: true })}>Turn on the logger</button><Link className="btn btn-secondary" to="/import/1">Import history</Link></div>
      </Empty>
    : <Empty h="Nothing here" line={tab === "unrated" ? "Every play you've counted has your rating." : "Every play is linked to a record in your library."}>
        <Link className="btn btn-secondary" to="/plays/all">See all plays</Link>
      </Empty>;
  else content = <>
    {[...groups].map(([label, ps]) => <DayGroup key={label} label={label} plays={ps} />)}
    {ri < RANGES.length - 1 && (
      <div className="card pad-s flex justify-between items-baseline gap-3 px-4 py-3">
        <span className="text-text-2">Showing {pl(plays.data.total, "play")} from the last {range === "30d" ? "30 days" : "12 months"}.</span>
        <button className="btn btn-secondary s" onClick={() => setRi(ri + 1)}>Show earlier days</button>
      </div>
    )}
  </>;

  return (
    <>
      <Topbar />
      <div className="flex flex-wrap gap-7 items-start">
        <section className="flex-[1.6_1_560px] min-w-0 flex flex-col gap-3">
          <PageHead title="Recent plays">
            <Seg label="Which plays" value={tab} onChange={(t) => nav(`/plays/${t}`)}
              options={[["all", "All"], ["unrated", `Not rated${n(unrated)}`], ["missing", `Not in library${n(missing)}`]]} />
          </PageHead>
          <span className="flex flex-wrap gap-1.5 items-center">{plays.data && plays.data.rows.length > 0 && <SourceTag sources={playSources(plays.data.rows)} />}<FreshnessNote freshness={fresh.data} loggerState={logger.data?.state} /></span>
          {content}
        </section>
        <aside className="flex-[1_1_320px] min-w-0 flex flex-col gap-3">
          <Logger />
          <UnmatchedCard />
        </aside>
      </div>
    </>
  );
}

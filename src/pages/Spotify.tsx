// SC-12 From Spotify (R3, CAP-32, 33, 34): Spotify's Top items and recent plays, shown as Spotify provides them,
// with attribution. Source A: never mixed into insights, and none of your own listening stats here (D27).
import { Link, Navigate, useNavigate, useParams, useSearchParams } from "react-router-dom";
import * as api from "../api";
import type { SpotifyEntry } from "../api";
import { Attribution, Empty, ErrorBanner, Seg, SkelRows, SpotifyLogo } from "../components/ui";
import { Topbar } from "../components/Shell";
import { useQuickAdd } from "../components/QuickAddContext";
import { useApi, useOnline } from "../hooks/useApi";
import { useRelease } from "../hooks/useRelease";
import { ago } from "../lib/format";

type Kind = "tracks" | "artists";
type SRange = "4w" | "6m" | "1y";
const MAX = 20; // S10: at most 20 per set, the set ends with "Open Spotify"

function Action({ e, kind }: { e: SpotifyEntry; kind: Kind }) {
  const quickAdd = useQuickAdd();
  if (e.in_library_id) return <Link to={`/item/${e.in_library_id}`} className="text-muted text-xs">In your library</Link>;
  return (
    <button className="btn btn-secondary s" aria-label={`Add ${e.name} to backlog`}
      onClick={() => quickAdd({ suggest: { name: e.name, artist: kind === "tracks" ? e.artist : undefined, type: kind === "tracks" ? "track" : "artist", uri: e.uri, from: "spotify" } })}>
      Add to backlog
    </button>
  );
}

export default function Spotify() {
  const { r3 } = useRelease();
  const kind = (useParams().tab ?? "tracks") as Kind;
  const [sp] = useSearchParams();
  const range = (sp.get("r") ?? "4w") as SRange;
  const nav = useNavigate();
  const online = useOnline();
  const status = useApi(api.getSpotifyStatus);
  const top = useApi(() => api.getTopItems(kind, range), [kind, range]);
  const recent = useApi(api.getRecentPlaysApi);
  if (!r3) return <Navigate to="/" replace />;

  const state = status.data?.state;
  const head = (
    <div className="page-head">
      <div className="flex flex-col gap-2">
        <div className="flex flex-wrap gap-2 items-center"><SpotifyLogo /><h1 className="page-title">From Spotify</h1></div>
        <span className="text-muted">Shown exactly as Spotify provides it. Not used in your insights.</span>
      </div>
      {state === "connected" && (
        <div className="flex flex-wrap gap-2 items-center">
          <Seg label="Top items" value={kind} onChange={(k) => nav(`/spotify/${k}?r=${range}`)} options={[["tracks", "Top tracks"], ["artists", "Top artists"]]} />
          <Seg label="Spotify time range" value={range} onChange={(r) => nav(`/spotify/${kind}?r=${r}`)} options={[["4w", "4 weeks"], ["6m", "6 months"], ["1y", "1 year"]]} />
        </div>
      )}
    </div>
  );

  let body;
  if (status.loading) body = <SkelRows n={10} />;
  else if (state === "disconnected" || top.error?.code === "spotify_auth") body = (
    <Empty h="Spotify isn't connected" line="Connect to see your Spotify Top items and recent plays from every device. Everything else in Runout keeps working without it.">
      <Link className="btn btn-primary" to="/settings">Connect Spotify in Settings</Link>
    </Empty>);
  else if (state === "reconnect") body = (
    <div className="banner warn">
      <span><b className="text-accent">Spotify needs you to sign in again.</b> <span className="text-text-2">Spotify asks every 6 months. Your data in Runout is unaffected.</span></span>
      <Link className="btn btn-primary s" to="/settings">Reconnect in Settings</Link>
    </div>);
  else if (!online || top.error?.code === "offline") body = <Empty h="You're offline" line="Top items and recent plays come straight from Spotify, so they aren't stored on this PC. They return when you reconnect." />;
  else if (top.loading) body = <SkelRows n={10} />;
  else if (top.error) body = <ErrorBanner error={top.error} title="Couldn't read from Spotify." onRetry={() => { top.reload(); recent.reload(); }} />;
  else {
    const rows = top.data!.slice(0, MAX);
    body = (
      <div className="flex flex-wrap gap-7 items-start">
        <section className="flex-[1.5_1_520px] min-w-0 flex flex-col gap-3.5" aria-label={`Your top ${kind} on Spotify`}>
          {rows.length ? <div className="card pad-s"><div className="list">
            {rows.map((t, i) => (
              <div key={t.uri} className="row grid-cols-[28px_minmax(0,1fr)_auto]">
                <span className="font-mono text-faint text-xs">{i + 1}</span>
                <span className="flex flex-col gap-0.5 min-w-0"><span className="truncate font-semibold">{t.name}</span>{kind === "tracks" && <span className="text-muted text-xs">{t.artist}</span>}</span>
                <Action e={t} kind={kind} />
              </div>
            ))}
          </div></div> : <Empty h="Nothing from Spotify yet" line="Spotify has no top items for this range. Try a longer one." />}
          <div className="flex flex-wrap gap-2 items-center justify-between">
            <Attribution name="your top items" />
            {top.data!.length > 0 && <span className="font-mono text-faint text-xs">{rows.length} of {top.data!.length}</span>}
          </div>
        </section>
        <aside className="flex-[1_1_320px] min-w-0 flex flex-col gap-5">
          <section className="card flex flex-col gap-3" aria-labelledby="fs-recent">
            <h2 id="fs-recent" className="card-title">Recently played on Spotify</h2>
            <span className="text-muted text-sm leading-[1.45]">Spotify's last finished plays from all your devices. Catch something you heard on the go.</span>
            {recent.error ? <ErrorBanner error={recent.error} onRetry={recent.reload} />
              : recent.loading ? <SkelRows n={4} />
              : recent.data!.length ? recent.data!.slice(0, 8).map((x) => (
                <div key={x.uri + x.played_at} className="flex justify-between items-center gap-3 py-1">
                  <span className="flex flex-col gap-0.5 min-w-0"><span className="truncate font-semibold">{x.name}</span><span className="text-muted text-xs">{x.artist}, {ago(x.played_at).toLowerCase()}</span></span>
                  <span className="flex flex-wrap gap-2 items-center flex-none"><Action e={x} kind="tracks" /></span>
                </div>)) : <span className="text-muted">No recent plays on Spotify.</span>}
            <Attribution name="your recent plays" />
          </section>
        </aside>
      </div>
    );
  }
  return <><Topbar />{head}{body}</>;
}

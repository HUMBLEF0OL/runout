// SC-11 Playlists: read-only check-up from the export (R2, CAP-22) and Studio live from Spotify (R3, CAP-29, 30).
// Studio suggests cuts from your own ratings and duplicates only; it never shows listening stats (D27).
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Link, Navigate, useNavigate, useParams, useSearchParams } from "react-router-dom";
import * as api from "../api";
import type { AppError, Draft, StudioDiff } from "../api";
import { Attribution, Dialog, Dots, Empty, ErrorBanner, Seg, SkelRows, SourceTag, Stat, Stars, useToast } from "../components/ui";
import { Topbar } from "../components/Shell";
import { useApi, useOnline } from "../hooks/useApi";
import { useRelease } from "../hooks/useRelease";
import { cutReason, keepList } from "../lib/studio";
import { day, pl } from "../lib/format";

type Tab = "checkup" | "studio";

function Layout({ tab, list, title, subline, children }: { tab: Tab; list: ReactNode; title: ReactNode; subline?: ReactNode; children: ReactNode }) {
  const { r3 } = useRelease();
  const nav = useNavigate();
  const [sp] = useSearchParams();
  const fresh = useApi(api.getFreshness);
  const exp = fresh.data?.export_latest_event;
  return (
    <>
      <Topbar />
      <div className="flex flex-wrap gap-7 items-start">
        <nav className="flex flex-col gap-1.5 flex-[0_0_240px]" aria-label="Playlists">
          <h2 className="text-muted text-sm font-bold px-2">{tab === "studio" ? "Your playlists on Spotify" : exp ? `From your export of ${day(exp)}` : "From your export"}</h2>
          {list}
        </nav>
        <div className="flex-[1_1_620px] min-w-0 flex flex-col gap-3.5">
          <div className="page-head">
            <div className="flex flex-col gap-1"><h1 className="page-title">{title}</h1>{subline && <span className="text-muted">{subline}</span>}</div>
            {r3 && <Seg label="Mode" value={tab} onChange={(t) => nav(`/playlists/${t}?${sp}`)}
              options={[["checkup", exp ? `Check-up (export of ${day(exp)})` : "Check-up"], ["studio", "Studio (live from Spotify)"]]} />}
          </div>
          {children}
        </div>
      </div>
    </>
  );
}

const PlaylistLink = ({ tab, name, sel, children }: { tab: Tab; name: string; sel: boolean; children: ReactNode }) => (
  <Link to={`/playlists/${tab}?p=${encodeURIComponent(name)}`} className={`row grid-cols-[minmax(0,1fr)] ${sel ? "bg-surface-2" : ""}`} aria-current={sel ? "page" : undefined}>
    <span className="flex flex-col gap-0.5 min-w-0"><span className="truncate font-semibold text-text">{name}</span><span className="text-muted text-xs">{children}</span></span>
  </Link>
);

/* ------------------------------------------------------------------ check-up (read-only) */
function Checkup() {
  const { r3 } = useRelease();
  const [sp] = useSearchParams();
  const pls = useApi(api.listExportedPlaylists);
  const sel = pls.data?.find((p) => p.name === sp.get("p")) ?? pls.data?.[0];
  const cu = useApi(() => (sel ? api.getPlaylistCheckup(sel.id) : Promise.resolve(null)), [sel?.id]);
  const list = pls.data?.map((p) => (
    <PlaylistLink key={p.id} tab="checkup" name={p.name} sel={p === sel}>
      {pl(p.tracks, "track")}{p.dupes > 0 && <>, <span className="text-accent">{pl(p.dupes, "duplicate")}</span></>}{p.low > 0 && <>, <span className="text-accent">{p.low} rated low</span></>}
    </PlaylistLink>
  ));
  const wrap = (body: ReactNode) => <Layout tab="checkup" list={list} title={sel?.name ?? "Playlists"} subline={sel && `Your playlist, ${pl(sel.tracks, "track")}`}>{body}</Layout>;
  if (pls.loading || cu.loading) return wrap(<SkelRows n={8} />);
  if (pls.error || cu.error) return wrap(<ErrorBanner error={pls.error ?? cu.error} title="Couldn't load this playlist." onRetry={() => { pls.reload(); cu.reload(); }} />);
  if (!sel || !cu.data) return wrap(<Empty h="No playlists in your export" line="Import an Account data export from Spotify to check your playlists."><Link className="btn btn-secondary" to="/import/1">Import</Link></Empty>);
  const t = cu.data.tracks;
  const never = t.filter((x) => x.never_played).length;
  return wrap(<>
    <p className="text-text-2 m-0">Read-only. Shows the playlist as it was in your export. To change it, use Studio{r3 ? "" : " (Release 3)"}.</p>
    <SourceTag sources={["E", "U", "S"]} />
    <div className="stats">
      <Stat k="Duplicates" v={sel.dupes} d="Same track twice" act={sel.dupes > 0} />
      <Stat k="Rated 2 or lower" v={sel.low} d="By you" act={sel.low > 0} />
      <Stat k="Never played" v={never} d="In your plays" />
      <Stat k="Tracks" v={t.length} d="In the export" />
    </div>
    <div className="card pad-s"><div className="list">
      {t.map((x) => {
        const low = x.rating != null && x.rating <= 2;
        return (
          <div key={x.position} className="row grid-cols-[28px_minmax(0,1fr)_80px_160px]">
            <span className="font-mono text-faint text-xs">{x.position}</span>
            <span className="flex flex-col gap-0.5"><span className="font-semibold">{x.name}</span><span className="text-muted text-xs">{x.artist}</span></span>
            <span>{x.rating ? <Stars r={x.rating} /> : <span className="text-faint text-xs">Not rated</span>}</span>
            <span className={`text-xs ${x.dupe || low ? "text-accent" : "text-muted"}`}>{x.dupe ? "Duplicate" : low ? "Rated low" : x.never_played ? "Never played" : ""}</span>
          </div>
        );
      })}
    </div></div>
    {r3 && <Link className="btn btn-primary self-start" to={`/playlists/studio?p=${encodeURIComponent(sel.name)}`}>Open in Studio</Link>}
  </>);
}

/* ------------------------------------------------------------------ Studio (live, R3) */
function Studio() {
  const [sp] = useSearchParams();
  const online = useOnline();
  const toast = useToast();
  const pls = useApi(api.listSpotifyPlaylists);
  const sel = pls.data?.find((p) => p.name === sp.get("p")) ?? pls.data?.[0];
  const draft = useApi(() => (sel ? api.loadStudioDraft(sel.uri) : Promise.resolve(null)), [sel?.uri]);
  const last = useApi(() => (sel ? api.getLastApply(sel.uri) : Promise.resolve(null)), [sel?.uri]);
  const suggested = useMemo(() => new Set((draft.data?.tracks ?? []).flatMap((t, i, all) => (cutReason(t, all) ? [i] : []))), [draft.data]);
  const [cut, setCut] = useState<Set<number>>(suggested);
  const [confirm, setConfirm] = useState<{ d: Draft; diff: StudioDiff } | null>(null);
  const [busy, setBusy] = useState(false);
  // Which call failed decides the banner: a failed apply/undo may have changed the playlist (TSD §5.2).
  const [fail, setFail] = useState<{ op: "preview" | "apply" | "undo"; e: AppError }>();
  // A freshly loaded draft starts from the suggestions. Every write reloads the draft and the undo
  // record (api.ts), failed or not, so the error is kept until another playlist is picked or retried.
  useEffect(() => { setCut(suggested); }, [suggested]);
  useEffect(() => { setFail(undefined); }, [sel?.uri]);

  const list = pls.data?.map((p) => <PlaylistLink key={p.uri} tab="studio" name={p.name} sel={p === sel}>{pl(p.tracks, "track")}</PlaylistLink>);
  const wrap = (body: ReactNode) => <Layout tab="studio" list={list} title={sel?.name ?? "Studio"} subline={sel && `Your playlist, ${pl(sel.tracks, "track")}`}>{body}</Layout>;
  const err = pls.error ?? draft.error;
  if (err?.code === "spotify_auth" || err?.code === "spotify_forbidden") return wrap(
    <Empty h="Connect Spotify to use Studio" line="Studio edits your own playlists on Spotify. The check-up from your export works without connecting.">
      <Link className="btn btn-primary" to="/settings">Connect Spotify in Settings</Link>
    </Empty>);
  if (!online || err?.code === "offline") return wrap(<Empty h="Studio needs a connection" line="Your draft is kept. Reconnect to read the playlist from Spotify and apply changes." />);
  if (pls.loading || draft.loading) return wrap(<><span className="text-muted">Reading the playlist from Spotify…</span><SkelRows n={8} /></>);
  if (err) return wrap(<ErrorBanner error={err} title="Couldn't read the playlist from Spotify." onRetry={() => { pls.reload(); draft.reload(); }} />);
  if (!sel || !draft.data) return wrap(<Empty h="No playlists of yours on Spotify" line="Studio edits playlists you own. Make one in Spotify, then come back." />);

  const d = draft.data;
  const changes = d.tracks.map((t, i) => ({ t, i })).filter(({ i }) => cut.has(i));
  const toggle = (i: number, v: "keep" | "cut") => setCut((s) => { const n = new Set(s); if (v === "cut") n.add(i); else n.delete(i); return n; });
  const openConfirm = () => { setBusy(true); api.previewStudioApply(d, keepList(d.tracks, cut)).then((diff) => setConfirm({ d, diff }), (e) => setFail({ op: "preview", e: api.toAppError(e) })).finally(() => setBusy(false)); };
  // Rejections: api.ts has already reloaded draft and last (a kept undo shows its banner).
  const undo = () => { setFail(undefined); api.undoLastApply(sel.uri).then(() => toast(`${sel.name} restored to how it was before you applied`), (e) => setFail({ op: "undo", e: api.toAppError(e) })); };
  const apply = () => {
    setBusy(true); setFail(undefined);
    api.applyStudioDraft(d, keepList(d.tracks, cut)).then(() => {
      setConfirm(null);
      toast(`${pl(changes.length, "change")} applied to ${sel.name}. Undo is available for 30 days.`, undo);
    }, (e) => { setConfirm(null); setFail({ op: "apply", e: api.toAppError(e) }); }).finally(() => setBusy(false));
  };
  const banner = !fail ? null
    : fail.e.code === "conflict" && fail.op === "apply"
      ? <ErrorBanner error={{ ...fail.e, message: "Someone edited it since you started. Reload to see the new version." }} title="This playlist changed on Spotify." onRetry={() => { setFail(undefined); draft.reload(); }} />
      : fail.e.code === "conflict" ? <ErrorBanner error={fail.e} title="This playlist changed on Spotify." />
      : fail.op === "undo" ? <ErrorBanner error={fail.e} title="Undo didn't finish; the playlist may be partly restored. Try Undo again." />
      : fail.op === "apply" && fail.e.code !== "validation"
        ? <ErrorBanner error={fail.e} title="This may have been partly applied on Spotify. If Undo is offered below, it puts the playlist back." />
        : <ErrorBanner error={fail.e} title="Nothing was changed on Spotify." />;

  return wrap(<>
    <Attribution uri={sel.uri} name={sel.name}><span className="text-text-2 text-sm">Tracks from Spotify, read just now, with your own ratings and tags. No listening data is used here.</span></Attribution>
    {banner}
    {last.data && (
      <div className="banner">
        <span><b>Applied {day(last.data.applied_at)}.</b> <span className="text-text-2">Undo is available until {day(last.data.expires_at)}.</span></span>
        <button className="btn btn-secondary s" onClick={undo}>Undo last apply</button>
      </div>
    )}
    <div className="flex flex-wrap gap-7 items-start">
      <div className="card pad-s flex-[1.5_1_520px] min-w-0 flex flex-col gap-3.5"><div className="list">
        {d.tracks.map((t, i) => {
          const isCut = cut.has(i); const why = cutReason(t, d.tracks);
          return (
            <div key={i} className={`row grid-cols-[24px_minmax(160px,1fr)_64px_auto] ${isCut ? " sel" : ""}`}>
              <span className="font-mono text-faint text-xs">{i + 1}</span>
              <span className="flex flex-col gap-0.5 min-w-0">
                <span className={`font-semibold ${isCut ? "line-through text-muted" : ""}`}>{t.name}</span>
                <span className="text-muted text-xs">{t.artist}{why && <span className="text-accent">, {why}</span>}</span>
              </span>
              <span>{t.rating ? <Stars r={t.rating} /> : null}</span>
              <Seg label={`Keep or cut ${t.name}`} radio value={isCut ? "cut" : "keep"} onChange={(v) => toggle(i, v)} options={[["keep", "Keep"], ["cut", "Cut"]]} />
            </div>
          );
        })}
      </div></div>
      <aside className="card flex-[0_1_300px] min-w-0 flex flex-col gap-3">
        <h2 className="card-title">Your changes</h2>
        <span className="text-muted text-sm">Draft only. Nothing changes on Spotify until you apply.</span>
        {changes.length ? changes.map(({ t, i }) => (
          <div key={i} className="flex flex-col gap-0.5 px-3 py-2.5 rounded-control bg-surface-2">
            <span className="text-accent text-xs font-bold">{t.dupe_of ? "Remove duplicate" : "Remove"}</span><span>{t.name}</span>
          </div>)) : <span className="text-muted">No changes yet.</span>}
        <button className="btn btn-primary l" disabled={!changes.length || busy} onClick={openConfirm}>{busy && !confirm ? <Dots /> : `Apply ${pl(changes.length, "change")} to Spotify`}</button>
        <span className="text-muted text-xs leading-[1.45]">Runout checks the playlist hasn't changed on Spotify first. You can undo for 30 days.</span>
        <button className="btn btn-secondary" onClick={() => { setCut(new Set(suggested)); toast("Draft discarded"); }}>Discard draft</button>
      </aside>
    </div>
    {confirm && (
      <Dialog title={`Apply changes to ${sel.name}?`} onClose={() => setConfirm(null)}
        footer={<><button className="btn btn-secondary" onClick={() => setConfirm(null)}>Cancel</button>
          <button className="btn btn-primary" onClick={apply} disabled={busy}>{busy ? <Dots /> : `Apply ${pl(confirm.diff.removed + confirm.diff.added + confirm.diff.reordered, "change")}`}</button></>}>
        <span className="text-text-2">Runout reads the playlist from Spotify again first. If it changed since you started, nothing is applied.</span>
        <div className="card pad-s"><div className="list">
          {changes.map(({ t, i }) => (
            <div key={i} className="row grid-cols-[130px_minmax(0,1fr)]">
              <span className="text-accent text-xs font-bold">{t.dupe_of ? "Remove duplicate" : "Remove"}</span><span>{t.name}, {t.artist}</span>
            </div>))}
        </div></div>
        <span className="help">You can undo this for 30 days.</span>
      </Dialog>
    )}
  </>);
}

export default function Playlists() {
  const tab = useParams().tab ?? "checkup";
  const { r3 } = useRelease();
  if (tab === "studio" && !r3) return <Navigate to="/playlists/checkup" replace />;
  return tab === "studio" ? <Studio /> : <Checkup />;
}

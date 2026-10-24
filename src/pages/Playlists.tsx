// SC-11 Playlists: read-only check-up from the export (R2, CAP-22) and Studio live from Spotify (R3, CAP-29, 30).
// Studio suggests cuts from your own ratings and duplicates only; it never shows listening stats (D27).
// M9 (24 Oct 2026, user: "refactor the playlists and from spotify feature using taste + motion"): each exported playlist
// shows its share of issues as a bar; the check-up draws where the issues sit along the playlist and can show only those;
// Studio keeps its draft panel in view, a kept/cut meter springs with every choice, and changes slide in and out.
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { AnimatePresence, m, useReducedMotion } from "motion/react";
import { Link, Navigate, useLocation, useNavigate, useNavigationType, useParams, useSearchParams } from "react-router-dom";
import * as api from "../api";
import type { AppError, Draft, StudioDiff } from "../api";
import { Attribution, Dialog, Dots, Empty, ErrorBanner, Seg, SkelRows, SourceTag, spot, Stars, Stat, TabPanel, useSlide, useToast } from "../components/ui";
import { Topbar } from "../components/Shell";
import { useApi, useOnline } from "../hooks/useApi";
import { useRelease } from "../hooks/useRelease";
import { cutReason, keepList, stripBg } from "../lib/studio";
import { day, pl } from "../lib/format";
import { EASE, LAND, swap } from "../lib/motion";

type Tab = "checkup" | "studio";

function Layout({ tab, list, title, subline, children }: { tab: Tab; list: ReactNode; title: ReactNode; subline?: ReactNode; children: ReactNode }) {
  const { r3 } = useRelease();
  const nav = useNavigate();
  const [sp] = useSearchParams();
  const fresh = useApi(api.getFreshness);
  const exp = fresh.data?.export_latest_event;
  // Check-up and Studio are separate pages, so arrowing the Mode switch remounts it: put focus back on the new tab
  // (M7 installed run: focus fell to <body>).
  const head = useRef<HTMLDivElement>(null);
  const loc = useLocation();
  const how = useNavigationType();
  useEffect(() => { // only right after the switch itself: Back/Forward or a reload replays the history state
    if (how !== "POP" && (loc.state as { modeFocus?: boolean } | null)?.modeFocus) head.current?.querySelector<HTMLElement>('[aria-label="Mode"] [aria-selected="true"]')?.focus();
  }, [loc.key, loc.state]);
  // the two modes are separate pages, so the slide direction travels in the history state (Check-up is left of Studio)
  const dir = (loc.state as { slide?: "from-right" | "from-left" } | null)?.slide ?? "from-right";
  return (
    <>
      <Topbar />
      <div className="py-page lit flex flex-wrap gap-7 items-start" onPointerMove={spot}>
        <nav className="flex flex-col gap-1.5 flex-[0_0_240px]" aria-label="Playlists">
          <h2 className="text-muted text-sm font-bold px-2">{tab === "studio" ? "Your playlists on Spotify" : exp ? `From your export of ${day(exp)}` : "From your export"}</h2>
          {list}
        </nav>
        <div className="flex-[1_1_620px] min-w-0 flex flex-col gap-3.5">
          <div className="page-head" ref={head}>
            <div className="flex flex-col gap-1"><h1 className="page-title">{title}</h1>{subline && <span className="text-muted">{subline}</span>}</div>
            {r3 && <Seg label="Mode" value={tab} onChange={(t) => nav(`/playlists/${t}?${sp}`, { state: { modeFocus: true, slide: t === "studio" ? "from-right" : "from-left" } })}
              options={[["checkup", exp ? `Check-up (export of ${day(exp)})` : "Check-up"], ["studio", "Studio (live from Spotify)"]]} />}
          </div>
          <TabPanel key={tab} dir={dir} label={tab === "studio" ? "Studio" : "Check-up"} className="flex flex-col gap-3.5">{children}</TabPanel>
        </div>
      </div>
    </>
  );
}

// `p` is the playlist id (check-up) or uri (Studio): names are not unique on Spotify.
// `issues` (check-up only): the share of tracks flagged, drawn as a bar under the counts.
const PlaylistLink = ({ tab, id, name, sel, issues, children }: { tab: Tab; id: string; name: string; sel: boolean; issues?: number; children: ReactNode }) => (
  <Link to={`/playlists/${tab}?p=${encodeURIComponent(id)}`} className="row grid-cols-[minmax(0,1fr)]" aria-current={sel ? "page" : undefined}>
    <span className="flex flex-col gap-0.5 min-w-0"><span className="truncate font-semibold text-text">{name}</span><span className="text-muted text-xs font-mono">{children}</span>
      {issues != null && <span className="py-health" aria-hidden><i style={{ width: `${issues ? Math.max(4, issues * 100) : 0}%` }} /></span>}</span>
  </Link>
);

/** A row rising in: capped so a long playlist is in place within ~0.5 s. */
const rise = (reduce: boolean | null, i: number) => ({
  initial: reduce ? false : { opacity: 0, y: 6 }, animate: { opacity: 1, y: 0 },
  transition: { duration: 0.28, ease: EASE, delay: Math.min(i, 14) * 0.025 },
} as const);

/** Where the flagged tracks sit along the playlist (`stripBg`: one element whatever the length). */
function Strip({ flags, note }: { flags: boolean[]; note: string }) {
  const reduce = useReducedMotion();
  return (
    <div className="flex flex-col gap-1.5">
      <m.div className="py-strip" style={{ background: stripBg(flags), originX: 0 }} initial={reduce ? false : { scaleX: 0 }} animate={{ scaleX: 1 }} transition={{ duration: 0.7, ease: EASE, delay: 0.1 }} aria-hidden />
      <div className="flex justify-between items-baseline gap-3 text-[11px]">
        <span className="font-mono text-faint" aria-hidden>Track 1</span><span className="text-muted text-xs text-center">{note}</span><span className="font-mono text-faint" aria-hidden>Track {flags.length}</span>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ check-up (read-only) */
function Checkup() {
  const { r3 } = useRelease();
  const reduce = useReducedMotion();
  const [show, setShow] = useState<"all" | "issues">("all");
  const [dir, slide] = useSlide(["all", "issues"], show);
  const [sp] = useSearchParams();
  const pls = useApi(api.listExportedPlaylists);
  const sel = pls.data?.find((p) => String(p.id) === sp.get("p")) ?? pls.data?.[0];
  const cu = useApi(() => (sel ? api.getPlaylistCheckup(sel.id) : Promise.resolve(null)), [sel?.id]);
  const list = pls.data?.map((p) => (
    <PlaylistLink key={p.id} tab="checkup" id={String(p.id)} name={p.name} sel={p === sel} issues={p.tracks ? Math.min(1, (p.dupes + p.low) / p.tracks) : 0}>
      {pl(p.tracks, "track")}{p.dupes > 0 && <>, <span className="text-accent">{pl(p.dupes, "duplicate")}</span></>}{p.low > 0 && <>, <span className="text-accent">{p.low} rated low</span></>}
    </PlaylistLink>
  ));
  const wrap = (body: ReactNode) => <Layout tab="checkup" list={list} title={sel?.name ?? "Playlists"} subline={sel && `Your playlist, ${pl(sel.tracks, "track")}`}>{body}</Layout>;
  if (pls.loading || cu.loading) return wrap(<SkelRows n={8} />);
  if (pls.error || cu.error) return wrap(<ErrorBanner error={pls.error ?? cu.error} title="Couldn't load this playlist." onRetry={() => { pls.reload(); cu.reload(); }} />);
  if (!sel || !cu.data) return wrap(<Empty h="No playlists in your export" line="Import an Account data export from Spotify to check your playlists."><Link className="btn btn-primary" to="/import/1">Import history</Link></Empty>);
  const t = cu.data.tracks;
  const never = t.filter((x) => x.never_played).length;
  const flag = (x: (typeof t)[number]) => x.dupe || (x.rating != null && x.rating <= 2);
  const flagged = t.filter(flag).length;
  const shown = show === "issues" ? t.filter(flag) : t;
  return wrap(<>
    <p className="text-text-2 m-0">Read-only. Shows the playlist as it was in your export. To change it, use Studio{r3 ? "" : " (Release 3)"}.</p>
    <SourceTag sources={["E", "U", "S"]} />
    <div className="stats">
      <Stat k="Duplicates" v={sel.dupes} d="Same track twice" act={sel.dupes > 0} />
      <Stat k="Rated 2 or lower" v={sel.low} d="By you" act={sel.low > 0} />
      <Stat k="Never played" v={never} d="In your plays" />
      <Stat k="Tracks" v={t.length} d="In the export" />
    </div>
    <section className="card py-tracks" aria-label="Tracks">
      <Seg label="Show" value={show} onChange={(v) => { slide(v); setShow(v); }} options={[["all", `All tracks (${t.length})`], ["issues", `Needs a look (${flagged})`]]} />
      {t.length > 0 && <Strip flags={t.map(flag)} note={flagged ? `${pl(flagged, "track")} flagged, in accent along the playlist` : "Nothing flagged"} />}
      <TabPanel key={show} dir={dir} label={show === "issues" ? "Tracks that need a look" : "All tracks"} className="list">
        {shown.length ? shown.map((x, i) => {
          const low = x.rating != null && x.rating <= 2;
          return (
            <m.div key={x.position} className={`row py-row${flag(x) ? " flag" : ""}`} {...rise(reduce, i)}>
              <span className="font-mono text-faint text-xs">{x.position}</span>
              <span className="flex flex-col gap-0.5 min-w-0"><span className="font-semibold truncate">{x.name}</span><span className="text-muted text-xs truncate">{x.artist}</span></span>
              <span>{x.rating ? <Stars r={x.rating} /> : <span className="text-faint text-xs">Not rated</span>}</span>
              <span className="justify-self-end">{x.dupe ? <span className="badge need">Duplicate</span> : low ? <span className="badge need">Rated low</span> : x.never_played ? <span className="text-faint text-xs">Never played</span> : null}</span>
            </m.div>
          );
        }) : <span className="text-muted px-2.5 py-2">No duplicates or low ratings in this playlist.</span>}
      </TabPanel>
    </section>
    {r3 && <Link className="btn btn-primary self-start" to={`/playlists/studio?p=${encodeURIComponent(sel.name)}&tracks=${sel.tracks}`}>Open in Studio</Link>}
  </>);
}

/* ------------------------------------------------------------------ Studio (live, R3) */
/** Apply failures after which nothing was written on Spotify (API.md §5): the draft and keep/cut choices stay. */
const NOTHING_WRITTEN = new Set<AppError["code"]>(["conflict", "validation", "spotify_busy"]);
const wait = (e: AppError) => (e.retry_after_s ? `in ${e.retry_after_s} s` : "in a moment");

/** Kept against cut, as two bars that spring to their share with every choice (no track behind them). */
function Meter({ kept, total }: { kept: number; total: number }) {
  const reduce = useReducedMotion();
  const cut = total - kept;
  const t = total || 1;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-baseline gap-2">
        <AnimatePresence mode="popLayout" initial={false}>
          <m.span key={kept} className="py-big" initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -10 }} transition={swap(reduce, LAND)}>{kept}</m.span>
        </AnimatePresence>
        <span className="text-muted text-sm">of {pl(total, "track")} kept</span>
      </div>
      <div className="py-meter" aria-hidden>
        <m.i className="keep" style={{ originX: 0 }} initial={false} animate={{ scaleX: kept / t }} transition={LAND} />
        <m.i className="cut" style={{ originX: 1 }} initial={false} animate={{ scaleX: cut / t }} transition={LAND} />
      </div>
    </div>
  );
}

function Studio() {
  const reduce = useReducedMotion();
  const [sp] = useSearchParams();
  const online = useOnline();
  const toast = useToast();
  // Not live: the draft is what the user edits, so another write in the app (QuickAdd, a failed apply) must not
  // swap it out under them or skip the pre-apply conflict check. It reloads only when this page says so.
  const pls = useApi(api.listSpotifyPlaylists, [], false);
  // By uri; a name and optional track count only from the check-up's "Open in Studio" (the export has no uri, G-31).
  // ponytail: the export's count can drift from the live one, then the first same-name playlist wins; match by track list if that bites.
  const match = pls.data?.filter((p) => p.name === sp.get("p"));
  const tracksParam = sp.get("tracks");
  const sel = pls.data?.find((p) => p.uri === sp.get("p"))
    ?? (tracksParam ? match?.find((p) => p.tracks === Number(tracksParam)) : undefined)
    ?? match?.[0]
    ?? pls.data?.[0];
  const draft = useApi(() => (sel ? api.loadStudioDraft(sel.uri) : Promise.resolve(null)), [sel?.uri], false);
  // Not live either: a refetch that fails elsewhere would drop the Undo banner. Re-read after each apply/undo.
  // Tagged with its playlist so a previous playlist's record never shows (or undoes) under the new one while loading.
  const last = useApi(() => (sel ? api.getLastApply(sel.uri).then((r) => r && { ...r, uri: sel.uri }) : Promise.resolve(null)), [sel?.uri], false);
  const suggested = useMemo(() => new Set((draft.data?.tracks ?? []).flatMap((t, i, all) => (cutReason(t, all) ? [i] : []))), [draft.data]);
  // Keep/cut edits belong to one draft: playlist + its track URI list. A reload that reads the same list keeps them.
  const key = draft.data ? `${draft.data.playlist_uri} ${draft.data.tracks.map((t) => t.uri).join(" ")}` : "";
  const [edit, setEdit] = useState<{ key: string; cut: Set<number> }>();
  const cut = edit?.key === key ? edit.cut : suggested;
  const [confirm, setConfirm] = useState<{ d: Draft; diff: StudioDiff } | null>(null);
  const [busy, setBusy] = useState(false);
  // Which call failed decides the banner: a failed apply/undo may have changed the playlist (TSD §5.2).
  const [fail, setFail] = useState<{ op: "preview" | "apply" | "undo"; e: AppError }>();
  // "Check other playlists" (G-24): only on request, one Spotify read per playlist; tagged with its playlist.
  const [cross, setCross] = useState<{ uri: string; busy?: boolean; e?: AppError; also?: Map<string, string[]> }>();
  const crossNow = cross?.uri === sel?.uri ? cross : undefined;
  const checkOthers = () => {
    if (!sel) return;
    const uri = sel.uri;
    setCross({ uri, busy: true });
    api.findCrossPlaylistDupes(uri).then((rows) => setCross({ uri, also: new Map(rows.map((r) => [r.uri, r.playlists.map((p) => p.name)])) }),
      (e) => setCross({ uri, e: api.toAppError(e) }));
  };
  useEffect(() => { setFail(undefined); }, [sel?.uri]);
  const refresh = () => { pls.reload(); draft.reload(); };

  const undo = () => {
    if (!sel) return;
    setFail(undefined);
    api.undoLastApply(sel.uri).then(() => { setEdit(undefined); refresh(); toast(`${sel.name} restored to how it was before you applied`); }, (e) => {
      const err = api.toAppError(e);
      setFail({ op: "undo", e: err });
      if (!["conflict", "not_found", "spotify_busy"].includes(err.code)) refresh(); // may be partly restored
    }).finally(last.reload);
  };
  const banner = !fail ? null
    : fail.e.code === "conflict" && fail.op === "apply"
      ? <ErrorBanner error={{ ...fail.e, message: "Someone edited it since you started. Reload to see the new version." }} title="This playlist changed on Spotify." onRetry={() => { setFail(undefined); refresh(); }} />
      : fail.e.code === "conflict" ? <ErrorBanner error={fail.e} title="This playlist changed on Spotify." />
      : fail.e.code === "spotify_busy" && fail.op === "undo"
        ? <ErrorBanner error={{ ...fail.e, message: "Nothing was restored yet, and the undo is kept." }} title={`Spotify may still be finishing your last change. Try Undo again ${wait(fail.e)}.`} />
      : fail.e.code === "spotify_busy"
        ? <ErrorBanner error={{ ...fail.e, message: "Nothing was changed, and your draft is kept." }} title={`Spotify is still finishing a change; try again ${wait(fail.e)}.`} />
      : fail.op === "undo" && fail.e.code === "not_found"
        ? <ErrorBanner error={{ ...fail.e, message: "It was already undone, or its 30 days are over. Nothing was changed." }} title="There's nothing to undo." />
      : fail.op === "undo" ? <ErrorBanner error={fail.e} title="Undo didn't finish; the playlist may be partly restored. Try Undo again." />
      : fail.op === "apply" && fail.e.code !== "validation"
        ? <ErrorBanner error={fail.e} title="This may have been partly applied on Spotify. If Undo is offered below, it puts the playlist back." />
        : <ErrorBanner error={fail.e} title="Nothing was changed on Spotify." />;
  // The warning and the Undo banner stay on screen even when a reload fails or the page is offline.
  const top = <>{banner}<AnimatePresence initial={false}>{last.data?.uri === sel?.uri && last.data && (
    <m.div key="undo" className="banner" initial={{ opacity: 0, y: -8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, transition: swap(reduce, { duration: 0.15 }) }} transition={swap(reduce, { duration: 0.32, ease: EASE })}>
      <span><b>Applied {day(last.data.applied_at)}.</b> <span className="text-text-2">Undo is available until {day(last.data.expires_at)}.</span></span>
      <button className="btn btn-secondary s" onClick={undo}>Undo last apply</button>
    </m.div>
  )}</AnimatePresence></>;

  const list = pls.data?.map((p) => <PlaylistLink key={p.uri} tab="studio" id={p.uri} name={p.name} sel={p === sel}>{pl(p.tracks, "track")}</PlaylistLink>);
  const wrap = (body: ReactNode) => <Layout tab="studio" list={list} title={sel?.name ?? "Studio"} subline={sel && `Your playlist, ${pl(sel.tracks, "track")}`}>{body}</Layout>;
  const err = pls.error ?? draft.error;
  if (err?.code === "spotify_auth" || err?.code === "spotify_forbidden") return wrap(<>{top}
    <Empty h="Connect Spotify to use Studio" line="Studio edits your own playlists on Spotify. The check-up from your export works without connecting.">
      <Link className="btn btn-primary" to="/settings?tab=spotify">Connect Spotify in Settings</Link>
    </Empty></>);
  if (!online || err?.code === "offline") return wrap(<>{top}<Empty h="Studio needs a connection" line="Your draft is kept. Reconnect to read the playlist from Spotify and apply changes." /></>);
  if (pls.loading || draft.loading) return wrap(<><span className="text-muted">Reading the playlist from Spotify…</span><SkelRows n={8} /></>);
  if (err) return wrap(<>{top}<ErrorBanner error={err} title="Couldn't read the playlist from Spotify." onRetry={refresh} /></>);
  if (!sel || !draft.data) return wrap(<Empty h="No playlists of yours on Spotify" line="Studio edits playlists you own. Make one in Spotify, then come back." />);

  const d = draft.data;
  const changes = d.tracks.map((t, i) => ({ t, i })).filter(({ i }) => cut.has(i));
  const toggle = (i: number, v: "keep" | "cut") => { const n = new Set(cut); if (v === "cut") n.add(i); else n.delete(i); setEdit({ key, cut: n }); };
  const openConfirm = () => { setBusy(true); api.previewStudioApply(d, keepList(d.tracks, cut)).then((diff) => setConfirm({ d, diff }), (e) => setFail({ op: "preview", e: api.toAppError(e) })).finally(() => setBusy(false)); };
  const apply = () => {
    setBusy(true); setFail(undefined);
    api.applyStudioDraft(d, keepList(d.tracks, cut)).then(() => {
      setConfirm(null); setEdit(undefined); refresh();
      toast(`${pl(changes.length, "change")} applied to ${sel.name}. Undo is available for 30 days.`, undo);
    }, (e) => {
      const err = api.toAppError(e);
      setConfirm(null); setFail({ op: "apply", e: err });
      // May have landed: read the playlist again (keep/cut survive if its track list didn't change).
      if (!NOTHING_WRITTEN.has(err.code)) refresh();
    }).finally(() => { setBusy(false); last.reload(); });
  };

  return wrap(<>
    <Attribution uri={sel.uri} name={sel.name}><span className="text-text-2 text-sm">Tracks from Spotify, read just now, with your own ratings and tags. No listening data is used here.</span></Attribution>
    {top}
    <div className="flex flex-wrap gap-7 items-start">
      <div className="card pad-s flex-[1.5_1_520px] min-w-0 flex flex-col gap-3.5"><div className="list">
        {d.tracks.map((t, i) => {
          const isCut = cut.has(i); const why = cutReason(t, d.tracks); const also = crossNow?.also?.get(t.uri);
          return (
            <m.div key={i} className={`row py-row py-studio${isCut ? " cut" : ""}${why ? " flag" : ""}`} {...rise(reduce, i)}>
              <span className="font-mono text-faint text-xs">{i + 1}</span>
              <span className="flex flex-col gap-0.5 min-w-0">
                <span className="py-name font-semibold truncate">{t.name}</span>
                <span className="text-muted text-xs">{t.artist}{why && <span className="text-accent">, {why}</span>}{also && <span className="text-accent">, also in {also.join(", ")}</span>}</span>
              </span>
              <span>{t.rating ? <Stars r={t.rating} /> : null}</span>
              <Seg label={`Keep or cut ${t.name}`} radio accent={isCut} value={isCut ? "cut" : "keep"} onChange={(v) => toggle(i, v)} options={[["keep", "Keep"], ["cut", "Cut"]]} />
            </m.div>
          );
        })}
      </div></div>
      <aside className="card py-side flex-[1_1_300px] min-w-0 flex flex-col gap-3">
        <h2 className="card-title">Your changes</h2>
        <Meter kept={d.tracks.length - changes.length} total={d.tracks.length} />
        <span className="text-muted text-sm">Draft only. Nothing changes on Spotify until you apply.</span>
        <div className="flex flex-col">
          <AnimatePresence initial={false}>
            {changes.map(({ t, i }) => (
              <m.div key={i} className="overflow-hidden" initial={{ height: 0, opacity: 0 }} animate={{ height: "auto", opacity: 1 }} exit={{ height: 0, opacity: 0 }} transition={swap(reduce, { duration: 0.24, ease: EASE })}>
                <div className="flex flex-col gap-0.5 px-3 py-2.5 mb-2 rounded-control bg-surface-2 py-change">
                  <span className="text-accent text-xs font-bold">{t.dupe_of ? "Remove duplicate" : "Remove"}</span><span className="truncate">{t.name}</span>
                </div>
              </m.div>))}
          </AnimatePresence>
          {!changes.length && <span className="text-muted">No changes yet.</span>}
        </div>
        <button className="btn btn-primary" disabled={!changes.length || busy} onClick={openConfirm}>{busy && !confirm ? <Dots /> : `Apply ${pl(changes.length, "change")} to Spotify`}</button>
        <span className="text-muted text-xs leading-[1.45]">Runout checks the playlist hasn't changed on Spotify first. You can undo for 30 days.</span>
        <button className="btn btn-secondary" onClick={() => { setEdit(undefined); toast("Draft discarded"); }}>Discard draft</button>
        <h2 className="card-title">In your other playlists</h2>
        <span className="text-muted text-xs leading-[1.45]">Looks for these tracks in every other playlist of yours. Reads each one from Spotify, so it can take a while.</span>
        {crossNow?.e && <ErrorBanner error={crossNow.e} title={crossNow.e.code === "offline" ? "You're offline. Check again when you reconnect." : "Couldn't check your other playlists."} onRetry={checkOthers} />}
        {crossNow?.also && <span className="text-text-2 text-sm" role="status">{crossNow.also.size ? `${pl(crossNow.also.size, "track")} also in other playlists, marked in the list.` : "None of these tracks are in your other playlists."}</span>}
        <button className="btn btn-secondary" disabled={!online || crossNow?.busy} onClick={checkOthers}
          aria-label={crossNow?.busy ? "Checking your other playlists" : undefined}>
          {crossNow?.busy ? <Dots /> : crossNow?.also ? "Check again" : "Check other playlists"}
        </button>
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

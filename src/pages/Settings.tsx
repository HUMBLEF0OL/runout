// SC-13 Settings and data (+ SC-13c connect, SC-13d restore, SC-13e Shortcut steps, SC-13f delete library).
// Tray menu and the NSIS uninstall dialog are native, not React.
import { useEffect, useState, type ReactNode } from "react";
import { CaretRight } from "@phosphor-icons/react";
import { Link, useSearchParams } from "react-router-dom";
import {
  backupNow, checkBackup, checkInbox, chooseBackupDir, createInbox, deleteLibrary, getInbox, setInboxOptions, disconnectSpotify, exportLibrary, getCapture, getLoggerStatus,
  getSettings, getSpotifyStatus, getSuccessMetrics, listBackups, listImports, listTags, moveLibrary, newPairingCode, onEvent, pauseLogger,
  pickBackup, restoreBackup, resumeLogger, setCaptureEnabled, setLogger, updateSettings, toAppError, type AppError, type Backup, type RestoreCheck,
} from "../api";
import { Topbar } from "../components/Shell";
import { ConnectDialog, ShortcutDialog } from "../components/SetupDialogs";
import { CopyButton, CountUp, Dialog, Dots, ErrorBanner, Seg, SkelRows, Switch, useSlide, useToast } from "../components/ui";
import { useApi, useOnline } from "../hooks/useApi";
import { useRelease } from "../hooks/useRelease";
import { day, dayY, pl, time } from "../lib/format";

const mb = (bytes: number) => `${(bytes / 1048576).toFixed(1)} MB`;
const backupLabel = (b: Backup, i: number) => `${day(b.created_at)}, ${time(b.created_at)}${i === 0 ? " (latest)" : ""}`;


function Card({ id, title, children, danger }: { id: string; title: ReactNode; children: ReactNode; danger?: boolean }) {
  return (
    <section className="card flex flex-col gap-3" aria-labelledby={`st-${id}`}>
      <h2 id={`st-${id}`} className={`card-title${danger ? " text-danger" : ""}`}>{title}</h2>
      {children}
    </section>
  );
}
const Metric = ({ v, k }: { v: ReactNode; k: string }) =>
  <div className="st-metric"><span className="font-mono text-xl font-semibold"><CountUp text={String(v)} /></span><span className="text-muted text-xs">{k}</span></div>;

/** Spotlight border: the card under the pointer gets the pointer position, read by `.st-panel .card::before`. */
const spot = (e: React.PointerEvent) => {
  const c = (e.target as HTMLElement).closest<HTMLElement>(".card");
  if (!c) return;
  const r = c.getBoundingClientRect();
  c.style.setProperty("--mx", `${e.clientX - r.left}px`);
  c.style.setProperty("--my", `${e.clientY - r.top}px`);
};

/** Run a write with a busy key; errors stay inline (DESIGN §5 toast rule). */
function useRun() {
  const [busy, setBusy] = useState<string>();
  const [err, setErr] = useState<AppError>();
  const run = async <T,>(key: string, fn: () => Promise<T>): Promise<T | undefined> => {
    setBusy(key); setErr(undefined);
    try { return await fn(); } catch (e) { setErr(toAppError(e)); return undefined; } finally { setBusy(undefined); }
  };
  return { busy, err, setErr, run };
}

export default function Settings() {
  const { r2, r3 } = useRelease();
  const online = useOnline();
  const toast = useToast();
  const s = useApi(getSettings);
  const backups = useApi(listBackups);
  const cap = useApi(getCapture);
  const logger = useApi(() => (r2 ? getLoggerStatus() : Promise.resolve(null)), [r2]);
  const spotify = useApi(() => (r3 ? getSpotifyStatus() : Promise.resolve(null)), [r3]);
  const inbox = useApi(() => (r3 ? getInbox() : Promise.resolve(null)), [r3]);
  const imports = useApi(() => (r2 ? listImports() : Promise.resolve([])), [r2]);
  const tags = useApi(listTags);
  const metrics = useApi(getSuccessMetrics);
  const { busy, err, run } = useRun();
  const [dlg, setDlg] = useState<null | "restore" | "shortcut" | "connect" | "disconnect" | "delete">(null);
  const [capOff, setCapOff] = useState<"wrong_code" | "error">();
  const [warn, setWarn] = useState("");
  const [params, setParams] = useSearchParams();
  const [ibGone, setIbGone] = useState(false); // Check now found the Inbox deleted in Spotify (check_inbox → not_found)
  useEffect(() => onEvent("capture-disabled", (p) => setCapOff(p.reason)), []);
  useEffect(() => { if (s.data) setWarn(String(s.data.size_warn_mb)); }, [s.data]);

  const tabs: [string, string][] = [
    ["data", "Your data"], ["spotify", "Spotify"], ...(r3 ? [["inbox", "Inbox"] as [string, string]] : []),
    ...(r2 ? [["logger", "Logger"] as [string, string]] : []), ["phone", "Wi-Fi capture"],
    ...(r2 ? [["import", "History"] as [string, string]] : []), ["tags", "Tags"], ["metrics", "Metrics"],
  ];
  // ?tab= deep-links a section (Connect Spotify → spotify, Import → import); unknown or hidden by release → Your data
  const tab = tabs.some(([id]) => id === params.get("tab")) ? params.get("tab")! : "data";
  const idx = tabs.findIndex(([id]) => id === tab);
  const [dir, slide] = useSlide(tabs.map((t) => t[0]), tab);
  const pick = (id: string) => { slide(id); setParams(id === "data" ? {} : { tab: id }, { replace: true }); };

  const head = <><Topbar /><h1 className="page-title">Settings and data</h1></>;
  if (s.loading) return <>{head}<SkelRows n={6} /></>;
  if (s.error || !s.data) return <>{head}<ErrorBanner title="Couldn’t load your settings." error={s.error} onRetry={s.reload} /></>;
  const st = s.data;
  const latest = backups.data?.[0];
  // account batches carry no plays (latest_event_utc null), so take the newest that does
  const lastPlay = imports.data?.find((m) => m.latest_event_utc)?.latest_event_utc;
  const lg = logger.data;
  const sp = spotify.data;
  const ib = inbox.data;
  const connected = sp?.state === "connected";
  const gone = ibGone || ib?.state === "missing";
  const check = () => checkInbox().catch((e) => {
    if (toAppError(e).code !== "not_found") throw e;
    setIbGone(true); inbox.reload(); return undefined;
  });


  return (
    <>
      {head}
      <Seg label="Settings sections" options={tabs} value={tab} onChange={pick} />
      {err && <ErrorBanner error={err} title="That didn’t work." />}
      {latest?.size_warning && (
        <div className="banner err" role="alert">
          <span><b className="text-danger">Last backup was larger than your limit.</b> <span className="text-text-2">It was {mb(latest.size_bytes)}; your limit is {st.size_warn_mb} MB. The backup was still kept.</span></span>
          <button className="btn btn-secondary s" onClick={() => run("limit", () => updateSettings({ size_warn_mb: st.size_warn_mb + 50 })).then((r) => r && toast(`Limit raised to ${r.size_warn_mb} MB`))}>Raise the limit</button>
        </div>
      )}
      <div key={tab} role="tabpanel" aria-label={tabs[idx][1]} className={`st-panel tab-panel ${dir}`} onPointerMove={spot}>
        {tab === "data" && <Card id="data" title="Your data">
          <div className="field">
            <span className="label">Library location</span>
            <span className="flex items-center gap-1"><span className="font-mono text-xs">{st.library_path}</span><CopyButton text={st.library_path} label="Copy library location" /></span>
            <button className="btn btn-secondary s self-start" disabled={!!busy}
              onClick={() => run("move", moveLibrary).then((p) => p && toast(`Library moved to ${p}. A library moved elsewhere isn’t removed when you uninstall.`))}>
              {busy === "move" ? <Dots /> : "Change location"}
            </button>
          </div>
          <div className="field">
            <span className="label">Backups, newest 3 kept</span>
            {backups.loading ? <div className="skel h-[60px]" /> : backups.data?.length
              ? backups.data.map((b, i) => <span key={b.path} className="flex justify-between items-baseline gap-3"><span className="font-mono text-xs">{backupLabel(b, i)}</span><span className="font-mono text-text-2 text-xs">{mb(b.size_bytes)}</span></span>)
              : <span className="text-muted">No backups yet. Back up now to make the first one.</span>}
            <span className="flex justify-between items-baseline gap-3">
              <span className="flex items-center gap-1"><span className="font-mono text-text-2 text-xs">{st.backup_dir}</span><CopyButton text={st.backup_dir} label="Copy backup folder" /></span>
              <button className="btn btn-ghost s" disabled={!!busy} onClick={() => run("dir", async () => { const d = await chooseBackupDir(); if (d) { s.reload(); toast(`Backups now go to ${d}`); } })}>Change folder</button>
            </span>
            <Switch label="Back up weekly" on={st.backup_weekly} onChange={(v) => run("weekly", () => updateSettings({ backup_weekly: v }))} />
            <label className="flex flex-wrap gap-2 items-center text-sm">Warn when a backup is over
              <input className="input h-8 w-[70px]" inputMode="numeric" value={warn} aria-label="Backup size limit in MB"
                onChange={(e) => setWarn(e.target.value.replace(/\D/g, ""))}
                onBlur={() => { const n = Number(warn); if (n > 0 && n !== st.size_warn_mb) run("warn", () => updateSettings({ size_warn_mb: n })); else setWarn(String(st.size_warn_mb)); }} /> MB
            </label>
          </div>
          <div className="flex flex-wrap gap-2 items-center">
            <button className="btn btn-light" disabled={!!busy} onClick={() => run("backup", backupNow).then((b) => b && toast(`Backup saved to ${st.backup_dir} (${mb(b.size_bytes)}). Oldest copy removed.`))}>
              {busy === "backup" ? <Dots /> : "Back up now"}
            </button>
            <button className="btn btn-secondary" onClick={() => setDlg("restore")}>Restore</button>
            <button className="btn btn-secondary" disabled={!!busy} onClick={() => run("export", exportLibrary).then((r) => r && toast(`Exported ${pl(r.count, "record")} to ${r.path}`))}>
              {busy === "export" ? <Dots /> : "Export as JSON"}
            </button>
          </div>
        </Card>}

        {tab === "spotify" && <Card id="spotify" title="Spotify connection">
          {!r3 ? <span className="text-muted">Arrives in Release 3: Playlist Studio, autofill and the From Spotify page.</span>
            : spotify.loading ? <div className="skel h-[60px]" />
            : sp?.state === "connected" ? (!online
              ? <span>Connected, but you’re offline. Spotify features pause until you reconnect.</span>
              : <>
                  <span>Connected. Spotify asks you to sign in again by {sp.renew_by ? dayY(sp.renew_by) : "its next renewal"}.</span>
                  <span className="text-muted text-sm leading-[1.5]">Used for Playlist Studio, autofill, Saved in Spotify and the From Spotify page. Disconnecting deletes everything Runout cached from Spotify, right away.</span>
                  <div><button className="btn btn-secondary" onClick={() => setDlg("disconnect")}>Disconnect</button></div>
                </>)
            : <>
                <span>{sp?.state === "reconnect" ? "Spotify needs you to sign in again." : "Not connected."}</span>
                <span className="text-muted text-sm">Needs a Spotify Premium account. Runout opens Spotify’s sign-in page in your browser.</span>
                <div><button className="btn btn-primary" disabled={!online} onClick={() => setDlg("connect")}>{sp?.state === "reconnect" ? "Sign in again" : "Connect Spotify"}</button></div>
                {!online && <span className="help">Connect when you’re back online.</span>}
              </>}
        </Card>}

        {r3 && tab === "inbox" && (
          <Card id="inbox" title="Runout Inbox">
            {inbox.loading || spotify.loading ? <div className="skel h-[90px]" /> : inbox.error || !ib ? <ErrorBanner error={inbox.error} title="Couldn’t read the Inbox settings." onRetry={inbox.reload} /> : <>
              <span>{!connected ? "Runout Inbox · connect Spotify first" : gone ? "Runout Inbox · missing" : ib.state === "ready" ? "Runout Inbox · ready" : "Runout Inbox · not created yet"}</span>
              <span className="text-muted text-sm leading-[1.5]">On your phone, add a track or album to the private “Runout Inbox” playlist in Spotify. Runout checks it at start and every 15 minutes, saves each one to your backlog as “Name needed” and takes it out of the Inbox.</span>
              {gone && connected && <span className="text-text-2 text-sm" role="status">Your Runout Inbox is gone from Spotify. Create a new one? Runout won’t make one unless you ask.</span>}
              {ib.state !== "none" && (
                <span className="flex justify-between items-baseline gap-3 text-sm"><span>Last check</span>
                  <span className="font-mono text-text-2 text-xs">{ib.last_check ? `${day(ib.last_check)}, ${time(ib.last_check)} · ${pl(ib.imported_last, "item")} imported` : "not yet"}</span></span>)}
              {connected && <div className="flex flex-wrap gap-2 items-center">
                {ib.state === "ready" && !gone
                  ? <button className="btn btn-secondary" disabled={!online || !!busy} onClick={() => run("inbox", check).then((r) => r && toast(r.imported ? `${pl(r.imported, "item")} from your Inbox saved to the backlog as Name needed` : "Nothing new in your Inbox"))}>
                      {busy === "inbox" ? <Dots /> : "Check now"}</button>
                  : <button className="btn btn-primary" disabled={!online || !!busy} onClick={() => run("inbox", createInbox).then((r) => { if (r) { setIbGone(false); toast("Runout Inbox created in your Spotify playlists"); } })}>
                      {busy === "inbox" ? <Dots /> : "Create Inbox"}</button>}
              </div>}
              {connected && !online && <span className="help">Check the Inbox when you’re back online.</span>}
              {connected && <>
                <Switch label="Also import albums I save in Spotify" on={ib.saved_albums} disabled={!!busy} onChange={(v) => run("ibopt", () => setInboxOptions(v, ib.followed_artists))} />
                <Switch label="Also import artists I follow" on={ib.followed_artists} disabled={!!busy} onChange={(v) => run("ibopt", () => setInboxOptions(ib.saved_albums, v))} />
                <span className="text-muted text-xs">Following artists asks Spotify for one more permission: you approve it once when you next sign in.</span>
                {ib.needs_reconnect && <div className="flex flex-wrap gap-2 items-center"><span className="text-text-2 text-sm">Sign in again to allow reading who you follow.</span>
                  <button className="btn btn-secondary s" disabled={!online} onClick={() => setDlg("connect")}>Sign in again</button></div>}
              </>}
            </>}
          </Card>
        )}

        {r2 && tab === "logger" && (
          <Card id="logger" title="Desktop logger">
            {logger.loading || !lg ? <div className="skel h-[90px]" /> : <>
              <Switch label="Log what Spotify plays on this PC" on={lg.state !== "off"} onChange={(v) => run("log", () => setLogger({ on: v }))} />
              <Switch label="Start with Windows, in the tray" on={lg.autostart} onChange={(v) => run("auto", () => setLogger({ autostart: v }))} />
              <Switch label="Include private-session plays in insights" on={lg.include_private} onChange={(v) => run("priv", () => setLogger({ include_private: v }))} />
              {lg.state === "paused"
                ? <div className="flex justify-between items-baseline gap-3"><span className="text-text-2">Paused{lg.paused_until ? ` until ${time(lg.paused_until)}` : " until you resume"}.</span>
                    <button className="btn btn-light s" onClick={() => run("resume", resumeLogger).then((r) => r && toast("Logging resumed"))}>Resume logging</button></div>
                : lg.state !== "off" && <div className="flex flex-wrap gap-2 items-center">
                    <button className="btn btn-secondary s" onClick={() => run("pause", () => pauseLogger(60)).then((r) => r && toast("Logging paused for 1 hour"))}>Pause for 1 hour</button>
                    <button className="btn btn-secondary s" onClick={() => run("pause", () => pauseLogger(null)).then((r) => r && toast("Logging paused until you resume"))}>Pause until I resume</button>
                  </div>}
              <span className="text-muted text-xs">A play counts after 30 seconds. Only Spotify is logged.</span>
            </>}
          </Card>
        )}

        {tab === "phone" && <Card id="phone" title="Home Wi-Fi capture">
          {cap.loading || !cap.data ? <div className="skel h-[90px]" /> : capOff ? <>
              <div className="banner err" role="alert"><span><b className="text-danger">Phone capture turned itself off.</b> <span className="text-text-2">{capOff === "wrong_code" ? "5 wrong pairing codes in a row. Make a new code and update your Shortcut." : "The listener stopped. Turn it on again to keep saving from your iPhone."}</span></span></div>
              <div><button className="btn btn-primary" onClick={() => run("code", async () => { await newPairingCode(); await setCaptureEnabled(true); }).then(() => { setCapOff(undefined); toast("New pairing code. Update it in your iPhone Shortcut."); })}>New code and turn on</button></div>
            </> : (
            <details className="st-details">
              <summary><CaretRight size={14} weight="bold" aria-hidden />Optional: instant capture on home Wi-Fi</summary>
              <div className="flex flex-col gap-3 pt-3">
                <Switch label="Accept shares from my iPhone on this Wi-Fi" on={cap.data.on} onChange={(v) => run("cap", () => setCaptureEnabled(v))} />
                <div className="tiles grid-cols-2">
                  <div className="field"><span className="label">Address</span><span className="flex items-center gap-2"><span className="font-mono">{cap.data.address}:{cap.data.port}</span><CopyButton text={`${cap.data.address}:${cap.data.port}`} label="Copy address" /></span></div>
                  <div className="field"><span className="label">Pairing code</span><span className="flex items-center gap-2"><span className="font-mono text-[18px] tracking-[2px]">{cap.data.code}</span><CopyButton text={cap.data.code} label="Copy pairing code" /></span></div>
                </div>
                <span className="text-muted text-xs">Windows may ask to allow Runout on private networks. Your Wi-Fi must be set to Private.</span>
                <div className="flex flex-wrap gap-2 items-center">
                  <button className="btn btn-light" onClick={() => setDlg("shortcut")}>Shortcut steps</button>
                  <button className="btn btn-secondary" onClick={() => run("code", newPairingCode).then((c) => c && toast("New pairing code. Update it in your iPhone Shortcut."))}>New code</button>
                </div>
              </div>
            </details>)}
        </Card>}

        {r2 && tab === "import" && (
          <Card id="import" title="Spotify history">
            {lastPlay ? <span>Last import covers plays up to {dayY(lastPlay)}.</span> : <span>No listening history imported yet.</span>}
            <span className="text-muted text-sm leading-[1.5]">Request a fresh copy from Spotify’s privacy page, then import it here. Plays already imported are skipped.</span>
            {!!imports.data?.length && (
              <div className="flex flex-col gap-1">
                {imports.data.map((m) => (
                  <span key={m.id} className="flex justify-between items-baseline gap-3 text-sm">
                    <span>{m.kind === "extended" ? "Extended history" : "Account data"}, imported {day(m.imported_at)}</span>
                    <span className="font-mono text-text-2 text-xs">{pl(m.record_count, "play")}</span>
                  </span>))}
              </div>
            )}
            <div><Link className="btn btn-light" to="/import/1">Import an export</Link></div>
          </Card>
        )}

        {tab === "tags" && <Card id="tags" title="Tags">
          <span className="text-text-2">{tags.data ? `${pl(tags.data.length, "tag")} in use.` : " "}</span>
          <div><Link className="btn btn-secondary" to="/settings/tags">Rename, merge or delete tags</Link></div>
        </Card>}

        {tab === "metrics" && <Card id="metrics" title="Is Runout working for you?">
          <span className="text-muted text-sm">Counted on this PC only, never sent anywhere.</span>
          {metrics.data && (
            <div className="tiles grid-cols-2">
              <Metric v={`${metrics.data.capture_median_s} s`} k="Typical time to save a link" />
              <Metric v={`${metrics.data.days_opened_this_week} of 7`} k="Days opened this week" />
              <Metric v={metrics.data.searches_this_week} k="Searches this week" />
              <Metric v={metrics.data.rediscover_visits_this_week} k="Rediscover visits this week" />
              <Metric v={`${metrics.data.moved_after_14d_pct}%`} k="Backlog records moved on within 14 days" />
              <Metric v={`${metrics.data.explored_rated_pct}%`} k="Explored records you rated" />
            </div>
          )}
        </Card>}

        {tab === "data" && <section className="card danger flex flex-wrap justify-between items-center gap-3" aria-labelledby="st-danger">
        <div className="flex flex-col gap-1">
          <h2 id="st-danger" className="card-title text-danger">Delete your library</h2>
          <span className="text-text-2">Removes every record, note and play from this PC. Backups are kept. Uninstalling Runout asks the same question.</span>
        </div>
        <button className="btn btn-danger" onClick={() => setDlg("delete")}>Delete library</button>
        </section>}
      </div>

      {dlg === "restore" && <RestoreDialog backups={backups.data ?? []} onClose={() => setDlg(null)} />}
      {dlg === "shortcut" && <ShortcutDialog onClose={() => setDlg(null)} />}
      {dlg === "connect" && <ConnectDialog onClose={() => setDlg(null)} />}
      {dlg === "disconnect" && <DisconnectDialog onClose={() => setDlg(null)} />}
      {dlg === "delete" && <DeleteLibraryDialog backupDir={st.backup_dir} onClose={() => setDlg(null)} />}
    </>
  );
}

/* ------------------------------------------------------------------ SC-13d restore */
function RestoreDialog({ backups, onClose }: { backups: Backup[]; onClose: () => void }) {
  const toast = useToast();
  const [step, setStep] = useState<{ k: "pick" } | { k: "check"; path: string; label: string } | { k: "ok"; path: string; label: string; check: RestoreCheck } | { k: "fail"; error: string }>({ k: "pick" });
  const [busy, setBusy] = useState(false);
  const check = async (path: string, label: string) => {
    setStep({ k: "check", path, label });
    try {
      const c = await checkBackup(path);
      setStep(c.ok ? { k: "ok", path, label, check: c } : { k: "fail", error: c.error ?? "The file is damaged." });
    } catch (e) { setStep({ k: "fail", error: toAppError(e).message }); }
  };
  const restore = async (path: string, label: string) => {
    setBusy(true);
    try {
      const r = await restoreBackup(path);
      if (!r.ok) { setStep({ k: "fail", error: r.error ?? "The file is damaged." }); return; }
      onClose(); toast(`Restored the backup from ${label}. ${pl(r.counts.library ?? 0, "record")} verified.`);
    } catch (e) { setStep({ k: "fail", error: toAppError(e).message }); } finally { setBusy(false); }
  };
  if (step.k === "check") return (
    <Dialog title="Restore backup" onClose={onClose}>
      <div className="flex flex-wrap gap-2 items-center"><Dots /><span>Checking the backup from {step.label}…</span></div>
      <span className="help">Nothing changes until the check passes.</span>
    </Dialog>);
  if (step.k === "fail") return (
    <Dialog title="Restore backup" onClose={onClose} footer={<button className="btn btn-secondary" onClick={() => setStep({ k: "pick" })}>Pick another</button>}>
      <div className="banner err" role="alert"><span><b className="text-danger">This backup didn’t pass the check.</b> <span className="text-text-2">{step.error} Your current library is unchanged. Try an older backup.</span></span></div>
    </Dialog>);
  if (step.k === "ok") {
    const c = step.check.counts;
    return (
      <Dialog title={`Restore the backup from ${step.label}?`} onClose={onClose}
        footer={<><button className="btn btn-secondary" onClick={onClose}>Cancel</button><button className="btn btn-primary" disabled={busy} onClick={() => restore(step.path, step.label)}>{busy ? <Dots /> : "Restore"}</button></>}>
        <span className="text-text-2">The check passed. It holds {pl(c.library ?? 0, "record")}: {pl(c.albums ?? 0, "album")}, {pl(c.tracks ?? 0, "track")}, {pl(c.artists ?? 0, "artist")}.</span>
        <span className="help">Your current library is kept as a copy until Runout starts again.</span>
      </Dialog>);
  }
  return (
    <Dialog title="Restore a backup" onClose={onClose}
      footer={<><button className="btn btn-ghost" onClick={() => pickBackup().then((p) => { if (p) check(p, "the file you picked"); })}>Choose a file</button><button className="btn btn-secondary" onClick={onClose}>Cancel</button></>}>
      <span className="text-text-2">Your current library is kept as a copy until Runout starts again.</span>
      {backups.length ? (
        <div className="card pad-s"><div className="list">{backups.map((b, i) => (
          <div key={b.path} className="row grid-cols-[minmax(0,1fr)_70px_auto]">
            <span className="font-mono text-xs">{backupLabel(b, i)}</span><span className="font-mono text-muted text-xs">{mb(b.size_bytes)}</span>
            <button className="btn btn-secondary s" onClick={() => check(b.path, day(b.created_at))} aria-label={`Restore the backup from ${day(b.created_at)}`}>Restore</button>
          </div>))}
        </div></div>
      ) : <span className="text-muted">No backups in your backup folder yet. Choose a backup file instead.</span>}
    </Dialog>
  );
}

function DisconnectDialog({ onClose }: { onClose: () => void }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<AppError>();
  const go = async () => {
    setBusy(true);
    try { await disconnectSpotify(); onClose(); toast("Spotify disconnected. Everything Runout cached from Spotify is deleted."); }
    catch (e) { setErr(toAppError(e)); } finally { setBusy(false); }
  };
  return (
    <Dialog title="Disconnect Spotify?" onClose={onClose}
      footer={<><button className="btn btn-secondary" onClick={onClose}>Cancel</button><button className="btn btn-danger" disabled={busy} onClick={go}>{busy ? <Dots /> : "Disconnect"}</button></>}>
      <span className="text-text-2">Runout deletes everything it cached from Spotify and any undo snapshots for playlists. Your library, ratings and listening history stay.</span>
      {err && <ErrorBanner error={err} title="Couldn’t disconnect." />}
    </Dialog>
  );
}

/* ------------------------------------------------------------------ SC-13f delete library */
function DeleteLibraryDialog({ backupDir, onClose }: { backupDir: string; onClose: () => void }) {
  const toast = useToast();
  const [v, setV] = useState("");
  const [help, setHelp] = useState("");
  const [busy, setBusy] = useState(false);
  const go = async () => {
    if (v !== "DELETE") { setHelp("Type DELETE in capitals to confirm."); return; }
    setBusy(true);
    try { await deleteLibrary(v); onClose(); toast("Library deleted. Your backups are untouched."); }
    catch (e) { setHelp(toAppError(e).message); } finally { setBusy(false); }
  };
  return (
    <Dialog title="Delete your whole library?" onClose={onClose}
      footer={<><button className="btn btn-secondary" onClick={onClose}>Cancel</button><button className="btn btn-danger" disabled={busy} onClick={go}>{busy ? <Dots /> : "Delete library"}</button></>}>
      <span className="text-text-2">Every record, note, tag and play on this PC is removed. Backups in {backupDir} are kept, so you can restore later.</span>
      <div className="field">
        <label className="label" htmlFor="del-confirm">Type DELETE to confirm</label>
        <input id="del-confirm" className={`input${help ? " error" : ""}`} autoFocus autoComplete="off" value={v}
          onChange={(e) => { setV(e.target.value); setHelp(""); }} onKeyDown={(e) => e.key === "Enter" && go()} aria-describedby="del-help" />
        <span className="help err" id="del-help" role={help ? "alert" : undefined}>{help}</span>
      </div>
    </Dialog>
  );
}

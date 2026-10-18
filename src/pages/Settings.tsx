// SC-13 Settings and data (+ SC-13c connect, SC-13d restore, SC-13e Shortcut steps, SC-13f delete library).
// Tray menu and the NSIS uninstall dialog are native, not React.
import { useEffect, useState, type ReactNode } from "react";
import { Link } from "react-router-dom";
import {
  backupNow, checkBackup, chooseBackupDir, deleteLibrary, disconnectSpotify, exportLibrary, getCapture, getLoggerStatus,
  getSettings, getSpotifyStatus, getSuccessMetrics, listBackups, listImports, listTags, moveLibrary, newPairingCode, onEvent, pauseLogger,
  pickBackup, restoreBackup, resumeLogger, setCaptureEnabled, setLogger, updateSettings, toAppError, type AppError, type Backup, type RestoreCheck,
} from "../api";
import { Topbar } from "../components/Shell";
import { ConnectDialog, ShortcutDialog } from "../components/SetupDialogs";
import { Dialog, Dots, ErrorBanner, SkelRows, useToast } from "../components/ui";
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
function Switch({ label, on, onChange, disabled }: { label: string; on: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return <label className="flex justify-between items-center gap-3"><span>{label}</span><input type="checkbox" checked={on} disabled={disabled} onChange={(e) => onChange(e.target.checked)} /></label>;
}
const Metric = ({ v, k }: { v: ReactNode; k: string }) =>
  <div className="flex flex-col gap-0.5"><span className="font-mono text-xl font-semibold">{v}</span><span className="text-muted text-xs">{k}</span></div>;

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
  const imports = useApi(() => (r2 ? listImports() : Promise.resolve([])), [r2]);
  const tags = useApi(listTags);
  const metrics = useApi(getSuccessMetrics);
  const { busy, err, run } = useRun();
  const [dlg, setDlg] = useState<null | "restore" | "shortcut" | "connect" | "disconnect" | "delete">(null);
  const [capOff, setCapOff] = useState<"wrong_code" | "error">();
  const [warn, setWarn] = useState("");
  useEffect(() => onEvent("capture-disabled", (p) => setCapOff(p.reason)), []);
  useEffect(() => { if (s.data) setWarn(String(s.data.size_warn_mb)); }, [s.data]);

  const head = <><Topbar /><h1 className="page-title">Settings and data</h1></>;
  if (s.loading) return <>{head}<SkelRows n={6} /></>;
  if (s.error || !s.data) return <>{head}<ErrorBanner title="Couldn’t load your settings." error={s.error} onRetry={s.reload} /></>;
  const st = s.data;
  const latest = backups.data?.[0];
  const lastImport = imports.data?.[0];
  const lg = logger.data;
  const sp = spotify.data;

  return (
    <>
      {head}
      {err && <ErrorBanner error={err} title="That didn’t work." />}
      {latest?.size_warning && (
        <div className="banner err" role="alert">
          <span><b className="text-danger">Last backup was larger than your limit.</b> <span className="text-text-2">It was {mb(latest.size_bytes)}; your limit is {st.size_warn_mb} MB. The backup was still kept.</span></span>
          <button className="btn btn-secondary s" onClick={() => run("limit", () => updateSettings({ size_warn_mb: st.size_warn_mb + 50 })).then((r) => r && toast(`Limit raised to ${r.size_warn_mb} MB`))}>Raise the limit</button>
        </div>
      )}
      <div className="tiles grid-cols-[repeat(auto-fill,minmax(min(100%,420px),1fr))] items-start">
        <Card id="data" title="Your data">
          <div className="field">
            <span className="label">Library location</span>
            <span className="font-mono text-xs">{st.library_path}</span>
            <button className="btn btn-secondary s self-start" disabled={!!busy}
              onClick={() => run("move", moveLibrary).then((p) => p && toast(`Library moved to ${p}. A library moved elsewhere isn’t removed when you uninstall.`))}>
              {busy === "move" ? <Dots /> : "Change location"}
            </button>
          </div>
          <div className="field">
            <span className="label">Backups, newest 3 kept</span>
            {backups.loading ? <div className="skel h-[60px]" /> : backups.data?.length
              ? backups.data.map((b, i) => <span key={b.path} className="flex justify-between items-baseline gap-3"><span>{backupLabel(b, i)}</span><span className="font-mono text-text-2 text-xs">{mb(b.size_bytes)}</span></span>)
              : <span className="text-muted">No backups yet. Back up now to make the first one.</span>}
            <span className="flex justify-between items-baseline gap-3">
              <span className="font-mono text-text-2 text-xs">{st.backup_dir}</span>
              <button className="btn btn-ghost s" disabled={!!busy} onClick={() => run("dir", async () => { const d = await chooseBackupDir(); if (d) { await updateSettings({ backup_dir: d }); toast(`Backups now go to ${d}`); } })}>Change folder</button>
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
        </Card>

        <Card id="spotify" title="Spotify connection">
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
        </Card>

        {r2 && (
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

        <Card id="phone" title="Phone capture">
          {cap.loading || !cap.data ? <div className="skel h-[90px]" /> : capOff ? <>
              <div className="banner err" role="alert"><span><b className="text-danger">Phone capture turned itself off.</b> <span className="text-text-2">{capOff === "wrong_code" ? "5 wrong pairing codes in a row. Make a new code and update your Shortcut." : "The listener stopped. Turn it on again to keep saving from your iPhone."}</span></span></div>
              <div><button className="btn btn-primary" onClick={() => run("code", async () => { await newPairingCode(); await setCaptureEnabled(true); }).then(() => { setCapOff(undefined); toast("New pairing code. Update it in your iPhone Shortcut."); })}>New code and turn on</button></div>
            </> : <>
              <Switch label="Accept shares from my iPhone on this Wi-Fi" on={cap.data.on} onChange={(v) => run("cap", () => setCaptureEnabled(v))} />
              <div className="tiles grid-cols-2">
                <div className="field"><span className="label">Address</span><span className="font-mono">{cap.data.address}:{cap.data.port}</span></div>
                <div className="field"><span className="label">Pairing code</span><span className="font-mono text-[18px] tracking-[2px]">{cap.data.code}</span></div>
              </div>
              <span className="text-muted text-xs">Windows may ask to allow Runout on private networks. Your Wi-Fi must be set to Private.</span>
              <div className="flex flex-wrap gap-2 items-center">
                <button className="btn btn-light" onClick={() => setDlg("shortcut")}>Shortcut steps</button>
                <button className="btn btn-secondary" onClick={() => run("code", newPairingCode).then((c) => c && toast("New pairing code. Update it in your iPhone Shortcut."))}>New code</button>
              </div>
            </>}
        </Card>

        {r2 && (
          <Card id="import" title="Spotify history">
            {lastImport ? <span>Last import covers plays up to {dayY(lastImport.latest_event_utc)}.</span> : <span>No export imported yet.</span>}
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

        <Card id="tags" title="Tags">
          <span className="text-text-2">{tags.data ? `${pl(tags.data.length, "tag")} in use.` : " "}</span>
          <div><Link className="btn btn-secondary" to="/settings/tags">Rename, merge or delete tags</Link></div>
        </Card>

        <Card id="metrics" title="Is Runout working for you?">
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
        </Card>
      </div>

      <section className="card danger flex flex-wrap justify-between items-center gap-3" aria-labelledby="st-danger">
        <div className="flex flex-col gap-1">
          <h2 id="st-danger" className="card-title text-danger">Delete your library</h2>
          <span className="text-text-2">Removes every record, note and play from this PC. Backups are kept. Uninstalling Runout asks the same question.</span>
        </div>
        <button className="btn btn-danger" onClick={() => setDlg("delete")}>Delete library</button>
      </section>

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
            <span>{backupLabel(b, i)}</span><span className="font-mono text-muted text-xs">{mb(b.size_bytes)}</span>
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

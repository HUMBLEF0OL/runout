// SC-10 Import your Spotify history: choose → check → done (CAP-19). Files are read on this PC; nothing is uploaded.
import { useState } from "react";
import { Link, Navigate, useLocation, useNavigate, useParams } from "react-router-dom";
import * as api from "../api";
import type { AppError, ImportResult } from "../api";
import { Dots, Empty, ErrorBanner } from "../components/ui";
import { Topbar } from "../components/Shell";
import { useApi, useOnline } from "../hooks/useApi";
import { dayY, num, pl } from "../lib/format";

const shortPath = (p: string) => p.split(/[\\/]/).slice(-2).join("\\");
const UNRECOGNIZED = "This folder doesn't look like a Spotify export.";

function Choose() {
  const nav = useNavigate();
  const online = useOnline();
  const [path, setPath] = useState<string | null>(null);
  const [err, setErr] = useState<AppError | undefined>((useLocation().state as { error?: AppError } | null)?.error);
  const [busy, setBusy] = useState(false);
  const choose = () => { setBusy(true); setErr(undefined); api.pickImportSource().then(setPath, (e) => setErr(api.toAppError(e))).finally(() => setBusy(false)); };
  return <>
    <div className="card flex flex-col gap-3 items-center text-center p-10 border border-dashed border-line-strong">
      <b className="text-lg">Choose the folder or .zip from Spotify's email</b>
      <span className="text-text-2">Runout reads it on this PC; nothing is uploaded.</span>
      <button className="btn btn-secondary" onClick={choose} disabled={busy}>{busy ? <Dots /> : "Choose folder or .zip"}</button>
      {path && <span className="font-mono text-text-2 text-xs">{shortPath(path)} (selected)</span>}
    </div>
    {err && <ErrorBanner error={err.code === "import_unrecognized" ? { ...err, message: "Choose the unzipped folder from Spotify's email, or the .zip itself." } : err}
      title={err.code === "import_unrecognized" ? UNRECOGNIZED : "Couldn't open that."} />}
    {!online && <span className="text-muted text-xs">Importing works offline. The files are read from your PC.</span>}
    <div className="flex flex-wrap gap-2 items-center justify-end">
      <Link className="btn btn-secondary" to="/settings?tab=import">Cancel</Link>
      <button className="btn btn-primary" disabled={!path} onClick={() => nav("/import/2", { state: { path } })}>Next</button>
    </div>
  </>;
}

function Check({ path }: { path: string }) {
  const nav = useNavigate();
  const pv = useApi(() => api.previewImport(path), [path]);
  const [running, setRunning] = useState(false);
  const [runErr, setRunErr] = useState<AppError>();
  if (pv.error?.code === "import_unrecognized") return <Navigate to="/import/1" replace state={{ error: pv.error }} />;
  if (pv.loading) return <div className="card flex flex-col gap-3"><div className="flex flex-wrap gap-2 items-center"><Dots /><span>Reading the export…</span></div></div>;
  if (pv.error) return <ErrorBanner error={pv.error} title="Couldn't read the export." onRetry={pv.reload} />;
  const p = pv.data!;
  if (!p.music) return <Empty h="No music plays in this export" line="It may hold only account details. Request Extended streaming history from Spotify for your plays."><Link className="btn btn-secondary" to="/import/1">Choose another</Link></Empty>;
  const run = () => {
    setRunning(true); setRunErr(undefined);
    api.runImport(path).then((result) => nav("/import/3", { state: { result, preview: p } }), (e) => { setRunErr(api.toAppError(e)); setRunning(false); });
  };
  const rows: [string, string, number, "" | "text-muted" | "text-accent"][] = [
    ["Music plays", `${dayY(p.from)} to ${dayY(p.until)}`, p.music, ""],
    ["Podcasts and audiobooks", "Skipped. Runout keeps music only", p.podcasts_skipped, "text-muted"],
    ["Already on your desktop log", "Merged with the export, never counted twice", p.overlap, "text-muted"],
    ["Playlists", "For the read-only playlist check-up", p.playlists, ""],
    ["Private-session plays", "Kept, but hidden from insights unless you turn them on", p.private, "text-accent"],
  ];
  return <>
    <div className="card flex flex-col gap-3">
      <div className="flex justify-between items-baseline gap-3"><h2 className="card-title">What we found</h2><span className="font-mono text-muted text-xs">{shortPath(path)}</span></div>
      {rows.map(([w, n, c, m]) => (
        <div key={w} className="flex justify-between items-baseline gap-3 py-1.5">
          <span className="flex flex-col gap-0.5"><span className="font-semibold">{w}</span><span className="text-muted text-xs">{n}</span></span>
          <span className={`font-mono text-base ${m}`}>{num(c)}</span>
        </div>
      ))}
    </div>
    <div className="flex flex-wrap gap-7 items-start">
      <div className="card flex-[1.5_1_520px] min-w-0 flex flex-col gap-3"><h2 className="card-title">What Runout keeps</h2><span className="text-text-2 leading-[1.55]">When each play ended, how long it played, the track, artist, album and link, the device type, how it started and ended, shuffle, skip, offline and private-session flags.</span></div>
      <div className="card flex-[1_1_320px] min-w-0 flex flex-col gap-3"><h2 className="card-title">What it drops</h2><span className="text-text-2 leading-[1.55]">Your IP address, user agent and username are never stored. The original files are not copied.</span></div>
    </div>
    {running && <span className="text-muted text-xs">Importing… You can keep using Runout while this runs.</span>}
    {runErr && <ErrorBanner error={runErr} title="Import stopped. Nothing was changed." onRetry={run} />}
    <div className="flex flex-wrap gap-2 items-center justify-end">
      <Link className="btn btn-secondary" to="/import/1">Back</Link>
      <button className="btn btn-primary" onClick={run} disabled={running}>{running ? <Dots /> : `Import ${pl(p.music, "play")}`}</button>
    </div>
  </>;
}

function Done({ result, preview }: { result: ImportResult; preview: api.ImportPreview }) {
  if (!result.added) return <Empty h="Nothing new to import" line={`Every play in this export is already in Runout. Your history covers up to ${dayY(preview.until)}.`}><Link className="btn btn-secondary" to="/insights">Open insights</Link></Empty>;
  return (
    <div className="card flex flex-col gap-3 p-7">
      <b className="text-xl">Imported {pl(result.added, "play")}.</b>
      <span className="text-text-2">{num(result.duplicates)} matched your desktop log and were merged. {num(preview.podcasts_skipped)} podcast entries skipped. Insights now cover {dayY(preview.from)} to {dayY(preview.until)}.</span>
      <div className="flex flex-wrap gap-2 items-center"><Link className="btn btn-primary" to="/insights">See your insights</Link><Link className="btn btn-secondary" to="/plays/missing">Review plays not in your library</Link></div>
      <span className="text-muted text-xs">Importing a newer export later only adds what's new.</span>
    </div>
  );
}

export default function Import() {
  const step = Number(useParams().step ?? 1);
  const st = (useLocation().state ?? {}) as { path?: string; result?: ImportResult; preview?: api.ImportPreview };
  let body;
  if (step === 2) body = st.path ? <Check path={st.path} /> : <Navigate to="/import/1" replace />;
  else if (step === 3) body = st.result && st.preview ? <Done result={st.result} preview={st.preview} /> : <Navigate to="/import/1" replace />;
  else body = <Choose />;
  return (
    <>
      <Topbar />
      <Link to="/settings?tab=import" className="text-sm font-semibold">Back to settings</Link>
      <div className="flex flex-col gap-1.5">
        <h1 className="page-title">Import your Spotify history</h1>
        <span className="text-muted">Uses the files Spotify emails you after you request your data.</span>
      </div>
      <ol className="steps" aria-label="Steps">
        {["Choose files", "Check", "Import"].map((t, i) => <li key={t} aria-current={i + 1 === step ? "step" : undefined} className={i + 1 < step ? "done" : ""}>{i + 1}. {t}</li>)}
      </ol>
      {body}
    </>
  );
}

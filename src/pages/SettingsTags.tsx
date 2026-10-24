// SC-13b Tags: counts, rename, merge, delete (confirm).
import { useState } from "react";
import { Link } from "react-router-dom";
import { deleteTag, listTags, mergeTags, renameTag, toAppError, type AppError, type Tag } from "../api";
import { Topbar } from "../components/Shell";
import { Dialog, Dots, Empty, ErrorBanner, SkelRows, useToast } from "../components/ui";
import { useApi } from "../hooks/useApi";
import { pl } from "../lib/format";

type Act = { k: "rename" | "merge" | "delete"; tag: Tag };

export default function SettingsTags() {
  const tags = useApi(listTags);
  const [act, setAct] = useState<Act | null>(null);
  const rows = [...(tags.data ?? [])].sort((a, b) => b.item_count - a.item_count || a.name.localeCompare(b.name));
  return (
    <>
      <Topbar />
      <Link to="/settings?tab=tags" className="text-sm font-semibold">Back to settings</Link>
      <h1 className="page-title">Tags</h1>
      {tags.loading ? <SkelRows n={6} />
        : tags.error ? <ErrorBanner title="Couldn’t load your tags." error={tags.error} onRetry={tags.reload} />
        : !rows.length ? <Empty h="No tags yet" line="Add tags on any record to group music your way."><Link className="btn btn-secondary" to="/library">Open your library</Link></Empty>
        : (
          <div className="card pad-s"><div className="list">{rows.map((t) => (
            <div key={t.id} className="row grid-cols-[minmax(0,1fr)_100px_auto]">
              <span className="chip justify-self-start">{t.name}</span>
              <span className="font-mono text-muted text-xs">{pl(t.item_count, "record")}</span>
              <span className="flex flex-wrap gap-2 items-center">
                <button className="btn btn-secondary s" onClick={() => setAct({ k: "rename", tag: t })} aria-label={`Rename ${t.name}`}>Rename</button>
                <button className="btn btn-secondary s" disabled={rows.length < 2} onClick={() => setAct({ k: "merge", tag: t })} aria-label={`Merge ${t.name} into another tag`}>Merge into…</button>
                <button className="btn btn-danger s" onClick={() => setAct({ k: "delete", tag: t })} aria-label={`Delete ${t.name}`}>Delete</button>
              </span>
            </div>))}
          </div></div>
        )}
      {act && <TagDialog act={act} all={rows} onClose={() => setAct(null)} />}
    </>
  );
}

function TagDialog({ act, all, onClose }: { act: Act; all: Tag[]; onClose: () => void }) {
  const toast = useToast();
  const { k, tag } = act;
  const others = all.filter((t) => t.id !== tag.id);
  const [name, setName] = useState(tag.name);
  const [into, setInto] = useState(others[0]?.id ?? 0);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<AppError>();
  const go = async () => {
    setBusy(true); setErr(undefined);
    try {
      if (k === "rename") { const n = name.trim().toLowerCase(); if (!n || n === tag.name) { onClose(); return; } await renameTag(tag.id, n); toast(`Renamed ${tag.name} to ${n}`); }
      if (k === "merge") { const t = await mergeTags(tag.id, into); toast(`Merged ${tag.name} into ${t.name}`); }
      if (k === "delete") { await deleteTag(tag.id); toast(`Tag ${tag.name} removed from ${pl(tag.item_count, "record")}`); }
      onClose();
    } catch (e) { setErr(toAppError(e)); } finally { setBusy(false); }
  };
  const cancel = <button className="btn btn-secondary" onClick={onClose}>Cancel</button>;
  const label = k === "rename" ? "Rename" : k === "merge" ? "Merge" : "Delete tag";
  const confirm = <button className={`btn ${k === "delete" ? "btn-danger" : "btn-primary"}`} disabled={busy} onClick={go}>{busy ? <Dots /> : label}</button>;
  return (
    <Dialog title={k === "rename" ? `Rename “${tag.name}”` : k === "merge" ? `Merge “${tag.name}” into another tag` : `Delete the tag “${tag.name}”?`}
      onClose={onClose} footer={<>{cancel}{confirm}</>}>
      {k === "rename" && <div className="field"><label className="label" htmlFor="tr">New name</label>
        <input id="tr" className="input" autoFocus value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && go()} /></div>}
      {k === "merge" && <>
        <div className="field"><label className="label" htmlFor="tm">Merge into</label>
          <select id="tm" className="input" value={into} onChange={(e) => setInto(Number(e.target.value))}>{others.map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}</select></div>
        <span className="help">{pl(tag.item_count, "record")} move to the chosen tag; records that already have both keep it once.</span>
      </>}
      {k === "delete" && <span className="text-text-2">It’s removed from {pl(tag.item_count, "record")}. The records stay.</span>}
      {err && <ErrorBanner error={err} title="That didn’t work." />}
    </Dialog>
  );
}

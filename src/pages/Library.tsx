// SC-04 Library: album / track / artist tabs, status and rating filters, 20 per page, artwork toggle, save as filter.
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { getCounts, listItems, saveFilter, toAppError, type AppError, type ItemFilter, type ItemType, type Status } from "../api";
import { Topbar } from "../components/Shell";
import { useQuickAdd } from "../components/QuickAddContext";
import { ArtTiles, Dialog, Empty, ErrorBanner, RecordCard, Seg, SkelTiles, Switch, TabPanel, useSlide, useToast } from "../components/ui";
import { useApi } from "../hooks/useApi";
import { pl, STATUS, STATUS_ORDER, statusMeta, TYPE } from "../lib/format";

const PER = 20;
const TABS: ItemType[] = ["album", "track", "artist"];

export default function Library() {
  const p = useParams().type;
  const tab: ItemType = TABS.includes(p as ItemType) ? (p as ItemType) : "album";
  const nav = useNavigate();
  const quickAdd = useQuickAdd();
  const toast = useToast();
  const [status, setStatusF] = useState<Status | "any">("any");
  const [rating, setRating] = useState("any");
  const [page, setPage] = useState(1);
  const [art, setArt] = useState(false);
  const [saving, setSaving] = useState(false);
  const filter: ItemFilter = { type: tab, ...(status !== "any" ? { status: [status] } : {}), ...(rating !== "any" ? { min_rating: Number(rating) } : {}) };
  const q = useApi(() => listItems({ ...filter, sort: "modified_at", offset: (page - 1) * PER, limit: PER }), [tab, status, rating, page]);
  const counts = useApi(getCounts).data;
  const n = { album: counts?.albums ?? 0, track: counts?.tracks ?? 0, artist: counts?.artists ?? 0 };
  const filtered = status !== "any" || rating !== "any";
  const [dir, slide] = useSlide(TABS, tab);
  const reset = () => { setStatusF("any"); setRating("any"); setPage(1); };

  const tabs = <Seg<ItemType> label="Type" options={TABS.map((t) => [t, `${TYPE[t]}s ${n[t]}`])} value={tab} onChange={(t) => { slide(t); setPage(1); nav(`/library/${t}`); }} />;
  const head = (
    <div className="page-head">
      <div className="flex flex-col gap-2.5"><h1 className="page-title">Library</h1>{tabs}</div>
      <div className="flex flex-wrap gap-2 items-center">
      <label className="label" htmlFor="lb-status">Status</label>
      <select id="lb-status" className="input surface" value={status} onChange={(e) => { setStatusF(e.target.value as Status | "any"); setPage(1); }}>
        <option value="any">Any</option>{STATUS_ORDER.map((s) => <option key={s} value={s}>{STATUS[s]}</option>)}
      </select>
      <label className="label" htmlFor="lb-rating">Rating</label>
      <select id="lb-rating" className="input surface" value={rating} onChange={(e) => { setRating(e.target.value); setPage(1); }}>
        <option value="any">Any</option>{[3, 4, 4.5, 5].map((r) => <option key={r} value={r}>{r} and up</option>)}
      </select>
      <button className="btn btn-secondary" onClick={() => setSaving(true)}>Save as filter</button>
      <Link className="btn btn-secondary" to="/settings/tags">Manage tags</Link>
      <Switch label="Artwork" on={art} onChange={async (v) => { setArt(v); return true; }} />
      </div>
    </div>
  );

  let body;
  if (q.loading) body = <SkelTiles n={10} />;
  else if (q.error) body = <ErrorBanner title="Couldn’t load your library." error={q.error} onRetry={q.reload} />;
  else if (!q.data?.total && !filtered && !counts?.library) body = <Empty h="Your library is empty" line="Albums, tracks and artists you save show up here."><button className="btn btn-primary" onClick={() => quickAdd()}>Add a link</button></Empty>;
  else if (!q.data?.total) body = <Empty h="Nothing matches" line="No records fit these filters.">{filtered && <button className="btn btn-secondary" onClick={reset}>Clear filters</button>}</Empty>;
  else {
    const { rows, total, offset } = q.data;
    const pages = Math.ceil(total / PER);
    body = <>
      <span className="text-muted text-sm">
        {pl(total, TYPE[tab].toLowerCase())}{filtered ? ` matching ${status !== "any" ? STATUS[status].toLowerCase() : "any status"}${rating !== "any" ? `, rated ${rating} and up` : ""}` : ""}. Sorted by last change.
      </span>
      {art ? <ArtTiles items={rows} total={total} meta={statusMeta} />
        : <div className="tiles">{rows.map((it) => <RecordCard key={it.id} it={it} act={it.status === "in_progress"} />)}</div>}
      {pages > 1 && <nav className="flex flex-wrap gap-2 items-center" aria-label="Pages">
        {Array.from({ length: pages }, (_, i) => (
          <button key={i} className={`btn ${i + 1 === page ? "btn-light" : "btn-secondary"} s`} aria-current={i + 1 === page ? "page" : undefined} aria-label={`Page ${i + 1}`} onClick={() => setPage(i + 1)}>{i + 1}</button>))}
        <span className="font-mono text-faint text-xs">{offset + 1} to {offset + rows.length} of {total}</span>
      </nav>}
    </>;
  }

  return (
    <>
      <Topbar />
      {head}
      <TabPanel key={tab} dir={dir} label={`${TYPE[tab]}s`} className="flex flex-col gap-6">{body}</TabPanel>
      {saving && <SaveFilter filter={filter} onClose={() => setSaving(false)} onSaved={() => { setSaving(false); toast("Filter saved"); }} />}
    </>
  );
}

function SaveFilter({ filter, onClose, onSaved }: { filter: ItemFilter; onClose: () => void; onSaved: () => void }) {
  const [name, setName] = useState("");
  const [err, setErr] = useState<AppError>();
  const save = async () => {
    if (!name.trim()) return;
    try { await saveFilter(name.trim(), filter); onSaved(); } catch (e) { setErr(toAppError(e)); }
  };
  return (
    <Dialog title="Save this filter" onClose={onClose} footer={<>
      <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
      <button className="btn btn-primary" disabled={!name.trim()} onClick={save}>Save</button></>}>
      <form onSubmit={(e) => { e.preventDefault(); save(); }} className="field">
        <label className="label" htmlFor="sf-name">Name</label>
        <input id="sf-name" className="input" autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="For example, Explored, 4 and up" />
      </form>
      {err && <ErrorBanner title="Couldn’t save the filter." error={err} />}
      <span className="help">Saved filters appear in Rediscover and can become a Spotify playlist.</span>
    </Dialog>
  );
}

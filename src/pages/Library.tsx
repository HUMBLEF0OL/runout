// SC-04 Library: album / track / artist tabs, status, rating and sort, artwork toggle, save as filter.
// M9 (taste + Motion): filters on their own row, a result line that can clear them, collection cards that rise in,
// and "Show more" that appends (cards keep their place; new ones rise) instead of numbered pages.
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { m, useReducedMotion } from "motion/react";
import { getCounts, listItems, saveFilter, toAppError, type AppError, type ItemFilter, type ItemType, type Status } from "../api";
import { Topbar } from "../components/Shell";
import { useQuickAdd } from "../components/QuickAddContext";
import { ArtTiles, CollectionCard, Dialog, Empty, ErrorBanner, Seg, SkelTiles, spot, Switch, TabPanel, useSlide, useToast } from "../components/ui";
import { useApi } from "../hooks/useApi";
import { num, pl, STATUS, STATUS_ORDER, statusMeta, TYPE } from "../lib/format";

const PER = 20;
const TABS: ItemType[] = ["album", "track", "artist"];
type Sort = NonNullable<ItemFilter["sort"]>;
// Directions are the API defaults: dates and rating newest/highest first, name A to Z.
const SORTS: [Sort, string][] = [["modified_at", "Last change"], ["added_at", "Recently added"], ["rating", "Rating"], ["name", "Name"]];
const RATINGS = [3, 4, 4.5, 5];

export default function Library() {
  const p = useParams().type;
  const tab: ItemType = TABS.includes(p as ItemType) ? (p as ItemType) : "album";
  const nav = useNavigate();
  const quickAdd = useQuickAdd();
  const toast = useToast();
  const reduce = useReducedMotion();
  const [status, setStatusF] = useState<Status | "any">("any");
  const [rating, setRating] = useState("any");
  const [sort, setSort] = useState<Sort>("modified_at");
  const [shown, setShown] = useState(PER);
  const [art, setArt] = useState(false);
  const [saving, setSaving] = useState(false);
  const filter: ItemFilter = { type: tab, ...(status !== "any" ? { status: [status] } : {}), ...(rating !== "any" ? { min_rating: Number(rating) } : {}) };
  const q = useApi(() => listItems({ ...filter, sort, offset: 0, limit: shown }), [tab, status, rating, sort, shown]);
  const counts = useApi(getCounts).data;
  const n = { album: counts?.albums ?? 0, track: counts?.tracks ?? 0, artist: counts?.artists ?? 0 };
  const filtered = status !== "any" || rating !== "any";
  const [dir, slide] = useSlide(TABS, tab);
  const reset = () => { setStatusF("any"); setRating("any"); setShown(PER); };
  const more = q.data && q.data.rows.length < q.data.total;

  const head = (
    <div className="flex flex-col gap-3.5">
      <div className="flex flex-col gap-1">
        <h1 className="page-title">Library</h1>
        {counts && <span className="text-muted">{pl(counts.library, "record")}: {pl(n.album, "album")}, {pl(n.track, "track")}, {pl(n.artist, "artist")}</span>}
      </div>
      <Seg<ItemType> label="Type" options={TABS.map((t) => [t, `${TYPE[t]}s`])} value={tab} onChange={(t) => { slide(t); setShown(PER); nav(`/library/${t}`); }} />
      <div className="lb-bar">
        <div className="lb-group">
          <label className="label" htmlFor="lb-status">Status</label>
          <select id="lb-status" className="input surface" value={status} onChange={(e) => { setStatusF(e.target.value as Status | "any"); setShown(PER); }}>
            <option value="any">Any</option>{STATUS_ORDER.map((s) => <option key={s} value={s}>{STATUS[s]}</option>)}
          </select>
          <label className="label" htmlFor="lb-rating">Rating</label>
          <select id="lb-rating" className="input surface" value={rating} onChange={(e) => { setRating(e.target.value); setShown(PER); }}>
            <option value="any">Any</option>{RATINGS.map((r) => <option key={r} value={r}>{r} and up</option>)}
          </select>
          <label className="label" htmlFor="lb-sort">Sort</label>
          <select id="lb-sort" className="input surface" value={sort} onChange={(e) => { setSort(e.target.value as Sort); setShown(PER); }}>
            {SORTS.map(([v, t]) => <option key={v} value={v}>{t}</option>)}
          </select>
        </div>
        <div className="lb-group">
          <button className="btn btn-secondary" onClick={() => setSaving(true)}>Save as filter</button>
          <Link className="btn btn-ghost" to="/settings/tags">Manage tags</Link>
          <Switch label="Artwork" on={art} onChange={async (v) => { setArt(v); return true; }} />
        </div>
      </div>
    </div>
  );

  let body;
  if (q.loading) body = <SkelTiles n={10} />;
  else if (q.error) body = <ErrorBanner title="Couldn’t load your library." error={q.error} onRetry={q.reload} />;
  else if (!q.data?.total && !filtered && !counts?.library) body = <Empty h="Your library is empty" line="Albums, tracks and artists you save show up here."><button className="btn btn-primary" onClick={() => quickAdd()}>Add a link</button></Empty>;
  else if (!q.data?.total) body = <Empty h="Nothing matches" line="No records fit these filters.">{filtered && <button className="btn btn-secondary" onClick={reset}>Clear filters</button>}</Empty>;
  else {
    const { rows, total } = q.data;
    const what = filtered ? ` ${status !== "any" ? STATUS[status].toLowerCase() : "any status"}${rating !== "any" ? `, rated ${rating} and up` : ""}` : "";
    body = <>
      <p className="lb-result">
        <span><b className="font-mono">{num(total)}</b> {pl(total, TYPE[tab].toLowerCase()).replace(/^[\d,]+ /, "")}{what}, by {SORTS.find((x) => x[0] === sort)![1].toLowerCase()}</span>
        {filtered && <button className="btn btn-ghost s" onClick={reset}>Clear filters</button>}
      </p>
      {art ? <ArtTiles items={rows} total={total} meta={statusMeta} />
        : <m.div className="lb-grid" initial={reduce ? false : "hidden"} animate="show">
            {rows.map((it, i) => <CollectionCard key={it.id} it={it} i={i % PER} />)}
          </m.div>}
      {!art && (
        <div className="lb-more">
          <span className="font-mono text-faint text-xs">{num(rows.length)} of {num(total)}</span>
          {more && <button className="btn btn-secondary" onClick={() => setShown((s) => s + PER)}>Show {Math.min(PER, total - rows.length)} more</button>}
        </div>
      )}
    </>;
  }

  return (
    <>
      <Topbar />
      {head}
      <TabPanel key={tab} dir={dir} label={`${TYPE[tab]}s`} className="lit flex flex-col gap-4" onPointerMove={spot}>{body}</TabPanel>
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
      <span className="help">Saved filters appear in Rediscover as shelves and can become a Spotify playlist.</span>
    </Dialog>
  );
}

// SC-07 Rediscover: shelves from your own rules, filters, saved filters (FR-013); R2 "Loved, not played lately";
// R3 "Make a Spotify playlist" (SC-07b, FR-039). Nothing here is chosen by Spotify.
import { useState, type ReactNode } from "react";
import { Link } from "react-router-dom";
import {
  createPlaylistFromFilter, createPlaylistFromItems, deleteSavedFilter, getRediscover, getTakeInsights, listItems, listSavedFilters, listTags,
  saveFilter, toAppError, type AppError, type ItemFilter, type ItemSummary, type SavedFilter, type Status,
} from "../api";
import { Topbar } from "../components/Shell";
import { Dialog, Dots, Empty, ErrorBanner, ItemSet, SkelTiles, Attribution, useToast } from "../components/ui";
import { useApi, useOnline } from "../hooks/useApi";
import { useRelease } from "../hooks/useRelease";
import { age, nameOf, pl, stars, STATUS, STATUS_ORDER, TYPE } from "../lib/format";
import { safeUri } from "../lib/sanitize";

interface Shelf { name: string; rule: string; items: ItemSummary[]; meta: (it: ItemSummary) => ReactNode; act?: boolean }
interface Filter { tag: string; min: number; revisit: boolean }
/** Page filters narrow every shelf; artists never sit on shelves (they have no tracks to play back). */
export const shelfFilter = (f: Filter) => (it: ItemSummary) =>
  it.type !== "artist" && (!f.tag || it.tags.includes(f.tag)) && (it.rating ?? 0) >= f.min && (!f.revisit || it.revisit);

type Make = { title: string; items: ItemSummary[]; kind: "shelf" | "filter"; filterId?: number };

export default function Rediscover() {
  const { r2, r3 } = useRelease();
  const online = useOnline();
  const toast = useToast();
  const red = useApi(getRediscover);
  const take = useApi(() => (r2 ? getTakeInsights() : Promise.resolve(null)), [r2]);
  const saved = useApi(listSavedFilters);
  const tags = useApi(listTags);
  const [f, setF] = useState<Filter>({ tag: "", min: 0, revisit: false });
  const [art, setArt] = useState(false);
  const [active, setActive] = useState<SavedFilter | null>(null);
  const activeRows = useApi(() => (active ? listItems({ ...active.definition, limit: 200 }) : Promise.resolve(null)), [active?.id]);
  const [make, setMake] = useState<Make | null>(null);
  const [newShelf, setNewShelf] = useState(false);

  const ok = shelfFilter(f);
  const d = red.data;
  const loved = take.data?.data.loved_but_forgotten ?? [];
  const shelves: Shelf[] = d ? ([
    { name: "Flagged to revisit", rule: "Everything you marked Revisit later", items: d.revisit, meta: (it) => `Flagged, ${it.rating ? stars(it.rating) : "unrated"}` },
    { name: "Your best", rule: "Rated 4.5 or more", items: d.top_rated, meta: (it) => stars(it.rating) },
    { name: "Words still missing", rule: "Explored, with no review or no rating yet", items: d.incomplete_reviews, meta: (it) => (it.rating ? `Rated ${it.rating}, no review` : "No rating yet"), act: true },
    { name: "Waiting too long", rule: "In the backlog for more than 60 days", items: d.stale_backlog, meta: (it) => `Added ${age(it.added_at)} ago`, act: true },
    ...(r2 ? [{ name: "Loved, not played lately", rule: "Rated 4.5 or more, no plays in 90 days (your records and your plays)",
      items: loved.map((t) => t.item), meta: (it: ItemSummary) => loved.find((t) => t.item.id === it.id)?.line ?? "" }] : []),
  ] satisfies Shelf[]).map((s) => ({ ...s, items: s.items.filter(ok) })).filter((s) => s.items.length) : [];

  const tagNames = (tags.data ?? []).map((t) => t.name).sort();
  const playlistBtn = (m: Make, label = "Make a Spotify playlist") => r3 && (
    <button className="btn btn-secondary s" disabled={!online} title={online ? undefined : "Needs a connection"} onClick={() => setMake(m)}
      aria-label={`${label} from ${m.title}`}>{label}</button>);

  return (
    <>
      <Topbar />
      <div className="page-head">
        <div className="flex flex-col gap-1">
          <h1 className="page-title">Rediscover</h1>
          <span className="text-muted">Shelves built from your own rules. Nothing here is chosen by Spotify.</span>
        </div>
        <div className="flex flex-wrap gap-2 items-center">
          <label className="sr-only" htmlFor="rd-tag">Tag</label>
          <select id="rd-tag" className="input surface" value={f.tag} onChange={(e) => setF({ ...f, tag: e.target.value })}>
            <option value="">All tags</option>{tagNames.map((t) => <option key={t}>{t}</option>)}
          </select>
          <label className="sr-only" htmlFor="rd-min">Rating</label>
          <select id="rd-min" className="input surface" value={f.min} onChange={(e) => setF({ ...f, min: Number(e.target.value) })}>
            <option value={0}>Any rating</option>{[4, 4.5, 5].map((n) => <option key={n} value={n}>{n} and up</option>)}
          </select>
          <button className="btn btn-secondary" aria-pressed={f.revisit} onClick={() => setF({ ...f, revisit: !f.revisit })}>{f.revisit ? "Revisit only: on" : "Revisit only"}</button>
          <button className="btn btn-secondary" aria-pressed={art} onClick={() => setArt(!art)}>{art ? "Hide artwork" : "Show artwork"}</button>
          <button className="btn btn-primary" onClick={() => setNewShelf(true)}>New shelf</button>
        </div>
      </div>
      <div className="flex flex-wrap gap-2 items-center">
        <span className="text-muted text-sm">Saved filters:</span>
        {saved.data?.length ? saved.data.map((s) => (
          <button key={s.id} className={`chip${active?.id === s.id ? " on" : ""}`} aria-pressed={active?.id === s.id} onClick={() => setActive(active?.id === s.id ? null : s)}>{s.name}</button>
        )) : <span className="text-faint text-sm">None yet. New shelf saves one.</span>}
      </div>

      {active && (
        <section className="flex flex-col gap-3" aria-label={`Saved filter ${active.name}`}>
          <div className="flex justify-between items-baseline gap-3">
            <div className="flex flex-col gap-0.5"><h2 className="section-title">{active.name}</h2><span className="text-muted text-sm">Your saved filter</span></div>
            <span className="flex flex-wrap gap-2 items-center">
              {playlistBtn({ title: active.name, items: activeRows.data?.rows ?? [], kind: "filter", filterId: active.id }, "Make a Spotify playlist")}
              <button className="btn btn-ghost s" onClick={async () => {
                const gone = active; setActive(null); await deleteSavedFilter(gone.id);
                toast(`Filter ${gone.name} deleted`, () => saveFilter(gone.name, gone.definition));
              }}>Delete filter</button>
              {activeRows.data && <span className="font-mono text-faint text-xs">{pl(activeRows.data.total, "record")}</span>}
            </span>
          </div>
          {activeRows.loading ? <SkelTiles n={6} /> : activeRows.error ? <ErrorBanner title="Couldn’t load this filter." error={activeRows.error} onRetry={activeRows.reload} />
            : activeRows.data?.rows.length ? <ItemSet items={activeRows.data.rows.slice(0, 6)} total={activeRows.data.total} art={art} meta={(it) => (it.rating ? stars(it.rating) : STATUS[it.status])} />
            : <span className="text-muted">Nothing matches this filter right now.</span>}
        </section>
      )}

      {red.loading ? <><SkelTiles n={6} /><SkelTiles n={6} /></>
        : red.error ? <ErrorBanner title="Couldn’t load your shelves." error={red.error} onRetry={red.reload} />
        : !shelves.length ? (
          <Empty h="No shelves yet" line={f.tag || f.min || f.revisit ? "No records match these filters. Clear a filter to see more." : "Shelves fill up as you rate, review and flag records. Try rating a few things you’ve explored."}>
            {f.tag || f.min || f.revisit
              ? <button className="btn btn-secondary" onClick={() => setF({ tag: "", min: 0, revisit: false })}>Clear filters</button>
              : <Link className="btn btn-secondary" to="/library">Open your library</Link>}
          </Empty>)
        : shelves.map((s) => (
          <section key={s.name} className="flex flex-col gap-3" aria-label={s.name}>
            <div className="flex justify-between items-baseline gap-3">
              <div className="flex flex-col gap-0.5"><h2 className="section-title">{s.name}</h2><span className="text-muted text-sm">{s.rule}</span></div>
              <span className="flex flex-wrap gap-2 items-center">
                {playlistBtn({ title: s.name, items: s.items, kind: "shelf" })}
                <span className="font-mono text-faint text-xs">{pl(s.items.length, "record")}</span>
              </span>
            </div>
            <ItemSet items={s.items.slice(0, 6)} total={s.items.length} art={art} meta={s.meta} act={() => !!s.act} />
          </section>
        ))}

      {make && <MakePlaylistDialog make={make} onClose={() => setMake(null)} />}
      {newShelf && <NewShelfDialog tags={tagNames} start={f} onClose={() => setNewShelf(false)} onSaved={(s) => { setActive(s); setNewShelf(false); }} />}
    </>
  );
}

/* ------------------------------------------------------------------ SC-07b */
function MakePlaylistDialog({ make, onClose }: { make: Make; onClose: () => void }) {
  const toast = useToast();
  const [name, setName] = useState(make.title);
  const [priv, setPriv] = useState(true);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<AppError>();
  const rows = make.items.filter((i) => i.type !== "artist").slice(0, 50);
  // A failure after the create carries the new playlist in `detail` (API.md): link it, and don't offer a retry that makes a second one.
  const made = safeUri(err?.detail)?.type === "playlist" ? err!.detail : undefined;
  const go = async () => {
    setBusy(true); setErr(undefined);
    try {
      const r = make.filterId != null ? await createPlaylistFromFilter(make.filterId, name.trim(), !priv) : await createPlaylistFromItems(rows.map((i) => i.id), name.trim(), !priv);
      onClose();
      toast(`${name.trim()} created on Spotify with ${pl(r.added, "track")}${r.skipped_no_uri ? `. ${pl(r.skipped_no_uri, "record")} without a Spotify link skipped` : ""}.`);
    } catch (e) { setErr(toAppError(e)); } finally { setBusy(false); }
  };
  return (
    <Dialog title={`New Spotify playlist from “${make.title}”`} onClose={onClose}
      footer={<><button className="btn btn-secondary" onClick={onClose}>Cancel</button><button className="btn btn-primary" disabled={busy || !!made || !name.trim() || !rows.length} onClick={go}>{busy ? <Dots /> : "Create playlist"}</button></>}>
      <div className="field"><label className="label" htmlFor="mp-name">Playlist name</label>
        <input id="mp-name" className="input" autoFocus value={name} onChange={(e) => setName(e.target.value)} /></div>
      <span className="text-text-2">{pl(rows.length, "record")} from this {make.kind}. Albums add all their tracks.</span>
      <div className="card pad-s max-h-[220px] overflow-auto" tabIndex={0} aria-label="Records in the playlist"><div className="list">{rows.map((i) => (
        <div key={i.id} className="row grid-cols-[minmax(0,1fr)_70px]">
          <span>{nameOf(i)}{i.artist_names.length > 0 && <span className="text-muted">, {i.artist_names.join(", ")}</span>}</span><span className="text-muted text-xs">{TYPE[i.type]}</span>
        </div>))}
      </div></div>
      <label className="flex flex-wrap gap-2 items-center"><input type="checkbox" checked={priv} onChange={(e) => setPriv(e.target.checked)} />Private playlist</label>
      <Attribution><span className="help">Created on your Spotify account.</span></Attribution>
      {made ? <><ErrorBanner error={err} title="The playlist was created on Spotify, but not every track was added." /><Attribution uri={made} name={name.trim()} /></>
        : err && <ErrorBanner error={err} title="Couldn’t create the playlist." />}
    </Dialog>
  );
}

/** New shelf = a saved filter over your own records (FR-013). */
function NewShelfDialog({ tags, start, onClose, onSaved }: { tags: string[]; start: Filter; onClose: () => void; onSaved: (s: SavedFilter) => void }) {
  const toast = useToast();
  const [name, setName] = useState("");
  const [tag, setTag] = useState(start.tag);
  const [min, setMin] = useState(start.min);
  const [status, setStatus] = useState<Status | "">("");
  const def: ItemFilter = { ...(tag ? { tags: [tag] } : {}), ...(min ? { min_rating: min } : {}), ...(status ? { status: [status] } : {}), ...(start.revisit ? { revisit: true } : {}) };
  const count = useApi(() => listItems({ ...def, limit: 1 }), [tag, min, status, start.revisit]).data?.total;
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<AppError>();
  const go = async () => {
    setBusy(true);
    try { const s = await saveFilter(name.trim(), def); toast("Shelf added to Rediscover"); onSaved(s); } catch (e) { setErr(toAppError(e)); } finally { setBusy(false); }
  };
  return (
    <Dialog title="New shelf" onClose={onClose}
      footer={<><button className="btn btn-secondary" onClick={onClose}>Cancel</button><button className="btn btn-primary" disabled={busy || !name.trim()} onClick={go}>{busy ? <Dots /> : "Add shelf"}</button></>}>
      <div className="field"><label className="label" htmlFor="ns-name">Shelf name</label>
        <input id="ns-name" className="input" autoFocus placeholder="For example, Rainy evenings" value={name} onChange={(e) => setName(e.target.value)} /></div>
      <div className="tiles grid-cols-3">
        <div className="field"><label className="label" htmlFor="ns-tag">Tag</label>
          <select id="ns-tag" className="input" value={tag} onChange={(e) => setTag(e.target.value)}><option value="">Any</option>{tags.map((t) => <option key={t}>{t}</option>)}</select></div>
        <div className="field"><label className="label" htmlFor="ns-r">Rating</label>
          <select id="ns-r" className="input" value={min} onChange={(e) => setMin(Number(e.target.value))}><option value={0}>Any</option>{[3, 4, 4.5, 5].map((n) => <option key={n} value={n}>{n} and up</option>)}</select></div>
        <div className="field"><label className="label" htmlFor="ns-s">Status</label>
          <select id="ns-s" className="input" value={status} onChange={(e) => setStatus(e.target.value as Status | "")}><option value="">Any</option>{STATUS_ORDER.map((s) => <option key={s} value={s}>{STATUS[s]}</option>)}</select></div>
      </div>
      <span className="help">Shelves use only your own records.{count != null ? ` ${pl(count, "record")} match right now.` : ""}</span>
      {err && <ErrorBanner error={err} title="Couldn’t save the shelf." />}
    </Dialog>
  );
}

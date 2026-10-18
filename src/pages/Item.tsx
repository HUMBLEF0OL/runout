// SC-05 Item detail (album, track, artist) and ?edit=1 edit mode. R2 adds the listening panel; R3 adds
// "Saved in your Spotify library" and filling an album's track list from Spotify. Keys 1–5 set status.
import { useCallback, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  autofill, bulkAddAlbumTracks, deleteItem, getItem, getItemListening, libraryContains, removeFromSpotifyLibrary,
  saveToSpotifyLibrary, setStatus, updateItem, toAppError, type AppError, type ItemDetail, type ItemPatch, type Status,
} from "../api";
import { Topbar } from "../components/Shell";
import { useQuickAdd } from "../components/QuickAddContext";
import {
  Attribution, Dialog, Dots, EmbedSlot, Empty, ErrorBanner, FreshnessNote, PrioritySelect, Rating, SkelBlock, SkelRows, SourceTag,
  StatusBadge, StatusControl, TagChips, useToast,
} from "../components/ui";
import { useApi, useOnline } from "../hooks/useApi";
import { useRelease } from "../hooks/useRelease";
import { useShortcuts, type ShortcutAction } from "../hooks/useShortcuts";
import { ago, day, nameOf, pl, STATUS, STATUS_ORDER, statusMeta, TYPE } from "../lib/format";

export default function Item() {
  const id = Number(useParams().id);
  const q = useApi(() => getItem(id), [id]);
  if (q.loading) return <><Topbar /><section className="flex flex-wrap gap-7 items-start"><div className="flex-[0_1_360px]"><SkelBlock h={352} /></div><div className="flex-[1.5_1_520px] min-w-0 flex flex-col gap-3.5"><SkelRows n={4} /></div></section></>;
  if (q.error?.code === "not_found" || (!q.error && !q.data)) return <><Topbar /><Empty h="This record no longer exists" line="It was deleted, or the link points to something that isn’t in your library."><Link className="btn btn-secondary" to="/library">Back to library</Link></Empty></>;
  if (q.error || !q.data) return <><Topbar /><ErrorBanner title="Couldn’t load this record." error={q.error} onRetry={q.reload} /></>;
  return <Detail key={q.data.id} it={q.data} />;
}

function Detail({ it }: { it: ItemDetail }) {
  const { r2, r3 } = useRelease();
  const online = useOnline();
  const toast = useToast();
  const nav = useNavigate();
  const quickAdd = useQuickAdd();
  const edit = useSearchParams()[0].get("edit") === "1";
  const name = nameOf(it);
  const [note, setNote] = useState("");
  const [review, setReview] = useState(it.review ?? "");
  const [notes, setNotes] = useState(it.notes ?? "");
  const [err, setErr] = useState<{ title: string; e?: AppError; retry?: () => void }>();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [filling, setFilling] = useState(false);

  const run = async (title: string, p: () => Promise<unknown>) => {
    setErr(undefined);
    try { await p(); return true; } catch (e) { setErr({ title, e: toAppError(e), retry: () => run(title, p) }); return false; }
  };
  const patch = (title: string, pt: ItemPatch) => run(title, () => updateItem(it.id, pt));

  const changeStatus = useCallback(async (s: Status) => {
    if (s === it.status) return;
    const from = it.status;
    if (await run("Couldn’t change the status.", () => setStatus(it.id, s, note.trim() || undefined))) {
      setNote("");
      toast(`${name} is now ${STATUS[s]}`, () => setStatus(it.id, from));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [it.id, it.status, note, name]);
  useShortcuts(useCallback((a: ShortcutAction) => { if (typeof a === "object" && !edit) changeStatus(STATUS_ORDER[a.status - 1]); }, [changeStatus, edit]));

  const listening = useApi(() => (r2 ? getItemListening(it.id) : Promise.resolve(null)), [r2, it.id]);
  const saved = useApi(() => (r3 && online && it.spotify_uri ? libraryContains([it.spotify_uri]) : Promise.resolve(null)), [r3, online, it.spotify_uri]);

  const artistLink = it.links.find((l) => l.kind === "by_artist" && l.direction === "from")?.item;
  const albumLink = it.links.find((l) => l.kind === "on_album" && l.direction === "from")?.item;
  const byArtist = it.links.filter((l) => l.kind === "by_artist" && l.direction === "to").map((l) => l.item);
  const onAlbum = it.links.filter((l) => l.kind === "on_album" && l.direction === "to").map((l) => l.item);
  const artistName = artistLink ? nameOf(artistLink) : it.artist_names.join(", ");
  const back = it.in_backlog ? "backlog" : "library";

  const head = edit ? <EditForm it={it} artistName={artistName} onDone={() => nav(`/item/${it.id}`)} /> : (
    <div className="flex flex-col gap-1.5">
      <span className="text-muted text-sm">{TYPE[it.type]}, added {day(it.added_at)}{it.added_via === "phone" ? " from your iPhone" : ""}</span>
      <div className="flex flex-wrap gap-2 items-center">
        <h1 className="text-[34px] font-extrabold tracking-[-0.025em]">{name}</h1>
        <Link className="btn btn-secondary s" to={`/item/${it.id}?edit=1`} aria-label={`Edit ${name}`}>Edit</Link>
      </div>
      {it.type !== "artist" && artistName && (
        <span className="text-text-2">
          {artistLink ? <Link to={`/item/${artistLink.id}`}>{artistName}</Link> : artistName}
          {it.type === "track" && it.album_name && <>, from {albumLink ? <Link to={`/item/${albumLink.id}`}>{it.album_name}</Link> : it.album_name}</>}
        </span>)}
      {it.recommended_by && <span className="text-muted text-sm">Recommended by {it.recommended_by}</span>}
    </div>
  );

  const spotifySave = !r3 || !it.spotify_uri ? null
    : !online ? <span className="text-faint text-xs">Saved in Spotify can’t change while you’re offline</span>
    : saved.error ? <span className="text-faint text-xs">Saved in Spotify isn’t available. {saved.error.message}</span>
    : <label className="flex flex-wrap gap-2 items-center"><input type="checkbox" checked={!!saved.data?.[0]} disabled={saved.loading} onChange={async (e) => {
        const on = e.target.checked, u = [it.spotify_uri!];
        if (await run("Couldn’t change your Spotify library.", () => (on ? saveToSpotifyLibrary(u) : removeFromSpotifyLibrary(u)))) toast(on ? `${name} saved in your Spotify library` : `${name} removed from your Spotify library`);
      }} />Saved in your Spotify library</label>;

  const controls = (
    <div className="field">
      <span className="label">Status</span>
      <StatusControl value={it.status} onChange={changeStatus} />
      <div className="flex flex-wrap gap-4 items-center text-text-2 text-sm">
        <label className="flex flex-wrap gap-2 items-center"><input type="checkbox" checked={it.revisit} onChange={async (e) => {
          const v = e.target.checked; if (await patch("Couldn’t save that change.", { revisit: v })) toast(`${name}: ${v ? "flagged to revisit" : "revisit flag removed"}`, () => updateItem(it.id, { revisit: !v }));
        }} />Revisit later</label>
        <label className="flex flex-wrap gap-2 items-center"><input type="checkbox" checked={it.favourite} onChange={async (e) => {
          const v = e.target.checked; if (await patch("Couldn’t save that change.", { favourite: v })) toast(`${name}: ${v ? "favourite" : "no longer a favourite"}`, () => updateItem(it.id, { favourite: !v }));
        }} />Favourite</label>
        {it.in_backlog && <>
          <label htmlFor="dt-prio">Priority</label>
          <PrioritySelect id="dt-prio" className="input h-[34px]" value={it.priority ?? ""} onChange={(p) => patch("Couldn’t save that change.", { priority: p || null })} /></>}
        {spotifySave}
      </div>
      <div className="flex flex-wrap gap-2 items-center">
        <label className="sr-only" htmlFor="st-note">Note for the next status change</label>
        <input id="st-note" className="input h-[34px] flex-[1_1_260px]" placeholder="Optional note for the next status change" value={note} onChange={(e) => setNote(e.target.value)} />
      </div>
    </div>
  );

  const saveText = (field: "review" | "notes", v: string) => {
    if (v === (it[field] ?? "")) return;
    patch(field === "review" ? "Couldn’t save your review." : "Couldn’t save your notes.", { [field]: v.trim() || null }).then((ok) => ok && toast(field === "review" ? "Review saved" : "Notes saved"));
  };
  const journal = it.type === "artist" ? (
    <div className="field"><label className="label" htmlFor="dt-notes">Why this artist</label>
      <textarea id="dt-notes" className="input" rows={3} value={notes} onChange={(e) => setNotes(e.target.value)} onBlur={() => saveText("notes", notes)} /></div>
  ) : <>
    <div className="flex flex-wrap gap-8 items-start">
      <div className="field"><span className="label">Your rating</span>
        <Rating label={`Your rating for ${name}`} value={it.rating} onChange={async (v) => {
          const old = it.rating; if (await patch("Couldn’t save your rating.", { rating: v })) toast(v ? `Rated ${name} ${v}` : `Rating removed from ${name}`, () => updateItem(it.id, { rating: old }));
        }} />
        <span className="kbd">Arrow keys change by half a star</span></div>
      <div className="field"><span className="label">Tags</span>
        <TagChips tags={it.tags}
          onAdd={(t) => { if (!it.tags.includes(t)) patch("Couldn’t add the tag.", { tags: [...it.tags, t] }); }}
          onRemove={async (t) => { if (await patch("Couldn’t remove the tag.", { tags: it.tags.filter((x) => x !== t) })) toast(`Removed tag ${t}`, () => updateItem(it.id, { tags: [...it.tags] })); }} /></div>
    </div>
    <div className="field"><label className="label" htmlFor="dt-review">Review</label>
      <textarea id="dt-review" className="input" rows={4} placeholder="What stayed with you?" value={review} onChange={(e) => setReview(e.target.value)} onBlur={() => saveText("review", review)} /></div>
    <div className="field"><label className="label" htmlFor="dt-notes">Notes</label>
      <textarea id="dt-notes" className="input" rows={2} placeholder="Context, where you heard it, what to listen for" value={notes} onChange={(e) => setNotes(e.target.value)} onBlur={() => saveText("notes", notes)} /></div>
  </>;

  const addTrack = () => quickAdd({ suggest: { name: "", artist: artistName, type: "track", from: "plays" } });
  const tracks = it.type !== "album" ? null : (
    <section className="card flex flex-col gap-3">
      <div className="flex justify-between items-baseline gap-3"><h2 className="card-title">Tracks you’ve added</h2></div>
      {it.track_list?.length ? it.track_list.map((t) => (
        <div key={t.position} className="flex justify-between items-baseline gap-3 py-[3px]">
          <span><span className="font-mono text-faint text-xs mr-2.5">{t.position}</span>{t.item_id ? <Link to={`/item/${t.item_id}`} className="text-text">{t.name}</Link> : t.name}</span>
        </div>))
        : onAlbum.length ? onAlbum.map((t) => (
          <div key={t.id} className="flex justify-between items-baseline gap-3 py-[3px]"><Link to={`/item/${t.id}`} className="text-text">{nameOf(t)}</Link><span className="text-muted text-xs">{statusMeta(t)}</span></div>))
        : <span className="text-muted">None yet. Add tracks to see which ones you haven’t played.</span>}
      <div className="flex flex-wrap gap-2 items-center">
        <button className="btn btn-secondary s" onClick={addTrack}>Add a track</button>
        {r3 && it.spotify_uri && !it.track_list?.length && <button className="btn btn-secondary s" disabled={!online} onClick={() => setFilling(true)}>Fill the track list from Spotify</button>}
      </div>
    </section>
  );

  const artistBody = it.type !== "artist" ? null : (
    <section className="flex flex-col gap-3">
      <div className="flex justify-between items-baseline gap-3"><h2 className="section-title">In your library</h2><span className="text-muted text-xs">Only what you’ve saved. This is not the artist’s full discography.</span></div>
      {byArtist.length ? <div className="card pad-s"><div className="list">{byArtist.map((x) => (
        <div key={x.id} className="row grid-cols-[minmax(0,1fr)_140px_130px]">
          <Link to={`/item/${x.id}`} className="flex flex-col gap-0.5 text-inherit"><span className="font-semibold">{nameOf(x)}</span><span className="text-muted text-xs">{TYPE[x.type]}</span></Link>
          <span className="text-text-2 text-sm">{statusMeta(x)}</span>
          <span className="justify-self-end">{x.in_backlog ? <StatusBadge>In backlog</StatusBadge>
            : <button className="btn btn-secondary s" aria-label={`Add ${nameOf(x)} to backlog`} onClick={async () => {
                if (await patchOther(x.id)) toast(`${nameOf(x)} added to your backlog`, () => updateItem(x.id, { in_backlog: false }));
              }}>Add to backlog</button>}</span>
        </div>))}</div></div> : <span className="text-muted">Nothing saved by this artist yet.</span>}
      <button className="btn btn-secondary self-start" onClick={() => quickAdd({ suggest: { name: "", artist: name, type: "album", from: "plays" } })}>Add an album or track by {name}</button>
    </section>
  );
  const patchOther = (xid: number) => run("Couldn’t save that change.", () => updateItem(xid, { in_backlog: true }));

  const l = listening.data;
  const listeningCard = r2 && (
    <div className="card flex flex-col gap-1.5">
      <div className="flex justify-between items-baseline gap-3"><h2 className="card-title">Your listening</h2>{l && <SourceTag sources={l.sources} />}</div>
      {listening.loading ? <SkelBlock h={60} />
        : listening.error ? <ErrorBanner title="Couldn’t load your listening." error={listening.error} onRetry={listening.reload} />
        : l && l.data.plays ? <>
          <span className="font-mono text-[22px] font-semibold">{pl(l.data.plays, "play")}</span>
          <span className="text-text-2 text-sm">{l.data.last_played_local ? `Last played ${ago(l.data.last_played_local).toLowerCase()}.` : ""}{it.type === "album" ? " Counts every track from this album." : it.type === "artist" ? " Counts every track by this artist." : ""}</span>
        </> : <span className="text-text-2">No plays yet.</span>}
      {l && <FreshnessNote freshness={l.freshness} />}
    </div>
  );

  const history = (
    <div className="card flex flex-col gap-3">
      <h2 className="card-title">History</h2>
      {it.history.length ? it.history.map((h, n) => (
        <div key={n} className="grid grid-cols-[64px_minmax(0,1fr)] gap-2.5 text-sm">
          <span className="font-mono text-faint">{day(h.at)}</span>
          <span className="text-text-2">{h.from ? `${STATUS[h.from]} to ${STATUS[h.to]}${h.note ? `: ${h.note}` : ""}` : h.note || "Added"}</span>
        </div>)) : <span className="text-muted">No changes yet.</span>}
    </div>
  );

  const danger = (
    <div className="flex flex-wrap gap-2 items-center">
      {it.in_backlog && <button className="btn btn-secondary" onClick={async () => {
        if (await patch("Couldn’t save that change.", { in_backlog: false })) toast(`${name} removed from backlog. The record stays in your library.`, () => updateItem(it.id, { in_backlog: true }));
      }}>Remove from backlog</button>}
      <button className="btn btn-danger" onClick={() => setConfirmDelete(true)} aria-label={`Delete ${name}`}>Delete</button>
    </div>
  );

  return (
    <>
      <Topbar />
      <Link to={`/${back}`} className="text-sm font-semibold">Back to {back}</Link>
      {err && <ErrorBanner title={err.title} error={err.e} onRetry={err.retry} />}
      <section className="flex flex-wrap gap-7 items-start">
        <div className="flex flex-col gap-3 flex-[0_1_360px] min-w-[280px]">
          <EmbedSlot uri={it.spotify_uri} height={it.type === "track" ? 152 : 352} />
          {it.spotify_uri && <Attribution uri={it.spotify_uri} name={name} />}
        </div>
        <div className="flex-[1.5_1_520px] min-w-0 flex flex-col gap-[18px]">{head}{!edit && <>{controls}{journal}</>}</div>
      </section>
      {artistBody}
      <div className="flex flex-wrap gap-7 items-start">
        <div className="flex-[1.3_1_420px] min-w-0 flex flex-col gap-3.5">{tracks}</div>
        <div className="flex-[1_1_320px] min-w-0 flex flex-col gap-5">{listeningCard}{history}{danger}</div>
      </div>
      {confirmDelete && (
        <Dialog title={`Delete ${name}?`} onClose={() => setConfirmDelete(false)} footer={<>
          <button className="btn btn-secondary" onClick={() => setConfirmDelete(false)}>Cancel</button>
          <button className="btn btn-danger" onClick={async () => {
            if (await run("Couldn’t delete this record.", () => deleteItem(it.id))) { setConfirmDelete(false); nav("/library"); toast(`${name} deleted`); } else setConfirmDelete(false);
          }}>Delete</button></>}>
          <span className="text-text-2">Its rating, review, notes, tags and history are deleted. Plays stay in your listening history.</span>
        </Dialog>)}
      {filling && it.spotify_uri && <FillTracks albumId={it.id} uri={it.spotify_uri} onClose={() => setFilling(false)} />}
    </>
  );
}

/** Edit mode: name, artist, recommended by (update_item). */
function EditForm({ it, artistName, onDone }: { it: ItemDetail; artistName: string; onDone: () => void }) {
  const toast = useToast();
  const [name, setName] = useState(it.display_name ?? "");
  const [artist, setArtist] = useState(artistName);
  const [rec, setRec] = useState(it.recommended_by ?? "");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<AppError>();
  const save = async () => {
    setBusy(true); setErr(undefined);
    try {
      await updateItem(it.id, { display_name: name.trim() || null, recommended_by: rec.trim() || null, ...(it.type !== "artist" && artist !== artistName ? { artist_name: artist.trim() || null } : {}) });
      onDone(); toast("Changes saved");
    } catch (e) { setErr(toAppError(e)); } finally { setBusy(false); }
  };
  const links = it.links.filter((l) => l.direction === "from");
  return (
    <form className="card flex flex-col gap-3" onSubmit={(e) => { e.preventDefault(); save(); }}>
      <h2 className="card-title">Edit record</h2>
      {err && <ErrorBanner title="Couldn’t save your changes." error={err} />}
      <div className="tiles grid-cols-3">
        <div className="field"><label className="label" htmlFor="ed-name">Name</label><input id="ed-name" className="input" autoFocus value={name} onChange={(e) => setName(e.target.value)} /></div>
        {it.type !== "artist" && <div className="field"><label className="label" htmlFor="ed-artist">Artist</label><input id="ed-artist" className="input" value={artist} onChange={(e) => setArtist(e.target.value)} /></div>}
        <div className="field"><label className="label" htmlFor="ed-rec">Recommended by</label><input id="ed-rec" className="input" value={rec} onChange={(e) => setRec(e.target.value)} /></div>
      </div>
      {links.length > 0 && <div className="field"><span className="label">Links to other records</span><div className="flex flex-wrap gap-2 items-center">
        {links.map((l) => <span key={l.kind + l.item.id} className="chip">{l.kind === "by_artist" ? "By" : l.kind === "on_album" ? "On" : "Version of"} {nameOf(l.item)}</span>)}
      </div></div>}
      <div className="flex flex-wrap gap-2 items-center">
        <button type="submit" className="btn btn-primary min-w-[120px]" disabled={busy}>{busy ? <Dots /> : "Save changes"}</button>
        <Link className="btn btn-secondary" to={`/item/${it.id}`}>Cancel</Link>
      </div>
    </form>
  );
}

/** R3: tick the album's tracks to keep (autofill → bulk_add_album_tracks). Nothing is stored unticked. */
function FillTracks({ albumId, uri, onClose }: { albumId: number; uri: string; onClose: () => void }) {
  const toast = useToast();
  const a = useApi(() => autofill(uri), [uri]);
  const [keep, setKeep] = useState<Set<string> | null>(null);
  const [err, setErr] = useState<AppError>();
  const list = a.data?.track_list ?? [];
  const on = keep ?? new Set(list.map((t) => t.uri));
  const add = async () => {
    try { const r = await bulkAddAlbumTracks(albumId, list.filter((t) => on.has(t.uri)).map(({ uri: u, name }) => ({ uri: u, name }))); onClose(); toast(`${pl(r.length, "track")} added`); }
    catch (e) { setErr(toAppError(e)); }
  };
  return (
    <Dialog title="Fill the track list from Spotify" onClose={onClose} footer={<>
      <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
      <button className="btn btn-primary" disabled={!on.size} onClick={add}>Add tracks</button></>}>
      <Attribution uri={uri}><span className="text-text-2">From Spotify. Tick the tracks you want to keep in your library.</span></Attribution>
      {a.loading ? <SkelRows n={3} /> : a.error ? <ErrorBanner title="Couldn’t get the track list." error={a.error} onRetry={a.reload} />
        : list.length ? list.map((t) => (
          <label key={t.uri} className="flex flex-wrap gap-2 items-center"><input type="checkbox" checked={on.has(t.uri)} onChange={(e) => {
            const n = new Set(on); if (e.target.checked) n.add(t.uri); else n.delete(t.uri); setKeep(n);
          }} />{t.name}</label>))
        : <span className="text-muted">Spotify has no track list for this album.</span>}
      {err && <ErrorBanner title="Couldn’t add the tracks." error={err} />}
      <span className="help">Unticked tracks aren’t stored. Kept tracks become your records.</span>
    </Dialog>
  );
}

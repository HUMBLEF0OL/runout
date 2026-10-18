// SC-06 Quick add: paste link, short link (offline: save for later), title prefill, manual entry,
// naming a phone capture, artist mode, suggestions from plays/Spotify lists, and R3 autofill.
import { useEffect, useRef, useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import {
  autofill, bulkAddAlbumTracks, createItem, deleteItem, getItem, isAppError, linkItems, parseLink, prefillTitle, resolveLink, toAppError, updateItem,
  type ItemDetail, type ItemType, type ParsedLink, type Priority,
} from "../api";
import { useOnline } from "../hooks/useApi";
import { useRelease } from "../hooks/useRelease";
import { an, day, nameOf, openUrl, STATUS } from "../lib/format";
import { qaStep } from "../lib/backlog";
import { linkKind } from "../lib/link";
import type { QuickAddOpts } from "./QuickAddContext";
import { Attribution, Dialog, Dots, EmbedSlot, PrioritySelect, useToast } from "./ui";

type Resolved = { uri: string; type: ItemType; existing_id?: number };
const msg = (e: unknown) => toAppError(e).message;

export default function QuickAdd({ opts, onClose }: { opts: QuickAddOpts; onClose: () => void }) {
  const t0 = useRef(performance.now()); // capture_ms: open → save (FR-045)
  const online = useOnline();
  const { r3 } = useRelease();
  const toast = useToast();
  const naming = opts.namingItemId;
  const sg = opts.suggest;

  const [link, setLink] = useState(opts.link ?? (sg?.uri ? openUrl(sg.uri) : ""));
  const [manual, setManual] = useState(!!sg && !sg.uri);
  const [versionOf, setVersionOf] = useState<number | null>(null);
  const [parsed, setParsed] = useState<ParsedLink>();
  const [resolved, setResolved] = useState<Resolved>();
  const [dup, setDup] = useState<ItemDetail>();
  const [err, setErr] = useState<string | null>(null);
  const [type, setType] = useState<ItemType>(sg?.type ?? "album");
  const [name, setName] = useState(sg?.name ?? "");
  const [suggested, setSuggested] = useState<string | null>(sg?.name || null);
  const fromSpotify = !sg?.name || sg.from === "spotify"; // oEmbed prefill or a Spotify list; plays-based names are the user's
  const [artist, setArtist] = useState(sg?.artist ?? "");
  const [artistFromSpotify, setArtistFromSpotify] = useState<string | null>(null);
  const [notes, setNotes] = useState("");
  const [prio, setPrio] = useState<Priority | "">("");
  const [rec, setRec] = useState("");
  const [tracks, setTracks] = useState<{ uri: string; name: string; keep: boolean }[]>([]);
  const [prefilling, setPrefilling] = useState(false);
  const [saving, setSaving] = useState(false);

  // Naming a phone capture: the link is the capture's own.
  useEffect(() => {
    if (naming == null) return;
    getItem(naming).then((it) => { if (it.spotify_uri) setLink(openUrl(it.spotify_uri)); setType(it.type); setRec(it.recommended_by ?? ""); }, (e) => setErr(msg(e)));
  }, [naming]);

  // Link → parse_link (+ resolve_link for short links when online).
  useEffect(() => {
    let live = true;
    setParsed(undefined); setResolved(undefined); setErr(null);
    const raw = link.trim();
    if (!raw) return;
    parseLink(raw).then(async (p) => {
      if (!live) return;
      setParsed(p);
      if (p.state === "short" && online) {
        try { const r = await resolveLink(raw); if (live) setResolved(r); } catch (e) { if (live) setErr(msg(e)); }
      }
    }, (e) => live && setErr(msg(e)));
    return () => { live = false; };
  }, [link, online]);

  const step = qaStep({ manual, link, online, kind: linkKind(link), self: naming, parsed, resolved });
  const uri = parsed?.uri ?? resolved?.uri;
  const kind = parsed?.type ?? resolved?.type ?? type;
  const existingId = parsed?.existing_id ?? resolved?.existing_id;

  // Duplicate: show what's already saved.
  useEffect(() => {
    setDup(undefined);
    if (step === "duplicate" && existingId != null) getItem(existingId).then(setDup, (e) => setErr(msg(e)));
  }, [step, existingId]);

  // New link: title from oEmbed (never stored unless saved), and in R3 Spotify's fields to confirm.
  useEffect(() => {
    if (step !== "form" || !uri) return;
    let live = true;
    setType(kind);
    if (!sg?.name) {
      setPrefilling(true);
      prefillTitle(uri).then((r) => { if (live && r.title) { setSuggested(r.title); setName((n) => n || r.title!); } }, () => {}).finally(() => live && setPrefilling(false));
    }
    if (r3 && naming == null) {
      autofill(uri).then((a) => {
        if (!live) return;
        if (a.fields.artist) { setArtistFromSpotify(a.fields.artist); setArtist((x) => x || a.fields.artist!); }
        setTracks((a.track_list ?? []).map((t) => ({ uri: t.uri, name: t.name, keep: false })));
      }, () => {});
    }
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step, uri]);

  const isArtist = (manual ? type : kind) === "artist";

  async function save(e?: FormEvent) {
    e?.preventDefault();
    if (saving) return;
    const nm = name.trim();
    if (manual && !nm) { setErr("Give it a name."); return; }
    setSaving(true); setErr(null);
    const capture_ms = Math.round(performance.now() - t0.current);
    const artistName = !isArtist && artist.trim() ? artist.trim() : undefined;
    const promoted_fields = [
      ...(fromSpotify && !!nm && nm === suggested ? ["display_name"] : []),
      ...(artistName && artistName === artistFromSpotify ? ["artist"] : []),
    ];
    try {
      if (naming != null) {
        await updateItem(naming, {
          display_name: nm || null, priority: prio || null, recommended_by: rec.trim() || null, artist_name: artistName,
          ...(isArtist && notes.trim() ? { notes: notes.trim() } : {}), promoted_fields,
        });
        toast(`${nm || "Link"} named`, () => updateItem(naming, { display_name: null }));
      } else {
        const it = await createItem({
          type: manual ? type : kind, display_name: nm || null, spotify_uri: manual ? null : uri ?? null, priority: prio || undefined,
          recommended_by: rec.trim() || undefined, provenance: "user", promoted_fields, capture_ms, artist_name: artistName,
          notes: isArtist ? notes.trim() || undefined : undefined,
        });
        const keep = tracks.filter((t) => t.keep);
        if (keep.length) await bulkAddAlbumTracks(it.id, keep.map(({ uri: u, name: n }) => ({ uri: u, name: n })));
        if (versionOf != null) await linkItems(it.id, versionOf, "version_of");
        toast(`${nameOf(it)} saved to your backlog`, () => deleteItem(it.id));
      }
      onClose();
    } catch (e) {
      if (isAppError(e) && e.code === "duplicate" && e.existing_id != null) getItem(e.existing_id).then(setDup);
      setErr(msg(e));
    } finally { setSaving(false); }
  }

  async function saveLater() {
    setSaving(true);
    try { const r = await resolveLink(link.trim()); setResolved(r); } // back online meanwhile: carry on with the form
    catch (e) {
      if (isAppError(e) && e.code === "offline") { toast("Saved. It will be named when you’re back online."); onClose(); } else setErr(msg(e));
    } finally { setSaving(false); }
  }

  const cancel = <button type="button" className="btn btn-secondary" onClick={onClose}>Cancel</button>;
  const saveBtn = <button type="submit" className="btn btn-primary min-w-[72px]" disabled={saving || step !== "form" && step !== "manual"}>{saving ? <Dots /> : "Save"}</button>;
  const errLine = err && <span className="help err" role="alert">{err}</span>;

  if (step === "manual") {
    return (
      <Dialog title="Add without a link" onClose={onClose}>
        <form className="flex flex-col gap-3" onSubmit={save}>
          <div className="field"><label className="label" htmlFor="qa-type">Type</label>
            <select id="qa-type" className="input" value={type} onChange={(e) => setType(e.target.value as ItemType)}>
              <option value="album">Album</option><option value="track">Track</option><option value="artist">Artist</option>
            </select></div>
          <div className="field"><label className="label" htmlFor="qa-name">Name</label>
            <input id="qa-name" className="input" autoFocus placeholder="For example, Spirit of Eden" value={name} onChange={(e) => setName(e.target.value)} /></div>
          {isArtist ? <WhyArtist v={notes} set={setNotes} />
            : <div className="field"><label className="label" htmlFor="qa-artist">Artist</label>
                <input id="qa-artist" className="input" placeholder="Optional" value={artist} onChange={(e) => setArtist(e.target.value)} /></div>}
          <span className="help">You can add the Spotify link later from the record.</span>
          {errLine}
          <div className="flex flex-wrap gap-2 items-center justify-end">{cancel}{saveBtn}</div>
        </form>
      </Dialog>
    );
  }

  let state = null, body = null, foot = null;
  switch (step) {
    case "blank":
      state = <span className="help">{versionOf != null ? "Paste the link to the other version." : "Detects albums, tracks and artists. Short links from the iPhone share sheet work too."}</span>;
      foot = <><button type="button" className="btn btn-ghost" onClick={() => setManual(true)}>Add without a link</button>{saveBtn}</>;
      break;
    case "checking":
      state = <span className="flex flex-wrap gap-2 items-center"><Dots /><span className="text-muted">{parsed?.state === "short" ? "Opening the short link…" : "Checking the link…"}</span></span>;
      foot = <>{cancel}{saveBtn}</>;
      break;
    case "invalid":
      state = <span className="help err">That isn’t a Spotify link. Paste a link from Share, then Copy link, in Spotify.</span>;
      foot = cancel;
      break;
    case "podcast":
      state = <span className="help err">This is a podcast {linkKind(link)}. Runout keeps albums, tracks and artists.</span>;
      foot = <button type="button" className="btn btn-secondary" onClick={onClose}>Close</button>;
      break;
    case "playlist":
      state = <span className="help">Playlists aren’t saved as records. Open Playlists to check or tidy one.</span>;
      foot = <Link className="btn btn-primary" to="/playlists" onClick={onClose}>Open Playlists</Link>;
      break;
    case "offline-short":
      body = <div className="card flex flex-col gap-3"><span className="text-text-2 font-bold text-sm">Offline</span>
        <span>You’re offline, so the short link can’t be opened yet. It’s saved and will be filled in when you’re back online.</span></div>;
      foot = <>{cancel}<button type="button" className="btn btn-primary" disabled={saving} onClick={saveLater}>{saving ? <Dots /> : "Save for later"}</button></>;
      break;
    case "duplicate":
      body = <div className="card flex flex-col gap-3 border-[rgba(245,181,68,0.35)]">
        <span className="text-accent font-bold text-sm">Already saved</span>
        <span>{dup ? `${nameOf(dup)} is already in your library, ${STATUS[dup.status]} since ${day(dup.history[0]?.at ?? dup.added_at)}.` : "This is already in your library."}</span></div>;
      foot = <>
        {naming == null && <button type="button" className="btn btn-secondary" onClick={() => { setVersionOf(existingId ?? null); setLink(""); }}>Add a different version</button>}
        <Link className="btn btn-light" to={`/item/${existingId}`} onClick={onClose}>Open existing</Link></>;
      break;
    case "form":
      state = <span className="help">{parsed?.state === "short" ? `Short link opened: ${an(kind)}.` : `Detected ${an(kind)}.`}{naming == null ? " Not in your library yet." : ""}</span>;
      body = <>
        <div className="flex gap-4 items-start">
          <div className="flex-[0_0_152px]"><EmbedSlot uri={uri ?? null} height={152} /></div>
          <div className="flex flex-col gap-3 flex-auto min-w-0">
            <div className="field"><label className="label" htmlFor="qa-name">Name</label>
              <input id="qa-name" className="input" autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder={prefilling ? "Getting the title from Spotify…" : "Name it"} />
              {prefilling ? <span className="help flex flex-wrap gap-2 items-center"><Dots />Getting the title from Spotify…</span>
                : suggested && (fromSpotify
                  ? <span className="help"><Attribution uri={uri} name={name || undefined}>Suggested by Spotify. Edit it if you like.</Attribution></span>
                  : <span className="help">From your plays. Edit it if you like; it’s saved as yours.</span>)}</div>
            {isArtist ? <WhyArtist v={notes} set={setNotes} />
              : <div className="field"><label className="label" htmlFor="qa-artist">Artist</label>
                  <input id="qa-artist" className="input" placeholder="Type the artist (optional)" value={artist} onChange={(e) => setArtist(e.target.value)} />
                  {artistFromSpotify && artist === artistFromSpotify && <span className="help"><Attribution uri={uri} name={name || undefined}><span className="badge need">From Spotify</span>Saved only if you keep it.</Attribution></span>}</div>}
          </div>
        </div>
        {tracks.length > 0 && (
          <fieldset className="card flex flex-col gap-3 p-3.5 m-0">
            <legend className="sr-only">Track list from Spotify</legend>
            <div className="flex justify-between items-baseline gap-3"><span className="label">Track list from Spotify</span><Attribution uri={uri} name={name || undefined} /></div>
            {tracks.map((t, n) => (
              <label key={t.uri} className="flex flex-wrap gap-2 items-center text-sm">
                <input type="checkbox" checked={t.keep} onChange={(e) => setTracks((ts) => ts.map((x, i) => (i === n ? { ...x, keep: e.target.checked } : x)))} />{t.name}
              </label>))}
            <span className="help">Ticked tracks are added to your library with the album. The rest stays with Spotify.</span>
          </fieldset>)}
        <div className="tiles grid-cols-2">
          <div className="field"><label className="label" htmlFor="qa-prio">Priority</label>
            <PrioritySelect id="qa-prio" value={prio} onChange={setPrio} /></div>
          <div className="field"><label className="label" htmlFor="qa-rec">Recommended by</label>
            <input id="qa-rec" className="input" placeholder="Optional" value={rec} onChange={(e) => setRec(e.target.value)} /></div>
        </div>
        <span className="help">Saved as Planned. Only the link is required. <span className="kbd">Ctrl+N</span> opens this from anywhere.</span>
      </>;
      foot = <>{cancel}{saveBtn}</>;
      break;
  }

  return (
    <Dialog title={naming != null ? "Name a phone capture" : "Add to backlog"} onClose={onClose}>
      <form className="flex flex-col gap-3" onSubmit={save}>
        <div className="field">
          <label className="label" htmlFor="qa-link">Spotify link</label>
          <input id="qa-link" className="input mono" value={link} readOnly={naming != null} autoFocus={step === "blank" || naming == null && !link}
            placeholder="Paste an album, track or artist link" onChange={(e) => setLink(e.target.value)} />
          {state}
        </div>
        {body}
        {errLine}
        <div className="flex flex-wrap gap-2 items-center justify-end">{foot}</div>
      </form>
    </Dialog>
  );
}

const WhyArtist = ({ v, set }: { v: string; set: (s: string) => void }) => (
  <div className="field"><label className="label" htmlFor="qa-notes">Why this artist</label>
    <textarea id="qa-notes" className="input" rows={2} placeholder="Who sent you here, what to start with" value={v} onChange={(e) => set(e.target.value)} /></div>
);

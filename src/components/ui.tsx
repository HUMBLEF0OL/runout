// Shared UI components, DESIGN.md §5. Markup and class names follow the M1 prototype (styles in src/styles/app.css).
import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { XIcon as X } from "@phosphor-icons/react";
import type { AppError, Freshness, ItemSummary, Play, Priority, Source, Status } from "../api";
import { ago, mmss, time, nameOf, openUrl, PRIO, STATUS, STATUS_ORDER, stars, statusMeta, sub, TYPE } from "../lib/format";
import { freshnessText, ratingClick, ratingKey, sourceText } from "./logic";
import { useOnline } from "../hooks/useApi";
import { safeUri } from "../lib/sanitize";
import spotifyLogo from "../assets/spotify/spotify-logo-white.svg";

/* ------------------------------------------------------------------ layout bits */
export function PageHead({ title, children, sub: subline }: { title: ReactNode; sub?: ReactNode; children?: ReactNode }) {
  return (
    <div className="page-head">
      <div className="flex flex-col gap-1.5">
        <h1 className="page-title">{title}</h1>
        {subline && <span className="text-muted">{subline}</span>}
      </div>
      {children && <div className="flex flex-wrap gap-2 items-center">{children}</div>}
    </div>
  );
}

/** Segmented control. `role` tablist for views, radiogroup for values (status). */
export function Seg<T extends string>({ label, options, value, onChange, radio, accent }: {
  label: string; options: [T, ReactNode][]; value: T; onChange: (v: T) => void; radio?: boolean; accent?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const move = (e: React.KeyboardEvent, i: number) => {
    const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const n = (i + d + options.length) % options.length;
    onChange(options[n][0]);
    (ref.current?.children[n] as HTMLElement | undefined)?.focus();
  };
  return (
    <div ref={ref} className={`seg${accent ? " accent" : ""}`} role={radio ? "radiogroup" : "tablist"} aria-label={label}>
      {options.map(([v, t], i) => (
        <button key={v} type="button" role={radio ? "radio" : "tab"} {...(radio ? { "aria-checked": v === value } : { "aria-selected": v === value })}
          tabIndex={v === value || (!options.some((o) => o[0] === value) && i === 0) ? 0 : -1} onClick={() => onChange(v)} onKeyDown={(e) => move(e, i)}>{t}</button>
      ))}
    </div>
  );
}

export function Stat({ k, v, d, act }: { k: ReactNode; v: ReactNode; d?: ReactNode; act?: boolean }) {
  return <div className="card stat hover"><span className="k">{k}</span><span className="v">{v}</span>{d != null && <span className={`d${act ? " act" : ""}`}>{d}</span>}</div>;
}

export function Empty({ h, line, children }: { h: ReactNode; line: ReactNode; children?: ReactNode }) {
  return <div className="card empty"><span className="h">{h}</span><span className="text-text-2">{line}</span>{children}</div>;
}

/** Errors stay inline (DESIGN §5 toast rule). */
export function ErrorBanner({ error, title = "Something went wrong.", onRetry }: { error: AppError | undefined; title?: string; onRetry?: () => void }) {
  return (
    <div className="banner err" role="alert">
      <span><b className="text-danger">{title}</b> <span className="text-text-2">{error?.message}</span></span>
      {onRetry && <button className="btn btn-secondary s" onClick={onRetry}>Try again</button>}
    </div>
  );
}

export function SkelRows({ n = 6 }: { n?: number }) {
  return (
    <div className="card pad-s" aria-busy="true" aria-label="Loading">
      <div className="list">{Array.from({ length: n }, (_, i) => (
        <div key={i} className="row grid-cols-[36px_minmax(0,1fr)_60px]">
          <div className="skel size-9" />
          <div className="flex flex-col gap-1.5"><div className="skel h-2.5" style={{ width: `${60 + ((i * 13) % 30)}%` }} /><div className="skel h-2" style={{ width: `${30 + ((i * 7) % 20)}%` }} /></div>
          <div className="skel h-2" />
        </div>))}
      </div>
    </div>
  );
}
export function SkelTiles({ n = 5 }: { n?: number }) {
  return <div className="tiles five" aria-busy="true" aria-label="Loading">{Array.from({ length: n }, (_, i) => (
    <div key={i} className="flex flex-col gap-1.5"><div className="skel aspect-square rounded-control" /><div className="skel h-2.5 w-[70%]" /><div className="skel h-2 w-[45%]" /></div>))}
  </div>;
}
export const SkelBlock = ({ h = 120 }: { h?: number }) => <div className="skel rounded-card" style={{ height: h }} aria-busy="true" aria-label="Loading" />;

/** Button label replaced by 3-dot progress while working; width kept (DESIGN §3 loading state). */
export const Dots = () => <span aria-label="Working" className="tracking-[3px]">•••</span>;

/* ------------------------------------------------------------------ badges, chips, small marks */
export const LiveDot = () => <span className="live" aria-hidden="true" />;
export const NameNeeded = () => <span className="badge need">Name needed</span>;
export const StatusBadge = ({ children }: { children: ReactNode }) => <span className="badge status">{children}</span>;
export const SourceTag = ({ sources }: { sources: Source[] }) => !sources.length ? null : <span className="text-faint text-xs">{sourceText(sources)}</span>;
export const FreshnessNote = ({ freshness, loggerState }: { freshness: Freshness | undefined; loggerState?: string }) =>
  <span className="freshness">{freshnessText(freshness, loggerState)}</span>;
export const Stars = ({ r }: { r: number | null }) => (r ? <span className="stars" aria-label={`Rated ${r} of 5`}>{stars(r)}</span> : null);

export function TagChips({ tags, onRemove, onAdd }: { tags: string[]; onRemove?: (t: string) => void; onAdd?: (t: string) => void }) {
  const [adding, setAdding] = useState(false);
  return (
    <div className="flex flex-wrap gap-2 items-center">
      {tags.map((t) => onRemove
        ? <button key={t} className="chip" onClick={() => onRemove(t)} aria-label={`Remove tag ${t}`}>{t} <X size={12} aria-hidden /></button>
        : <span key={t} className="chip">{t}</span>)}
      {onAdd && (adding
        ? <input className="input h-8 w-[140px]" autoFocus aria-label="New tag" placeholder="New tag"
            onKeyDown={(e) => { if (e.key === "Enter") { const v = e.currentTarget.value.trim().toLowerCase(); if (v) onAdd(v); setAdding(false); } if (e.key === "Escape") setAdding(false); }}
            onBlur={() => setAdding(false)} />
        : <button className="chip add" onClick={() => setAdding(true)}>Add tag</button>)}
    </div>
  );
}

/* ------------------------------------------------------------------ opinion controls */
/** 5 stars with halves; role="slider", arrows ±0.5 (DESIGN §5). */
export function Rating({ value, onChange, label = "Your rating" }: { value: number | null; onChange: (v: number | null) => void; label?: string }) {
  const v = value ?? 0;
  return (
    <div className="rating" role="slider" tabIndex={0} aria-label={label} aria-valuemin={0} aria-valuemax={5} aria-valuenow={v}
      aria-valuetext={value ? `${value} of 5 stars` : "Not rated"}
      onKeyDown={(e) => { const n = ratingKey(v, e.key); if (n != null) { e.preventDefault(); onChange(n || null); } }}>
      {[0, 1, 2, 3, 4].map((i) => (
        <button key={i} type="button" tabIndex={-1} aria-hidden="true" className={v >= i + 1 ? "on" : v >= i + 0.5 ? "half" : ""}
          onClick={(e) => { const r = e.currentTarget.getBoundingClientRect(); onChange(ratingClick(value, i, e.clientX - r.left < r.width / 2)); }}>★</button>
      ))}
    </div>
  );
}

/** Planned · In progress · Explored · Not for me · Abandoned (segmented). Revisit is a separate toggle. */
export const StatusControl = ({ value, onChange }: { value: Status; onChange: (s: Status) => void }) =>
  <Seg label="Status" radio accent options={STATUS_ORDER.map((s) => [s, STATUS[s]])} value={value} onChange={onChange} />;

/** Priority <select> (quick add, item detail); "" means none. */
export const PrioritySelect = ({ id, value, onChange, className = "input" }: { id: string; value: Priority | ""; onChange: (p: Priority | "") => void; className?: string }) => (
  <select id={id} className={className} value={value} onChange={(e) => onChange(e.target.value as Priority | "")}>
    <option value="">None</option>{(Object.keys(PRIO) as Priority[]).map((p) => <option key={p} value={p}>{p[0].toUpperCase() + p.slice(1)}</option>)}
  </select>
);

/* ------------------------------------------------------------------ rows and cards */
/** Record card (default, text-first; no artwork). */
export function RecordCard({ it, meta, act }: { it: ItemSummary; meta?: ReactNode; act?: boolean }) {
  return (
    <Link to={`/item/${it.id}`} className="card hover flex flex-col gap-1 text-inherit">
      <span className="text-muted text-xs">{TYPE[it.type]}</span>
      <span className="font-bold">{nameOf(it)}</span>
      {it.type !== "artist" && <span className="text-muted text-sm">{sub(it)}</span>}
      <span className={`font-mono text-xs ${act ? "text-accent" : "text-text-2"}`}>{meta ?? statusMeta(it)}</span>
      {it.revisit && <span className="badge status self-start">Revisit</span>}
    </Link>
  );
}

/** Rank bar row: rank, name + your-status suffix, bar without track, mono count. */
export function RankRow({ rank, name, by, href, suffix, value, max, label, hot }: {
  rank: number; name: string; by?: string; href?: string; suffix?: ReactNode; value: number; max: number; label: ReactNode; hot?: boolean;
}) {
  return (
    <div className="rank">
      <span className="font-mono text-faint text-xs">{rank}</span>
      <div className="min-w-0">
        <div className="truncate">{href ? <Link to={href} className="text-text font-semibold">{name}</Link> : <b>{name}</b>}{by && <span className="text-muted"> {by}</span>}{suffix && <span className="text-muted text-xs"> {suffix}</span>}</div>
        <div className={`bar${hot ? " hot" : ""}`} style={{ width: `${Math.max(2, (value / (max || 1)) * 100)}%` }} />
      </div>
      <span className="font-mono text-text-2 whitespace-nowrap text-xs text-right">{label}</span>
    </div>
  );
}

/** Play row: track, artist, your stars or a Rate button, mono relative time. Skipped text in --faint (never opacity). */
export function PlayRow({ p, onRate, onSave, wide }: { p: Play; onRate?: (p: Play) => void; onSave?: (p: Play) => void; wide?: boolean }) {
  if (wide) return <PlayRowWide p={p} onRate={onRate} onSave={onSave} />;
  return (
    <div className="row grid-cols-[minmax(0,1fr)_auto_70px]">
      <span className="flex flex-col gap-0.5 min-w-0">
        {p.item ? <Link to={`/item/${p.item.id}`} className={`truncate font-bold ${p.skipped ? "text-faint" : "text-text"}`}>{p.track_name}</Link>
          : <b className={`truncate ${p.skipped ? "text-faint" : ""}`}>{p.track_name}</b>}
        <span className="text-muted truncate text-xs">{p.artist_name}{p.source === "export" ? ", phone or other device" : ""}</span>
      </span>
      {p.skipped ? <span className="text-faint text-xs">Skipped, not counted</span> : <PlayAction p={p} onRate={onRate} onSave={onSave} save="Save to backlog" />}
      <span className="font-mono text-faint text-xs text-right">{ago(p.ended_at_local)}</span>
    </div>
  );
}

/** SC-08 variant: clock time, track, device, duration, your stars or an action. */
function PlayRowWide({ p, onRate, onSave }: { p: Play; onRate?: (p: Play) => void; onSave?: (p: Play) => void }) {
  const dim = p.skipped && !p.counts_as_play;
  return (
    <div className="row grid-cols-[44px_minmax(0,1fr)_64px_50px_140px]">
      <span className="font-mono text-faint text-xs">{time(p.ended_at_local)}</span>
      <span className="flex flex-col gap-0.5 min-w-0">
        <span className={`truncate font-semibold ${dim ? "text-faint" : ""}`}>{p.item ? <Link to={`/item/${p.item.id}`} className="text-inherit">{p.track_name}</Link> : p.track_name}</span>
        <span className="text-muted truncate text-xs">{p.artist_name}</span>
      </span>
      <span className="text-muted text-xs">{p.source === "logger" ? "This PC" : "Phone"}</span>
      <span className={`font-mono text-xs ${dim ? "text-faint" : "text-text-2"}`}>{mmss(p.ms_played)}</span>
      <span className="text-sm justify-self-end">
        {dim ? <span className="text-faint">Skipped, not counted</span> : <PlayAction p={p} onRate={onRate} onSave={onSave} save="Save" />}
      </span>
    </div>
  );
}

/** Your stars (the track's, else the album's with an "album" suffix), else Rate (play linked to a record), else Save (not in your library). */
function PlayAction({ p, onRate, onSave, save }: { p: Play; onRate?: (p: Play) => void; onSave?: (p: Play) => void; save: string }) {
  if (p.item?.rating) return <Stars r={p.item.rating} />;
  if (p.album_rating) return <span className="stars whitespace-nowrap" aria-label={`Album rated ${p.album_rating} of 5`}>{stars(p.album_rating)}<span className="text-faint text-[11px] tracking-normal"> album</span></span>;
  return p.item
    ? <button className="btn btn-secondary s" onClick={() => onRate?.(p)} aria-label={`Rate ${p.track_name}`}>Rate</button>
    : <button className="btn btn-secondary s" onClick={() => onSave?.(p)} aria-label={`Save ${p.track_name} to backlog`}>{save}</button>;
}

/* ------------------------------------------------------------------ Spotify content */
/** Spotify logo (DESIGN §7: official file, unaltered, ≥ 70 px wide). */
export const SpotifyLogo = ({ width = 80 }: { width?: number }) => <img src={spotifyLogo} alt="Spotify" width={Math.max(70, width)} className="flex-none" />;
/** Opens the Spotify app / site for a URI (Rust opens the URL in M6; here a plain link). */
export const OpenSpotify = ({ uri, name }: { uri?: string | null; name?: string }) =>
  <a href={uri ? openUrl(uri) : "https://open.spotify.com"} target="_blank" rel="noreferrer" className="text-sm font-semibold" aria-label={name ? `Open ${name} in Spotify` : undefined}>Open Spotify</a>;
/** Attribution: Spotify logo + "Open Spotify" wherever Spotify-sourced content appears outside the embed. */
export const Attribution = ({ uri, name, children }: { uri?: string | null; name?: string; children?: ReactNode }) =>
  <span className="flex flex-wrap gap-2 items-center"><SpotifyLogo />{children}<OpenSpotify uri={uri} name={name} /></span>;

/** Embed URL derived from the stored URI (TSD §4.3); null for a malformed URI, so nothing is mounted. */
export const embedUrl = (uri: string) => { const u = safeUri(uri); return u ? `https://open.spotify.com/embed/${u.type}/${u.id}` : null; };

/** Spotify embed slot: iframe only when online and scrolled into view; neutral placeholder offline. Never cropped or overlaid. */
export function EmbedSlot({ uri, height = 152, compact }: { uri: string | null; height?: number; compact?: boolean }) {
  const online = useOnline();
  const ref = useRef<HTMLDivElement>(null);
  const [seen, setSeen] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el || seen) return;
    if (typeof IntersectionObserver === "undefined") { setSeen(true); return; }
    const io = new IntersectionObserver((es) => { if (es.some((e) => e.isIntersecting)) { setSeen(true); io.disconnect(); } }, { rootMargin: "200px" });
    io.observe(el);
    return () => io.disconnect();
  }, [seen]);
  // Spotify embed heights are 152 or 352 (kb/spotify.md); never clip the widget (DESIGN §5).
  const style = { height: compact ? 152 : Math.max(152, height) };
  const src = uri ? embedUrl(uri) : null;
  if (!src) return <div ref={ref} className="embed off" style={style} role="img" aria-label="No Spotify link"><b className="text-text-2">No link</b>Paste a Spotify link to see it here</div>;
  if (!online) return <div ref={ref} className="embed off" style={style} role="img" aria-label="Artwork unavailable offline"><b className="text-text-2">Offline</b>Artwork returns when you reconnect</div>;
  return (
    <div ref={ref} style={style}>
      {seen && <iframe title="Spotify embed" src={src} width="100%" height={style.height} className="border-0 block rounded-[12px]"
        allow="autoplay; clipboard-write; encrypted-media; fullscreen; picture-in-picture" loading="lazy" />}
    </div>
  );
}

/** Artwork mode set (DESIGN §5 artwork tile): compact embed slots, at most 20, ending with logo + "Open Spotify". */
export function ArtTiles({ items, total, meta }: { items: ItemSummary[]; total?: number; meta?: (it: ItemSummary) => ReactNode }) {
  const shown = items.slice(0, 20);
  return (
    <>
      <div className="tiles">{shown.map((it) => (
        <div key={it.id} className="tile">
          <EmbedSlot uri={it.spotify_uri} compact />
          <Link to={`/item/${it.id}`} className="t text-text">{nameOf(it)}</Link>
          <span className="s">{it.type === "artist" ? "Artist" : it.artist_names.join(", ")}</span>
          {meta && <span className={`m${it.status === "in_progress" ? " act" : ""}`}>{meta(it)}</span>}
        </div>))}
      </div>
      <div className="flex flex-wrap gap-2 items-center justify-between">
        <Attribution />
        <span className="font-mono text-faint text-xs">{shown.length} of {total ?? items.length}</span>
      </div>
    </>
  );
}

/** A record set as text cards (default) or ArtTiles (artwork on). Text mode keeps the same 20 cap and attribution footer. */
export function ItemSet({ items, art, total, meta, act }: {
  items: ItemSummary[]; art: boolean; total?: number; meta: (it: ItemSummary) => ReactNode; act?: (it: ItemSummary) => boolean;
}) {
  if (!items.length) return null;
  if (art) return <ArtTiles items={items} total={total} meta={meta} />;
  const shown = items.slice(0, 20);
  const all = total ?? items.length;
  return (
    <>
      <div className="tiles">{shown.map((it) => <RecordCard key={it.id} it={it} meta={meta(it)} act={act?.(it)} />)}</div>
      {all > shown.length && <span className="font-mono text-faint text-xs">{shown.length} of {all}</span>}
    </>
  );
}

/* ------------------------------------------------------------------ dialog */
/** Native modal <dialog>: focus trap and Esc come free (DESIGN: Esc closes a dialog). */
export function Dialog({ title, onClose, children, footer, wide }: { title: ReactNode; onClose: () => void; children: ReactNode; footer?: ReactNode; wide?: boolean }) {
  const ref = useRef<HTMLDialogElement>(null);
  // Open before the content mounts: showModal() moves focus to the first control (Close), so an autoFocus field
  // (quick-add link, name) must mount inside the already-open dialog to keep focus.
  const [open, setOpen] = useState(false);
  useLayoutEffect(() => { const d = ref.current; if (d && !d.open) d.showModal?.(); setOpen(true); }, []);
  return (
    <dialog ref={ref} className={`overlay dialog ${wide ? "w-[min(760px,100%)]" : ""}`} aria-labelledby="dlg-t"
      onCancel={(e) => { e.preventDefault(); onClose(); }} onClick={(e) => { if (e.target === ref.current) onClose(); }}>
      <div className="flex justify-between items-center gap-3">
        <h1 id="dlg-t" className="text-xl font-extrabold">{title}</h1>
        <button className="icon-btn" onClick={onClose} aria-label="Close"><X size={18} aria-hidden /></button>
      </div>
      {open && children}
      {open && footer && <div className="flex flex-wrap gap-2 items-center justify-end">{footer}</div>}
    </dialog>
  );
}

/* ------------------------------------------------------------------ toast */
interface ToastMsg { text: ReactNode; undo?: () => void | Promise<unknown> }
const ToastCtx = createContext<(text: ReactNode, undo?: ToastMsg["undo"]) => void>(() => {});
/** Transient confirmations only, with Undo where the action can be undone (5 s). */
export const useToast = () => useContext(ToastCtx);
export function ToastProvider({ children }: { children: ReactNode }) {
  const [msg, setMsg] = useState<(ToastMsg & { n: number }) | null>(null);
  const timer = useRef<number>(undefined);
  const show = useCallback((text: ReactNode, undo?: ToastMsg["undo"]) => {
    setMsg({ text, undo, n: Date.now() });
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setMsg(null), 5000);
  }, []);
  return (
    <ToastCtx.Provider value={show}>
      {children}
      <div aria-live="polite">{msg && (
        <div key={msg.n} className="toast overlay" role="status">
          <span>{msg.text}</span>
          {msg.undo && <button className="btn btn-secondary s" onClick={() => { const u = msg.undo; setMsg(null); u?.(); }}>Undo</button>}
        </div>)}
      </div>
    </ToastCtx.Provider>
  );
}


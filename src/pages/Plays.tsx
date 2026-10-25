// SC-08 Recent plays: plays by day, desktop logger control, unmatched plays (CAP-18, 20, 21).
// M9 (24 Oct 2026, user: "refactor the recent play feature using taste + motion"): built for years of history. Plays load
// newest first, a page at a time as you scroll (offset pages appended), each day is a section with its own numbers and a
// 24-hour strip, and off-screen days skip layout and paint (`content-visibility`). Motion: rows arrive, a day opens.
import { memo, useCallback, useEffect, useRef, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { AnimatePresence, m, useReducedMotion } from "motion/react";
import { DesktopIcon, DeviceMobileIcon } from "@phosphor-icons/react";
import * as api from "../api";
import type { Play, Unmatched } from "../api";
import { Dialog, Dots, Empty, ErrorBanner, FreshnessNote, LiveDot, PageHead, Seg, SkelRows, SourceTag, Stars, TabPanel, useSlide, useToast } from "../components/ui";
import { Topbar } from "../components/Shell";
import { useQuickAdd } from "../components/QuickAddContext";
import { useApi } from "../hooks/useApi";
import { daysSince, day, hm, mmss, nameOf, num, pl, stars, sub, time } from "../lib/format";

type Tab = "all" | "unrated" | "missing";
const TABS: Tab[] = ["all", "unrated", "missing"];
/** Plays per request; at most Rust's `MAX_PAGE` (200, insights/plays.rs; the mock applies it too, check.py "plays page"). */
export const PAGE = 150;
const PER_DAY = 12;
const EASE = [0.16, 1, 0.3, 1] as const;
/** Reduced motion: MotionConfig stops transforms but still fades, and AnimatePresence still waits; go instant. */
const swap = (reduce: boolean | null, t: object) => (reduce ? { duration: 0 } : t);

const dayLabel = (iso: string) => { const d = daysSince(iso); return d === 0 ? "Today" : d === 1 ? "Yesterday" : day(iso); };

/** Plays grouped by calendar day, newest first (rows arrive sorted). */
function byDay(rows: Play[]) {
  const days: { key: string; plays: Play[] }[] = [];
  for (const p of rows) {
    const k = p.ended_at_local.slice(0, 10);
    const last = days[days.length - 1];
    if (last?.key === k) last.plays.push(p); else days.push({ key: k, plays: [p] });
  }
  return days;
}

/* ------------------------------------------------------------------ one play */
function PlayLine({ p, i, onRate, onSave }: { p: Play; i: number; onRate: (p: Play) => void; onSave: (p: Play) => void }) {
  const reduce = useReducedMotion();
  const dim = p.skipped && !p.counts_as_play;
  const pc = p.source === "logger";
  const Device = pc ? DesktopIcon : DeviceMobileIcon;
  return (
    <m.div className="row pl-row" data-h={Number(p.ended_at_local.slice(11, 13))}
      initial={reduce ? false : { opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.28, ease: EASE, delay: Math.min(i, 12) * 0.022 }}>
      <span className="font-mono text-faint text-xs">{time(p.ended_at_local)}</span>
      <span className="flex flex-col gap-0.5 min-w-0">
        <span className={`truncate font-semibold ${dim ? "text-faint" : ""}`}>{p.item ? <Link to={`/item/${p.item.id}`} className="pl-stretch text-inherit">{p.track_name}</Link> : p.track_name}</span>
        <span className="text-muted truncate text-xs">{p.artist_name}</span>
      </span>
      <span className="pl-dev" title={pc ? "This PC" : "Phone or another device, from your export"}><Device size={16} aria-hidden /><span className="sr-only">{pc ? "This PC" : "Phone"}</span></span>
      <span className={`font-mono text-xs ${dim ? "text-faint" : "text-text-2"}`}>{mmss(p.ms_played)}</span>
      <span className="text-sm justify-self-end pl-act">
        {dim ? <span className="text-faint text-xs">Skipped, not counted</span>
          : p.item?.rating ? <Stars r={p.item.rating} />
          : p.album_rating ? <span className="stars whitespace-nowrap" aria-label={`Album rated ${p.album_rating} of 5`}>{stars(p.album_rating)}<span className="text-faint text-[11px] tracking-normal"> album</span></span>
          : p.item ? <button className="btn btn-secondary s" onClick={() => onRate(p)} aria-label={`Rate ${p.track_name}`}>Rate</button>
          : <button className="btn btn-secondary s" onClick={() => onSave(p)} aria-label={`Save ${p.track_name} to backlog`}>Save</button>}
      </span>
    </m.div>
  );
}

/* ------------------------------------------------------------------ one day: numbers, hour strip, plays */
function DayGroupView({ k, plays }: { k: string; plays: Play[] }) {
  const reduce = useReducedMotion();
  const [all, setAll] = useState(false);
  const nav = useNavigate();
  const quickAdd = useQuickAdd();
  const strip = useRef<HTMLSpanElement>(null);
  const counted = plays.filter((p) => p.counts_as_play);
  const ms = counted.reduce((a, p) => a + p.ms_played, 0);
  const hours = Array<number>(24).fill(0);
  for (const p of counted) hours[Number(p.ended_at_local.slice(11, 13))]++;
  const top = Math.max(1, ...hours);
  const busiest = hours.indexOf(Math.max(...hours));
  const phone = plays.some((p) => p.source === "export");
  const rate = (x: Play) => nav(`/item/${x.item!.id}`);
  const save = (x: Play) => quickAdd({ suggest: { name: x.track_name, artist: x.artist_name, type: "track", uri: x.spotify_uri, from: "plays" } });
  // Pointing at a play lights its hour in the strip (DOM class, no React state per move).
  const light = (e: React.PointerEvent) => {
    const h = (e.target as HTMLElement).closest<HTMLElement>("[data-h]")?.dataset.h;
    strip.current?.querySelectorAll("i").forEach((el, i) => el.classList.toggle("on", String(i) === h));
  };
  const line = (p: Play, i: number) => <PlayLine key={p.id} p={p} i={i} onRate={rate} onSave={save} />;
  const id = `pl-${k}`;
  return (
    <section className="pl-day" aria-labelledby={id} onPointerOver={light} onPointerLeave={light}>
      <div className="pl-day-head">
        <span className="flex flex-col gap-0.5 min-w-0">
          <h2 id={id} className="text-md font-extrabold tracking-[-0.01em]">{dayLabel(plays[0].ended_at_local)}</h2>
          <span className="text-muted text-xs">{pl(counted.length, "play")}, {hm(ms)}{phone ? ", including plays from your Spotify export" : ""}</span>
        </span>
        <span className="pl-clock">
          <span ref={strip} className="pl-hours" role="img" aria-label={counted.length ? `Plays by hour, busiest at ${String(busiest).padStart(2, "0")}:00` : "No counted plays"}>
            {hours.map((c, h) => <i key={h} style={{ height: `${c ? 20 + (c / top) * 80 : 8}%` }} className={c ? undefined : "zero"} />)}
          </span>
          <span className="pl-ticks" aria-hidden><span>00</span><span>06</span><span>12</span><span>18</span></span>
        </span>
      </div>
      <div className="card pad-s"><div className="list">
        {plays.slice(0, PER_DAY).map(line)}
        <AnimatePresence initial={false}>
          {all && plays.length > PER_DAY && (
            <m.div key="more" className="overflow-hidden" initial={{ height: 0, opacity: 0 }} animate={{ height: "auto", opacity: 1 }} exit={{ height: 0, opacity: 0 }}
              transition={swap(reduce, { duration: 0.3, ease: EASE })}>{plays.slice(PER_DAY).map((p, i) => line(p, i))}</m.div>
          )}
        </AnimatePresence>
      </div></div>
      {plays.length > PER_DAY && (
        <div className="flex flex-wrap gap-2 items-center justify-between">
          <span className="font-mono text-faint text-xs">{all ? plays.length : PER_DAY} of {plays.length} shown</span>
          <button className="btn btn-secondary s" onClick={() => setAll(!all)} aria-expanded={all}>{all ? "Show fewer" : `Show ${num(plays.length - PER_DAY)} more`}</button>
        </div>
      )}
    </section>
  );
}

const DayGroup = memo(DayGroupView, (a, b) => a.k === b.k && a.plays.length === b.plays.length && a.plays.every((p, i) => p === b.plays[i]));

/* ------------------------------------------------------------------ side: logger, unmatched */
function Logger() {
  const st = useApi(api.getLoggerStatus);
  const [busy, setBusy] = useState(false);
  const act = (fn: () => Promise<unknown>) => { setBusy(true); fn().finally(() => setBusy(false)); };
  const s = st.data?.state;
  const on = s === "listening" || s === "idle";
  return (
    <div className="card flex flex-col gap-3">
      <div className="flex justify-between items-center gap-3">
        <h2 className="card-title">Desktop logger</h2>
        {st.data && !st.error && s !== "error" && <span className={`pl-state${on ? " on" : ""}`}>{on && <LiveDot />}{s === "listening" ? "Logging" : s === "idle" ? "Waiting for Spotify" : s === "paused" ? "Paused" : "Off"}</span>}
      </div>
      {st.error || s === "error"
        ? <ErrorBanner error={st.error ?? { code: "internal", message: "Windows isn't sharing media info. Restart Spotify, then try again." }} title="Logger stopped." onRetry={() => act(api.resumeLogger)} />
        : <>
          <span className="text-text-2 leading-[1.5]">
            {s === "paused" ? "Nothing is logged until you resume." : s === "off" ? "Nothing on this PC is logged." : "Spotify on this PC is logged. A play counts after 30 seconds."}
          </span>
          {s === "listening" && st.data?.now_playing && <span className="text-sm min-w-0 truncate"><span className="text-muted">Now: </span><b>{st.data.now_playing.track_name}</b><span className="text-muted">, {st.data.now_playing.artist_name}</span></span>}
          <div className="flex flex-wrap gap-2 items-center">
            {busy ? <Dots /> : s === "paused" ? <button className="btn btn-light" onClick={() => act(api.resumeLogger)}>Resume logging</button>
              : s === "off" ? <button className="btn btn-light" onClick={() => act(() => api.setLogger({ on: true }))}>Turn on the desktop logger</button>
              : <><button className="btn btn-light" onClick={() => act(() => api.pauseLogger(60))}>Pause for 1 hour</button>
                <button className="btn btn-secondary" onClick={() => act(() => api.pauseLogger(null))}>Pause</button></>}
          </div>
        </>}
      <span className="text-muted text-xs leading-[1.45]">Private sessions can't be detected on the PC. Pause before one if you don't want it logged.</span>
    </div>
  );
}

function UnmatchedCard() {
  const um = useApi(api.listUnmatched);
  const toast = useToast();
  const [choosing, setChoosing] = useState<Unmatched | null>(null);
  const link = (u: Unmatched, itemId: number) => api.linkPlay(u.event_id, itemId).then(
    () => { setChoosing(null); toast(`Linked. ${pl(u.plays, "play")} now count for this record`); },
    (e) => toast(api.toAppError(e).message));
  if (um.error) return <ErrorBanner error={um.error} title="Couldn't load unmatched plays." onRetry={um.reload} />;
  if (!um.data?.length) return null;
  return (
    <div className="card flex flex-col gap-3">
      <div className="flex justify-between items-baseline gap-3"><h2 className="card-title">Unmatched plays</h2><span className="font-mono text-accent text-xs">{um.data.length}</span></div>
      <span className="text-text-2 text-sm leading-[1.5]">More than one record in your library could be the track. Pick the right one.</span>
      {um.data.map((u) => (
        <div key={u.norm_key} className="flex flex-col gap-1.5 px-3 py-2.5 rounded-control bg-surface-2">
          <span className="font-semibold">{u.track_name}, {u.artist_name}</span>
          <span className="text-muted text-xs">{u.detail}. {pl(u.plays, "play")} waiting.</span>
          <div className="flex flex-wrap gap-2 items-center">
            {u.candidates[0] && <button className="btn btn-light s" onClick={() => link(u, u.candidates[0].id)} aria-label={`${u.suggestion} for ${u.track_name}`}>{u.suggestion}</button>}
            <button className="btn btn-secondary s" onClick={() => setChoosing(u)} aria-label={`Choose another record for ${u.track_name}`}>Choose another</button>
          </div>
        </div>
      ))}
      {choosing && (
        <Dialog title={`Which record is ${choosing.track_name}?`} onClose={() => setChoosing(null)}>
          <div className="list">
            {choosing.candidates.map((it) => (
              <button key={it.id} className="row hover grid-cols-[minmax(0,1fr)_auto] text-left bg-transparent border-0 text-inherit cursor-pointer" onClick={() => link(choosing, it.id)}>
                <span className="flex flex-col gap-0.5"><b>{nameOf(it)}</b><span className="text-muted text-xs">{it.type === "album" ? "Album" : "Track"}, {sub(it)}</span></span>
                <span className="text-text-2 text-sm">Link</span>
              </button>
            ))}
          </div>
        </Dialog>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ page */
/** Newest page live (refetched on every write and new play); earlier pages appended once by offset. A new play shifts
 *  offsets by one, so pages are merged by play id. `date` (M10 Trends calendar) keeps one local day. */
function usePlayPages(which: Tab, date: string | undefined) {
  const tab = which + (date ? `@${date}` : ""); // pages belong to a tab and a day
  const q = { tab: which, range: "all" as const, ...(date && { date }) };
  const first = useApi(() => api.listPlays({ ...q, offset: 0, limit: PAGE }).then((page) => ({ tab, page })), [tab]);
  const [older, setOlder] = useState<{ tab: string; rows: Play[]; end: boolean }>({ tab, rows: [], end: false });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<api.AppError>();
  const cur = first.data?.tab === tab ? first.data.page : undefined; // never show the previous tab's rows
  const mine = older.tab === tab ? older : { tab, rows: [], end: false };
  const seen = new Set<number>();
  const rows = [...(cur?.rows ?? []), ...mine.rows].filter((p) => !seen.has(p.id) && !!seen.add(p.id));
  const end = !cur || cur.rows.length < PAGE || mine.end || rows.length >= cur.total;
  // ponytail: older pages don't refetch on writes (a Save there shows on the next visit); the newest page does.
  const more = useCallback(() => {
    if (busy || end) return;
    setBusy(true); setError(undefined);
    api.listPlays({ ...q, offset: rows.length, limit: PAGE }).then(
      (pg) => setOlder((o) => ({ tab, rows: [...(o.tab === tab ? o.rows : []), ...pg.rows], end: pg.rows.length < PAGE })),
      (e) => setError(api.toAppError(e)),
    ).finally(() => setBusy(false));
  }, [busy, end, tab, rows.length]); // eslint-disable-line react-hooks/exhaustive-deps -- q follows tab
  return { loading: !cur && !first.error, error: first.error, reload: first.reload, rows, total: cur?.total ?? 0, end, busy, more, moreError: error };
}

export default function Plays() {
  const tab = (useParams().tab ?? "all") as Tab;
  const nav = useNavigate();
  const rawDate = useSearchParams()[0].get("date"); // #/plays/all?date=YYYY-MM-DD from the Trends calendar
  const date = rawDate && /^\d{4}-\d{2}-\d{2}$/.test(rawDate) ? rawDate : undefined; // a hand-edited link falls back to every day
  const plays = usePlayPages(tab, date);
  const unrated = useApi(() => api.listPlays({ tab: "unrated", range: "all", offset: 0, limit: 1 }));
  const missing = useApi(() => api.listPlays({ tab: "missing", range: "all", offset: 0, limit: 1 }));
  const fresh = useApi(api.getFreshness);
  const logger = useApi(api.getLoggerStatus);
  const [dir, slide] = useSlide(TABS, tab);
  const { rows, total, end, busy, more } = plays;
  const sentinel = useRef<HTMLDivElement>(null);
  // The next page loads before you reach the end (600 px ahead); the button below does the same by keyboard.
  // Re-armed per tab and per page: the panel remounts on a tab switch, and an observer fires once per entry.
  useEffect(() => {
    const el = sentinel.current;
    if (!el || end || busy || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver(([e]) => { if (e.isIntersecting) more(); }, { rootMargin: "600px" });
    io.observe(el);
    return () => io.disconnect();
  }, [end, busy, more, tab, rows.length]);
  const n = (s: typeof unrated) => (s.data ? ` ${num(s.data.total)}` : "");
  // Sources of the whole history (FR-032), not only the pages loaded so far: the export and logger dates say which exist.
  const sources: api.Source[] = [...(rows.some((p) => p.item) ? ["U" as const] : []), ...(fresh.data?.logger_last_event ? ["S" as const] : []), ...(fresh.data?.export_latest_event ? ["E" as const] : [])];

  let content;
  if (plays.loading) content = <SkelRows n={10} />;
  else if (plays.error) content = <ErrorBanner error={plays.error} title="Couldn't load your plays." onRetry={plays.reload} />;
  else if (!rows.length && date) content = <Empty h="No plays that day" line="Nothing was counted on this day."><Link className="btn btn-secondary" to={`/plays/${tab}`}>Show every day</Link></Empty>;
  else if (!rows.length) content = tab === "all"
    ? <Empty h="No plays yet" line="Turn on the desktop logger, or import your Spotify history to see plays from every device.">
        <div className="flex flex-wrap gap-2 items-center"><Link className="btn btn-primary" to="/import/1">Import history</Link><button className="btn btn-secondary" onClick={() => api.setLogger({ on: true })}>Turn on the desktop logger</button></div>
      </Empty>
    : <Empty h="Nothing here" line={tab === "unrated" ? "Every play you've counted has your rating." : "Every play is linked to a record in your library."}>
        <Link className="btn btn-secondary" to="/plays/all">See all plays</Link>
      </Empty>;
  else content = <>
    {byDay(rows).map((d) => <DayGroup key={d.key} k={d.key} plays={d.plays} />)}
    <div ref={sentinel} className="pl-more">
      <span className="font-mono text-faint text-xs">{num(rows.length)} of {pl(total, "play")}</span>
      {plays.moreError ? <ErrorBanner error={plays.moreError} title="Couldn't load earlier plays." onRetry={more} />
        : end ? <span className="text-muted text-xs">That’s everything, back to your first play.</span>
        // One button stays mounted and focusable while a page loads (`disabled` would drop keyboard focus); more() ignores repeats
        : <button className="btn btn-secondary s" onClick={more} aria-disabled={busy} aria-busy={busy}>{busy ? <Dots /> : "Load earlier plays"}</button>}
    </div>
  </>;

  return (
    <>
      <Topbar />
      <div className="flex flex-wrap gap-7 items-start">
        <section className="flex-[1.6_1_560px] min-w-0 flex flex-col gap-3">
          <PageHead title="Recent plays">
            <Seg label="Which plays" value={tab} onChange={(t) => { slide(t); nav(`/plays/${t}`); }}
              options={[["all", "All"], ["unrated", `Not rated${n(unrated)}`], ["missing", `Not in library${n(missing)}`]]} />
          </PageHead>
          <TabPanel key={tab} dir={dir} label={tab === "all" ? "All plays" : tab === "unrated" ? "Not rated" : "Not in library"} className="flex flex-col gap-5">
            <span className="flex flex-wrap gap-1.5 items-center">{rows.length > 0 && <SourceTag sources={sources} />}<FreshnessNote freshness={fresh.data} loggerState={logger.data?.state} /></span>
            {date && <span className="flex flex-wrap gap-3 items-center text-sm"><b>Plays on {day(date + "T00:00")} {date.slice(0, 4)}</b><Link to={`/plays/${tab}`}>Show every day</Link></span>}
            {content}
          </TabPanel>
        </section>
        <aside className="pl-side flex-[1_1_320px] min-w-0 flex flex-col gap-3">
          <Logger />
          <UnmatchedCard />
        </aside>
      </div>
    </>
  );
}

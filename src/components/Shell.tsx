// App shell (DESIGN §4): rail (wordmark + Quick add, grouped nav with one springing indicator, Settings at the foot,
// status card = now playing + freshness), main column, offline banner,
// global quick-add dialog and shortcuts. Pages render their own <Topbar>.
import { lazy, Suspense, useCallback, useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type FormEvent, type ReactNode } from "react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import {
  ArrowsClockwiseIcon, BooksIcon, ChartBarIcon, ClockCounterClockwiseIcon, CompassIcon, GearSixIcon, HouseIcon, PlaylistIcon, PlusIcon, QueueIcon, SidebarSimpleIcon,
} from "@phosphor-icons/react";
import { getCapture, getCounts, getFreshness, getLoggerStatus, invalidateOnDataEvents, MOCK, onEvent, recordPageVisit, type LoggerStatus } from "../api";
import { useApi, useOnline } from "../hooks/useApi";
import { useRelease } from "../hooks/useRelease";
import { useShortcuts, type ShortcutAction } from "../hooks/useShortcuts";
import { fieldButton, freshnessText } from "./logic";
import { LiveDot } from "./ui";
import { QuickAddCtx, useQuickAdd, type QuickAddOpts } from "./QuickAddContext";
import QuickAdd from "./QuickAdd";
import { pl } from "../lib/format";
import appIcon from "../../src-tauri/icons/icon.svg"; // the app icon master: one source for the rail and the installer

// Dev builds only: a shipped build drops the switcher. The mocks themselves go only when VITE_MOCK=0 (api.ts).
const DevSwitcher = import.meta.env.DEV && MOCK ? lazy(() => import("./DevSwitcher")) : () => null;

type Nav = { to: string; label: string; icon: ReactNode; count?: "backlog" | "library" | "revisit"; rel?: "r2" | "r3" };
// M9 rail: grouped by what you're doing (DESIGN §4). Home stands alone; Settings sits apart at the bottom.
const GROUPS: { name?: string; items: Nav[] }[] = [
  { items: [{ to: "/home", label: "Home", icon: <HouseIcon size={20} /> }] },
  { name: "Listening", items: [
    { to: "/plays", label: "Recent plays", icon: <ClockCounterClockwiseIcon size={20} />, rel: "r2" },
    { to: "/insights", label: "Insights", icon: <ChartBarIcon size={20} />, rel: "r2" },
  ] },
  { name: "Collection", items: [
    { to: "/backlog", label: "Backlog", icon: <QueueIcon size={20} />, count: "backlog" },
    { to: "/library", label: "Library", icon: <BooksIcon size={20} />, count: "library" },
    { to: "/rediscover", label: "Rediscover", icon: <ArrowsClockwiseIcon size={20} />, count: "revisit" },
  ] },
  { name: "Spotify", items: [
    { to: "/playlists", label: "Playlists", icon: <PlaylistIcon size={20} />, rel: "r2" },
    { to: "/spotify", label: "From Spotify", icon: <CompassIcon size={20} />, rel: "r3" },
  ] },
];
const SETTINGS: Nav = { to: "/settings", label: "Settings", icon: <GearSixIcon size={20} /> };

// Collapsed rail: the user's choice (button or Ctrl+B, remembered on this PC), forced below 1100 px.
const NARROW = "(max-width: 1100px)";
const onNarrow = (f: () => void) => { const m = window.matchMedia?.(NARROW); m?.addEventListener("change", f); return () => m?.removeEventListener("change", f); };
const isNarrow = () => !!window.matchMedia?.(NARROW).matches;
function useRailMini() {
  const narrow = useSyncExternalStore(onNarrow, isNarrow);
  const [pref, setPref] = useState(() => { try { return localStorage.getItem("runout.rail") === "mini"; } catch { return false; } });
  const toggle = useCallback(() => setPref((p) => {
    try { localStorage.setItem("runout.rail", p ? "full" : "mini"); } catch { /* private storage: just this session */ }
    return !p;
  }), []);
  return { mini: narrow || pref, narrow, toggle };
}

function Rail({ mini, narrow, toggle }: { mini: boolean; narrow: boolean; toggle: () => void }) {
  const { r2, r3 } = useRelease();
  const counts = useApi(getCounts).data;
  const add = useQuickAdd();
  const loc = useLocation();
  const ref = useRef<HTMLElement>(null);
  // One accent marker on the rail edge springs to the current page (same spring as the tabs).
  const [ind, setInd] = useState<{ y: number } | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const place = () => {
      const a = el.querySelector<HTMLElement>('.nav-item[aria-current="page"]');
      setInd(a ? { y: a.offsetTop + (a.offsetHeight - 20) / 2 } : null); // 20 = marker height
    };
    place();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(place);
    ro.observe(el);
    return () => ro.disconnect();
  }, [loc.pathname, r2, r3, mini]);
  const link = (n: Nav) => (
    <NavLink key={n.to} to={n.to} className="nav-item" aria-label={n.label} title={n.label}>
      <span className="nav-label"><span className="nav-icon" aria-hidden>{n.icon}</span><span className="nav-text">{n.label}</span></span>
      <span className="count">{n.count && counts ? counts[n.count] : ""}</span>
    </NavLink>
  );
  const shown = (n: Nav) => !n.rel || (n.rel === "r2" ? r2 : r3);
  return (
    <aside className={`rail${mini ? " mini" : ""}`}>
      <div className="rail-head">
        <NavLink className="logo" to="/home" aria-label="Runout, home"><img src={appIcon} alt="" className="logo-icon" /><span className="nav-text">Runout</span></NavLink>
        {!narrow && <button type="button" className="icon-btn rail-toggle" aria-label={mini ? "Expand sidebar" : "Collapse sidebar"} aria-expanded={!mini}
          title={`${mini ? "Expand" : "Collapse"} sidebar (Ctrl+B)`} onClick={toggle}><SidebarSimpleIcon size={18} /></button>}
      </div>
      <button type="button" className="rail-add" aria-label="Quick add" title="Quick add (Ctrl+N)" onClick={() => add()}>
        <PlusIcon size={18} weight="bold" aria-hidden /><span className="nav-text">Quick add</span><span className="kbd nav-text ml-auto">Ctrl N</span>
      </button>
      <nav ref={ref} className="nav" aria-label="Main">
        {GROUPS.map((g) => {
          const items = g.items.filter(shown);
          if (!items.length) return null;
          return g.name
            ? <div key={g.name} role="group" aria-label={g.name} className="nav-group"><span className="nav-group-name" aria-hidden>{g.name}</span>{items.map(link)}</div>
            : items.map(link);
        })}
        <div className="nav-foot">{link(SETTINGS)}</div>
        {ind && <span className="nav-ind" style={{ transform: `translateY(${ind.y}px)` }} aria-hidden />}
      </nav>
      {!mini && (r2 ? <FreshnessBox /> : <CaptureBox waiting={counts?.name_needed ?? 0} />)}
    </aside>
  );
}

/** What's playing, on every screen (was Home only): live dot, track, artist; opens Home's Now playing card. */
function NowMini({ logger }: { logger: LoggerStatus | undefined }) {
  const t = logger?.state === "listening" ? logger.now_playing : null;
  if (!t) return null;
  return (
    <NavLink to="/home" className="rail-now" aria-label={`Now playing: ${t.track_name} by ${t.artist_name}`}>
      <span className="flex items-center gap-1.5 text-xs text-muted"><LiveDot />Now playing</span>
      <span className="font-semibold truncate">{t.track_name}</span>
      <span className="text-muted text-xs truncate">{t.artist_name}</span>
    </NavLink>
  );
}

function FreshnessBox() {
  const f = useApi(getFreshness).data;
  const logger = useApi(getLoggerStatus).data;
  return (
    <div className="rail-box">
      <NowMini logger={logger} />
      <span className="font-semibold">Data freshness</span>
      <span className="text-muted leading-[1.45]">{freshnessText(f, logger?.state)}</span>
      <NavLink to="/import/1" className="font-semibold">Import a newer export</NavLink>
    </div>
  );
}

function CaptureBox({ waiting }: { waiting: number }) {
  const cap = useApi(getCapture).data;
  if (!cap) return null;
  return (
    <div className="rail-box">
      <span className="font-semibold">Phone capture is {cap.on ? "on" : "off"}</span>
      <span className="text-muted leading-[1.45]">
        {waiting ? `${pl(waiting, "share")} from your iPhone ${waiting > 1 ? "are" : "is"} waiting for a name.` : cap.on ? "Nothing waiting." : "Turn it on in Settings to save from your iPhone."}
      </span>
    </div>
  );
}

/** Combined field "Search your library, or paste a Spotify link to add it" + Add (DESIGN §4). `children`: the page's range/filter control. */
export function Topbar({ children }: { children?: ReactNode }) {
  const [v, setV] = useState("");
  const nav = useNavigate();
  const add = useQuickAdd();
  const b = fieldButton(v);
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (b.mode === "add") { add({ link: v.trim() }); setV(""); } else nav(`/search/${encodeURIComponent(v.trim())}`);
  };
  return (
    <div className="topbar">
      <form role="search" onSubmit={submit}>
        <label className="sr-only" htmlFor="top-field">Search your library, or paste a Spotify link to add it</label>
        <input id="top-field" className="input surface flex-auto" autoComplete="off" value={v} onChange={(e) => setV(e.target.value)}
          placeholder="Search your library, or paste a Spotify link to add it" />
        <button className="btn btn-primary min-w-[92px]" type="submit">{b.label}</button>
      </form>
      {children}
    </div>
  );
}

export default function Shell() {
  const loc = useLocation();
  const online = useOnline(true); // banner only once offline is known
  const bare = loc.pathname === "/first-run"; // SC-02b has no rail
  const [qa, setQa] = useState<QuickAddOpts | null>(null);
  const rail = useRailMini();
  const open = useCallback((o: QuickAddOpts = {}) => setQa(o), []);
  useEffect(invalidateOnDataEvents, []);
  useEffect(() => onEvent("open-quick-add", () => setQa({})), []); // tray "Quick add" (G-35): Rust shows the window
  useEffect(() => {
    recordPageVisit(loc.pathname.split("/")[1] || "home").catch(() => {});
    window.scrollTo(0, 0);
    setQa(null);
  }, [loc.pathname]);
  const onKey = useCallback((a: ShortcutAction, e: KeyboardEvent) => {
    if (a === "quick-add") { e.preventDefault(); setQa({}); }
    if (a === "search") { e.preventDefault(); document.getElementById("top-field")?.focus(); }
    if (a === "toggle-rail" && !rail.narrow) { e.preventDefault(); rail.toggle(); }
  }, [rail.narrow, rail.toggle]);
  useShortcuts(onKey);
  return (
    <QuickAddCtx.Provider value={open}>
      <div className={bare ? "app" : "app has-rail"}>
        {!bare && <Rail {...rail} />}
        <main className="main" id="main">
          {!online && <div className="banner warn" role="status"><span><b>You’re offline.</b> Everything you saved still works. Artwork and short links come back when you reconnect.</span></div>}
          <Outlet />
        </main>
      </div>
      {qa && <QuickAdd opts={qa} onClose={() => setQa(null)} />}
      <Suspense><DevSwitcher /></Suspense>
    </QuickAddCtx.Provider>
  );
}

// App shell (DESIGN §4): rail with nav + mono counts + freshness box, main column, offline banner,
// global quick-add dialog and shortcuts. Pages render their own <Topbar>.
import { lazy, Suspense, useCallback, useEffect, useState, type FormEvent, type ReactNode } from "react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import {
  ArrowsClockwiseIcon, BooksIcon, ChartBarIcon, ClockCounterClockwiseIcon, CompassIcon, GearSixIcon, HouseIcon, PlaylistIcon, QueueIcon,
} from "@phosphor-icons/react";
import { getCapture, getCounts, getFreshness, getLoggerStatus, MOCK, recordPageVisit } from "../api";
import { useApi, useOnline } from "../hooks/useApi";
import { useRelease } from "../hooks/useRelease";
import { useShortcuts, type ShortcutAction } from "../hooks/useShortcuts";
import { fieldButton, freshnessText } from "./logic";
import { QuickAddCtx, useQuickAdd, type QuickAddOpts } from "./QuickAddContext";
import QuickAdd from "./QuickAdd";
import { pl } from "../lib/format";

// Dev builds only: a shipped build drops the switcher. The mocks themselves go only when VITE_MOCK=0 (api.ts).
const DevSwitcher = import.meta.env.DEV && MOCK ? lazy(() => import("./DevSwitcher")) : () => null;

const NAV: { to: string; label: string; icon: ReactNode; count?: "backlog" | "library" | "revisit"; rel?: "r2" | "r3" }[] = [
  { to: "/home", label: "Home", icon: <HouseIcon size={20} /> },
  { to: "/plays", label: "Recent plays", icon: <ClockCounterClockwiseIcon size={20} />, rel: "r2" },
  { to: "/insights", label: "Insights", icon: <ChartBarIcon size={20} />, rel: "r2" },
  { to: "/backlog", label: "Backlog", icon: <QueueIcon size={20} />, count: "backlog" },
  { to: "/library", label: "Library", icon: <BooksIcon size={20} />, count: "library" },
  { to: "/rediscover", label: "Rediscover", icon: <ArrowsClockwiseIcon size={20} />, count: "revisit" },
  { to: "/playlists", label: "Playlists", icon: <PlaylistIcon size={20} />, rel: "r2" },
  { to: "/spotify", label: "From Spotify", icon: <CompassIcon size={20} />, rel: "r3" },
  { to: "/settings", label: "Settings", icon: <GearSixIcon size={20} /> },
];

function Rail() {
  const { r2, r3 } = useRelease();
  const counts = useApi(getCounts).data;
  return (
    <aside className="rail">
      <NavLink className="logo" to="/home">Runout</NavLink>
      <nav className="nav" aria-label="Main">
        {NAV.filter((n) => !n.rel || (n.rel === "r2" ? r2 : r3)).map((n) => (
          <NavLink key={n.to} to={n.to} className="nav-item" aria-label={n.label}>
            <span className="nav-label"><span className="nav-icon" aria-hidden>{n.icon}</span><span className="nav-text">{n.label}</span></span>
            <span className="count">{n.count && counts ? counts[n.count] : ""}</span>
          </NavLink>
        ))}
      </nav>
      {r2 ? <FreshnessBox /> : <CaptureBox waiting={counts?.name_needed ?? 0} />}
    </aside>
  );
}

function FreshnessBox() {
  const f = useApi(getFreshness).data;
  const logger = useApi(getLoggerStatus).data;
  return (
    <div className="rail-box">
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
  const online = useOnline();
  const bare = loc.pathname === "/first-run"; // SC-02b has no rail
  const [qa, setQa] = useState<QuickAddOpts | null>(null);
  const open = useCallback((o: QuickAddOpts = {}) => setQa(o), []);
  useEffect(() => {
    recordPageVisit(loc.pathname.split("/")[1] || "home").catch(() => {});
    window.scrollTo(0, 0);
    setQa(null);
  }, [loc.pathname]);
  const onKey = useCallback((a: ShortcutAction, e: KeyboardEvent) => {
    if (a === "quick-add") { e.preventDefault(); setQa({}); }
    if (a === "search") { e.preventDefault(); document.getElementById("top-field")?.focus(); }
  }, []);
  useShortcuts(onKey);
  return (
    <QuickAddCtx.Provider value={open}>
      <div className={bare ? "app" : "app has-rail"}>
        {!bare && <Rail />}
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

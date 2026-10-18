// Dev-only state switcher (dev builds only): forces any screen into loading | empty | offline | error,
// picks the release, and simulates an iPhone share. Mirrors the prototype controls; not product UI.
import { useSyncExternalStore } from "react";
import { Link } from "react-router-dom";
import { dev, onDevChange, setDev, type Release, type View } from "../mocks/devState";
import { simulateCapture } from "../mocks";
import { useQuickAdd } from "./QuickAddContext";

const A = "3QJzdZJYIAcoET1GcfpNGi";
const SCREENS: [string, string][] = [["/home", "Home"], ["/first-run", "First run"], ["/backlog", "Backlog"], ["/library", "Library"], ["/search/coltrane", "Search"],
  ["/item/19", "Item (album)"], ["/item/1", "Item (artist)"], ["/rediscover", "Rediscover"], ["/plays", "Recent plays"], ["/insights", "Insights"],
  ["/import/1", "Import"], ["/playlists", "Playlists"], ["/spotify", "From Spotify"], ["/settings", "Settings"], ["/system", "Design system"]];

export default function DevSwitcher() {
  const s = useSyncExternalStore(onDevChange, () => `${dev.view}|${dev.release}`);
  const [view, release] = s.split("|") as [View, Release];
  const add = useQuickAdd();
  const links: [string, string][] = [["new album", `https://open.spotify.com/album/${A}`], ["episode", `https://open.spotify.com/episode/${A}`],
    ["short link", "https://spotify.link/aXk3P9q"], ["artist", "https://open.spotify.com/artist/7mEBp3V1D8Xz2Rk4Qc9TnY"]];
  return (
    <details className="proto">
      <summary>Dev state</summary>
      <div className="in">
        <label>Release <select value={release} onChange={(e) => setDev({ release: e.target.value as Release })}>
          <option value="mvp">MVP</option><option value="r2">Release 2</option><option value="r3">Release 3</option></select></label>
        <label>State <select value={view} onChange={(e) => setDev({ view: e.target.value as View })}>
          {["normal", "empty", "loading", "offline", "error"].map((v) => <option key={v} value={v}>{v}</option>)}</select></label>
        <span>Quick add with:</span>
        <div className="grid">{links.map(([t, l]) => <a key={t} href="#" onClick={(e) => { e.preventDefault(); add({ link: l }); }}>{t}</a>)}
          <a href="#" onClick={(e) => { e.preventDefault(); simulateCapture(); }}>iPhone share</a></div>
        <span>Screens:</span>
        <div className="grid">{SCREENS.map(([h, t]) => <Link key={h} to={h}>{t}</Link>)}</div>
      </div>
    </details>
  );
}

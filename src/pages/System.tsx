// #/system: the design system rendered with the real components (DESIGN §5); side-by-side review target for M2.
import { useState } from "react";
import { Link } from "react-router-dom";
import type { Status } from "../api";
import { Attribution, Dots, Empty, EmbedSlot, LiveDot, NameNeeded, Rating, SkelRows, Stat, StatusBadge, StatusControl, TagChips, useToast } from "../components/ui";

const SW: [string, string, string][] = [["--bg", "#101113", "Page canvas"], ["--rail", "#0B0C0E", "Sidebar"], ["--surface", "#17191C", "Cards, inputs"], ["--surface-2", "#1F2226", "Hover, selected"], ["--line", "#23262B", "Dividers"],
  ["--line-strong", "#3A3F46", "Control borders"], ["--text", "#ECEDEF", "Primary text, 16.1:1"], ["--text-2", "#C3C7CE", "Secondary, 11.1:1"], ["--muted", "#B6BBC3", "Labels, 9.8:1"], ["--faint", "#9A9FA8", "Ranks, times, 7.1:1"],
  ["--accent", "#F5B544", "Needs you, 10.4:1"], ["--link", "#F5C46B", "Links, 11.7:1"], ["--bar", "#4A5059", "Chart bars"], ["--danger", "#FF7A6B", "Destructive, 7.4:1"], ["--accent-ink", "#101113", "Text on accent, 10.4:1"]];
const STATES = ["Default", "Hover", "Pressed", "Focus", "Disabled", "Loading"];
// Static pictures of each interaction state (the live row below has the real ones).
const STATE_CLASS: Record<string, string> = {
  Hover: "brightness-110", Pressed: "scale-[0.98] brightness-[0.92]", Focus: "outline-2 outline-accent outline-offset-2", Disabled: "opacity-40",
};
const TYPE: [string, string, string][] = [["32 / 800", "text-3xl font-extrabold tracking-[-0.025em]", "Promises"], ["26 / 800", "text-2xl font-extrabold", "Backlog"],
  ["20 / 800", "text-xl font-extrabold", "Top artists"], ["15 / 600", "text-md font-semibold", "Journey in Satchidananda"], ["14 / 400", "text-text-2", "Recommended by Meera."],
  ["12 / 400", "text-xs text-muted", "Desktop plays: live"], ["Mono 26 / 600", "font-mono text-2xl font-semibold", "6,519 plays"]];
const KEYS: [string, string][] = [["Quick add from anywhere", "Ctrl+N"], ["Search", "Ctrl+K"], ["Set status on an item page", "1 to 5"], ["Rating, by half a star", "Arrow keys"], ["Close a dialog", "Esc"]];

const col = "flex flex-col gap-3";
const rowFlex = "flex flex-wrap gap-2 items-center";
const cols = "flex flex-wrap gap-7 items-start";
const colMain = "flex-[1.5_1_520px] min-w-0 flex flex-col gap-3.5";
const colSide = "flex-[1_1_320px] min-w-0 flex flex-col gap-5";
const playRow = "row grid-cols-[minmax(0,1fr)_auto_50px]";

export default function System() {
  const [status, setStatus] = useState<Status>("in_progress");
  const [rating, setRating] = useState<number | null>(3.5);
  const [tags, setTags] = useState(["late-night", "jazz"]);
  const toast = useToast();
  const btn = (cls: string, label: string) => STATES.map((s) => <span key={s} className={`btn ${cls} ${STATE_CLASS[s] ?? ""}`}>{s === "Loading" ? <Dots /> : label}</span>);
  return (
    <div className="app">
      <main className="main mx-auto max-w-[1240px] gap-11 pt-10">
        <header className="flex flex-col gap-2">
          <Link to="/home" className="text-sm font-semibold">Back to Runout</Link>
          <h1 className="text-[40px] font-extrabold tracking-[-0.03em]">Runout design system</h1>
          <p className="text-text-2 m-0 text-[16px] max-w-[720px]">Direction D · Pulse, v1.1. Graphite layers, one amber accent, mono numbers. Everything on this page is live: hover, press and tab through it.</p>
        </header>
        <section className={col}><h2 className="section-title">Color</h2>
          <div className="tiles grid-cols-[repeat(5,minmax(0,1fr))]">{SW.map(([n, h, u]) => (
            <div key={n} className="card flex flex-col p-2.5 gap-1.5">
              <div className="h-14 rounded-control border border-[rgba(255,255,255,0.08)]" style={{ background: h }} />
              <span className="font-mono text-xs font-semibold">{n}</span><span className="font-mono text-muted text-xs">{h}</span><span className="text-text-2 text-xs">{u}</span>
            </div>))}
          </div>
        </section>
        <div className={cols}>
          <section className={colMain}><h2 className="section-title">Type</h2>
            <div className={`card ${col}`}>{TYPE.map(([s, cls, t]) => (
              <div key={s} className="grid grid-cols-[120px_minmax(0,1fr)] gap-4 items-baseline"><span className="font-mono text-faint text-xs">{s}</span><span className={cls}>{t}</span></div>))}
            </div>
          </section>
          <section className={colSide}><h2 className="section-title">Depth and shape</h2>
            <div className="tiles grid-cols-[repeat(3,minmax(0,1fr))]">
              <div className="h-[110px] rounded-card border border-dashed border-line-2 p-3 flex items-end"><b>Level 0</b></div>
              <div className="card hover h-[110px] flex items-end"><b>Level 1</b></div>
              <div className="overlay h-[110px] p-3 flex items-end"><b>Level 2</b></div>
            </div>
            <div className={`card ${col}`}><span className="label">Keyboard</span>{KEYS.map(([t, k]) => <span key={k} className="flex justify-between items-baseline gap-3"><span>{t}</span><span className="kbd">{k}</span></span>)}</div>
          </section>
        </div>
        <section className={col}><h2 className="section-title">Buttons</h2>
          <div className="card grid grid-cols-[110px_repeat(6,minmax(0,1fr))] gap-x-2.5 gap-y-3.5 items-center">
            <span />{STATES.map((s) => <span key={s} className="text-muted text-xs font-semibold">{s}</span>)}
            <span className="text-text-2">Primary</span>{btn("btn-primary", "Save")}<span className="text-text-2">Secondary</span>{btn("btn-secondary", "Add a note")}
            <span className="text-text-2">Light</span>{btn("btn-light", "Rate it")}<span className="text-text-2">Danger</span>{btn("btn-danger", "Delete")}
          </div>
          <div className={`card ${rowFlex}`}><span className="text-muted w-[110px]">Live</span>
            <button className="btn btn-primary" onClick={() => toast("Thembi saved to your backlog", () => {})}>Save</button>
            <button className="btn btn-secondary">Add a note</button><button className="btn btn-light">Rate it</button><button className="btn btn-danger">Delete</button>
            <button className="btn btn-ghost">Open Spotify</button><button className="btn btn-primary" disabled>Disabled</button>
          </div>
        </section>
        <div className={cols}>
          <section className={colMain}><h2 className="section-title">Inputs</h2>
            <div className={`card ${col}`}>
              <div className="field"><label className="label" htmlFor="ds-1">Live field</label><input id="ds-1" className="input" placeholder="Search your library, or paste a Spotify link" /></div>
              <div className="field"><label className="label" htmlFor="ds-2">Error</label><input id="ds-2" className="input error" defaultValue="open.spotify.com/episode/4rOoJ6Egrf8K2IrywzwOMk" aria-describedby="ds-2h" /><span id="ds-2h" className="help err">Podcast episodes can’t be saved.</span></div>
              <div className="field opacity-40"><label className="label" htmlFor="ds-3">Disabled</label><input id="ds-3" className="input" disabled defaultValue="Connect Spotify to use autofill" /></div>
            </div>
          </section>
          <section className={colSide}><h2 className="section-title">Selection</h2>
            <div className={`card ${col}`}>
              <span className="label">Status</span><StatusControl value={status} onChange={setStatus} />
              <span className="label">Rating</span><Rating value={rating} onChange={setRating} label="Example rating" />
              <span className="label">Tags and badges</span>
              <TagChips tags={tags} onRemove={(t) => setTags(tags.filter((x) => x !== t))} onAdd={(t) => setTags([...tags, t])} />
              <div className={rowFlex}><span className="chip on">jazz (filter on)</span><NameNeeded /><StatusBadge>Revisit</StatusBadge><span className={`${rowFlex} text-accent text-sm font-semibold`}><LiveDot />Playing now</span></div>
            </div>
          </section>
        </div>
        <section className={col}><h2 className="section-title">Data and feedback</h2>
          <div className={cols}>
            <div className={`${colMain} flex-[1_1_320px]`}>
              <div className="stats"><Stat k="Listening time" v="27h 40m" d="3h 10m more" /><Stat k="Unrated plays" v="86" d="Rate what you keep coming back to" act /></div>
              <div className="card pad-s"><div className="list">
                <div className={playRow}><span className="flex flex-col gap-0.5"><b>Movement 6</b><span className="text-muted text-xs">Floating Points</span></span><span className="stars">★★★★★</span><span className="font-mono text-faint text-xs">18 min</span></div>
                <div className={playRow}><span className="flex flex-col gap-0.5"><b>Teardrop</b><span className="text-muted text-xs">Massive Attack</span></span><button className="btn btn-secondary s">Rate</button><span className="font-mono text-faint text-xs">3 h</span></div>
                <div className={playRow}><span className="flex flex-col gap-0.5"><b className="text-faint">Glow</b><span className="text-muted text-xs">Kelly Lee Owens</span></span><span className="text-faint text-xs">Skipped, not counted</span><span className="font-mono text-faint text-xs">4 h</span></div>
              </div></div>
            </div>
            <div className={colSide}>
              <div className="flex flex-wrap gap-2 items-stretch">
                <div className="flex-[1_1_140px]"><EmbedSlot uri={null} /></div>
                <div className="flex-[1_1_140px]"><div className="embed off h-[152px]"><b className="text-text-2">Offline</b>Artwork returns when you reconnect</div></div>
              </div>
              <div className={`card ${rowFlex}`}><Attribution><span className="text-text-2 text-sm">Attribution next to Spotify content.</span></Attribution></div>
              <Empty h="Nothing waiting" line="Paste a Spotify link, or share one from your phone."><button className="btn btn-primary">Add a link</button></Empty>
              <SkelRows n={3} />
            </div>
          </div>
        </section>
      </main>
    </div>
  );
}

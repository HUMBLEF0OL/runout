// SC-09 Insights v3, Explore tab (M10 WI 8), per docs/design/M10-insights-v3.md §5 and mockup PNG 7. A builder (up to 3
// artists, albums or tracks; from–to; grain; measure; only some hours, weekdays, and, export plays only, devices and
// shuffle), a line chart you can scrub by pointer or arrow keys, the same numbers as a table, ready-made questions that
// fill the builder (no free text: that needs an AI model, design §2), saved views (stored like saved filters) and Copy as
// CSV. The CSV holds plays and hours only, never fingerprint scores (design §2 anti-goals). Sources U, S, E (FR-031).
import { useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import { m, useReducedMotion } from "motion/react";
import { XIcon } from "@phosphor-icons/react";
import * as api from "../api";
import type { Coverage, CoverageGap, Covered, DeviceClass, ExploreQuery, ExploreResult } from "../api";
import { Empty, ErrorBanner, InsightCard as Card, Seg, SourcedTag, useToast } from "../components/ui";
import { useApi } from "../hooks/useApi";
import { day, dayY, hrs, monthName, num, pl } from "../lib/format";
import { EASE, RISE } from "../lib/motion";

type Kind = ExploreQuery["series"][number]["kind"];
type Grain = ExploreQuery["grain"];
type Measure = ExploreQuery["measure"];
const MAX_SERIES = 3;
const KINDS: [Kind, string][] = [["artist", "Artist"], ["album", "Album"], ["track", "Track"]];
const GRAINS: [Grain, string][] = [["week", "Week"], ["month", "Month"], ["year", "Year"]];
const MEASURES: [Measure, string][] = [["hours", "Hours"], ["share", "Share"], ["plays", "Plays"]];
const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const DAY_NAMES = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
/** Export plays carry a device; desktop plays ("This PC") are left out once a device or shuffle filter is set (API.md §4.1). */
const DEVICES: [DeviceClass, string][] = [["phone", "Phone"], ["computer", "Computer"], ["web", "Web player"], ["other", "Other"]];
const hh = (h: number) => `${String(h).padStart(2, "0")}:00`;

/* ------------------------------------------------------------------ pure helpers (tested) */
/** Hours from `from` up to, not including, `to`, wrapping past midnight: 22 → 2 = 22, 23, 0, 1. */
export const hourSpan = (from: number, to: number) => Array.from({ length: (to - from + 24) % 24 }, (_, i) => (from + i) % 24);

const seriesName = (s: ExploreResult["series"][number]) => (s.by && s.kind !== "artist" ? `${s.name} by ${s.by}` : s.name);
/** One CSV cell: quoted when it holds a comma, quote or line break; a leading = + - @ is defused so a spreadsheet never runs it. */
const cell = (v: string | number) => {
  let s = String(v);
  if (typeof v === "string" && /^[=+\-@\t\r]/.test(s)) s = "'" + s;
  return /[",\r\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
};
/** Copy as CSV: one row per period, hours then plays for each series. Plays and hours only, never fingerprint scores.
 *  A bucket with no data (`gaps`, from bucketGaps) gets empty cells: unknown, never 0 (design §4.4). */
export function exploreCsv(hours: ExploreResult, plays: ExploreResult, gaps: BucketGap[] = []): string {
  const head = ["period", ...hours.series.flatMap((s) => [`${seriesName(s)} hours`, `${seriesName(s)} plays`])];
  const rows = hours.buckets.map((b, i): (string | number)[] => [b, ...hours.series.flatMap<string | number>((s, k) => gaps[i] === "all" ? ["", ""] : [s.values[i] ?? 0, plays.series[k]?.values[i] ?? 0])]);
  return [head, ...rows].map((r) => r.map(cell).join(",")).join("\r\n") + "\r\n";
}

/** Per bucket: "all" when coverage gaps cover every day of it (clipped to from–to), "part" when they cover some, else
 *  null. An "all" bucket is unknown: no point, no line through it, an empty cell (design §4.4, API.md §4.1). */
export type BucketGap = "all" | "part" | null;
const plus = (d: string, n: number) => new Date(Date.parse(d + "T00:00Z") + n * 86_400_000).toISOString().slice(0, 10);
export function bucketGaps(buckets: string[], grain: Grain, gaps: CoverageGap[], from: string, to: string): BucketGap[] {
  const sorted = [...gaps].sort((a, b) => (a.from < b.from ? -1 : 1));
  return buckets.map((b) => {
    const s = grain === "year" ? b + "-01-01" : grain === "month" ? b + "-01" : b;
    const e = grain === "year" ? b + "-12-31" : grain === "month" ? plus(plus(b + "-01", 31).slice(0, 7) + "-01", -1) : plus(b, 6);
    const lo = s > from ? s : from, hi = e < to ? e : to;
    let cur = lo;
    for (const g of sorted) if (g.from <= cur && g.to >= cur) cur = plus(g.to, 1);
    return cur > hi ? "all" : sorted.some((g) => g.from <= hi && g.to >= lo) ? "part" : null;
  });
}
const gapsOf = (r: ExploreResult, c: Coverage, q: ExploreQuery) => bucketGaps(r.buckets, q.grain, c.gaps, q.from, q.to);

/** Ready-made questions (design §5): each fills the builder for one artist; there's no free-text box. */
export function questions(artist: string, since: string, today: string): { label: string; query: ExploreQuery }[] {
  const base: ExploreQuery = { series: [{ kind: "artist", name: artist }], from: since, to: today, grain: "month", measure: "hours" };
  const yearsAgo = (n: number) => `${+today.slice(0, 4) - n}${today.slice(4)}`;
  const recent = yearsAgo(3) > since ? yearsAgo(3) : since;
  return [
    { label: `When did I start and stop listening to ${artist}?`, query: base },
    { label: `How big a share of my listening was ${artist}, year by year?`, query: { ...base, grain: "year", measure: "share" } },
    { label: `Do I play ${artist} late at night?`, query: { ...base, from: recent, hours: hourSpan(22, 2) } },
    { label: `Is ${artist} a weekend thing?`, query: { ...base, from: recent, weekdays: [6, 7] } },
    { label: `Do I play ${artist} on my phone?`, query: { ...base, from: recent, devices: ["phone"] } },
    { label: `Does ${artist} come up on shuffle?`, query: { ...base, from: recent, grain: "year", measure: "plays", shuffle: true } },
  ];
}

const bucketLabel = (b: string, g: Grain) => (g === "year" ? b : g === "month" ? `${monthName(b)} ${b.slice(0, 4)}` : `Week of ${dayY(b + "T00:00")}`);
const fmt = (v: number, measure: Measure) => (measure === "hours" ? hrs(v) : measure === "share" ? `${v}%` : num(v));

/* ------------------------------------------------------------------ the tab */
export function Explore({ today }: { today: string }) {
  const rec = useApi(api.getRecords);
  if (rec.loading) return <Skel />;
  if (rec.error) return <ErrorBanner error={rec.error} title="Couldn’t load Explore." onRetry={rec.reload} />;
  const d = rec.data!.data;
  if (!d.plays_total || !d.since) return (
    <Empty h="No listening history yet" line="Import your Spotify history to see this.">
      <Link className="btn btn-primary" to="/import/1">Import history</Link>
    </Empty>);
  const since = d.since.slice(0, 10);
  return <Builder since={since} today={today} first={d.top_artist?.name ?? null} />;
}

function Builder({ since, today, first }: { since: string; today: string; first: string | null }) {
  const reduce = useReducedMotion();
  const toast = useToast();
  const [q, setQ] = useState<ExploreQuery>(() => ({ series: first ? [{ kind: "artist", name: first }] : [], from: since, to: today, grain: "month", measure: "hours" }));
  const set = (p: Partial<ExploreQuery>) => setQ((x) => ({ ...x, ...p }));
  const bad = !q.from || !q.to || q.from > q.to;
  const key = JSON.stringify(q);
  // Resolves to the result together with the query it was fetched for: while a new query loads, the old result stays on
  // screen and must be drawn with its own grain and dates (a week bucket read as a month throws), never with the new q.
  const res = useApi(() => (q.series.length && !bad ? api.explore(q).then((r) => ({ q, r })) : Promise.resolve(null)), [key]);
  const shown = res.data;
  const views = useApi(api.listExploreViews);
  const [naming, setNaming] = useState<string | null>(null);
  const [copying, setCopying] = useState(false);
  const qs = questions(q.series[0]?.kind === "artist" ? q.series[0].name : first ?? "your top artist", since, today);
  const fill = (n: ExploreQuery) => { setQ(n); document.getElementById("ex-h")?.scrollIntoView?.({ block: "start", behavior: reduce ? "auto" : "smooth" }); };

  const save = async (e: FormEvent) => {
    e.preventDefault();
    const name = naming?.trim(); if (!name) return;
    try { await api.saveExploreView(name, q); setNaming(null); toast(`Saved “${name}”. It’s under Saved views.`); }
    catch (err) { toast(api.toAppError(err).message); }
  };
  const copy = async () => {
    setCopying(true);
    try {
      const [h, p] = await Promise.all([api.explore({ ...q, measure: "hours" }), api.explore({ ...q, measure: "plays" })]);
      await navigator.clipboard.writeText(exploreCsv(h.data, p.data, gapsOf(h.data, h.coverage, q)));
      toast(`Copied ${pl(h.data.buckets.length, "row")} as CSV: hours and plays.`);
    } catch { toast("Couldn’t copy. Try again."); } finally { setCopying(false); }
  };

  return (
    <m.div className="flex flex-col gap-6" initial={reduce ? false : "hidden"} animate="show">
      <m.section variants={RISE} custom={0} className="card flex flex-col gap-4" aria-labelledby="ex-h">
        <div className="flex flex-col gap-0.5">
          <h2 id="ex-h" className="card-title">Explore</h2>
          <span className="text-text-2 text-sm">Choose what to look at, over which stretch, and narrow it down.</span>
        </div>
        <Show q={q} set={set} />
        <Only q={q} set={set} />
        {bad ? <p className="text-danger text-sm m-0" role="alert">Choose a start date on or before the end date.</p>
          : !q.series.length ? <p className="text-muted m-0">Add an artist, album or track to see it over time.</p>
          : res.error ? <ErrorBanner error={res.error} title="Couldn’t run that." onRetry={res.reload} />
          : !shown ? <div className="skel h-[240px]" aria-busy="true" aria-label="Loading" />
          : <Result key={JSON.stringify(shown.q)} res={shown.r} q={shown.q} />}
      </m.section>
      <div className="flex flex-wrap gap-7 items-start">
        <m.div variants={RISE} custom={1} className="flex-[1_1_380px] min-w-0">
          <Card title="Ready-made questions" res={shown?.r}>
            <ul className="in3-qs">{qs.map((x) => (
              <li key={x.label}><button type="button" className="in3-q" onClick={() => fill(x.query)}>{x.label}</button></li>))}</ul>
            <span className="text-muted text-xs">Each question fills the builder above. There’s no free-text box: it would need an AI model, and nothing leaves this PC.</span>
            <h3 className="text-sm font-bold m-0 mt-2">Saved views</h3>
            {views.error ? <span className="text-danger text-sm">Couldn’t load your saved views.</span>
              : !views.data?.length ? <span className="text-muted text-sm">None yet. Save this view keeps the builder, not the numbers.</span>
              : <ul className="in3-qs">{views.data.map((v) => (
                <li key={v.id} className="flex items-center gap-1">
                  <button type="button" className="in3-q flex-1" onClick={() => fill(v.query)}>{v.name}</button>
                  <button type="button" className="icon-btn" aria-label={`Delete saved view ${v.name}`}
                    onClick={() => api.deleteExploreView(v.id).then(() => toast(`Deleted “${v.name}”.`), (e) => toast(api.toAppError(e).message))}><XIcon size={14} /></button>
                </li>))}</ul>}
          </Card>
        </m.div>
        <m.div variants={RISE} custom={2} className="flex-[1.4_1_460px] min-w-0">
          <Card title="Result as a table" res={shown?.r}>
            {shown ? <Table res={shown.r} q={shown.q} /> : <span className="text-muted">The table fills in with the chart.</span>}
            {naming != null ? (
              <form className="flex flex-wrap gap-2 items-center" onSubmit={save}>
                <input className="input" aria-label="Name for this view" value={naming} maxLength={100} autoFocus onChange={(e) => setNaming(e.target.value)}
                  onKeyDown={(e) => e.key === "Escape" && setNaming(null)} />
                <button className="btn btn-primary s" disabled={!naming.trim()}>Save</button>
                <button type="button" className="btn btn-secondary s" onClick={() => setNaming(null)}>Cancel</button>
              </form>
            ) : (
              <div className="flex flex-wrap gap-2 items-center">
                <button type="button" className="btn btn-secondary s" disabled={!q.series.length || bad} onClick={() => setNaming(q.series.map((s) => s.name).join(" and "))}>Save this view</button>
                <button type="button" className="btn btn-secondary s" disabled={!shown || copying} onClick={copy}>Copy as CSV</button>
              </div>)}
            <span className="text-muted text-xs">Copy as CSV copies hours and plays per {q.grain}, for a spreadsheet.</span>
          </Card>
        </m.div>
      </div>
    </m.div>
  );
}

/* ------------------------------------------------------------------ builder rows */
function Show({ q, set }: { q: ExploreQuery; set: (p: Partial<ExploreQuery>) => void }) {
  const [adding, setAdding] = useState(false);
  const [kind, setKind] = useState<Kind>("artist");
  const [name, setName] = useState(""), [by, setBy] = useState("");
  const add = (e: FormEvent) => {
    e.preventDefault();
    const n = name.trim(); if (!n) return;
    if (q.series.some((s) => s.kind === kind && s.name.toLowerCase() === n.toLowerCase())) { setAdding(false); return; }
    set({ series: [...q.series, { kind, name: n, ...(kind !== "artist" && by.trim() && { by: by.trim() }) }] });
    setName(""); setBy(""); setAdding(false);
  };
  return (
    <div className="in3-qb">
      <span className="lbl">Show</span>
      {q.series.map((s, i) => (
        <span key={`${s.kind}:${s.name}`} className="chip">
          <i className={`in3-sw s${i}`} aria-hidden />{s.name}{s.by ? <span className="text-muted">by {s.by}</span> : null}
          <button type="button" className="in3-x" aria-label={`Remove ${s.name}`} onClick={() => set({ series: q.series.filter((_, k) => k !== i) })}><XIcon size={12} /></button>
        </span>))}
      {adding ? (
        <form className="flex flex-wrap gap-2 items-center" onSubmit={add}>
          <select className="input h-9" aria-label="What to add" value={kind} onChange={(e) => setKind(e.target.value as Kind)}>
            {KINDS.map(([k, l]) => <option key={k} value={k}>{l}</option>)}</select>
          <input className="input h-9" aria-label={`${KINDS.find((k) => k[0] === kind)![1]} name`} placeholder="Name, as Spotify shows it" value={name} maxLength={200} autoFocus
            onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setAdding(false)} />
          {kind !== "artist" && <input className="input h-9" aria-label="By (artist, optional)" placeholder="By (optional)" value={by} maxLength={200} onChange={(e) => setBy(e.target.value)} />}
          <button className="btn btn-primary s" disabled={!name.trim()}>Add</button>
          <button type="button" className="btn btn-secondary s" onClick={() => setAdding(false)}>Cancel</button>
        </form>
      ) : q.series.length < MAX_SERIES && <button type="button" className="chip add" onClick={() => setAdding(true)}>{q.series.length ? "+ Compare" : "+ Add"}</button>}
      {/* Each label stays with its control when the row wraps. */}
      <span className="inline-flex flex-wrap items-center gap-2">
        <span className="lbl">from</span>
        <input type="date" className="input h-9 mono" aria-label="From" value={q.from} max={q.to || undefined} onChange={(e) => set({ from: e.target.value })} />
        <span className="lbl">to</span>
        <input type="date" className="input h-9 mono" aria-label="To" value={q.to} min={q.from || undefined} onChange={(e) => set({ to: e.target.value })} />
      </span>
      <span className="inline-flex items-center gap-2"><span className="lbl">by</span>
        <Seg radio label="Group by" value={q.grain} onChange={(g) => set({ grain: g })} options={GRAINS} /></span>
      <span className="inline-flex items-center gap-2"><span className="lbl">as</span>
        <Seg radio label="Measure" value={q.measure} onChange={(v) => set({ measure: v })} options={MEASURES} /></span>
    </div>
  );
}

function Only({ q, set }: { q: ExploreQuery; set: (p: Partial<ExploreQuery>) => void }) {
  const hours = q.hours ?? [];
  const from = hours.length ? hours[0] : null, to = hours.length ? (hours[hours.length - 1] + 1) % 24 : null;
  // A start equal to the old end keeps one hour, never an empty list (which would match nothing).
  const setHours = (f: number | null, t: number | null) => set({ hours: f == null ? undefined : t == null || t === f ? [f] : hourSpan(f, t) });
  const wd = q.weekdays ?? [];
  const toggleDay = (n: number) => { const w = wd.includes(n) ? wd.filter((x) => x !== n) : [...wd, n].sort(); set({ weekdays: w.length && w.length < 7 ? w : undefined }); };
  const dv = q.devices ?? [];
  const toggleDev = (k: DeviceClass) => { const v = dv.includes(k) ? dv.filter((x) => x !== k) : [...dv, k]; set({ devices: v.length ? v : undefined }); };
  const shuffle = q.shuffle == null ? "any" : q.shuffle ? "on" : "off";
  return (
    <div className="in3-qb">
      <span className="lbl">Only</span>
      <select className="input h-9 mono" aria-label="Hours of day, from" value={from ?? ""} onChange={(e) => setHours(e.target.value === "" ? null : +e.target.value, to)}>
        <option value="">Any hour</option>{Array.from({ length: 24 }, (_, h) => <option key={h} value={h}>{hh(h)}</option>)}</select>
      {from != null && <>
        <span className="lbl">to</span>
        <select className="input h-9 mono" aria-label="Hours of day, until" value={to ?? ""} onChange={(e) => setHours(from, +e.target.value)}>
          {Array.from({ length: 24 }, (_, h) => h).filter((h) => h !== from).map((h) => <option key={h} value={h}>{hh(h)}</option>)}</select>
      </>}
      <span className="in3-days" role="group" aria-label="Weekdays (none chosen = every day)">
        {DAYS.map((l, i) => <button key={l} type="button" className={`chip${wd.includes(i + 1) ? " on" : ""}`} aria-pressed={wd.includes(i + 1)} aria-label={DAY_NAMES[i]} onClick={() => toggleDay(i + 1)}>{l}</button>)}
      </span>
      <span className="inline-flex items-center gap-2"><span className="lbl">device</span>
        <span className="in3-days" role="group" aria-label="Devices, export plays only">
          {DEVICES.map(([k, l]) => <button key={k} type="button" className={`chip${dv.includes(k) ? " on" : ""}`} aria-pressed={dv.includes(k)} onClick={() => toggleDev(k)}>{l}</button>)}
        </span></span>
      <span className="inline-flex items-center gap-2"><span className="lbl">shuffle</span>
      <Seg radio label="Shuffle, export plays only" value={shuffle} onChange={(v) => set({ shuffle: v === "any" ? undefined : v === "on" })}
        options={[["any", "Any"], ["on", "On"], ["off", "Off"]]} /></span>
      {(dv.length > 0 || q.shuffle != null) && <span className="text-muted text-xs basis-full">Device and shuffle come from your Spotify export, so plays on this PC are left out.</span>}
    </div>
  );
}

/* ------------------------------------------------------------------ the line chart */
function Result({ res, q }: { res: Covered<ExploreResult>; q: ExploreQuery }) {
  const reduce = useReducedMotion();
  const r = res.data, n = r.buckets.length;
  const [pick, setI] = useState<number | null>(null);
  const i = pick == null ? null : Math.min(pick, n - 1); // never past the last bucket
  const gap = gapsOf(r, res.coverage, q);
  const max = Math.max(1e-9, ...r.series.flatMap((s) => s.values.filter((_, j) => gap[j] !== "all")));
  const x = (k: number) => (n < 2 ? 500 : (k / (n - 1)) * 1000);
  const y = (v: number) => 196 - (v / max) * 184;
  const at = (k: number) => `${bucketLabel(r.buckets[k], q.grain)}: ${gap[k] === "all" ? "no data"
    : r.series.map((s) => `${seriesName(s)} ${fmt(s.values[k] ?? 0, q.measure)}`).join(", ") + (gap[k] ? " (some days have no data)" : "")}`;
  // One polyline per covered run, so no line is drawn through a bucket with no data; a lone point is a dot.
  const runs = (vs: number[]) => vs.reduce<number[][]>((a, _, j) => {
    if (gap[j] === "all") a.push([]); else a[a.length - 1].push(j);
    return a;
  }, [[]]).filter((g) => g.length).map((g) => (g.length === 1 ? [g[0], g[0]] : g));
  const half = n < 2 ? 50 : 50 / (n - 1);
  const keyMove = (e: React.KeyboardEvent) => {
    // Steps from the shown value (the latest bucket at rest), like the other insight sliders.
    const step: Record<string, number> = { ArrowLeft: -1, ArrowDown: -1, ArrowRight: 1, ArrowUp: 1, PageDown: -12, PageUp: 12 };
    const k = i ?? n - 1;
    const nx = e.key in step ? k + step[e.key] : e.key === "Home" ? 0 : e.key === "End" ? n - 1 : null;
    if (nx == null) return;
    e.preventDefault(); setI(Math.max(0, Math.min(n - 1, nx)));
  };
  const every = Math.max(1, Math.ceil(n / 8)), yearsEvery = Math.max(1, Math.ceil(n / 12 / 8));
  // Long month runs label Januaries with their year; otherwise about 8 evenly spaced labels.
  const ticks = r.buckets.map((b, k) => q.grain === "month" && n > 24 ? (b.endsWith("-01") && +b.slice(0, 4) % yearsEvery === 0 ? b.slice(0, 4) : null)
    : k % every ? null : q.grain === "year" ? b : q.grain === "month" ? monthName(b) : day(b + "T00:00"));
  const c = res.coverage;
  if (!n) return <p className="text-muted m-0">Nothing in that stretch.</p>;
  return <>
    <div className="in3-xplot" role="slider" tabIndex={0} aria-label={`${MEASURES.find((m) => m[0] === q.measure)![1]} per ${q.grain}`}
      aria-valuemin={0} aria-valuemax={n - 1} aria-valuenow={i ?? n - 1} aria-valuetext={at(i ?? n - 1)} onKeyDown={keyMove}
      onPointerMove={(e) => { const b = e.currentTarget.getBoundingClientRect(); setI(Math.max(0, Math.min(n - 1, Math.round(((e.clientX - b.left) / b.width) * (n - 1))))); }}
      onPointerLeave={() => setI(null)} onBlur={() => setI(null)}>
      {/* Revealed left to right by a clip: Motion's pathLength can't be used with non-scaling strokes on a stretched SVG. */}
      <m.svg key={`${q.grain}:${q.measure}:${r.series.length}`} viewBox="0 0 1000 200" preserveAspectRatio="none" aria-hidden
        initial={reduce ? false : { clipPath: "inset(0 100% 0 0)" }} animate={{ clipPath: "inset(0 0% 0 0)" }} transition={{ duration: 0.8, ease: EASE }}>
        <line className="base" x1={0} x2={1000} y1={198} y2={198} vectorEffect="non-scaling-stroke" />
        {r.series.flatMap((s, k) => runs(s.values).map((g) => <polyline key={`${k}:${g[0]}`} className={`s${k}`} vectorEffect="non-scaling-stroke"
          points={g.map((j) => `${x(j).toFixed(1)},${y(s.values[j] ?? 0).toFixed(1)}`).join(" ")} />))}
        {i != null && <line className="cross" x1={x(i)} x2={x(i)} y1={0} y2={200} vectorEffect="non-scaling-stroke" />}
      </m.svg>
      {gap.map((g, k) => g && <i key={k} className={`in3-xgap ${g}`} style={{ left: `${Math.max(0, x(k) / 10 - half)}%`, right: `${Math.max(0, 100 - x(k) / 10 - half)}%` }} />)}
      {i != null && gap[i] !== "all" && r.series.map((s, k) => <i key={k} className={`in3-dot s${k}`} style={{ left: `${x(i) / 10}%`, top: `${y(s.values[i] ?? 0) / 2}%` }} />)}
    </div>
    <div className="in3-xticks" aria-hidden>{ticks.map((t, k) => t && <span key={k} style={{ left: `${x(k) / 10}%`, transform: `translateX(-${x(k) / 10}%)` }}>{t}</span>)}</div>
    <p className="text-text-2 text-sm m-0 min-h-[1.45em]">{i == null ? "Point at the chart, or focus it and use the arrow keys." : at(i) + "."}</p>
    <span className="in3-key">{r.series.map((s, k) => (
      <span key={k}><i className={`in3-sw s${k}`} />{seriesName(s)}
        <span className="text-muted">{s.peak ? ` · peak ${bucketLabel(s.peak.bucket, q.grain)}, ${fmt(s.peak.value, q.measure)}` : " · no plays in this stretch"}</span></span>))}</span>
    {r.export_only && <span className="text-muted text-xs">Export plays only{c.desktop_only_since ? ": nothing after your last Spotify export" : ""}.</span>}
    {!r.export_only && c.desktop_only_since && q.to >= c.desktop_only_since && <span className="text-muted text-xs">From {dayY(c.desktop_only_since + "T00:00")}, only plays on this PC are in Runout until your next Spotify export.</span>}
    {c.gaps.length > 0 && <span className="text-muted text-xs"><i className="in3-xgap key" aria-hidden />No data for {c.gaps.map((g) => `${dayY(g.from + "T00:00")} to ${dayY(g.to + "T00:00")}`).join(", ")}: counted as unknown, never as silence.</span>}
    <span className="text-muted text-xs">{q.measure === "share" ? "Share is the part of each period’s listening time. " : ""}Artists, albums and tracks are matched by name.</span>
    <SourcedTag res={res} />
  </>;
}

function Table({ res, q }: { res: Covered<ExploreResult>; q: ExploreQuery }) {
  const r = res.data, gap = gapsOf(r, res.coverage, q);
  if (!r.buckets.length) return <span className="text-muted">Nothing in that stretch.</span>;
  return (
    <div className="in3-xtable" tabIndex={0} role="region" aria-label="Result as a table">
      <table className="in3-log">
        <thead><tr><th scope="col">{GRAINS.find((g) => g[0] === q.grain)![1]}</th>{r.series.map((s, k) => <th key={k} scope="col" className="num">{seriesName(s)}</th>)}</tr></thead>
        <tbody>{r.buckets.map((b, j) => (
          <tr key={b}><th scope="row" className="font-mono">{bucketLabel(b, q.grain)}</th>
            {r.series.map((s, k) => gap[j] === "all" ? <td key={k} className="num text-muted">No data</td>
              : <td key={k} className="num font-mono">{fmt(s.values[j] ?? 0, q.measure)}</td>)}</tr>)).reverse()}</tbody>
      </table>
    </div>
  );
}

const Skel = () => (
  <div className="flex flex-col gap-6" aria-busy="true" aria-label="Loading">
    <div className="card h-[360px]"><div className="skel h-full" /></div>
    <div className="card h-[200px]"><div className="skel h-full" /></div>
  </div>
);

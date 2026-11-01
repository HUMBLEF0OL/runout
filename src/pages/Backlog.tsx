// SC-03 Backlog: backlog order grouped by priority, filters, phone captures waiting for a name, unresolved short
// links, keyboard + drag reorder (reorder_backlog), multi-select bar with Undo, artwork toggle.
// M9 (24 Oct 2026, user: "refactor the backlog using taste + motion"): a summary of the backlog's shape (priority mix,
// age), groups with counts that show 40 rows and open more on request, rows that glide to their new place on reorder
// (Motion `layout`, loaded only on this page) and leave or arrive with a short animation. Sized for thousands of records.
import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { AnimatePresence, LazyMotion, m, useReducedMotion } from "motion/react";
import { ArrowDownIcon, ArrowUpIcon } from "@phosphor-icons/react";
import {
  discardUnresolved, listItems, listUnresolved, reorderBacklog, retryUnresolved, setStatus, updateItem,
  toAppError, type AppError, type ItemSummary, type ItemType, type Priority,
} from "../api";
import { Topbar } from "../components/Shell";
import { useQuickAdd } from "../components/QuickAddContext";
import { ArtTiles, Empty, ErrorBanner, NameNeeded, PageHead, Seg, SkelRows, StatusBadge, Switch, TabPanel, useSlide, useToast, spot } from "../components/ui";
import { useApi } from "../hooks/useApi";
import { age, ago, daysSince, nameOf, num, pl, statusMeta, sub, TYPE } from "../lib/format";
import { moveTo } from "../lib/backlog";
import { EASE, LAND, swap } from "../lib/motion";

type TypeF = "all" | ItemType;
const TYPES: [TypeF, string][] = [["all", "All"], ["album", "Albums"], ["track", "Tracks"], ["artist", "Artists"]];
const GROUPS: [string, Priority | null][] = [["High priority", "high"], ["Medium", "medium"], ["Low", "low"], ["No priority", null]];
/** Rust's `list_items` cap (library/filters.rs MAX_LIMIT): the whole backlog in one read, so reorder sees every id. */
const ALL = 10_000;
const PER_GROUP = 40; // rows a group shows before "Show N more"; keeps the DOM bounded for large backlogs
const MORE = 200;
/** Layout animation (rows gliding on reorder) is only needed here, so its code loads with this page. */
const layoutFeatures = () => import("../lib/motionFeatures").then((mod) => mod.domMax);

/** Priority mix as one bar of four parts; each part jumps to its group. A pointer shortcut, out of the Tab order: the groups
 *  follow right below, and a long backlog already has many stops per row. Screen readers can still activate them by name. */
function Mix({ counts, total }: { counts: [string, number][]; total: number }) {
  const reduce = useReducedMotion();
  return (
    <div className="bl-mix" role="group" aria-label="Backlog by priority">
      <div className="bl-mix-bar">
        {counts.map(([g, n], i) => n > 0 && (
          <m.button key={g} type="button" tabIndex={-1} className={`bl-seg p${i}`} style={{ flexGrow: n, originX: 0 }} aria-label={`${g}: ${pl(n, "record")}. Go to the group`}
            onClick={() => document.getElementById(`bl-${i}`)?.scrollIntoView({ behavior: reduce ? "auto" : "smooth", block: "start" })}
            initial={reduce ? false : { scaleX: 0, opacity: 0 }} animate={{ scaleX: 1, opacity: 1 }} transition={{ ...LAND, delay: 0.1 + i * 0.06 }} />
        ))}
      </div>
      <div className="bl-mix-key" aria-hidden>
        {counts.map(([g, n], i) => <span key={g} className={n ? undefined : "zero"}><i className={`p${i}`} />{g} <b className="font-mono">{num(n)}</b>{total ? <span className="text-faint"> {Math.round((n / total) * 100)}%</span> : null}</span>)}
      </div>
    </div>
  );
}

export default function Backlog() {
  const items = useApi(() => listItems({ in_backlog: true, sort: "backlog_order", limit: ALL }));
  const unres = useApi(listUnresolved);
  const quickAdd = useQuickAdd();
  const toast = useToast();
  const reduce = useReducedMotion();
  const [type, setType] = useState<TypeF>("all");
  const [tag, setTag] = useState("");
  const [dir, slide] = useSlide(TYPES.map((t) => t[0]), type);
  const pickType = (t: TypeF) => { slide(t); setType(t); };
  const [sort, setSort] = useState<"order" | "added">("order");
  const [art, setArt] = useState(false);
  const [sel, setSel] = useState<Set<number>>(new Set());
  const [drag, setDrag] = useState<number | null>(null);
  const [actErr, setActErr] = useState<AppError>();
  const [shown, setShown] = useState<Record<string, number>>({});
  // The row just moved stays lit for a moment, so the eye follows it to its new place (feedback).
  const [moved, setMoved] = useState<number | null>(null);
  const movedT = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(movedT.current), []);

  const run = async (p: Promise<unknown>) => { setActErr(undefined); try { await p; return true; } catch (e) { setActErr(toAppError(e)); return false; } };

  const all = items.data?.rows ?? [];
  const named = all.filter((i) => i.display_name);
  const needName = all.filter((i) => !i.display_name);
  const unresolved = unres.data ?? [];
  const tags = [...new Set(named.flatMap((i) => i.tags))].sort();
  let list = named.filter((i) => (type === "all" || i.type === type) && (!tag || i.tags.includes(tag)));
  if (sort === "added") list = [...list].sort((a, b) => b.added_at.localeCompare(a.added_at));
  const groups: [string, ItemSummary[], number][] = sort === "added" ? [["All, newest first", list, 9]]
    : GROUPS.map(([g, p], i) => [g, list.filter((it) => it.priority === p), i] as [string, ItemSummary[], number]).filter((g) => g[1].length);

  const ids = all.map((i) => i.id); // full backlog order
  const reorder = async (id: number, toId: number) => {
    if (await run(reorderBacklog(moveTo(ids, id, toId)))) {
      setMoved(id); window.clearTimeout(movedT.current); movedT.current = window.setTimeout(() => setMoved(null), 900);
    }
  };
  const start = async (it: ItemSummary) => {
    if (await run(setStatus(it.id, "in_progress"))) toast(`${nameOf(it)} is now In progress`, () => setStatus(it.id, it.status));
  };
  const toggle = (id: number) => setSel((s) => { const n = new Set(s); if (n.has(id)) n.delete(id); else n.add(id); return n; });
  const chosen = all.filter((i) => sel.has(i.id));
  const selPrio = async () => {
    const old = chosen.map((i) => [i.id, i.priority] as const);
    if (await run(Promise.all(old.map(([id]) => updateItem(id, { priority: "high" }))))) {
      setSel(new Set());
      toast(`${pl(old.length, "record")} set to high priority`, () => Promise.all(old.map(([id, p]) => updateItem(id, { priority: p }))));
    }
  };
  const selRemove = async () => {
    const gone = chosen.map((i) => i.id);
    if (await run(Promise.all(gone.map((id) => updateItem(id, { in_backlog: false }))))) {
      setSel(new Set());
      toast(`${pl(gone.length, "record")} removed from backlog`, () => Promise.all(gone.map((id) => updateItem(id, { in_backlog: true }))));
    }
  };
  const retry = async () => {
    try { const r = await retryUnresolved(); toast(r.resolved ? `${pl(r.resolved, "link")} opened` : "Still can’t open the link. Runout tries again when you’re back online."); }
    catch (e) { setActErr(toAppError(e)); }
  };

  const planned = named.filter((i) => i.status === "planned").length;
  const inProg = named.filter((i) => i.status === "in_progress").length;
  const oldest = all.map((i) => i.added_at).sort()[0];
  const stale = named.filter((i) => i.status === "planned" && daysSince(i.added_at) > 180).length;
  const isEmpty = !items.loading && !items.error && !all.length && !unresolved.length;
  const filtered = type !== "all" || !!tag;
  const truncated = !!items.data && items.data.total > all.length;

  const head = (
    <PageHead title="Backlog">
      {!isEmpty && <>
        <label className="sr-only" htmlFor="bl-tag">Tag</label>
        <select id="bl-tag" className="input surface" value={tag} onChange={(e) => setTag(e.target.value)}>
          <option value="">All tags</option>{tags.map((t) => <option key={t}>{t}</option>)}
        </select>
        <label className="sr-only" htmlFor="bl-sort">Sort</label>
        <select id="bl-sort" className="input surface" value={sort} onChange={(e) => setSort(e.target.value as "order" | "added")}>
          <option value="order">Priority</option><option value="added">Date added</option>
        </select>
        <Switch label="Artwork" on={art} onChange={async (v) => { setArt(v); return true; }} />
      </>}
    </PageHead>
  );

  const tabs = <Seg<TypeF> label="Type" options={TYPES} value={type} onChange={pickType} />;
  if (items.loading) return <><Topbar />{head}{tabs}<SkelRows n={8} /></>;
  if (items.error) return <><Topbar />{head}<ErrorBanner title="Couldn’t load your backlog." error={items.error} onRetry={items.reload} /></>;
  if (isEmpty) return <><Topbar />{head}<Empty h="Nothing waiting" line="Paste a Spotify link above, or share one from your phone, and it lands here."><button className="btn btn-primary" onClick={() => quickAdd()}>Add a link</button></Empty></>;

  const oldestDays = Math.max(1, ...list.map((i) => daysSince(i.added_at)));
  const row = (it: ItemSummary, group: ItemSummary[], n: number, g: string, cap: number, order: string) => {
    const d = daysSince(it.added_at);
    // Moving the last shown row down shows one more first, so the moved row stays mounted and keeps focus
    const down = () => { if (n + 1 >= cap) setShown((s) => ({ ...s, [g]: cap + 1 })); reorder(it.id, group[n + 1].id); };
    return (
      <m.div key={it.id} layout={reduce ? false : "position"} layoutDependency={order} transition={{ layout: LAND }}
        initial={reduce ? false : { opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, x: -24, transition: swap(reduce, { duration: 0.2, ease: EASE }) }}
        className={`row bl-row${drag === it.id ? " dragging" : ""}${moved === it.id ? " moved" : ""}${sel.has(it.id) ? " sel" : ""}`}
        draggable={sort === "order"} onDragStart={() => setDrag(it.id)} onDragEnd={() => setDrag(null)}
        onDragOver={(e) => { if (drag != null && group.some((g) => g.id === drag)) e.preventDefault(); }}
        onDrop={(e) => { e.preventDefault(); if (drag != null && drag !== it.id) reorder(drag, it.id); setDrag(null); }}>
        <input type="checkbox" className="bl-above" checked={sel.has(it.id)} onChange={() => toggle(it.id)} aria-label={`Select ${nameOf(it)}`} />
        <Link to={`/item/${it.id}`} className="bl-stretch flex flex-col gap-0.5 text-inherit min-w-0">
          <span className="truncate font-semibold">{nameOf(it)}</span>
          <span className="truncate text-muted text-xs">{sub(it)}</span>
        </Link>
        <span className="text-text-2 text-sm bl-meta">{TYPE[it.type]}<span className="text-muted">, {it.recommended_by ? `from ${it.recommended_by}` : it.added_via === "pc" ? "added by you" : "from your phone"}</span></span>
        {/* How long it has waited, as a bar against the longest wait in view (no track) */}
        <span className="bl-age" title={`Added ${age(it.added_at)} ago`}>
          <span className="font-mono text-faint text-xs">{age(it.added_at)}</span>
          <i style={{ width: `${Math.max(4, (d / oldestDays) * 100)}%` }} className={it.status === "planned" && d > 180 ? "old" : undefined} />
        </span>
        <span className="justify-self-end bl-above">{it.status === "in_progress" ? <StatusBadge>In progress</StatusBadge>
          : <button className="btn btn-secondary s" onClick={() => start(it)} aria-label={`Start ${nameOf(it)}`}>Start</button>}</span>
        <span className="flex gap-1 items-center justify-self-end bl-above">{sort === "order" && <>
          {/* aria-disabled, not disabled: moving a record to the end of its group must not drop keyboard focus; out of the Tab order there */}
          <button className="icon-btn w-8 h-8" aria-disabled={n === 0} tabIndex={n === 0 ? -1 : undefined} onClick={() => n > 0 && reorder(it.id, group[n - 1].id)} aria-label={`Move ${nameOf(it)} up`}><ArrowUpIcon size={14} aria-hidden /></button>
          <button className="icon-btn w-8 h-8" aria-disabled={n === group.length - 1} tabIndex={n === group.length - 1 ? -1 : undefined} onClick={() => n < group.length - 1 && down()} aria-label={`Move ${nameOf(it)} down`}><ArrowDownIcon size={14} aria-hidden /></button>
        </>}</span>
      </m.div>
    );
  };

  return (
    <LazyMotion features={layoutFeatures}>
      <Topbar />
      {head}
      <div className="lit" onPointerMove={spot}>
      <div className="card bl-sum">
        <div className="flex flex-col gap-1 min-w-0">
          <span className="bl-big">{num(named.length)}<span className="text-muted text-sm font-sans font-normal"> waiting</span></span>
          <span className="text-text-2 text-sm">{num(planned)} planned, {num(inProg)} in progress{oldest ? `, oldest added ${age(oldest)} ago` : ""}</span>
          {stale > 0 && <span className="text-accent text-xs">{pl(stale, "record")} planned more than six months ago</span>}
        </div>
        {sort === "order" && !art && <Mix total={list.length} counts={GROUPS.map(([g, p]) => [g, list.filter((i) => i.priority === p).length])} />}
      </div>
      </div>
      {tabs}
      {actErr && <ErrorBanner title="Couldn’t save that change." error={actErr} />}
      {truncated && <div className="banner warn" role="status"><span>Showing the first {num(all.length)} of {num(items.data!.total)} records in your backlog. Reordering moves only these.</span></div>}
      <TabPanel key={type} dir={dir} label={TYPES.find((t) => t[0] === type)![1]} className="flex flex-col gap-6">
      {(needName.length > 0 || unresolved.length > 0) && !filtered && (
        <section className="flex flex-col gap-1.5">
          <h2 className="text-accent text-sm font-bold">Waiting for a name <span className="font-mono font-normal">{needName.length + unresolved.length}</span></h2>
          <div className="card pad-s"><div className="list">
            {needName.map((it) => (
              <div key={it.id} className="row grid-cols-[minmax(0,1fr)_auto_auto]">
                <span className="flex flex-col gap-0.5 min-w-0">
                  <span className="font-mono truncate text-text-2 text-xs">{nameOf(it)}</span>
                  <span className="text-muted text-xs">Shared from iPhone {ago(it.added_at).toLowerCase()}</span>
                </span>
                <NameNeeded />
                <button className="btn btn-light s" onClick={() => quickAdd({ namingItemId: it.id })} aria-label={`Name the link shared ${ago(it.added_at).toLowerCase()}`}>Name it</button>
              </div>))}
            {unresolved.map((u) => (
              <div key={u.id} className="row grid-cols-[minmax(0,1fr)_auto_auto]">
                <span className="flex flex-col gap-0.5 min-w-0">
                  <span className="font-mono truncate text-text-2 text-xs">{u.raw_link}</span>
                  <span className="text-muted text-xs">{u.source === "shortcut" ? "Shared from iPhone" : "Pasted"} {ago(u.received_at).toLowerCase()}, link not opened yet</span>
                </span>
                <button className="btn btn-secondary s" onClick={retry}>Try again</button>
                <button className="btn btn-danger s" onClick={async () => { await run(discardUnresolved(u.id)); }} aria-label={`Discard ${u.raw_link}`}>Discard</button>
              </div>))}
          </div></div>
        </section>)}
      {!list.length ? (filtered
        ? <Empty h="No matches" line="Nothing in the backlog fits these filters."><button className="btn btn-secondary" onClick={() => { pickType("all"); setTag(""); }}>Clear filters</button></Empty>
        : null)
        : art ? <ArtTiles items={list} meta={statusMeta} />
        : groups.map(([g, rows, gi]) => {
          const cap = shown[g] ?? PER_GROUP;
          const order = rows.map((r) => r.id).join(","); // layout measures only when this changes (a reorder), not on every render
          return (
            <section key={g} id={`bl-${gi}`} className="bl-group" aria-label={g}>
              <h2 className={`bl-group-head ${g === "High priority" ? "text-accent" : "text-text-2"}`}><i className={`p${gi}`} aria-hidden />{g}<span className="font-mono text-faint font-normal">{num(rows.length)}</span></h2>
              <div className="card pad-s"><div className="list">
                <AnimatePresence initial={false}>{rows.slice(0, cap).map((it, n) => row(it, rows, n, g, cap, order))}</AnimatePresence>
              </div></div>
              {rows.length > cap && (
                <div className="flex flex-wrap gap-2 items-center justify-between">
                  <span className="font-mono text-faint text-xs">{num(cap)} of {num(rows.length)} shown</span>
                  <button className="btn btn-secondary s" onClick={() => { setShown((s) => ({ ...s, [g]: cap + MORE })); requestAnimationFrame(() => document.querySelector<HTMLElement>(`#bl-${gi} .list > :nth-child(${cap + 1}) .bl-stretch`)?.focus()); }}>Show {num(Math.min(MORE, rows.length - cap))} more</button>
                </div>)}
            </section>);
        })}
      <span className="font-mono text-faint text-xs">{num(list.length)} of {num(named.length)} shown</span>
      </TabPanel>
      <AnimatePresence>
        {sel.size > 0 && (
          <m.div key="selbar" className="selbar" role="region" aria-label="Selection actions"
            initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 16 }} transition={swap(reduce, LAND)}>
            <span className="font-bold px-1.5">{sel.size} selected</span>
            <button className="btn" onClick={selPrio}>Set high priority</button>
            <button className="btn" onClick={selRemove}>Remove from backlog</button>
            <button className="btn" onClick={() => setSel(new Set())}>Clear</button>
          </m.div>)}
      </AnimatePresence>
    </LazyMotion>
  );
}

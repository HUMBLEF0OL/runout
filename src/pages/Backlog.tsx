// SC-03 Backlog: backlog order grouped by priority, filters, phone captures waiting for a name, unresolved short
// links, keyboard + drag reorder (reorder_backlog), multi-select bar with Undo, artwork toggle.
import { useState } from "react";
import { Link } from "react-router-dom";
import { ArrowDownIcon, ArrowUpIcon } from "@phosphor-icons/react";
import {
  discardUnresolved, listItems, listUnresolved, reorderBacklog, retryUnresolved, setStatus, updateItem,
  toAppError, type AppError, type ItemSummary, type ItemType, type Priority,
} from "../api";
import { Topbar } from "../components/Shell";
import { useQuickAdd } from "../components/QuickAddContext";
import { ArtTiles, Empty, ErrorBanner, NameNeeded, PageHead, Seg, SkelRows, StatusBadge, useToast } from "../components/ui";
import { useApi } from "../hooks/useApi";
import { age, ago, nameOf, pl, statusMeta, sub, TYPE } from "../lib/format";
import { moveTo } from "../lib/backlog";

type TypeF = "all" | ItemType;
const GROUPS: [string, Priority | null][] = [["High priority", "high"], ["Medium", "medium"], ["Low", "low"], ["No priority", null]];

export default function Backlog() {
  const items = useApi(() => listItems({ in_backlog: true, sort: "backlog_order", limit: 1000 }));
  const unres = useApi(listUnresolved);
  const quickAdd = useQuickAdd();
  const toast = useToast();
  const [type, setType] = useState<TypeF>("all");
  const [tag, setTag] = useState("");
  const [sort, setSort] = useState<"order" | "added">("order");
  const [art, setArt] = useState(false);
  const [sel, setSel] = useState<Set<number>>(new Set());
  const [drag, setDrag] = useState<number | null>(null);
  const [actErr, setActErr] = useState<AppError>();

  const run = async (p: Promise<unknown>) => { setActErr(undefined); try { await p; return true; } catch (e) { setActErr(toAppError(e)); return false; } };

  const all = items.data?.rows ?? [];
  const named = all.filter((i) => i.display_name);
  const needName = all.filter((i) => !i.display_name);
  const unresolved = unres.data ?? [];
  const tags = [...new Set(named.flatMap((i) => i.tags))].sort();
  let list = named.filter((i) => (type === "all" || i.type === type) && (!tag || i.tags.includes(tag)));
  if (sort === "added") list = [...list].sort((a, b) => b.added_at.localeCompare(a.added_at));
  const groups: [string, ItemSummary[]][] = sort === "added" ? [["All, newest first", list]]
    : GROUPS.map(([g, p]) => [g, list.filter((i) => i.priority === p)] as [string, ItemSummary[]]).filter((g) => g[1].length);

  const ids = all.map((i) => i.id); // full backlog order
  const reorder = (id: number, toId: number) => run(reorderBacklog(moveTo(ids, id, toId)));
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
  const isEmpty = !items.loading && !items.error && !all.length && !unresolved.length;
  const filtered = type !== "all" || !!tag;

  const head = (
    <PageHead title="Backlog" sub={isEmpty || items.loading || items.error ? undefined : `${planned} planned, ${inProg} in progress${oldest ? `, oldest added ${age(oldest)} ago` : ""}`}>
      {!isEmpty && <>
        <Seg<TypeF> label="Type" options={[["all", "All"], ["album", "Albums"], ["track", "Tracks"], ["artist", "Artists"]]} value={type} onChange={setType} />
        <label className="sr-only" htmlFor="bl-tag">Tag</label>
        <select id="bl-tag" className="input surface" value={tag} onChange={(e) => setTag(e.target.value)}>
          <option value="">All tags</option>{tags.map((t) => <option key={t}>{t}</option>)}
        </select>
        <label className="sr-only" htmlFor="bl-sort">Sort</label>
        <select id="bl-sort" className="input surface" value={sort} onChange={(e) => setSort(e.target.value as "order" | "added")}>
          <option value="order">Priority</option><option value="added">Date added</option>
        </select>
        <button className="btn btn-secondary" aria-pressed={art} onClick={() => setArt(!art)}>{art ? "Hide artwork" : "Show artwork"}</button>
      </>}
    </PageHead>
  );

  if (items.loading) return <><Topbar />{head}<SkelRows n={8} /></>;
  if (items.error) return <><Topbar />{head}<ErrorBanner title="Couldn’t load your backlog." error={items.error} onRetry={items.reload} /></>;
  if (isEmpty) return <><Topbar />{head}<Empty h="Nothing waiting" line="Paste a Spotify link above, or share one from your phone, and it lands here."><button className="btn btn-primary" onClick={() => quickAdd()}>Add a link</button></Empty></>;

  const row = (it: ItemSummary, group: ItemSummary[], n: number) => (
    <div key={it.id} className={`row grid-cols-[16px_minmax(0,1fr)_70px_90px_80px_104px_64px] ${drag === it.id ? "opacity-50" : ""}`}
      draggable={sort === "order"} onDragStart={() => setDrag(it.id)} onDragEnd={() => setDrag(null)}
      onDragOver={(e) => { if (drag != null && group.some((g) => g.id === drag)) e.preventDefault(); }}
      onDrop={(e) => { e.preventDefault(); if (drag != null && drag !== it.id) reorder(drag, it.id); setDrag(null); }}>
      <input type="checkbox" checked={sel.has(it.id)} onChange={() => toggle(it.id)} aria-label={`Select ${nameOf(it)}`} />
      <Link to={`/item/${it.id}`} className="flex flex-col gap-0.5 text-inherit min-w-0">
        <span className="truncate font-semibold">{nameOf(it)}</span>
        <span className="truncate text-muted text-xs">{sub(it)}</span>
      </Link>
      <span className="text-text-2 text-sm">{TYPE[it.type]}</span>
      <span className="text-text-2 truncate text-sm">{it.recommended_by || (it.added_via === "pc" ? "You" : "Phone")}</span>
      <span className="font-mono text-faint text-xs">{age(it.added_at)}</span>
      <span className="justify-self-end">{it.status === "in_progress" ? <StatusBadge>In progress</StatusBadge>
        : <button className="btn btn-secondary s" onClick={() => start(it)} aria-label={`Start ${nameOf(it)}`}>Start</button>}</span>
      <span className="flex flex-wrap gap-1 items-center justify-self-end">{sort === "order" && <>
        <button className="icon-btn w-8 h-8" disabled={n === 0} onClick={() => reorder(it.id, group[n - 1].id)} aria-label={`Move ${nameOf(it)} up`}><ArrowUpIcon size={14} aria-hidden /></button>
        <button className="icon-btn w-8 h-8" disabled={n === group.length - 1} onClick={() => reorder(it.id, group[n + 1].id)} aria-label={`Move ${nameOf(it)} down`}><ArrowDownIcon size={14} aria-hidden /></button>
      </>}</span>
    </div>
  );

  return (
    <>
      <Topbar />
      {head}
      {actErr && <ErrorBanner title="Couldn’t save that change." error={actErr} />}
      {(needName.length > 0 || unresolved.length > 0) && !filtered && (
        <section className="flex flex-col gap-1.5">
          <h2 className="text-accent text-sm font-bold">Waiting for a name</h2>
          <div className="card pad-s"><div className="list">
            {needName.map((it) => (
              <div key={it.id} className="row grid-cols-[minmax(0,1fr)_auto_auto]">
                <span className="flex flex-col gap-0.5 min-w-0">
                  <span className="font-mono truncate text-text-2 text-xs">{nameOf(it)}</span>
                  <span className="text-muted text-xs">Shared from iPhone {ago(it.added_at).toLowerCase()}</span>
                </span>
                <NameNeeded />
                <button className="btn btn-primary s" onClick={() => quickAdd({ namingItemId: it.id })} aria-label={`Name the link shared ${ago(it.added_at).toLowerCase()}`}>Name it</button>
              </div>))}
            {unresolved.map((u) => (
              <div key={u.id} className="row grid-cols-[minmax(0,1fr)_auto_auto]">
                <span className="flex flex-col gap-0.5 min-w-0">
                  <span className="font-mono truncate text-text-2 text-xs">{u.raw_link}</span>
                  <span className="text-muted text-xs">{u.source === "shortcut" ? "Shared from iPhone" : "Pasted"} {ago(u.received_at).toLowerCase()}, link not opened yet</span>
                </span>
                <button className="btn btn-secondary s" onClick={retry}>Try again</button>
                <button className="btn btn-ghost s" onClick={async () => { await run(discardUnresolved(u.id)); }} aria-label={`Discard ${u.raw_link}`}>Discard</button>
              </div>))}
          </div></div>
        </section>)}
      {!list.length ? (filtered
        ? <Empty h="No matches" line="Nothing in the backlog fits these filters."><button className="btn btn-secondary" onClick={() => { setType("all"); setTag(""); }}>Clear filters</button></Empty>
        : null)
        : art ? <ArtTiles items={list} meta={statusMeta} />
        : groups.map(([g, rows]) => (
          <section key={g} className="flex flex-col gap-1.5" aria-label={g}>
            <h2 className={`${g === "High priority" ? "text-accent" : "text-text-2"} text-sm font-bold`}>{g}</h2>
            <div className="card pad-s"><div className="list">{rows.map((it, n) => row(it, rows, n))}</div></div>
          </section>))}
      <span className="font-mono text-faint text-xs">{list.length} of {named.length} shown</span>
      {sel.size > 0 && (
        <div className="selbar" role="region" aria-label="Selection actions">
          <span className="font-bold px-1.5">{sel.size} selected</span>
          <button className="btn" onClick={selPrio}>Set high priority</button>
          <button className="btn" onClick={selRemove}>Remove from backlog</button>
          <button className="btn" onClick={() => setSel(new Set())}>Clear</button>
        </div>)}
    </>
  );
}

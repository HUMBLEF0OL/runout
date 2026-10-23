// SC-02 Home (MVP): numbers, in progress, worth a return, waiting for a name, up next, recently added.
// An empty library shows first run (SC-02b). HomeFull reuses MvpBody under its empty listening card.
import { useState } from "react";
import { Link } from "react-router-dom";
import { getDashboard, listItems, setStatus, updateItem, type Dashboard, type ItemSummary } from "../api";
import { Topbar } from "../components/Shell";
import { useQuickAdd } from "../components/QuickAddContext";
import { ArtTiles, ErrorBanner, ItemSet, OpenSpotify, SkelRows, Stat, useToast } from "../components/ui";
import { useApi } from "../hooks/useApi";
import { age, ago, daysSince, nameOf, PRIO, stars, TYPE, uriLink } from "../lib/format";
import { byPriority } from "../lib/backlog";
import FirstRun from "./FirstRun";

const from = (it: ItemSummary) => (it.recommended_by ? `, from ${it.recommended_by}` : "");
const withScheme = (raw: string) => (/^https?:\/\//.test(raw) ? raw : `https://${raw}`);

export const StatSkel = ({ className = "" }: { className?: string }) => <div className={`stats ${className}`}>{[1, 2, 3, 4].map((i) => <div key={i} className="card h-24"><div className="skel h-full" /></div>)}</div>;

export default function HomeMvp() {
  const d = useApi(getDashboard);
  if (d.data && d.data.counts.library === 0) return <FirstRun />;
  return (
    <>
      <Topbar />
      <h1 className="sr-only">Home</h1>
      {d.loading ? <><StatSkel /><SkelRows n={5} /></>
        : d.error || !d.data ? <ErrorBanner title="Couldn’t open your library." error={d.error} onRetry={d.reload} />
        : <MvpBody dash={d.data} />}
    </>
  );
}

export function MvpBody({ dash }: { dash: Dashboard }) {
  const toast = useToast();
  const add = useQuickAdd();
  const [art, setArt] = useState(false);
  const next = useApi(() => listItems({ in_backlog: true, status: ["planned"], sort: "backlog_order", limit: 200 }));
  const recent = useApi(() => listItems({ sort: "added_at", desc: true, limit: 4 }));
  const returns = useApi(() => listItems({ revisit: true, sort: "modified_at", limit: 20 }));
  const c = dash.counts;
  const oldest = dash.in_progress.map((i) => dash.started_at[i.id]).filter(Boolean).sort()[0];
  const upNext = [...(next.data?.rows ?? [])].sort(byPriority).slice(0, 5);

  const markExplored = async (it: ItemSummary) => {
    await setStatus(it.id, "explored");
    toast(`${nameOf(it)} is now Explored`, async () => { await setStatus(it.id, "in_progress"); if (it.in_backlog) await updateItem(it.id, { in_backlog: true }); });
  };

  return (
    <>
      <section className="stats" aria-label="Your numbers">
        <Stat k="In backlog" v={c.backlog} d={`${dash.planned} planned, ${c.in_progress} in progress`} />
        <Stat k="In progress" v={c.in_progress} d={oldest ? `Oldest started ${age(oldest)} ago` : "Nothing started"} act={!!oldest} />
        <Stat k="Explored this month" v={dash.explored_this_month} d={`${dash.not_for_me_this_month} marked not for me`} />
        <Stat k="Explored, no review" v={dash.explored_no_review} d={dash.explored_no_review ? "Write a line while it is fresh" : "Every explored record has a review"} act={!!dash.explored_no_review} />
      </section>
      <div className="flex flex-wrap gap-7 items-start">
        <section className="flex-[1.5_1_520px] min-w-0 flex flex-col gap-3.5" aria-labelledby="mvp-prog">
          <div className="flex justify-between items-baseline gap-3">
            <h2 id="mvp-prog" className="section-title">In progress</h2>
            <button className="btn btn-secondary" aria-pressed={art} onClick={() => setArt(!art)}>{art ? "Hide artwork" : "Show artwork"}</button>
          </div>
          {!dash.in_progress.length && <span className="text-muted">Nothing in progress. Start something from your backlog.</span>}
          {art && dash.in_progress.length > 0 && <ArtTiles items={dash.in_progress} meta={(it) => <span className="flex flex-wrap gap-2 items-center justify-between">
            Started {age(dash.started_at[it.id] ?? it.modified_at)} ago
            <button className="btn btn-light s" onClick={() => markExplored(it)} aria-label={`Mark explored: ${nameOf(it)}`}>Mark explored</button></span>} />}
          {!art && dash.in_progress.map((it) => (
            <div key={it.id} className="card flex flex-wrap gap-4 items-center">
              <div className="flex flex-col flex-[1_1_220px] gap-1 min-w-0">
                <Link to={`/item/${it.id}`} className="text-[18px] font-extrabold text-text">{nameOf(it)}</Link>
                <span className="text-text-2 truncate">{it.type === "artist" ? it.notes || "Artist" : it.artist_names.join(", ")}</span>
                <span className="font-mono text-faint text-xs">{TYPE[it.type]}, started {age(dash.started_at[it.id] ?? it.modified_at)} ago{from(it)}</span>
              </div>
              <div className="flex flex-wrap gap-2 items-center">
                <button className="btn btn-light" onClick={() => markExplored(it)} aria-label={`Mark explored: ${nameOf(it)}`}>Mark explored</button>
                <OpenSpotify uri={it.spotify_uri} name={nameOf(it)} />
              </div>
            </div>
          ))}
          <h2 className="section-title mt-3">Worth a return</h2>
          {returns.data?.rows.length
            ? <ItemSet items={returns.data.rows} total={returns.data.total} art={art} meta={(it) => (it.rating ? `Revisit, ${stars(it.rating)}` : "Revisit")} />
            : <span className="text-muted">Flag a record Revisit later and it shows up here.</span>}
        </section>

        <aside className="flex-[1_1_320px] min-w-0 flex flex-col gap-5" aria-label="Needs you">
          <section className="card flex flex-col gap-3" aria-labelledby="mvp-names">
            <h2 id="mvp-names" className="card-title">Waiting for a name</h2>
            {!dash.name_needed.length && !dash.unresolved.length && <span className="text-muted">Nothing waiting.</span>}
            {dash.name_needed.map((it) => (
              <NameRow key={`i${it.id}`} link={uriLink(it)} when={it.added_at} note="" onName={() => add({ namingItemId: it.id, link: withScheme(uriLink(it)) })} />
            ))}
            {dash.unresolved.map((u) => (
              <NameRow key={`u${u.id}`} link={u.raw_link} when={u.received_at} note=", link not opened yet" onName={() => add({ link: withScheme(u.raw_link) })} />
            ))}
          </section>
          <section className="card flex flex-col gap-3" aria-labelledby="mvp-next">
            <div className="flex justify-between items-baseline gap-3"><h2 id="mvp-next" className="card-title">Up next</h2><Link to="/backlog" className="text-sm font-semibold">Backlog ({c.backlog})</Link></div>
            {next.data && !upNext.length && <span className="text-muted">Nothing planned. Paste a link to add one.</span>}
            {upNext.map((it) => (
              <Link key={it.id} to={`/item/${it.id}`} className="lrow flex justify-between items-baseline gap-3 text-text py-1">
                <span className="flex flex-col gap-0.5"><span className="font-semibold">{nameOf(it)}</span><span className="text-muted text-xs">{TYPE[it.type]}{from(it)}</span></span>
                <span className={`text-xs ${it.priority === "high" ? "text-accent" : "text-muted"}`}>{it.priority ? PRIO[it.priority] : "Planned"}</span>
              </Link>
            ))}
          </section>
          <section className="card flex flex-col gap-3" aria-labelledby="mvp-recent">
            <h2 id="mvp-recent" className="card-title">Recently added</h2>
            {recent.data?.rows.map((it) => (
              <Link key={it.id} to={`/item/${it.id}`} className="lrow flex justify-between items-baseline gap-3 text-text py-0.5">
                <span className="truncate">{nameOf(it)}</span>
                <span className="font-mono text-faint whitespace-nowrap text-xs">{age(it.added_at)}{daysSince(it.added_at) ? " ago" : ""}</span>
              </Link>
            ))}
          </section>
        </aside>
      </div>
    </>
  );
}

function NameRow({ link, when, note, onName }: { link: string; when: string; note: string; onName: () => void }) {
  const t = ago(when).toLowerCase();
  return (
    <div className="row grid-cols-[minmax(0,1fr)_auto] bg-surface-2">
      <span className="flex flex-col gap-0.5 min-w-0">
        <span className="font-mono truncate text-text-2 text-xs">{link}</span>
        <span className="text-muted text-xs">Shared from iPhone {t}{note}</span>
      </span>
      <button className="btn btn-primary s" onClick={onName} aria-label={`Name the link shared ${t}`}>Name it</button>
    </div>
  );
}

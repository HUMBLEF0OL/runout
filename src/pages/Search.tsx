// SC-04c Search results: grouped by type, snippet with the match marked, and the field it matched in.
import { Link, useParams } from "react-router-dom";
import { search, type ItemType, type SearchHit } from "../api";
import { Topbar } from "../components/Shell";
import { Empty, ErrorBanner, SkelRows } from "../components/ui";
import { useApi } from "../hooks/useApi";
import { nameOf, pl, TYPE } from "../lib/format";

const FIELD: Record<string, string> = { recommended_by: "recommended by", tags: "tags" };

/** Snippet with the first case-insensitive match wrapped in <mark>. */
function Snip({ text, q }: { text: string; q: string }) {
  const i = text.toLowerCase().indexOf(q.toLowerCase());
  if (!q || i < 0) return <>{text}</>;
  return <>{text.slice(0, i)}<mark>{text.slice(i, i + q.length)}</mark>{text.slice(i + q.length)}</>;
}

export default function Search() {
  const q = (useParams().q ?? "").trim();
  const res = useApi(() => search(q), [q]);
  const hits = res.data ?? [];
  const groups = (["artist", "album", "track"] as ItemType[]).map((t) => [t, hits.filter((h) => h.item.type === t)] as [ItemType, SearchHit[]]).filter((g) => g[1].length);
  return (
    <>
      <Topbar />
      <div className="flex flex-col gap-1">
        <h1 className="page-title">Results for “{q}”</h1>
        {res.loading ? <span className="text-muted">Searching…</span>
          : !res.error && <span className="text-muted">{pl(hits.length, "result")} in names, reviews, notes, tags and recommendations</span>}
      </div>
      {res.loading ? <SkelRows n={5} />
        : res.error ? <ErrorBanner title="Couldn’t load search results." error={res.error} onRetry={res.reload} />
        : !hits.length ? <Empty h="Nothing matches" line={`No name, review, note or tag contains “${q}”.`}><Link className="btn btn-secondary" to="/library">Clear search</Link></Empty>
        : groups.map(([t, list]) => (
          <section key={t} className="flex flex-col gap-1.5">
            <h2 className="text-text-2 text-sm font-bold">{TYPE[t]}s</h2>
            <div className="card pad-s"><div className="list">{list.map(({ item: it, snippet, field }) => (
              <Link key={it.id} className="row grid-cols-[minmax(0,1fr)_130px] text-inherit" to={`/item/${it.id}`}>
                <span className="flex flex-col gap-0.5 min-w-0">
                  <span className="font-semibold">{nameOf(it)}{it.type !== "artist" && it.artist_names.length > 0 && <span className="text-muted font-normal">, {it.artist_names.join(", ")}</span>}</span>
                  <span className="text-text-2 text-sm"><Snip text={snippet} q={q} /></span>
                </span>
                <span className="text-muted text-xs text-right">Matched in {FIELD[field] ?? field}</span>
              </Link>))}
            </div></div>
          </section>))}
    </>
  );
}

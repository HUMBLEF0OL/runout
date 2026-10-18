// Display formatting shared by every screen (ported from the prototype's R.* helpers in app.js).
import type { ItemSummary, Priority, Status, ItemType } from "../api";
import { safeUri } from "./sanitize";

let TODAY: () => Date = () => new Date();
/** Mocks pin "now" to the dataset's day so relative times match the designs. */
export const setToday = (iso: string) => { const d = new Date(iso); TODAY = () => d; };
export const today = () => TODAY();

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
export const day = (iso: string) => { const d = new Date(iso); return `${d.getDate()} ${MONTHS[d.getMonth()]}`; };
export const dayY = (iso: string) => { const d = new Date(iso); return `${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`; };
export const monthName = (ym: string) => MONTHS[Number(ym.slice(5, 7)) - 1];
export const time = (iso: string) => iso.slice(11, 16);
const midnight = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
/** Calendar days between the date and today. */
export const daysSince = (iso: string) => Math.max(0, Math.round((midnight(TODAY()) - midnight(new Date(iso))) / 86400000));
export const num = (n: number) => Number(n).toLocaleString("en-US");
/** Pluralised count: pl(3, "play") → "3 plays". Never write `${n} plays` by hand. */
export const pl = (n: number, one: string, many?: string) => `${num(n)} ${n === 1 ? one : many || one + "s"}`;
export const an = (word: string) => (/^[aeiou]/i.test(word) ? "an " : "a ") + word;
export const ago = (iso: string) => {
  const days = daysSince(iso);
  if (days === 0) {
    const min = Math.round((TODAY().getTime() - new Date(iso).getTime()) / 60000);
    if (min < 2) return "now";
    return min < 60 ? `${min} min ago` : `${Math.round(min / 60)} h ago`;
  }
  if (days === 1) return "Yesterday";
  if (days < 7) return `${pl(days, "day")} ago`;
  return day(iso);
};
export const age = (iso: string) => {
  const d = daysSince(iso);
  if (d === 0) return "today";
  if (d < 14) return pl(d, "day");
  if (d < 60) return pl(Math.round(d / 7), "week");
  return pl(Math.round(d / 30), "month");
};
export const hm = (ms: number) => { const m = Math.round(ms / 60000); return m >= 60 ? (m % 60 ? `${Math.floor(m / 60)}h ${m % 60}m` : `${m / 60}h`) : `${m}m`; };
export const mmss = (ms: number) => { const s = Math.round(ms / 1000); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`; };
export const stars = (r: number | null) => (r ? "★".repeat(Math.floor(r)) + (r % 1 ? "½" : "") : "");

export const STATUS: Record<Status, string> = { planned: "Planned", in_progress: "In progress", explored: "Explored", not_for_me: "Not for me", abandoned: "Abandoned" };
export const STATUS_ORDER = Object.keys(STATUS) as Status[];
export const TYPE: Record<ItemType, string> = { album: "Album", track: "Track", artist: "Artist" };
export const PRIO: Record<Priority, string> = { high: "High priority", medium: "Medium", low: "Low" };

export const uriId = (uri: string | null) => (uri ?? "").split(":")[2] ?? "";
export const uriLink = (it: Pick<ItemSummary, "type" | "spotify_uri">) => `open.spotify.com/${it.type}/${uriId(it.spotify_uri)}`;
/** Spotify web link for a stored URI; a malformed URI falls back to Spotify's home page. */
export const openUrl = (uri: string) => { const u = safeUri(uri); return u ? `https://open.spotify.com/${u.type}/${u.id}` : "https://open.spotify.com"; };
/** Name, or the link while the name is still needed (phone captures). */
export const nameOf = (it: Pick<ItemSummary, "display_name" | "type" | "spotify_uri">) => it.display_name || uriLink(it);
/** Row subtitle: "Artist" / "Artist, from Album" / "Artist". */
export const sub = (it: ItemSummary) => it.type === "artist" ? "Artist" : it.type === "track" && it.album_name ? `${it.artist_names.join(", ")}, from ${it.album_name}` : it.artist_names.join(", ");
export const statusMeta = (it: Pick<ItemSummary, "status" | "rating">) => (it.rating ? `${STATUS[it.status]} ${stars(it.rating)}` : STATUS[it.status]);

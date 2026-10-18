// TS port of TSD §4.1 link parsing: the mocks' `parse_link`, and the combined field's instant button label.
// The real parse is the `parse_link` command; M3's links.rs reuses link.test.ts's table.
import type { ItemType, ParsedLink } from "../api";

const ID = "[A-Za-z0-9]{22}";
const URL_RE = new RegExp(`^(?:https?://)?open\\.spotify\\.com/(?:intl-[a-z]{2}(?:-[a-z]{2})?/)?([a-z]+)/(${ID})(?:[/?#].*)?$`, "i");
const URI_RE = new RegExp(`^spotify:([a-z]+):(${ID})$`, "i");
const SHORT_RE = /^(?:https?:\/\/)?spotify\.link\/[A-Za-z0-9]+\/?(?:\?.*)?$/i;
const ITEM: ItemType[] = ["album", "track", "artist"];
const UNSUPPORTED = ["episode", "show", "audiobook", "playlist"]; // playlist only via Playlist features, not as an item

const match = (raw: string) => { const s = raw.trim(); return s.match(URL_RE) || s.match(URI_RE); };

/** Pure parse; `existing_id` is filled by the caller. */
export function parseLink(raw: string): ParsedLink {
  if (SHORT_RE.test(raw.trim())) return { state: "short" };
  const m = match(raw);
  if (!m) return { state: "invalid" };
  const kind = m[1].toLowerCase();
  if (ITEM.includes(kind as ItemType)) return { state: "ok", type: kind as ItemType, uri: `spotify:${kind}:${m[2]}` };
  return { state: UNSUPPORTED.includes(kind) ? "unsupported" : "invalid" };
}

/** The Spotify kind a valid or unsupported link names ("album", "episode", …); undefined otherwise. */
export const linkKind = (raw: string) => (parseLink(raw).state === "invalid" ? undefined : match(raw)?.[1].toLowerCase());

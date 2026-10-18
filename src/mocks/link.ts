// TS port of TSD §4.1 link parsing, used only inside mocks. M3's links.rs reuses link.test.ts's table.
import type { ItemType, ParsedLink } from "../api";

const ID = "[A-Za-z0-9]{22}";
const URL_RE = new RegExp(`^(?:https?://)?open\.spotify\.com/(?:intl-[a-z]{2}(?:-[a-z]{2})?/)?([a-z]+)/(${ID})(?:[/?#].*)?$`, "i");
const URI_RE = new RegExp(`^spotify:([a-z]+):(${ID})$`, "i");
const SHORT_RE = /^(?:https?:\/\/)?spotify\.link\/[A-Za-z0-9]+\/?(?:\?.*)?$/i;
const ITEM: ItemType[] = ["album", "track", "artist"];
const UNSUPPORTED = ["episode", "show", "audiobook", "playlist"]; // playlist only via Playlist features, not as an item

/** Pure parse; `existing_id` is filled by the caller. */
export function parseLink(raw: string): ParsedLink {
  const s = raw.trim();
  if (SHORT_RE.test(s)) return { state: "short" };
  const m = s.match(URL_RE) || s.match(URI_RE);
  if (!m) return { state: "invalid" };
  const kind = m[1].toLowerCase();
  if (ITEM.includes(kind as ItemType)) return { state: "ok", type: kind as ItemType, uri: `spotify:${kind}:${m[2]}` };
  return { state: UNSUPPORTED.includes(kind) ? "unsupported" : "invalid" };
}

/** Does the text look like a Spotify link at all (combined field switches to Add)? */
export const looksLikeLink = (raw: string) => parseLink(raw).state !== "invalid";

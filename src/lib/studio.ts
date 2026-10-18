// Playlist Studio suggestions (SC-11, CAP-29): from your own ratings and duplicates only, never listening data (D27).
import type { DraftTrack } from "../api";

/** Why Studio suggests cutting this track, or null to keep: a duplicate, or rated 2 stars or lower by you. */
export function cutReason(t: DraftTrack, all: DraftTrack[]): string | null {
  if (t.dupe_of) return `Duplicate of ${all.findIndex((x) => x.uri === t.dupe_of) + 1}`;
  if (t.rating != null && t.rating <= 2) return `You rated it ${t.rating}`;
  return null;
}

/** The `keep` argument for preview/apply: uris of kept rows in playlist order (a duplicate row cut drops one copy). */
export const keepList = (tracks: DraftTrack[], cut: Set<number>) => tracks.filter((_, i) => !cut.has(i)).map((t) => t.uri);

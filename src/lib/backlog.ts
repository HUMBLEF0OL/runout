// Pure logic for SC-03 Backlog reorder and SC-06 Quick add (tested in backlog.test.ts).
import type { ItemSummary, ItemType, ParsedLink } from "../api";

const PRIO_ORDER = { high: 0, medium: 1, low: 2 } as const;
/** Sort comparator: high, medium, low, then no priority (Home "Up next", Home backlog card). Stable, so backlog order holds within a priority. */
export const byPriority = (a: Pick<ItemSummary, "priority">, b: Pick<ItemSummary, "priority">) =>
  (a.priority ? PRIO_ORDER[a.priority] : 3) - (b.priority ? PRIO_ORDER[b.priority] : 3);

/** Move `id` onto `toId`'s place in the full backlog order: after it when moving down, before it when moving up. */
export function moveTo(ids: number[], id: number, toId: number): number[] {
  const from = ids.indexOf(id), to = ids.indexOf(toId);
  if (from < 0 || to < 0 || from === to) return ids;
  const out = ids.filter((x) => x !== id);
  out.splice(out.indexOf(toId) + (from < to ? 1 : 0), 0, id);
  return out;
}

export type QaStep = "blank" | "manual" | "checking" | "invalid" | "podcast" | "playlist" | "offline-short" | "duplicate" | "form";

/** Which quick-add view shows (SC-06). `self` is the phone capture being named, never a duplicate of itself. */
export function qaStep(a: {
  manual: boolean; link: string; online: boolean; kind?: string; self?: number;
  parsed?: ParsedLink; resolved?: { uri: string; type: ItemType; existing_id?: number };
}): QaStep {
  if (a.manual) return "manual";
  if (!a.link.trim()) return "blank";
  const p = a.parsed;
  if (!p) return "checking";
  if (p.state === "invalid") return "invalid";
  if (p.state === "unsupported") return a.kind === "playlist" ? "playlist" : "podcast";
  if (p.state === "short") {
    if (!a.online && !a.resolved) return "offline-short";
    if (!a.resolved) return "checking";
  }
  const existing = p.existing_id ?? a.resolved?.existing_id;
  return existing != null && existing !== a.self ? "duplicate" : "form";
}

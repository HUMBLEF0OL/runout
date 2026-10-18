// Pure logic behind components (tested in logic.test.ts).
import type { Freshness, Play, Source } from "../api";
import { linkKind, parseLink } from "../lib/link";
import { day } from "../lib/format";

/** Rating keyboard (DESIGN §5): arrows ±0.5, Home/End, clamp 0–5. Returns null for keys it doesn't handle. */
export function ratingKey(value: number, key: string): number | null {
  const v = value ?? 0;
  switch (key) {
    case "ArrowRight": case "ArrowUp": return Math.min(5, v + 0.5);
    case "ArrowLeft": case "ArrowDown": return Math.max(0, v - 0.5);
    case "Home": return 0;
    case "End": return 5;
    default: return null;
  }
}
/** Click on star `index` (0-based); left half gives a half star. Clicking the current value clears it. */
export function ratingClick(current: number | null, index: number, leftHalf: boolean): number | null {
  const v = index + (leftHalf ? 0.5 : 1);
  return v === current ? null : v;
}

/** Combined search/add field (DESIGN §5): a Spotify link switches the button to Add. */
export function fieldButton(value: string): { mode: "add" | "search"; label: string } {
  const v = value.trim();
  if (!v) return { mode: "add", label: "Add" };
  const p = parseLink(v);
  if (p.state === "short") return { mode: "add", label: "Add link" };
  if (p.state === "ok" || p.state === "unsupported") return { mode: "add", label: `Add ${linkKind(v)}` };
  return { mode: "search", label: "Search" };
}

/** Freshness note (DESIGN §1.3, FR-032): "Desktop: live. All devices: up to 2 Oct." */
export function freshnessText(f: Freshness | undefined, loggerState?: string): string {
  const desktop = loggerState === "paused" ? "paused" : loggerState === "off" ? "off" : loggerState === "error" ? "not available" : f?.logger_last_event ? "live" : "no plays yet";
  const all = f?.export_latest_event ? `up to your export of ${day(f.export_latest_event)}` : "import your Spotify export to include them";
  return `Desktop plays: ${desktop}. All devices: ${all}.`;
}

const SRC = { U: "Your records", S: "Desktop log", E: "Spotify export" } as const;
/** Source tag (FR-032): "From: Your records + Desktop log." */
export const sourceText = (sources: (keyof typeof SRC)[]) => `From: ${sources.map((s) => SRC[s]).join(" + ")}.`;

/** Source tag for a play list (list_plays has no `sources`): U when any play is linked to a record, then each play source present. */
export const playSources = (rows: Play[]): Source[] => [
  ...(rows.some((p) => p.item) ? ["U" as const] : []),
  ...(rows.some((p) => p.source === "logger") ? ["S" as const] : []),
  ...(rows.some((p) => p.source === "export") ? ["E" as const] : []),
];

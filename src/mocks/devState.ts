// Dev-only state switcher (DESIGN §6: every screen renders loading, empty, offline and error;
// "conflict" makes apply_studio_draft and undo_last_apply fail as if the playlist changed on Spotify, G-9;
// "partial" makes Spotify writes fail after they may have landed: apply keeps its undo, a create keeps the new playlist in `detail`;
// "busy" makes apply_studio_draft and undo_last_apply answer spotify_busy with retry_after_s: nothing written, undo kept;
// "heavy" scales listening insights to a long-time listener: years of history, 180k plays, long names (M9 Home).
// Mocks read it; the switcher UI (components/DevSwitcher) writes it. Release picks which nav/screens show.
export const VIEWS = ["normal", "empty", "loading", "offline", "error", "conflict", "partial", "busy", "heavy"] as const;
export type View = (typeof VIEWS)[number];
export type Release = "mvp" | "r2" | "r3";
export const dev = { view: "normal" as View, release: "r3" as Release };
const subs = new Set<() => void>();
export const onDevChange = (fn: () => void) => { subs.add(fn); return () => { subs.delete(fn); }; };
export function setDev(patch: Partial<typeof dev>) { Object.assign(dev, patch); subs.forEach((f) => f()); }

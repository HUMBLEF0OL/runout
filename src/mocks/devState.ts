// Dev-only state switcher (DESIGN §6: every screen renders loading, empty, offline and error).
// Mocks read it; the switcher UI (components/DevSwitcher) writes it. Release picks which nav/screens show.
export type View = "normal" | "empty" | "loading" | "offline" | "error";
export type Release = "mvp" | "r2" | "r3";
export const dev = { view: "normal" as View, release: "r3" as Release };
const subs = new Set<() => void>();
export const onDevChange = (fn: () => void) => { subs.add(fn); return () => { subs.delete(fn); }; };
export function setDev(patch: Partial<typeof dev>) { Object.assign(dev, patch); subs.forEach((f) => f()); }

import { useEffect } from "react";

/** Global shortcuts (DESIGN keyboard card, MILESTONES M2 WI 6). Esc on dialogs is native <dialog>. */
export type ShortcutAction = "quick-add" | "search" | "toggle-rail" | { status: 1 | 2 | 3 | 4 | 5 };

const typing = (el: Element | null) => !!el && (/^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName) || (el as HTMLElement).isContentEditable);

/** Pure key → action mapping (tested). `1–5` only when not typing; the page decides whether it applies. */
export function shortcutFor(e: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey">, active: Element | null): ShortcutAction | null {
  const mod = e.ctrlKey || e.metaKey;
  if (mod && !e.altKey && e.key.toLowerCase() === "n") return "quick-add";
  if (mod && !e.altKey && e.key.toLowerCase() === "k") return "search";
  if (mod && !e.altKey && e.key.toLowerCase() === "b") return "toggle-rail";
  if (!mod && !e.altKey && /^[1-5]$/.test(e.key) && !typing(active) && !active?.closest("dialog,[role=slider]")) return { status: Number(e.key) as 1 | 2 | 3 | 4 | 5 };
  return null;
}

export function useShortcuts(handler: (a: ShortcutAction, e: KeyboardEvent) => void) {
  useEffect(() => {
    const on = (e: KeyboardEvent) => { const a = shortcutFor(e, document.activeElement); if (a) handler(a, e); };
    window.addEventListener("keydown", on);
    return () => window.removeEventListener("keydown", on);
  }, [handler]);
}

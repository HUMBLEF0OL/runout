// Motion presets for the app (DESIGN §3, M9). Springs for things that move between places or get interrupted
// (markers, underlines, switch thumbs); a short ease-out for fades and slides. Transforms and opacity only, so
// every animation stays on the compositor; `MotionConfig reducedMotion="user"` (main.tsx) drops the transforms.

/** DESIGN §3 curve, cubic-bezier(0.16, 1, 0.3, 1). */
export const EASE = [0.16, 1, 0.3, 1] as const;
/** Lands with a little overshoot: rail marker, tab underline, segment indicator, switch thumb. */
export const LAND = { type: "spring", bounce: 0.25, visualDuration: 0.32 } as const;
/** Press feedback: quick, no overshoot. */
export const PRESS = { type: "spring", bounce: 0, visualDuration: 0.12 } as const;
/** A view arriving (tab panel, disclosure). */
export const ARRIVE = { duration: 0.32, ease: EASE } as const;
/** Reduced motion: MotionConfig stops transforms but still fades and AnimatePresence still waits, so swaps go instant. */
export const swap = (reduce: boolean | null, t: object) => (reduce ? { duration: 0 } : t);
/** Items arriving (cards, shelves): fade + 12 px rise, 35 ms apart by `custom` index (capped so long lists don't drag). */
export const RISE = {
  hidden: { opacity: 0, y: 12 },
  show: (i = 0) => ({ opacity: 1, y: 0, transition: { duration: 0.4, ease: EASE, delay: Math.min(i, 12) * 0.035 } }),
};

// Charts shared by Home and Insights (M9): month bars you can scrub, and a bar that grows in. Motion for React; the
// recipes (.hm-plot, .hm-cols, .hm-tip, .hm-months, .grow) are in app.css. Sizes are fixed by the data (12 months),
// never by history length.
import { useEffect, useState, type ReactNode } from "react";
import { m, useMotionValue, useReducedMotion, useSpring, useTransform } from "motion/react";
import { monthName } from "../lib/format";

export const SPRING = { type: "spring", bounce: 0.15, visualDuration: 0.45 } as const;
export const monthLong = (ym: string) => new Date(`${ym}-01T00:00`).toLocaleString("en-GB", { month: "long", year: "numeric" });

/** One bar per month. Point at a month or use the keys (`role="slider"`): it turns accent (DESIGN §3 "active bar") and a
 *  value tip springs above it; at rest the latest month is active, or `hot` if given. `avg` draws the dashed average. */
export function MonthBars({ months, fmt, label, avg, hot, caption, valuetext, tick, ghost, gap }: {
  /** `month` is the bar's key: 'YYYY-MM' by default; with `tick`, any key (days, hours: M10 Report). */
  months: { month: string; value: number }[]; fmt: (v: number) => string; label: string; avg?: boolean; hot?: number;
  /** Axis label for bar `i` ("" hides it; `on` = the bar in focus, label it); default the short month name. */
  tick?: (i: number, on: boolean) => string;
  /** The compared period as a faint line over the bars (M10: "the faint line is August"); null = no value there. */
  ghost?: (number | null)[];
  /** Bars with no data (M10 coverage "no data"): hatched full height, never drawn as silence. */
  gap?: (i: number) => boolean;
  /** Line under the bars: `i` is the picked month, null at rest. */
  caption?: (i: number | null) => ReactNode;
  /** What a screen reader hears for month `i` (the caption is visual only); default "<Month year>: <value>". */
  valuetext?: (i: number) => string;
}) {
  const reduce = useReducedMotion();
  const [at, setAt] = useState<number | null>(null);
  const n = months.length;
  const rest = hot ?? n - 1;
  // Start at rest: on Item the effect's jump below moved sx but left x (useTransform) at 100% (M9 WI 8).
  const tx = useMotionValue(n ? (rest + 0.5) / n : 1);
  const sx = useSpring(tx, SPRING);
  const x = useTransform(sx, [0, 1], ["0%", "100%"]);
  useEffect(() => { if (n) { const k = (rest + 0.5) / n; tx.jump(k); sx.jump(k); } }, [n, rest, tx, sx]); // rest without a glide
  if (n < 2) return null;
  const max = Math.max(1e-9, ...months.map((x_) => x_.value), ...(ghost ?? []).map((g) => g ?? 0));
  const dense = n > 48 ? " dense xs" : n > 16 ? " dense" : ""; // days of a month, years of months: tighter gaps
  const line = ghost?.map((g, i) => (g == null ? null : `${((i + 0.5) / n) * 100},${100 - (g / max) * 100}`)).filter(Boolean).join(" ");
  const kept = months.filter((_, i) => !gap?.(i)); // no-data bars aren't silence: out of the average
  const mean = kept.reduce((a, x_) => a + x_.value, 0) / Math.max(1, kept.length);
  const cur = at ?? rest;
  const pick = (i: number | null) => {
    const k = ((i ?? rest) + 0.5) / n;
    setAt(i);
    tx.set(k);
    if (reduce) sx.jump(k); // useSpring ignores MotionConfig reducedMotion
  };
  const keys = (e: React.KeyboardEvent) => {
    const step: Record<string, number> = { ArrowLeft: -1, ArrowDown: -1, ArrowRight: 1, ArrowUp: 1, PageDown: -3, PageUp: 3 };
    const k = e.key in step ? cur + step[e.key] : e.key === "Home" ? 0 : e.key === "End" ? n - 1 : null;
    if (k === null) return;
    e.preventDefault();
    pick(Math.max(0, Math.min(n - 1, k)));
  };
  const mo = months[cur];
  return (
    <>
      <div className="hm-plot" role="slider" tabIndex={0} aria-label={label} aria-valuemin={0} aria-valuemax={n - 1} aria-valuenow={cur}
        aria-valuetext={valuetext ? valuetext(cur) : `${monthLong(mo.month)}: ${fmt(mo.value)}`}
        onPointerMove={(e) => { const r = e.currentTarget.getBoundingClientRect(); const i = Math.max(0, Math.min(n - 1, Math.floor(((e.clientX - r.left) / r.width) * n))); if (i !== at) pick(i); }}
        onPointerLeave={() => pick(null)} onKeyDown={keys} onBlur={() => pick(null)}>
        {/* inherit={false}: the tip has no variants; joining a parent RISE (Home) left it at 100% */}
        <m.div className="hm-tip-track" style={{ x }} inherit={false} aria-hidden><span className={`hm-tip${at === null ? "" : " on"}`}>{fmt(mo.value)}</span></m.div>
        <div className={`hm-cols${dense}`} style={{ gridTemplateColumns: `repeat(${n}, minmax(0, 1fr))` }} aria-hidden>
          {avg && <span className="hm-avg" style={{ bottom: `${(mean / max) * 100}%` }} />}
          {line && <svg className="hm-ghost" viewBox="0 0 100 100" preserveAspectRatio="none"><polyline points={line} vectorEffect="non-scaling-stroke" /></svg>}
          {months.map((x_, i) => (
            <span key={x_.month} className={`hm-col${i === cur ? " on" : ""}${gap?.(i) ? " gap" : ""}`}>
              <m.i style={{ height: gap?.(i) ? "100%" : `${Math.max(2, (x_.value / max) * 100)}%`, originY: 1 }}
                initial={reduce ? false : { scaleY: 0 }} animate={{ scaleY: 1 }} transition={{ ...SPRING, delay: 0.15 + i * 0.03 }} />
            </span>))}
        </div>
      </div>
      <div className={`hm-months${dense}`} style={{ gridTemplateColumns: `repeat(${n}, minmax(0, 1fr))` }} aria-hidden>
        {months.map((x_, i) => <span key={x_.month} className={i === cur ? "on" : undefined}>{tick ? tick(i, i === cur) : monthName(x_.month)}</span>)}
      </div>
      {caption && <span className="text-muted text-xs min-h-[17px]" aria-hidden>{caption(at)}</span>}
    </>
  );
}

/** A horizontal bar (no track) that grows in from the left on a spring; `i` staggers it in a list. */
export function Grow({ pct, i = 0, hot }: { pct: number; i?: number; hot?: boolean }) {
  const reduce = useReducedMotion();
  return <m.span className={`grow${hot ? " hot" : ""}`} style={{ width: `${Math.max(2, Math.min(100, pct))}%`, originX: 0 }}
    initial={reduce ? false : { scaleX: 0 }} animate={{ scaleX: 1 }} transition={{ ...SPRING, delay: 0.1 + i * 0.035 }} />;
}

/** This period against the one before as two bars (no track), drawn above the delta line (Home, Insights). */
export function Versus({ now, prev, children }: { now: number; prev: number; children: ReactNode }) {
  const reduce = useReducedMotion();
  const max = Math.max(now, prev, 1);
  const bar = (v: number, cls: string, delay: number) => <m.i className={cls} style={{ width: `${Math.max(2, (v / max) * 100)}%`, originX: 0 }}
    initial={reduce ? false : { scaleX: 0 }} animate={{ scaleX: 1 }} transition={{ ...SPRING, delay }} />;
  return (
    <span className="flex flex-col gap-2.5">
      <span className="hm-vs" aria-hidden><span>Now</span>{bar(now, "now", 0.35)}<span>Before</span>{bar(prev, "", 0.42)}</span>
      {children}
    </span>
  );
}

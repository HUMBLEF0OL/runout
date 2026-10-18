// NFR-12: every DESIGN §3 text/background pair ≥ 4.5:1. Reads tokens.css so a token edit can't slip below.
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

const css = readFileSync("src/styles/tokens.css", "utf-8"); // vitest runs from the repo root

const tok = (n: string) => { const m = css.match(new RegExp(`--color-${n}:\\s*(#[0-9A-Fa-f]{6})`)); if (!m) throw new Error(`no --${n}`); return m[1]; };
const lum = (hex: string) => {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
};
const contrast = (a: string, b: string) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };

const TEXT = ["text", "text-2", "muted", "faint", "accent", "link", "link-hover", "danger"];
const BG = ["bg", "rail", "surface", "surface-2"];

describe("token contrast ≥ 4.5:1", () => {
  for (const t of TEXT) for (const b of BG) it(`${t} on ${b}`, () => expect(contrast(tok(t), tok(b))).toBeGreaterThanOrEqual(4.5));
  it("accent-ink on accent (primary button)", () => expect(contrast(tok("accent-ink"), tok("accent"))).toBeGreaterThanOrEqual(4.5));
  it("accent-ink on text (light button, selected segment)", () => expect(contrast(tok("accent-ink"), tok("text"))).toBeGreaterThanOrEqual(4.5));
});

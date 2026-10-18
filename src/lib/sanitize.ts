// Input sanitization for everything the UI sends to the core, applied once in src/api.ts so every caller
// gets it. Rust validates again at its own boundary (M3); this keeps junk out of the contract either way.

/** Max lengths per field kind (characters, after trimming). */
export const LIMITS = { name: 200, person: 100, tag: 40, text: 10_000, query: 200, link: 500, filter: 100, playlist: 100, note: 500 } as const;

// C0/C1 control characters except tab and newline (kept only where multi-line text is allowed).
const CTRL = /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F]/g;
const CTRL_LINE = /[\u0000-\u001F\u007F-\u009F]/g;
// Zero-width and bidi override characters that can disguise text.
const INVISIBLE = /[​-‏‪-‮⁠-⁤⁦-⁩﻿]/g;

/** Single-line text: NFC, no control/invisible characters, whitespace collapsed, trimmed, capped. */
export function cleanLine(s: string, max: number): string {
  return s.normalize("NFC").replace(CTRL_LINE, " ").replace(INVISIBLE, "").replace(/\s+/g, " ").trim().slice(0, max);
}
/** Multi-line text (reviews, notes): keeps newlines and tabs, normalises line endings, trims, capped. */
export function cleanText(s: string, max: number = LIMITS.text): string {
  return s.normalize("NFC").replace(/\r\n?/g, "\n").replace(CTRL, "").replace(INVISIBLE, "").replace(/\n{3,}/g, "\n\n").trim().slice(0, max);
}
/** Tags are lower-case single words or phrases, no commas (they separate tags in exports). */
export const cleanTag = (s: string) => cleanLine(s, LIMITS.tag).toLowerCase().replace(/,/g, "");
/** Nullable helpers: an empty result becomes null so "cleared" stays "cleared". */
export const lineOrNull = (s: string | null | undefined, max: number) => (s == null ? s ?? null : cleanLine(s, max) || null);
export const textOrNull = (s: string | null | undefined) => (s == null ? s ?? null : cleanText(s) || null);
export const cleanTags = (tags: string[]) => [...new Set(tags.map(cleanTag).filter(Boolean))];

const URI = /^spotify:(album|track|artist|playlist):([A-Za-z0-9]{22})$/;
/** A stored Spotify URI, only if well-formed (TSD §4.1); anything else is treated as absent. */
export function safeUri(uri: string | null | undefined): { type: string; id: string } | null {
  const m = uri?.match(URI);
  return m ? { type: m[1], id: m[2] } : null;
}

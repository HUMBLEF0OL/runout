import { describe, expect, it } from "vitest";
import { shortcutFor } from "./useShortcuts";

const k = (key: string, mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean }> = {}) => ({ key, ctrlKey: false, metaKey: false, altKey: false, ...mods });
const el = (tag: string) => document.createElement(tag);

describe("shortcutFor", () => {
  it("Ctrl+N opens quick add, even while typing", () => expect(shortcutFor(k("n", { ctrlKey: true }), el("input"))).toBe("quick-add"));
  it("Ctrl+B collapses or expands the sidebar", () => expect(shortcutFor(k("b", { ctrlKey: true }), el("div"))).toBe("toggle-rail"));
  it("Ctrl+K focuses search", () => expect(shortcutFor(k("K", { ctrlKey: true }), document.body)).toBe("search"));
  it("1–5 set status when not typing", () => {
    expect(shortcutFor(k("1"), document.body)).toEqual({ status: 1 });
    expect(shortcutFor(k("5"), el("button"))).toEqual({ status: 5 });
  });
  it("digits are ignored in inputs, textareas and selects", () => {
    for (const t of ["input", "textarea", "select"]) expect(shortcutFor(k("3"), el(t))).toBeNull();
  });
  it("digits are ignored on the rating slider", () => {
    const s = el("div"); s.setAttribute("role", "slider"); document.body.append(s);
    expect(shortcutFor(k("3"), s)).toBeNull();
  });
  it("other keys and Ctrl+digits do nothing", () => {
    expect(shortcutFor(k("6"), document.body)).toBeNull();
    expect(shortcutFor(k("1", { ctrlKey: true }), document.body)).toBeNull();
    expect(shortcutFor(k("n"), document.body)).toBeNull();
  });
});

import { describe, expect, it } from "vitest";
import { byPriority, moveTo, qaStep } from "./backlog";

describe("moveTo", () => {
  it("moves up before the target and down after it", () => {
    expect(moveTo([1, 2, 3, 4], 3, 2)).toEqual([1, 3, 2, 4]);
    expect(moveTo([1, 2, 3, 4], 2, 3)).toEqual([1, 3, 2, 4]);
    expect(moveTo([1, 2, 3, 4], 1, 4)).toEqual([2, 3, 4, 1]);
    expect(moveTo([1, 2, 3, 4], 4, 1)).toEqual([4, 1, 2, 3]);
  });
  it("unknown ids or same place change nothing", () => {
    expect(moveTo([1, 2], 9, 1)).toEqual([1, 2]);
    expect(moveTo([1, 2], 1, 1)).toEqual([1, 2]);
  });
});

describe("qaStep", () => {
  const base = { manual: false, link: "x", online: true };
  it("blank, manual and checking", () => {
    expect(qaStep({ ...base, link: " " })).toBe("blank");
    expect(qaStep({ ...base, manual: true })).toBe("manual");
    expect(qaStep(base)).toBe("checking");
  });
  it("invalid and unsupported kinds", () => {
    expect(qaStep({ ...base, parsed: { state: "invalid" } })).toBe("invalid");
    expect(qaStep({ ...base, parsed: { state: "unsupported" }, kind: "episode" })).toBe("podcast");
    expect(qaStep({ ...base, parsed: { state: "unsupported" }, kind: "playlist" })).toBe("playlist");
  });
  it("short links wait for resolve, or offer save-for-later offline", () => {
    expect(qaStep({ ...base, parsed: { state: "short" } })).toBe("checking");
    expect(qaStep({ ...base, online: false, parsed: { state: "short" } })).toBe("offline-short");
    expect(qaStep({ ...base, parsed: { state: "short" }, resolved: { uri: "spotify:album:a", type: "album" } })).toBe("form");
    expect(qaStep({ ...base, parsed: { state: "short" }, resolved: { uri: "spotify:album:a", type: "album", existing_id: 4 } })).toBe("duplicate");
  });
  it("an existing item is a duplicate, unless it is the capture being named", () => {
    const parsed = { state: "ok" as const, uri: "spotify:album:a", type: "album" as const, existing_id: 7 };
    expect(qaStep({ ...base, parsed })).toBe("duplicate");
    expect(qaStep({ ...base, parsed, self: 7 })).toBe("form");
    expect(qaStep({ ...base, parsed: { ...parsed, existing_id: undefined } })).toBe("form");
  });
});

describe("byPriority", () => {
  it("orders high, medium, low, none and keeps order within a priority", () => {
    const rows = [{ id: 1, priority: null }, { id: 2, priority: "low" }, { id: 3, priority: "high" }, { id: 4, priority: null }, { id: 5, priority: "medium" }] as const;
    expect([...rows].sort(byPriority).map((r) => r.id)).toEqual([3, 5, 2, 1, 4]);
  });
});

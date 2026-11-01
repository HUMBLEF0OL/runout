import { describe, expect, it } from "vitest";
import { hmBig, hrs } from "./format";

// M9 Home: numbers stay short for a long-time listener (years of history, thousands of hours).
describe("hmBig / hrs", () => {
  it("keeps hours and minutes below 100 h, then whole grouped hours", () => {
    expect(hmBig(48_333_975)).toBe("13h 26m");
    expect(hmBig(359_999_999)).toBe("100h"); // rounds to the minute, still under the switch
    expect(hmBig(360_000_000)).toBe("100 h");
    expect(hmBig(35_434_800_000)).toBe("9,843 h");
  });
  it("chart hours: one decimal under 10, grouped whole hours above", () => {
    expect(hrs(1.94)).toBe("1.9 h");
    expect(hrs(10)).toBe("10 h");
    expect(hrs(1284.4)).toBe("1,284 h");
  });
});

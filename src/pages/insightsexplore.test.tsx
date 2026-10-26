// M10 WI 8 Explore + Your data (design §5, mockup PNGs 7, 8): CSV (hours and plays only), hours that wrap, ready-made
// questions, the export reminder's monthly rule, and both tabs rendered on the mock.
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { ExploreResult } from "../api";
import { ToastProvider } from "../components/ui";
import { exportReminderDue } from "../components/logic";
import { Explore, bucketGaps, exploreCsv, hourSpan, questions } from "./InsightsExplore";
import { YourData } from "./InsightsData";

const res = (values: number[][], names = ["Massive Attack", "Floating Points"]): ExploreResult => ({
  buckets: ["2025", "2026"], export_only: false,
  series: values.map((v, i) => ({ kind: "artist", name: names[i], by: "", values: v, total: v[0] + v[1], peak: null, first_played_local: null })),
});

describe("Copy as CSV (plays and hours only)", () => {
  it("one row per bucket, hours then plays per series", () => {
    expect(exploreCsv(res([[41, 18.5], [0, 4.9]]), res([[700, 310], [0, 80]]))).toBe(
      "period,Massive Attack hours,Massive Attack plays,Floating Points hours,Floating Points plays\r\n2025,41,700,0,0\r\n2026,18.5,310,4.9,80\r\n");
  });
  it("quotes names with commas or quotes and defuses spreadsheet formulas", () => {
    const csv = exploreCsv(res([[1, 2], [3, 4]], ["Earth, Wind & Fire", "=HYPERLINK(\"x\")"]), res([[1, 2], [3, 4]], ["Earth, Wind & Fire", "=HYPERLINK(\"x\")"]));
    expect(csv.split("\r\n")[0]).toBe('period,"Earth, Wind & Fire hours","Earth, Wind & Fire plays","\'=HYPERLINK(""x"") hours","\'=HYPERLINK(""x"") plays"');
  });
  it("names the artist of an album or track", () => {
    const r = res([[1, 2]], ["Mezzanine"]); r.series[0].kind = "album"; r.series[0].by = "Massive Attack";
    expect(exploreCsv(r, r).split("\r\n")[0]).toBe("period,Mezzanine by Massive Attack hours,Mezzanine by Massive Attack plays");
  });
});

describe("no-data buckets (design §4.4: unknown, never silence)", () => {
  const gaps = [{ from: "2024-03-04", to: "2024-03-12" }];
  it("marks a bucket fully inside a gap, and one a gap only touches", () => {
    expect(bucketGaps(["2024-02-26", "2024-03-04", "2024-03-11", "2024-03-18"], "week", gaps, "2024-02-26", "2024-03-24")).toEqual([null, "all", "part", null]);
    expect(bucketGaps(["2024-02", "2024-03"], "month", gaps, "2024-02-01", "2024-03-31")).toEqual([null, "part"]);
    expect(bucketGaps(["2024-03"], "month", gaps, "2024-03-05", "2024-03-10")).toEqual(["all"]); // clipped to from–to
    expect(bucketGaps(["2024-02"], "month", [{ from: "2024-02-01", to: "2024-02-10" }, { from: "2024-02-11", to: "2024-02-29" }], "2024-01-01", "2024-12-31")).toEqual(["all"]);
  });
  it("leaves the CSV cells of a no-data bucket empty, not 0", () => {
    expect(exploreCsv(res([[41, 0]]), res([[700, 0]]), [null, "all"])).toBe("period,Massive Attack hours,Massive Attack plays\r\n2025,41,700\r\n2026,,\r\n");
  });
});

describe("hours of day", () => {
  it("runs forward and wraps past midnight", () => {
    expect(hourSpan(9, 12)).toEqual([9, 10, 11]);
    expect(hourSpan(22, 2)).toEqual([22, 23, 0, 1]);
    expect(hourSpan(5, 5)).toEqual([]);
  });
});

describe("ready-made questions", () => {
  it("each fills a valid builder for the named artist", () => {
    const qs = questions("Alice Coltrane", "2015-02-14", "2026-10-24");
    expect(qs.length).toBeGreaterThanOrEqual(4);
    for (const q of qs) {
      expect(q.query.series).toEqual([{ kind: "artist", name: "Alice Coltrane" }]);
      expect(q.query.from <= q.query.to).toBe(true);
      expect(q.label).toContain("Alice Coltrane");
    }
    expect(qs.find((q) => q.query.hours)?.query.hours).toEqual([22, 23, 0, 1]);
  });
});

describe("export reminder (monthly from the last export's end)", () => {
  it("is due 30 days after the export ends, once per month", () => {
    expect(exportReminderDue("2026-10-02", "2026-10-31", null)).toBeNull();
    expect(exportReminderDue("2026-10-02", "2026-11-01", null)).toBe("2026-11-01");
    expect(exportReminderDue("2026-10-02", "2026-11-20", "2026-11-01")).toBeNull();
    expect(exportReminderDue("2026-10-02", "2026-12-01", "2026-11-01")).toBe("2026-12-01");
  });
  it("before the first import, reminds on the first open and then monthly", () => {
    expect(exportReminderDue(null, "2026-12-01", null)).toBe("2026-12-01");
    expect(exportReminderDue(null, "2026-12-20", "2026-12-01")).toBeNull();
    expect(exportReminderDue(null, "2026-12-31", "2026-12-01")).toBe("2026-12-31");
  });
});

/* ------------------------------------------------------------------ rendered on the mock */
beforeAll(() => api.loadMocks());
afterEach(cleanup);
const wrap = (el: ReactNode) => render(<ToastProvider><MemoryRouter>{el}</MemoryRouter></ToastProvider>);

describe("Explore tab", () => {
  it("starts on your top artist, and a ready-made question fills the builder", async () => {
    wrap(<Explore today="2026-10-24" />);
    const chart = await screen.findByRole("slider", { name: "Hours per month" }, { timeout: 3000 });
    expect(chart.getAttribute("aria-valuetext")).toMatch(/^Oct 2026: /);
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: /late at night\?$/ })); });
    expect((screen.getByRole("combobox", { name: "Hours of day, from" }) as HTMLSelectElement).value).toBe("22");
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: /come up on shuffle\?$/ })); });
    expect(await screen.findByText(/plays on this PC are left out/)).toBeTruthy();
  });
  it("saves the view under Saved views", async () => {
    wrap(<Explore today="2026-10-24" />);
    await screen.findByRole("slider", {}, { timeout: 3000 });
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Save this view" })); });
    fireEvent.change(screen.getByRole("textbox", { name: "Name for this view" }), { target: { value: "Late nights" } });
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Save" })); });
    expect(await screen.findByRole("button", { name: "Delete saved view Late nights" })).toBeTruthy();
  });
});

describe("Your data tab", () => {
  it("shows sources and imports; the private-session switch is the logger's include_private, inverted", async () => {
    const spy = vi.spyOn(api, "setLogger");
    wrap(<YourData today="2026-10-24" />);
    expect((await screen.findByRole("img", { name: /^Coverage: Spotify export from / }, { timeout: 3000 }))).toBeTruthy();
    expect(screen.getByRole("columnheader", { name: "Already had" })).toBeTruthy();
    const sw = await screen.findByRole("switch", { name: "Leave out private-session plays" }) as HTMLInputElement;
    expect(sw.checked).toBe(true);
    expect((screen.getByRole("switch", { name: "Remind me to request a new export" }) as HTMLInputElement).checked).toBe(false);
    await act(async () => { fireEvent.click(sw); });
    expect(spy).toHaveBeenCalledWith({ include_private: true });
  });
});

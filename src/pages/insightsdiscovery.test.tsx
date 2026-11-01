// M10 WI 7 Discovery (design §5): per-month split read aloud, export-only starts, the log with "now".
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { vi } from "vitest";
import * as api from "../api";
import { ToastProvider } from "../components/ui";
import { Discovery, monthSays } from "./InsightsDiscovery";

beforeAll(() => api.loadMocks());
afterEach(cleanup);
const T = "2026-10-18";

describe("monthSays", () => {
  it("names every part of the split, and only the open part when there is one", () => {
    expect(monthSays({ month: "2026-03", stuck: 2, dropped: 3, open: 0, coverage: "all_devices" })).toBe("March 2026: 5 new artists, 2 still playing, 3 dropped");
    expect(monthSays({ month: "2026-09", stuck: 0, dropped: 0, open: 1, coverage: "all_devices" })).toBe("September 2026: 1 new artist, 0 still playing, 0 dropped, 1 too soon to tell");
    expect(monthSays({ month: "2026-01", stuck: 0, dropped: 0, open: 0, coverage: "all_devices" })).toBe("January 2026: 0 new artists");
    expect(monthSays({ month: "2019-01", stuck: 0, dropped: 0, open: 0, coverage: "no_data" })).toBe("January 2019: no data");
  });
});

describe("Discovery tab", () => {
  it("shows the four stats, the starts split, the log and a keyboard chart", async () => {
    render(<ToastProvider><MemoryRouter><Discovery q={{ grain: "month", vs: "prev", at: "2026-09" }} today={T} /></MemoryRouter></ToastProvider>);
    await screen.findByRole("region", { name: "Discovery in Sep 2026" }, { timeout: 3000 });
    expect(screen.getByText("New artists this month")).toBeTruthy();
    expect(screen.getByRole("img", { name: /^Followed on \d+%, You picked it \d+%/ })).toBeTruthy();
    expect(screen.getByRole("rowheader", { name: "Floating Points" })).toBeTruthy();
    const plot = screen.getByRole("slider", { name: /^New artists per month/ });
    fireEvent.keyDown(plot, { key: "Home" });
    expect(plot.getAttribute("aria-valuenow")).toBe("0");
    expect(plot.getAttribute("aria-valuetext")).toMatch(/^\w+ \d{4}: (\d+ new artists?|no data)/);
  });

  it("still shows the tab for a period with history but no new artists, and only says Import with no history", async () => {
    const real = await api.getDiscovery({ grain: "month", vs: "prev", at: "2026-09" });
    const none = { ...real, data: { ...real.data, new_artists: { ...real.data.new_artists, value: 0 }, log: [] } };
    const spy = vi.spyOn(api, "getDiscovery").mockResolvedValue(none);
    render(<ToastProvider><MemoryRouter><Discovery q={{ grain: "month", vs: "prev", at: "2026-09" }} today={T} /></MemoryRouter></ToastProvider>);
    await screen.findByRole("region", { name: "Discovery in Sep 2026" }, { timeout: 3000 });
    expect(screen.getByText("Still playing them 3 months on")).toBeTruthy();
    expect(screen.getByText("No new artists in this period.")).toBeTruthy();
    expect(screen.queryByRole("link", { name: "Import history" })).toBeNull();
    cleanup();
    spy.mockResolvedValue({ ...none, data: { ...none.data, artists_known: 0 } });
    render(<ToastProvider><MemoryRouter><Discovery q={{ grain: "month", vs: "prev", at: "2026-09" }} today={T} /></MemoryRouter></ToastProvider>);
    expect(await screen.findByRole("link", { name: "Import history" }, { timeout: 3000 })).toBeTruthy();
    spy.mockRestore();
  });

  it("says the log is capped when more new artists arrived than rows", async () => {
    const real = await api.getDiscovery({ grain: "all", vs: "prev", at: null });
    const spy = vi.spyOn(api, "getDiscovery").mockResolvedValue({ ...real, data: { ...real.data, new_artists: { ...real.data.new_artists, value: 4211 } } });
    render(<ToastProvider><MemoryRouter><Discovery q={{ grain: "all", vs: "prev", at: null }} today={T} /></MemoryRouter></ToastProvider>);
    expect(await screen.findByText(new RegExp(`^Your latest ${real.data.log.length} of 4,211 new artists in your history`), {}, { timeout: 3000 })).toBeTruthy();
    spy.mockRestore();
  });
});

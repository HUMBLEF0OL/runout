// M10 WI 7 How you listen (design §5): export-only banner, every card, estimates labelled, deep-dive wording, empty state.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import { ToastProvider } from "../components/ui";
import { diveTitle, diveWhen, Habits } from "./InsightsHabits";

beforeAll(() => api.loadMocks());
afterEach(cleanup);
const T = "2026-10-18";
const Q = { grain: "month" as const, vs: "prev" as const, at: "2026-09" };
const view = () => render(<ToastProvider><MemoryRouter><Habits q={Q} today={T} /></MemoryRouter></ToastProvider>);
const dive = { by: "", start_local: "2026-09-11T21:30", end_local: "2026-09-12T01:10", tracks: 41, ms: 13_200_000 };

describe("deep dive lines", () => {
  it("names the artist, the album and its passes, or the track on repeat; one middle dot per line", () => {
    expect(diveTitle({ ...dive, kind: "artist", name: "Floating Points", times: null })).toBe("Floating Points");
    expect(diveTitle({ ...dive, kind: "album", name: "Mezzanine", times: 3 })).toBe("Mezzanine, 3 times through");
    expect(diveTitle({ ...dive, kind: "album", name: "Mezzanine", times: 1 })).toBe("Mezzanine");
    expect(diveTitle({ ...dive, kind: "repeat", name: "Teardrop", times: 9 })).toBe("“Teardrop” on repeat");
    expect(diveWhen({ ...dive, kind: "artist", name: "x", times: null })).toBe("Fri 11 Sep, 21:30 to 01:10 · 41 tracks");
    expect(diveWhen({ ...dive, kind: "repeat", name: "x", times: 9 })).toBe("Fri 11 Sep · 9 plays that day");
  });
});

describe("How you listen tab", () => {
  it("says which cards are export only, labels estimates and answers the keyboard", async () => {
    view();
    await screen.findByRole("region", { name: "How you listen in Sep 2026" }, { timeout: 3000 });
    expect(screen.getByRole("note").textContent).toMatch(/cover your export plays .*: [\d,]+ plays\./);
    for (const t of ["How plays ended", "Skips by hour", "Tracks you bail on", "Shuffle", "Where you listen", "Finished tracks", "Sessions", "Deep dives"])
      expect(screen.getByRole("heading", { name: t })).toBeTruthy();
    expect(screen.getByRole("img", { name: /^Played to the end \d+%, Skipped forward \d+%/ })).toBeTruthy();
    expect(screen.getAllByText(/^An estimate/).length).toBe(2); // sessions, deep dives
    expect(screen.getByText("Mezzanine, 3 times through")).toBeTruthy();
    const hours = screen.getByRole("slider", { name: "Share of plays skipped by hour of day" });
    fireEvent.keyDown(hours, { key: "Home" });
    expect(hours.getAttribute("aria-valuetext")).toMatch(/^00:00: \d+% of plays skipped$/);
    expect(screen.getByRole("slider", { name: "Sessions by length" }).getAttribute("aria-valuetext")).toMatch(/: [\d,]+ sessions?$/);
  });

  it("keeps desktop cards when the period has no export plays, and asks for an import only with nothing at all", async () => {
    const real = await api.getListeningHabits(Q);
    const noExport = { ...real, data: { ...real.data, export_plays: 0, ends: [], skips_by_hour: [], quick_bails: [], most_skipped: null } };
    const spy = vi.spyOn(api, "getListeningHabits").mockResolvedValue(noExport);
    view();
    await screen.findByRole("region", { name: "How you listen in Sep 2026" }, { timeout: 3000 });
    expect(screen.getAllByText(/^No Spotify export plays in this period/).length).toBe(3);
    expect(screen.getByRole("heading", { name: "Sessions" })).toBeTruthy();
    cleanup();
    spy.mockResolvedValue({ ...noExport, data: { ...noExport.data, sessions: { ...noExport.data.sessions, count: 0 } } });
    view();
    expect(await screen.findByRole("link", { name: "Import history" }, { timeout: 3000 })).toBeTruthy();
    spy.mockRestore();
  });

  it("names only the export on export-only cards, says no skips when none, and rounds before comparing shuffle", async () => {
    const real = await api.getListeningHabits(Q);
    const spy = vi.spyOn(api, "getListeningHabits").mockResolvedValue({ ...real, sources: ["S", "E"],
      data: { ...real.data, skips_by_hour: Array(24).fill(0), shuffle: { pct: 46, by_year: [{ year: 2025, pct: 30 }, { year: 2026, pct: 46.4 }] } } });
    view();
    await screen.findByRole("region", { name: "How you listen in Sep 2026" }, { timeout: 3000 });
    const tag = (t: string) => screen.getByRole("heading", { name: t }).parentElement!.textContent!;
    for (const t of ["How plays ended", "Skips by hour", "Tracks you bail on", "Finished tracks"]) {
      expect(tag(t)).toMatch(/From: Spotify export/);
      expect(tag(t)).not.toMatch(/Desktop log/);
    }
    expect(tag("Where you listen")).toMatch(/Desktop log/);
    expect(tag("Skips by hour")).toMatch(/No skips in this period\./);
    expect(tag("Shuffle")).not.toMatch(/your most was/);
    spy.mockRestore();
  });
});

// M10 WI 7 Records (design §5, mockup PNG 5): the eight all-time records, on this day, forgotten favourites with Play again.
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import { ToastProvider } from "../components/ui";
import { milestoneSoFar, onThisDayLine, Records } from "./InsightsRecords";

beforeAll(() => api.loadMocks());
afterEach(cleanup);
const T = "2026-10-24";
const otd = (o: Partial<api.Records["on_this_day"][number]>) => ({ year: 2020, plays: 120, ms: 25_320_000, top: null, first_artists: [], ...o });

describe("onThisDayLine", () => {
  it("leads with a first play, then the day's top, then the time", () => {
    expect(onThisDayLine(otd({ first_artists: ["Air"], top: { kind: "artist", name: "Kate Bush", by: "" } }))).toEqual(["First play of Air", "120 plays, 7h 2m"]);
    expect(onThisDayLine(otd({ first_artists: ["Air", "Bonobo", "Burial"] }))[0]).toBe("First play of Air and 2 others");
    expect(onThisDayLine(otd({ top: { kind: "album", name: "Mezzanine", by: "Massive Attack" } }))[0]).toBe("Mostly Mezzanine by Massive Attack");
    expect(onThisDayLine(otd({ top: { kind: "artist", name: "Kate Bush", by: "" } }))[0]).toBe("Mostly Kate Bush");
    expect(onThisDayLine(otd({}))[0]).toBe("7h 2m of music");
    expect(onThisDayLine(otd({ plays: 0, ms: 0 }))).toEqual(["No music that day", "A day off"]);
  });
});

describe("milestoneSoFar", () => {
  it("is the share of the way from the last multiple of 5,000 to the next", () => {
    expect(milestoneSoFar(183_750, 185_000)).toBe(75);
    expect(milestoneSoFar(0, 5_000)).toBe(0);
    expect(milestoneSoFar(5_000, 10_000)).toBe(0);
  });
});

describe("Records tab", () => {
  it("shows the eight records, labels the estimate, and links on-this-day dates to Recent plays", async () => {
    render(<ToastProvider><MemoryRouter><Records today={T} /></MemoryRouter></ToastProvider>);
    const recs = await screen.findByRole("region", { name: "All-time records" }, { timeout: 3000 });
    for (const k of ["First play in your history", "Most played track ever", "Longest streak", "Biggest year", "Biggest day", "Most loyal", "Next milestone", "Most artists in a month"])
      expect(recs.textContent).toContain(k);
    expect(recs.textContent).toMatch(/an estimate/);
    expect(screen.getByText("Teardrop")).toBeTruthy();
    const days = screen.getAllByRole("link", { name: /^Plays on 24 Oct \d{4}/ });
    expect(days.length).toBeGreaterThan(0);
    expect(days[0].getAttribute("href")).toMatch(/^\/plays\/all\?date=\d{4}-10-24$/);
  });

  it("Play again opens Spotify for a forgotten favourite, and says so when there are none", async () => {
    const real = await api.getRecords();
    const fav = { name: "Bon Iver", item_id: null, top10_year: 2017, last_played_local: "2022-03-02T08:41", play_uri: "spotify:track:0000000000000000000001" };
    const spy = vi.spyOn(api, "getRecords").mockResolvedValue({ ...real, data: { ...real.data, forgotten: [fav, { ...fav, name: "Air", play_uri: null }] } });
    render(<ToastProvider><MemoryRouter><Records today={T} /></MemoryRouter></ToastProvider>);
    const play = await screen.findByRole("link", { name: "Play Bon Iver again in Spotify" }, { timeout: 3000 });
    expect(play.getAttribute("href")).toBe("https://open.spotify.com/track/0000000000000000000001");
    expect(screen.getAllByText("top 10 in 2017 · last played Mar 2022")).toHaveLength(2);
    expect(screen.queryByRole("link", { name: "Play Air again in Spotify" })).toBeNull();
    cleanup();
    spy.mockResolvedValue({ ...real, data: { ...real.data, forgotten: [] } });
    render(<ToastProvider><MemoryRouter><Records today={T} /></MemoryRouter></ToastProvider>);
    expect(await screen.findByText(/No forgotten favourites/, {}, { timeout: 3000 })).toBeTruthy();
    cleanup();
    spy.mockResolvedValue({ ...real, data: { ...real.data, plays_total: 0 } });
    render(<ToastProvider><MemoryRouter><Records today={T} /></MemoryRouter></ToastProvider>);
    expect(await screen.findByRole("link", { name: "Import history" }, { timeout: 3000 })).toBeTruthy();
    spy.mockRestore();
  });
});

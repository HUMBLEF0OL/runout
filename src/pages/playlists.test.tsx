// Studio (SC-11) write paths: keep/cut edits survive unrelated refetches and failed applies that wrote nothing,
// and each failure shows its own banner (audit M5 r5).
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useNavigate } from "react-router-dom";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import * as api from "../api";
import { ToastProvider } from "../components/ui";
import { setDev } from "../mocks/devState";
import Playlists from "./Playlists";

let names: string[] = [];
let uris: string[] = [];
beforeAll(async () => {
  await api.loadMocks();
  const pls = await api.listSpotifyPlaylists();
  names = pls.map((p) => p.name);
  uris = pls.map((p) => p.uri);
});
afterEach(() => { cleanup(); setDev({ view: "normal" }); });

/** Render Studio on the n-th playlist; returns its first row's keep/cut group. */
async function studio(n: number) {
  render(<ToastProvider><MemoryRouter initialEntries={[`/playlists/studio?p=${encodeURIComponent(uris[n])}`]}>
    <Routes><Route path="/playlists/:tab" element={<Playlists />} /></Routes></MemoryRouter></ToastProvider>);
  const groups = await screen.findAllByRole("radiogroup", { name: /^Keep or cut / }, { timeout: 3000 });
  return groups[0];
}
const isCut = (g: HTMLElement) => within(g).getByRole("radio", { name: "Cut" }).getAttribute("aria-checked") === "true";
async function cutFirst() {
  const g = await studio(cutFirst.n++);
  fireEvent.click(within(g).getByRole("radio", { name: "Cut" }));
  expect(isCut(g)).toBe(true);
  return g;
}
cutFirst.n = 0;
async function apply() {
  fireEvent.click(screen.getByRole("button", { name: /^Apply \d+ changes? to Spotify$/ }));
  const dlg = await screen.findByRole("dialog", { hidden: true });
  fireEvent.click(within(dlg).getByRole("button", { name: /^Apply \d+ changes?$/, hidden: true }));
}
const alert = (re: RegExp) => screen.findByText(re, {}, { timeout: 3000 });
const settle = () => act(() => new Promise((r) => setTimeout(r, 400)));

describe("Studio keeps the user's draft", () => {
  it("a write elsewhere in the app (global invalidate) does not refetch the draft or reset keep/cut", async () => {
    const g = await cutFirst();
    act(() => api.invalidate());
    await settle();
    expect(isCut(screen.getAllByRole("radiogroup", { name: /^Keep or cut / })[0])).toBe(true);
    expect(g.isConnected).toBe(true);
  });
  it("apply answered spotify_busy: wait copy with retry_after_s, draft and choices kept", async () => {
    await cutFirst();
    setDev({ view: "busy" });
    await apply();
    await alert(/Spotify is still finishing a change; try again in 90 s\./);
    expect(isCut(screen.getAllByRole("radiogroup", { name: /^Keep or cut / })[0])).toBe(true);
    expect(screen.queryByRole("button", { name: "Undo last apply" })).toBeNull();
  });
  it("apply answered conflict: nothing reloads until asked, choices kept", async () => {
    await cutFirst();
    setDev({ view: "conflict" });
    await apply();
    await alert(/This playlist changed on Spotify\./);
    expect(isCut(screen.getAllByRole("radiogroup", { name: /^Keep or cut / })[0])).toBe(true);
  });
  it("apply that may have landed: partial warning and Undo banner both stay visible after the reload", async () => {
    await cutFirst();
    setDev({ view: "partial" });
    await apply();
    await alert(/may have been partly applied/);
    await screen.findByRole("button", { name: "Undo last apply" });
    setDev({ view: "error" }); // any later refetch would now fail and could replace the page
    act(() => api.invalidate());
    await settle();
    expect(screen.getByText(/may have been partly applied/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Undo last apply" })).toBeTruthy();
  });
});

describe("Studio undo banners", () => {
  async function applied() {
    await cutFirst();
    await apply();
    return screen.findByRole("button", { name: "Undo last apply" }, { timeout: 3000 });
  }
  it("undo answered spotify_busy: wait copy, not 'partly restored', and the undo stays", async () => {
    const b = await applied();
    setDev({ view: "busy" });
    fireEvent.click(b);
    await alert(/Try Undo again in 90 s\./);
    expect(screen.queryByText(/partly restored/)).toBeNull();
    expect(screen.getByRole("button", { name: "Undo last apply" })).toBeTruthy();
  });
  it("undo pressed twice: the second gets not_found with its own copy", async () => {
    const b = await applied();
    fireEvent.click(b); fireEvent.click(b);
    await alert(/There's nothing to undo\./);
    expect(screen.queryByText(/partly restored/)).toBeNull();
  });
});

describe("Studio selection", () => {
  it("links and selects playlists by uri (names are not unique on Spotify)", async () => {
    await studio(1);
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(names[1]);
    const link = within(screen.getByRole("navigation", { name: "Playlists" })).getByRole("link", { name: new RegExp(names[1]) });
    expect(link.getAttribute("href")).toContain(encodeURIComponent(uris[1]));
  });
  it("switching playlist never shows the previous playlist's draft under the new name", async () => {
    await studio(0);
    fireEvent.click(within(screen.getByRole("navigation", { name: "Playlists" })).getByRole("link", { name: new RegExp(names[1]) }));
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(names[1]);
    expect(screen.queryAllByRole("radiogroup", { name: /^Keep or cut / })).toHaveLength(0);
    await screen.findAllByRole("radiogroup", { name: /^Keep or cut / }, { timeout: 3000 });
  });
});

describe("Studio: Check other playlists (G-24)", () => {
  it("marks each track the other playlists also hold, naming them", async () => {
    let n = -1; let want: api.CrossDupe[] = [];
    for (const [i, u] of uris.entries()) { want = await api.findCrossPlaylistDupes(u); if (want.length) { n = i; break; } }
    expect(n).toBeGreaterThanOrEqual(0); // the mock dataset shares tracks across playlists
    await studio(n);
    expect(screen.queryByText(/also in /)).toBeNull(); // only on request
    fireEvent.click(screen.getByRole("button", { name: "Check other playlists" }));
    await screen.findByText(/also in other playlists, marked in the list\./, {}, { timeout: 3000 });
    expect(screen.getAllByText(new RegExp(`also in .*${want[0].playlists[0].name}`)).length).toBeGreaterThan(0);
  });
  it("a failed check keeps the draft and offers a retry (offline: the whole Studio shows its connection state)", async () => {
    await studio(0);
    setDev({ view: "error" });
    fireEvent.click(screen.getByRole("button", { name: "Check other playlists" }));
    await alert(/Couldn't check your other playlists\./);
    expect(screen.getAllByRole("radiogroup", { name: /^Keep or cut / }).length).toBeGreaterThan(0);
  });
});

describe("Mode switch keyboard", () => {
  // M7 installed run: Check-up and Studio are separate pages, so arrowing the switch remounted it and focus fell to <body>.
  it("focus stays on the Mode switch after ArrowRight changes the page", async () => {
    render(<ToastProvider><MemoryRouter initialEntries={["/playlists/checkup"]}>
      <Routes><Route path="/playlists/:tab" element={<Playlists />} /></Routes></MemoryRouter></ToastProvider>);
    const checkup = await screen.findByRole("tab", { name: /^Check-up/ }, { timeout: 3000 });
    checkup.focus();
    fireEvent.keyDown(checkup, { key: "ArrowRight" });
    const studioTab = await screen.findByRole("tab", { name: "Studio (live from Spotify)", selected: true }, { timeout: 3000 });
    await act(async () => { await new Promise((r) => setTimeout(r, 50)); });
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "Studio (live from Spotify)", selected: true }));
    expect(studioTab).toBeTruthy();
  });

  it("Back to an entry that carried the flag doesn't move focus", async () => {
    render(<ToastProvider><MemoryRouter initialEntries={["/playlists/checkup", { pathname: "/playlists/studio", state: { modeFocus: true } }, "/elsewhere"]} initialIndex={2}>
      <Routes><Route path="/playlists/:tab" element={<Playlists />} /><Route path="/elsewhere" element={<BackButton />} /></Routes></MemoryRouter></ToastProvider>);
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    await screen.findByRole("tab", { name: "Studio (live from Spotify)", selected: true }, { timeout: 3000 });
    await act(async () => { await new Promise((r) => setTimeout(r, 50)); });
    expect(document.activeElement?.getAttribute("role")).not.toBe("tab");
  });
});

function BackButton() { const nav = useNavigate(); return <button onClick={() => nav(-1)}>Back</button>; }

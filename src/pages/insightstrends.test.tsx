// M10 WI 7 Trends (design §5): the lifetime brush sets a custom Report period; a calendar day opens Recent plays for it.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import * as api from "../api";
import { ToastProvider } from "../components/ui";
import { brushPeriod, calendarDay, calendarLead, Trends } from "./InsightsTrends";

beforeAll(() => api.loadMocks());
afterEach(cleanup);
const T = "2026-10-18";

describe("brushPeriod", () => {
  it("turns a month range (either direction) into a custom period, whole months", () => {
    expect(brushPeriod("2026-08", "2026-06", T)).toEqual({ grain: "custom", vs: "prev", at: null, from: "2026-06-01", to: "2026-08-31" });
    expect(brushPeriod("2024-02", "2024-02", T).to).toBe("2024-02-29");
  });
  it("never reaches past today", () => {
    expect(brushPeriod("2026-09", "2026-10", T)).toMatchObject({ from: "2026-09-01", to: T });
  });
});

describe("calendar", () => {
  it("index 0 is 1 Jan; the first column starts on Monday", () => {
    expect(calendarDay(2024, 59)).toBe("2024-02-29");
    expect(calendarDay(2026, 364)).toBe("2026-12-31");
    expect(calendarLead(2026)).toBe(3); // 1 Jan 2026 is a Thursday
    expect(calendarLead(2024)).toBe(0); // a Monday
  });
});

const Where = () => <output data-testid="at">{useLocation().pathname + useLocation().search}</output>;
const show = () => render(<ToastProvider><MemoryRouter initialEntries={["/insights/trends"]}>
  <Routes><Route path="/insights/trends" element={<Trends today={T} />} /><Route path="*" element={<Where />} /></Routes></MemoryRouter></ToastProvider>);

describe("Trends links", () => {
  it("Shift+arrow keys pick a stretch; the Report link carries it as a custom period", async () => {
    show();
    const plot = await screen.findByRole("slider", { name: /per month since/ }, { timeout: 3000 });
    fireEvent.keyDown(plot, { key: "ArrowLeft", shiftKey: true });
    fireEvent.keyDown(plot, { key: "ArrowLeft", shiftKey: true });
    const link = screen.getByRole("link", { name: /^Report on Aug 2026 to Oct 2026/ });
    expect(link.getAttribute("href")).toBe("/insights/report?grain=custom&from=2026-08-01&to=2026-10-18&vs=prev");
  });
  it("a calendar day with plays opens Recent plays for that date", async () => {
    show();
    const d = await screen.findByRole("button", { name: /^Thu 1 Oct 2026: / }, { timeout: 3000 });
    fireEvent.click(d);
    expect(screen.getByTestId("at").textContent).toBe("/plays/all?date=2026-10-01");
  });
});

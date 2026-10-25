// Report past the export's end (FR-048, API.md §4 coverage, design §4.4): this-PC-only figures say so, and a
// comparison pairs like-for-like numbers. Stepper stops at PeriodInfo.prev_at / next_at.
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import * as api from "../api";
import { ToastProvider } from "../components/ui";
import { PeriodHeader, Report, shown } from "./InsightsReport";

beforeAll(() => api.loadMocks());
afterEach(cleanup);
const T = "2026-10-18";
const q: api.PeriodQuery = { grain: "month", vs: "prev", at: "2026-10" }; // mocks: desktop_only_since 2026-10-03
const show = () => render(<ToastProvider><MemoryRouter><Report q={q} today={T} /></MemoryRouter></ToastProvider>);

describe("Report while only this PC is counted", () => {
  it("labels the fingerprint and the timeline headline 'on this PC'", async () => {
    show();
    expect(await screen.findByText("Oct 2026, on this PC", {}, { timeout: 3000 })).toBeTruthy();
    expect(screen.getByRole("img", { name: /^Listening fingerprint for Oct 2026, on this PC,/ })).toBeTruthy();
    expect(screen.getByText(/^listened on this PC, busiest/)).toBeTruthy();
  });

  it("a ratio beside a 'before' shows the like-for-like now, not value", () => {
    expect(shown({ value: 3.4, now: 3.1, before: 2.9 })).toBe(3.1);
    expect(shown({ value: 3.4, now: null, before: null })).toBe(3.4);
  });
});

describe("Period stepper bounds", () => {
  it("disables ‹ when there is no earlier period and › when there is no later one", () => {
    render(<PeriodHeader q={{ grain: "month", vs: "prev", at: "2025-03" }} today={T} bounds={{ prev_at: null, next_at: null }} onChange={() => {}} />);
    expect(screen.getByRole("button", { name: "Previous month" }).getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByRole("button", { name: "Next month" }).getAttribute("aria-disabled")).toBe("true");
  });
});

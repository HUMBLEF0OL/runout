// Home's period strip (FR-048, M10 design §4.4): a delta past the export's end says it counts this PC only.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import * as api from "../api";
import { ToastProvider } from "../components/ui";
import HomeFull from "./HomeFull";

beforeAll(() => api.loadMocks());
afterEach(cleanup);

describe("Home period strip coverage note", () => {
  it("30 days crosses the export's end: note and 'on this PC' delta", async () => {
    render(<ToastProvider><MemoryRouter><HomeFull /></MemoryRouter></ToastProvider>);
    fireEvent.click(await screen.findByRole("tab", { name: "30 days" }, { timeout: 3000 }));
    expect(await screen.findByText(/^From 3 Oct, only plays on this PC are counted/, {}, { timeout: 3000 })).toBeTruthy();
    expect(screen.getAllByText(/on this PC than the period before|Same on this PC/).length).toBe(2);
  });

  it("12 months has no like-for-like source: the reason replaces the delta", async () => {
    render(<ToastProvider><MemoryRouter><HomeFull /></MemoryRouter></ToastProvider>);
    fireEvent.click(await screen.findByRole("tab", { name: "12 months" }, { timeout: 3000 }));
    expect(await screen.findByText(/only logged from/, {}, { timeout: 3000 })).toBeTruthy();
    expect(screen.queryByText(/than the period before/)).toBeNull();
  });
});

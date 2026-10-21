import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { Sourced } from "../api";
import { InsightCard } from "./ui";

// M7 plan step 1 proof (FR-032): every insight card shows the sources and freshness of the response it renders.
const res = (over: Partial<Sourced<null>> = {}): Sourced<null> => ({
  data: null, sources: ["U", "S", "E"],
  freshness: { logger_last_event: "2026-10-20T12:00:00Z", export_latest_event: "2026-10-12T12:00:00Z" }, ...over,
});

describe("InsightCard", () => {
  afterEach(cleanup);
  it("renders the response's sources and each listening source's date", () => {
    render(<InsightCard title="Top artists" res={res()}>x</InsightCard>);
    expect(screen.getByText("From: Your records + Desktop log (to 20 Oct 2026) + Spotify export (to 12 Oct 2026).")).toBeTruthy();
  });
  it("says when a listening source has no data yet", () => {
    render(<InsightCard title="t" res={res({ sources: ["S"], freshness: { logger_last_event: null, export_latest_event: null } })}>x</InsightCard>);
    expect(screen.getByText("From: Desktop log (none yet).")).toBeTruthy();
  });
  it("a response lacking sources or freshness never passes as sourced", () => {
    const { rerender } = render(<InsightCard title="t" res={{ data: null } as unknown as Sourced<null>}>x</InsightCard>);
    expect(screen.getByText("Sources unknown.")).toBeTruthy();
    rerender(<InsightCard title="t" res={{ data: null, sources: ["S"] } as unknown as Sourced<null>}>x</InsightCard>);
    expect(screen.getByText("Sources unknown.")).toBeTruthy();
    expect(screen.queryByText(/^From:/)).toBeNull();
  });
});

import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Rating } from "./ui";

// M7 installed run (21 Oct 2026): on Rust each save refetches, so keys read from the saved value dropped presses
// (8 × ArrowRight from unrated ended at 1 star). Keys move a local value; one save after the burst.
afterEach(() => { cleanup(); vi.useRealTimers(); });

describe("Rating keyboard", () => {
  it("a fast burst of arrows saves once, with the final value", async () => {
    vi.useFakeTimers();
    const onChange = vi.fn(() => new Promise<void>((r) => setTimeout(r, 1000))); // slow save
    render(<Rating value={null} onChange={onChange} />);
    const s = screen.getByRole("slider");
    for (let i = 0; i < 8; i++) fireEvent.keyDown(s, { key: "ArrowRight" });
    expect(s.getAttribute("aria-valuenow")).toBe("4");
    await act(async () => { vi.advanceTimersByTime(400); });
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith(4);
    expect(s.getAttribute("aria-valuenow")).toBe("4"); // still the draft while the save runs
  });

  it("returning to the saved value saves nothing", async () => {
    vi.useFakeTimers();
    const onChange = vi.fn();
    render(<Rating value={3} onChange={onChange} />);
    const s = screen.getByRole("slider");
    fireEvent.keyDown(s, { key: "ArrowRight" });
    fireEvent.keyDown(s, { key: "ArrowLeft" });
    await act(async () => { vi.advanceTimersByTime(400); });
    expect(onChange).not.toHaveBeenCalled();
  });
});

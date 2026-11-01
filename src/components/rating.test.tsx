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
    expect(onChange).toHaveBeenCalledWith(4, null);
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

  it("leaving within the debounce still saves the keyed value", () => {
    vi.useFakeTimers();
    const onChange = vi.fn();
    const { unmount } = render(<Rating value={2} onChange={onChange} />);
    const s = screen.getByRole("slider");
    fireEvent.keyDown(s, { key: "ArrowRight" });
    fireEvent.keyDown(s, { key: "ArrowRight" });
    unmount();
    expect(onChange).toHaveBeenCalledWith(3, 2);
  });

  it("a click after arrows wins and cancels the pending keys", async () => {
    vi.useFakeTimers();
    const onChange = vi.fn();
    render(<Rating value={2} onChange={onChange} />);
    const s = screen.getByRole("slider");
    fireEvent.keyDown(s, { key: "ArrowRight" });
    fireEvent.keyDown(s, { key: "ArrowRight" });
    fireEvent.click(s.querySelectorAll("button")[4], { clientX: 9999 }); // right half of the 5th star
    await act(async () => { vi.advanceTimersByTime(1000); });
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith(5, 2);
  });

  it("keeps the draft until the saved value arrives, and reverts on a failed save", async () => {
    vi.useFakeTimers();
    let ok = true;
    const onChange = vi.fn(async () => ok);
    const { rerender } = render(<Rating value={2} onChange={onChange} />);
    const s = screen.getByRole("slider");
    fireEvent.keyDown(s, { key: "ArrowRight" });
    await act(async () => { vi.advanceTimersByTime(400); });
    expect(s.getAttribute("aria-valuenow")).toBe("2.5"); // saved, refetch not back yet: no flash of the old value
    rerender(<Rating value={2.5} onChange={onChange} />);
    expect(s.getAttribute("aria-valuenow")).toBe("2.5");
    ok = false;
    fireEvent.keyDown(s, { key: "ArrowRight" });
    await act(async () => { vi.advanceTimersByTime(400); });
    expect(s.getAttribute("aria-valuenow")).toBe("2.5"); // failed: back to the saved value
  });

  it("a failed save doesn't drop arrows still pending, and each save names the value it replaces", async () => {
    vi.useFakeTimers();
    let reject!: () => void;
    const onChange = vi.fn((_v: number | null, _prev: number | null) => new Promise((_, j) => { reject = () => j(new Error("x")); }));
    render(<Rating value={2} onChange={onChange} />);
    const s = screen.getByRole("slider");
    fireEvent.keyDown(s, { key: "ArrowRight" });
    await act(async () => { vi.advanceTimersByTime(400); }); // 2.5 sent, still in flight
    fireEvent.keyDown(s, { key: "ArrowRight" }); // 3 pending
    await act(async () => { reject(); });
    expect(s.getAttribute("aria-valuenow")).toBe("3");
    expect(onChange).toHaveBeenLastCalledWith(2.5, 2);
    await act(async () => { vi.advanceTimersByTime(400); });
    expect(onChange).toHaveBeenLastCalledWith(3, 2); // the 2.5 save failed: 3 replaces the saved 2
  });
});

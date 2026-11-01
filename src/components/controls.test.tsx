import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { CountUp, Switch } from "./ui";

afterEach(cleanup);

// M9: switches (Settings, Item, Rediscover) flip before the write lands; a failed write (useRun resolves undefined) flips them back.
describe("Settings switch", () => {
  it("flips at once and stays on when the write succeeds", async () => {
    render(<Switch label="Back up weekly" on={false} onChange={() => Promise.resolve({})} />);
    const s = screen.getByRole("switch", { name: "Back up weekly" }) as HTMLInputElement;
    await act(async () => { fireEvent.click(s); });
    expect(s.checked).toBe(true);
  });

  it("flips back when the write fails", async () => {
    let done: (v: unknown) => void = () => {};
    render(<Switch label="Back up weekly" on={false} onChange={() => new Promise((r) => { done = r; })} />);
    const s = screen.getByRole("switch") as HTMLInputElement;
    fireEvent.click(s);
    expect(s.checked).toBe(true); // optimistic
    await act(async () => { done(undefined); });
    expect(s.checked).toBe(false);
  });

  it("a late failure of an older write doesn't undo a newer click", async () => {
    const done: ((v: unknown) => void)[] = [];
    render(<Switch label="Back up weekly" on={false} onChange={() => new Promise((r) => { done.push(r); })} />);
    const s = screen.getByRole("switch") as HTMLInputElement;
    fireEvent.click(s); fireEvent.click(s); fireEvent.click(s); // on, off, on: three writes in flight
    await act(async () => { done[2]({}); });
    await act(async () => { done[0](undefined); }); // the first write fails last
    expect(s.checked).toBe(true);
  });
});

describe("CountUp", () => {
  it("shows the final text when it can't animate (no IntersectionObserver)", () => {
    const { container } = render(<CountUp text="62%" />);
    expect(container.querySelector("[aria-hidden]")?.textContent).toBe("62%");
    expect(container.querySelector(".sr-only")?.textContent).toBe("62%");
  });
});

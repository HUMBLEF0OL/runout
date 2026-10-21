import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import * as api from "../api";
import { ConnectDialog } from "./SetupDialogs";

// M7 installed run (21 Oct 2026): a missing RUNOUT_SPOTIFY_CLIENT_ID showed "needs an active Premium plan", which
// sent the user (on Premium) the wrong way. Only a Spotify refusal gets the Premium copy; anything else says why.
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

async function failWith(e: api.AppError) {
  vi.spyOn(api, "getInbox").mockResolvedValue({ state: "none", playlist_uri: null, last_check: null, imported_last: 0, saved_albums: false, followed_artists: false, needs_reconnect: false });
  vi.spyOn(api, "connectSpotify").mockRejectedValue(e);
  render(<ConnectDialog onClose={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Open Spotify sign-in", hidden: true }));
  return (await screen.findByRole("alert", { hidden: true })).textContent ?? "";
}

describe("Connect dialog failure copy", () => {
  it("a missing client ID shows Rust's message, not the Premium hint", async () => {
    const t = await failWith({ code: "spotify_auth", message: "Spotify isn't set up: set RUNOUT_SPOTIFY_CLIENT_ID to your Spotify developer app's client ID, then restart Runout." });
    expect(t).toContain("RUNOUT_SPOTIFY_CLIENT_ID");
    expect(t).not.toContain("Premium");
  });
  it("a Spotify refusal shows the Premium / User Management hint", async () => {
    const t = await failWith({ code: "spotify_forbidden", message: "Forbidden" });
    expect(t).toContain("Premium");
  });
});

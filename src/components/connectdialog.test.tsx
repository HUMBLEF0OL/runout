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
  it("a missing client ID asks for it, not the Premium hint, and sends the pasted one", async () => {
    const t = await failWith({ code: "spotify_auth", message: "Spotify isn't set up yet: paste your Spotify developer app's Client ID.", detail: "client_id" });
    expect(t).toContain("Client ID");
    expect(t).not.toContain("Premium");
    fireEvent.change(screen.getByLabelText("Client ID"), { target: { value: "Abc123" } });
    fireEvent.click(screen.getByRole("button", { name: "Save and sign in", hidden: true }));
    expect(api.connectSpotify).toHaveBeenLastCalledWith("Abc123");
  });
  it("a Spotify refusal shows the Premium / User Management hint", async () => {
    const t = await failWith({ code: "spotify_forbidden", message: "Forbidden" });
    expect(t).toContain("Premium");
  });
  it("a retry that just doesn't finish drops the previous attempt's reason", async () => {
    await failWith({ code: "spotify_forbidden", message: "Forbidden" });
    vi.mocked(api.connectSpotify).mockResolvedValue({ state: "disconnected", display_name: null, renew_by: null });
    fireEvent.click(screen.getByRole("button", { name: "Try again", hidden: true }));
    const t = (await screen.findByRole("alert", { hidden: true })).textContent ?? "";
    expect(t).not.toContain("Premium");
  });
});

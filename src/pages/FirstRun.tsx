// SC-02b First run: shown at #/first-run and by Home while the library is empty.
import { useState } from "react";
import { Link } from "react-router-dom";
import { Topbar } from "../components/Shell";
import { useRelease } from "../hooks/useRelease";
import { ConnectDialog, ShortcutDialog } from "../components/SetupDialogs";

const later = <span className="text-faint text-xs">Available in a later release</span>;

export default function FirstRun() {
  const { r2, r3 } = useRelease();
  const [dlg, setDlg] = useState<"shortcut" | "connect" | null>(null);
  return (
    <div className="flex flex-col mx-auto my-0 max-w-[760px] w-full pt-18 gap-7">
      <span className="logo p-0 text-2xl">Runout</span>
      <div className="flex flex-col gap-2">
        <h1 className="text-[34px] font-extrabold tracking-[-0.025em]">Keep the music you mean to come back to.</h1>
        <span className="text-text-2 text-[16px]">Paste a Spotify link for an album, track or artist. It goes into your backlog.</span>
      </div>
      <Topbar />
      <div className="tiles grid-cols-[repeat(auto-fit,minmax(200px,1fr))]">
        <div className="card flex flex-col gap-3"><b>Share from your phone</b>
          <span className="text-text-2 text-sm">Set up the iPhone Shortcut so songs you hear on the go land here.</span>
          <button className="btn btn-light self-start" onClick={() => setDlg("shortcut")}>Set it up</button></div>
        <div className="card flex flex-col gap-3"><b>Bring your history</b>
          <span className="text-text-2 text-sm">Import your Spotify listening history to see what you actually play.</span>
          {r2 ? <Link to="/import/1" className="btn btn-light self-start">Import history</Link> : later}</div>
        <div className="card flex flex-col gap-3"><b>Clean up playlists</b>
          <span className="text-text-2 text-sm">Connect Spotify to tidy your own playlists with your ratings.</span>
          {r3 ? <button className="btn btn-light self-start" onClick={() => setDlg("connect")}>Connect Spotify</button> : later}</div>
      </div>
      <span className="text-faint text-xs">Everything stays on this PC. <span className="kbd">Ctrl+N</span> adds a link from anywhere.</span>
      {dlg === "shortcut" && <ShortcutDialog onClose={() => setDlg(null)} />}
      {dlg === "connect" && <ConnectDialog onClose={() => setDlg(null)} />}
    </div>
  );
}

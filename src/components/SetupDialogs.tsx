// Phone-capture and Spotify-connect dialogs, opened from Settings (SC-13) and First run (SC-02b).
import { useEffect, useState } from "react";
import { connectSpotify, getCapture, getInbox, onEvent, toAppError, type AppError } from "../api";
import { useApi } from "../hooks/useApi";
import { Attribution, Dialog, Dots } from "./ui";

/* ------------------------------------------------------------------ SC-13e Shortcut steps */
export function ShortcutDialog({ onClose }: { onClose: () => void }) {
  const cap = useApi(getCapture).data;
  const [got, setGot] = useState(false);
  useEffect(() => onEvent("capture-received", () => setGot(true)), []);
  const addr = cap ? `${cap.address}:${cap.port}` : "…";
  return (
    <Dialog title="Set up the iPhone Shortcut" wide onClose={onClose} footer={<button className="btn btn-secondary" onClick={onClose}>Done</button>}>
      <ol className="m-0 pl-[18px] list-decimal flex flex-col gap-2.5 leading-[1.5]">
        <li>On your iPhone, open <b>Shortcuts</b> and create a new shortcut named <b>Save to Runout</b>. Turn on <b>Show in Share Sheet</b> and accept <b>URLs</b>.</li>
        <li>Add <b>Get Contents of URL</b>: URL <span className="font-mono">http://{addr}/capture</span>, method <b>POST</b>, body <b>JSON</b> with <span className="font-mono">code</span> = <span className="font-mono">{cap?.code ?? "…"}</span> and <span className="font-mono">link</span> = Shortcut Input.</li>
        <li>Add <b>Show Result</b>.</li>
        <li>Test it: in Spotify, tap Share, then Save to Runout, on the same Wi-Fi as this PC. Allow <b>Local Network</b> when iOS asks.</li>
      </ol>
      {got ? <div className="banner" role="status"><span><b>Test share received.</b> <span className="text-text-2">It’s waiting for a name on Home.</span></span></div>
        : <div className="banner" role="status"><span className="text-text-2">Waiting for a test share…</span><Dots /></div>}
      <span className="help">If nothing arrives: check your Wi-Fi is set to Private in Windows, and that Windows Firewall allowed Runout.</span>
    </Dialog>
  );
}

/* ------------------------------------------------------------------ SC-13c connect / disconnect */
export function ConnectDialog({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState<"start" | "wait" | "done" | "fail">("start");
  const inbox = useApi(getInbox).data;
  const follow = inbox?.followed_artists;
  const [error, setError] = useState<AppError>();
  const go = async () => {
    setStep("wait");
    try { const s = await connectSpotify(); setStep(s.state === "connected" ? "done" : "fail"); } catch (e) { setError(toAppError(e)); setStep("fail"); }
  };
  if (step === "wait") return (
    <Dialog title="Connect Spotify" onClose={onClose} footer={<button className="btn btn-secondary" onClick={onClose}>Cancel</button>}>
      <div className="flex flex-wrap gap-2 items-center"><Dots /><span>Waiting for you to approve in your browser…</span></div>
      <span className="help">Spotify’s page lists what Runout may do. Check it says Runout, then click Agree.</span>
    </Dialog>);
  if (step === "fail") return (
    <Dialog title="Couldn’t connect" onClose={onClose} footer={<><button className="btn btn-secondary" onClick={onClose}>Close</button><button className="btn btn-primary" onClick={go}>Try again</button></>}>
      <div className="banner err" role="alert"><span><b className="text-danger">Spotify said no.</b> <span className="text-text-2">{error?.code === "offline" ? error.message
        : "Developer access needs an active Premium plan on the app owner’s account, and your account added under User Management."}</span></span></div>
    </Dialog>);
  if (step === "done") return (
    <Dialog title="Spotify connected" onClose={onClose} footer={<button className="btn btn-primary" onClick={onClose}>Done</button>}>
      <span>Playlist Studio, autofill, Saved in Spotify and From Spotify are ready. Spotify will ask you to sign in again in 6 months.</span>
    </Dialog>);
  return (
    <Dialog title="Connect Spotify" onClose={onClose} footer={<><button className="btn btn-secondary" onClick={onClose}>Cancel</button><button className="btn btn-primary" onClick={go} autoFocus>Open Spotify sign-in</button></>}>
      <Attribution />
      <span className="text-text-2 leading-[1.55]">Runout opens Spotify’s sign-in page in your browser. It asks to read and edit your playlists, read your top items and recent plays, and save to your library.</span>
      {(inbox?.state === "none" || follow) && <span className="text-text-2 leading-[1.55]">{inbox?.state === "none" ? "Connecting also creates a private “Runout Inbox” playlist in your Spotify account, for saving music from your phone." : ""}{follow ? `${inbox?.state === "none" ? " " : ""}Because you import artists you follow, it also asks to read who you follow.` : ""}</span>}
      <span className="help">Nothing Runout gets from Spotify feeds your insights. Disconnecting deletes it right away.</span>
    </Dialog>
  );
}

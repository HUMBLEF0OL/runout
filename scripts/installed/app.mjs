// Installed-build driver (M6): launch %LOCALAPPDATA%\Runout\runout.exe with WebView2 remote debugging, attach over CDP.
// launch({offline: true}) = dead proxy for Rust (HTTPS_PROXY) and WebView2 (--proxy-server): offline without admin rights.
// startMs = process spawn -> first render (NFR-04). quit() force-kills (test runs only).
import { spawn, execSync } from "node:child_process";
export const EXE = `${process.env.LOCALAPPDATA}\\Runout\\runout.exe`;
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function launch({ port = 9333, offline = false } = {}) {
  // Keep wry's defaults: this env var replaces them (SmartScreen etc. off in the real app).
  // Same as tauri.conf.json additionalBrowserArgs (this env var stands in for them) + the debug port.
  let args = `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,msEdgeIdentitySSO,msHubs,msProfileTokenBroker --disable-identity-proxy --disable-background-networking --disable-component-update --disable-domain-reliability --no-pings --disable-sync --disable-client-side-phishing-detection --remote-debugging-port=${port}`;
  const env = { ...process.env };
  if (offline) {
    // Dead proxy for both stacks: reqwest honours HTTPS_PROXY/HTTP_PROXY, WebView2 takes --proxy-server.
    args += " --proxy-server=http://127.0.0.1:9 --proxy-bypass-list=<-loopback>";
    Object.assign(env, { HTTPS_PROXY: "http://127.0.0.1:9", HTTP_PROXY: "http://127.0.0.1:9", ALL_PROXY: "http://127.0.0.1:9", NO_PROXY: "127.0.0.1,localhost" });
  }
  if (process.env.EXTRA_WV2_ARGS) args += " " + process.env.EXTRA_WV2_ARGS;
  env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = args;
  const t0 = Date.now();
  const proc = spawn(EXE, [], { env, detached: true, stdio: "ignore" });
  proc.unref();
  let target;
  for (let i = 0; i < 400 && !target; i++) {
    try { target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === "page" && !t.url.startsWith("devtools")); } catch { /* not up */ }
    if (!target) await sleep(25);
  }
  if (!target) throw new Error("no CDP target");
  const c = await connect(target.webSocketDebuggerUrl);
  // first render: the shell nav exists
  await c.waitFor("!!document.querySelector('nav a, main h1')", 15000, 10);
  return { ...c, pid: proc.pid, startMs: Date.now() - t0 };
}

export async function connect(url) {
  const ws = new WebSocket(url);
  await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
  let seq = 0; const pending = new Map();
  ws.onmessage = (m) => { const d = JSON.parse(m.data); if (pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
  const send = (method, params = {}) => new Promise((res, rej) => { pending.set(++seq, (d) => (d.error ? rej(new Error(`${method}: ${d.error.message}`)) : res(d.result))); ws.send(JSON.stringify({ id: seq, method, params })); });
  const js = async (expr) => {
    const r = await send("Runtime.evaluate", { expression: `(async () => (${expr}))()`, returnByValue: true, awaitPromise: true });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
    return r.result.value;
  };
  const waitFor = async (expr, ms = 5000, step = 50) => {
    const end = Date.now() + ms;
    while (Date.now() < end) { try { const v = await js(expr); if (v) return v; } catch { /* mid-render */ } await sleep(step); }
    throw new Error(`timed out: ${expr}`);
  };
  const invoke = (cmd, args = {}) => js(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
  const shot = async (file) => { const { data } = await send("Page.captureScreenshot", { format: "png" }); (await import("node:fs")).writeFileSync(file, Buffer.from(data, "base64")); };
  return { ws, send, js, waitFor, invoke, shot, close: () => ws.close() };
}

export function quit() {
  try { execSync("taskkill /IM runout.exe /F", { stdio: "ignore" }); } catch { /* not running */ }
}

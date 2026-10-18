// UI-side link sniffing for the combined field only (labels the button). The real parse is the
// `parse_link` command; this just mirrors its shape so typing feels instant.
const ID = "[A-Za-z0-9]{22}";
const RE = new RegExp(`(?:open\.spotify\.com/(?:intl-[a-z-]+/)?|spotify:)(album|track|artist|playlist|episode|show|audiobook)[/:](${ID})`, "i");
export function parseLinkLocal(s: string): { state: "ok" | "short" | "unsupported" | "invalid"; kind?: string } {
  if (/^(?:https?:\/\/)?spotify\.link\/\w+/i.test(s.trim())) return { state: "short" };
  const m = s.match(RE);
  if (!m) return { state: "invalid" };
  const kind = m[1].toLowerCase();
  return { state: ["album", "track", "artist"].includes(kind) ? "ok" : "unsupported", kind };
}

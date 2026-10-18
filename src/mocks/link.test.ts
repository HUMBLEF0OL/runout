import { describe, expect, it } from "vitest";
import { parseLink } from "./link";

const A = "3QJzdZJYIAcoET1GcfpNGi";
// Same table M3's links.rs must pass (TSD §4.1).
const TABLE: [string, ReturnType<typeof parseLink>][] = [
  [`https://open.spotify.com/album/${A}`, { state: "ok", type: "album", uri: `spotify:album:${A}` }],
  [`https://open.spotify.com/album/${A}?si=abc123`, { state: "ok", type: "album", uri: `spotify:album:${A}` }],
  [`https://open.spotify.com/intl-de/track/${A}`, { state: "ok", type: "track", uri: `spotify:track:${A}` }],
  [`https://open.spotify.com/intl-pt-br/artist/${A}?si=x`, { state: "ok", type: "artist", uri: `spotify:artist:${A}` }],
  [`  open.spotify.com/track/${A}  `, { state: "ok", type: "track", uri: `spotify:track:${A}` }],
  [`spotify:artist:${A}`, { state: "ok", type: "artist", uri: `spotify:artist:${A}` }],
  ["https://spotify.link/aXk3P9q", { state: "short" }],
  ["spotify.link/NBdJ4D2", { state: "short" }],
  [`https://open.spotify.com/episode/${A}`, { state: "unsupported" }],
  [`https://open.spotify.com/show/${A}`, { state: "unsupported" }],
  [`https://open.spotify.com/audiobook/${A}`, { state: "unsupported" }],
  [`https://open.spotify.com/playlist/${A}`, { state: "unsupported" }],
  [`https://open.spotify.com/album/${A.slice(1)}`, { state: "invalid" }],
  [`evil.example.com/album/${A}`, { state: "invalid" }],
  ["coltrane", { state: "invalid" }],
  ["", { state: "invalid" }],
];

describe("parseLink (TSD §4.1)", () => {
  it.each(TABLE)("%s", (raw, want) => expect(parseLink(raw)).toEqual(want));
});

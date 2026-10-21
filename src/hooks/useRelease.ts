import { useSyncExternalStore } from "react";
import { dev, onDevChange, type Release } from "../mocks/devState";

const R3 = { release: "r3", r2: true, r3: true } as const;

/** Which release's screens show. A real build (VITE_MOCK=0) runs everything on Rust: all of R3; R3 screens show
 *  their connect state from `get_spotify_status`. On mocks the dev switcher picks it (shipped mock builds: r3). */
export function useRelease(): { release: Release; r2: boolean; r3: boolean } {
  // Build-time constants, so the hook call order never changes at run time and a real build drops devState.
  if (import.meta.env.VITE_MOCK === "0" || !import.meta.env.DEV) return R3;
  // eslint-disable-next-line react-hooks/rules-of-hooks
  const release = useSyncExternalStore(onDevChange, () => dev.release);
  return { release, r2: release !== "mvp", r3: release === "r3" };
}

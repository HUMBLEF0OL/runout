import { useSyncExternalStore } from "react";
import { MOCK } from "../api";
import { dev, onDevChange, type Release } from "../mocks/devState";

/** Which release's screens show. On mocks the dev switcher picks it (shipped mock builds: r3); with mocks off only
 *  MVP runs on Rust until M7, so R2/R3 screens stay hidden rather than show mock data beside the real library. */
export function useRelease(): { release: Release; r2: boolean; r3: boolean } {
  const release = useSyncExternalStore(onDevChange, () => (!MOCK ? "mvp" : import.meta.env.DEV ? dev.release : "r3"));
  return { release, r2: release !== "mvp", r3: release === "r3" };
}

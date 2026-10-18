import { useSyncExternalStore } from "react";
import { dev, onDevChange, type Release } from "../mocks/devState";

/** Which release's screens show. The dev switcher picks it in M2; shipped builds show everything built (r3). */
export function useRelease(): { release: Release; r2: boolean; r3: boolean } {
  const release = useSyncExternalStore(onDevChange, () => (import.meta.env.DEV ? dev.release : "r3"));
  return { release, r2: release !== "mvp", r3: release === "r3" };
}

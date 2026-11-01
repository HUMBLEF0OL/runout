import { createContext, useContext } from "react";
import type { ItemType } from "../api";

/** How the quick-add dialog (SC-06) opens: blank (Ctrl+N), with a pasted link, with a suggestion from
 *  listening or Spotify data ("Save to backlog"; only `from: "spotify"` names count as promoted, TSD §4.2), or naming a phone capture. `albumId`: a new track is linked on_album to that album. */
export interface QuickAddOpts { link?: string; suggest?: { name: string; artist?: string; type: ItemType; uri?: string | null; from: "spotify" | "plays" }; namingItemId?: number; albumId?: number }
export const QuickAddCtx = createContext<(o?: QuickAddOpts) => void>(() => {});
export const useQuickAdd = () => useContext(QuickAddCtx);

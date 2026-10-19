import { create } from "zustand";
import { persist } from "zustand/middleware";

export type Tab = "chat" | "voice" | "notifications" | "settings";

/** Purely visual preferences, remembered in localStorage. */
interface UiState {
  tab: Tab;
  showTimestamps: boolean;
  muted: boolean;
  set: (patch: Partial<Omit<UiState, "set">>) => void;
}

export const useUi = create<UiState>()(
  persist(
    (set) => ({
      tab: "chat",
      showTimestamps: true,
      muted: false,
      set: (patch) => set(patch),
    }),
    { name: "tcr-ui" },
  ),
);

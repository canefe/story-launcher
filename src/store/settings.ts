import { create } from "zustand";
import { persist } from "zustand/middleware";

interface SettingsState {
  instanceFolderName: string;
  setInstanceFolderName: (name: string) => void;
}

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      instanceFolderName: "Story",
      setInstanceFolderName: (name: string) =>
        set({ instanceFolderName: name }),
    }),
    {
      name: "story-launcher-settings",
    }
  )
);

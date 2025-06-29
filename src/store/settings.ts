import { create } from "zustand";
import { persist } from "zustand/middleware";

interface SettingsState {
  instanceFolderName: string;
  setInstanceFolderName: (name: string) => void;
  instancesPath: string;
  setInstancesPath: (path: string) => void;
}

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      instanceFolderName: "Story",
      setInstanceFolderName: (name: string) =>
        set({ instanceFolderName: name }),
      instancesPath: "\\AppData\\Roaming\\PollyMC\\instances\\",
      setInstancesPath: (path: string) => set({ instancesPath: path }),
    }),
    {
      name: "story-launcher-settings",
    }
  )
);

import { create } from "zustand";
import { persist } from "zustand/middleware";

interface LauncherInfo {
  name: string;
  path: string;
  type: "MultiMC" | "PollyMC" | "PrismLauncher";
}

interface SettingsState {
  instanceFolderName: string;
  setInstanceFolderName: (name: string) => void;
  instancesPath: string;
  setInstancesPath: (path: string) => void;
  launcherType: string;
  setLauncherType: (type: string) => void;
  availableLaunchers: LauncherInfo[];
  setAvailableLaunchers: (launchers: LauncherInfo[]) => void;
  selectedLauncher: LauncherInfo | null;
  setSelectedLauncher: (launcher: LauncherInfo | null) => void;
}

// OS-specific default paths for different launchers
const getDefaultInstancesPath = (): string => {
  const platform = navigator.platform.toLowerCase();
  
  if (platform.includes('win')) {
    // Windows paths - try MultiMC first, then PollyMC, then PrismLauncher
    return "\\AppData\\Roaming\\MultiMC\\instances\\";
  } else if (platform.includes('mac')) {
    // macOS paths - try MultiMC first, then PollyMC, then PrismLauncher
    return "/Library/Application Support/MultiMC/instances/";
  } else {
    // Linux paths - try MultiMC first, then PollyMC, then PrismLauncher
    return "/.local/share/MultiMC/instances/";
  }
};

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      instanceFolderName: "Story",
      setInstanceFolderName: (name: string) =>
        set({ instanceFolderName: name }),
      instancesPath: getDefaultInstancesPath(),
      setInstancesPath: (path: string) => set({ instancesPath: path }),
      launcherType: "MultiMC",
      setLauncherType: (type: string) => set({ launcherType: type }),
      availableLaunchers: [],
      setAvailableLaunchers: (launchers: LauncherInfo[]) => set({ availableLaunchers: launchers }),
      selectedLauncher: null,
      setSelectedLauncher: (launcher: LauncherInfo | null) => set({ selectedLauncher: launcher }),
    }),
    {
      name: "story-launcher-settings",
    }
  )
);

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import Background from "./assets/bg.jpg";
import { listen } from "@tauri-apps/api/event";
import { homeDir } from "@tauri-apps/api/path";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { SettingsModal } from "./components/SettingsModal";
import { TitlebarButton } from "./components/TitlebarButton";
import { useSettingsStore } from "./store/settings";

function App() {
  const [manifestUrl] = useState(
    "https://story.idealcanayavefe.com/manifest.json"
  );
  const [folderPath, setFolderPath] = useState("");
  const [downloadProgress, setDownloadProgress] = useState(0);
  const [downloadedBytes, setDownloadedBytes] = useState(0);
  const [totalBytes, setTotalBytes] = useState(0);

  const [extractionProgress, setExtractionProgress] = useState(0);
  const [currentFile, setCurrentFile] = useState("");
  const [extractedFiles, setExtractedFiles] = useState(0);
  const [totalFiles, setTotalFiles] = useState(0);

  const [isDownloading, setIsDownloading] = useState(false);
  const [isExtracting, setIsExtracting] = useState(false);
  const [statusMessage, setStatusMessage] = useState("");
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  const [instanceExists, setInstanceExists] = useState(false);

  const { instanceFolderName, selectedLauncher } = useSettingsStore();

  // Helper function to get default instances path (same as in settings store)
  const getDefaultInstancesPath = (): string => {
    const platform = navigator.platform.toLowerCase();
    
    if (platform.includes('win')) {
      return "\\AppData\\Roaming\\MultiMC\\instances\\";
    } else if (platform.includes('mac')) {
      return "/Library/Application Support/MultiMC/instances/";
    } else {
      return "/.local/share/MultiMC/instances/";
    }
  };

  const appWindow = getCurrentWindow();

  useEffect(() => {
    // Function to make window draggable by the titlebar
    const makeDraggable = () => {
      const titlebar = document.getElementById("titlebar");

      if (titlebar) {
        titlebar.addEventListener("mousedown", (e) => {
          // Only process primary button clicks (left mouse button)
          if (e.buttons !== 1) return;

          // Don't drag if clicking on a button element
          const target = e.target as HTMLElement;
          if (target.closest(".titlebar-button")) return;

          console.log("Starting drag operation");
          appWindow.startDragging();
        });
      }
    };

    // Set a short timeout to ensure DOM is fully loaded
    const timeoutId = setTimeout(makeDraggable, 100);

    // Clean up
    return () => {
      clearTimeout(timeoutId);
      // We don't remove the event listener since the component re-renders often
      // and the titlebar is a static element that persists
    };
  }, []);

  useEffect(() => {
    runCheck();
    checkForUpdates();
  }, [isSettingsOpen]);

  useEffect(() => {
    // Updated download progress listener to handle both old and new formats
    const unlistenDownload = listen("download_progress", (event) => {
      const data = event.payload as any;

      // Handle new manifest-based format
      if (data.stage) {
        setCurrentFile(data.filename || data.message || "");
        if (data.current && data.total) {
          const progressPercent = Math.round((data.current / data.total) * 100);
          setDownloadProgress(progressPercent);
        } else if (data.percent !== undefined) {
          setDownloadProgress(data.percent);
        }
      }
      // Handle legacy format
      else if (data.percent !== undefined && data.downloaded !== undefined) {
        setDownloadProgress(data.percent);
        setDownloadedBytes(data.downloaded);
        setTotalBytes(data.total);
      }
    });

    // New extraction progress listener
    const unlistenExtraction = listen("extraction_progress", (event) => {
      const data = event.payload as {
        percent: number;
        current: number;
        total: number;
        filename: string;
      };
      setExtractionProgress(data.percent);
      setCurrentFile(data.filename);
      setExtractedFiles(data.current);
      setTotalFiles(data.total);
      setIsExtracting(data.percent < 100);
    });

    return () => {
      unlistenDownload.then((fn) => fn());
      unlistenExtraction.then((fn) => fn());
    };
  }, []);

  // Every 10 seconds, check instance status
  useEffect(() => {
    const interval = setInterval(() => {
      runCheck();
    }, 10000);

    return () => clearInterval(interval);
  }, [instanceExists, folderPath, instanceFolderName]);

  // Helper function to format bytes as MB with 2 decimal places
  const formatMB = (bytes: number) => {
    return (bytes / (1024 * 1024)).toFixed(2);
  };

  async function checkStoryInstance(folderBase: string) {
    const result = await invoke("check_story_instance", {
      instanceBase: folderBase,
      folderName: instanceFolderName,
    });
    console.log("Instance check result:", result);
    setInstanceExists(result as boolean);
    setFolderPath(folderBase + instanceFolderName + "\\.minecraft");
  }

  async function createStoryInstance(folderBase: string) {
    const result = await invoke("create_story_instance", {
      instanceBase: folderBase,
      folderName: instanceFolderName,
    });
    console.log("Create instance result:", result);
  }

  async function checkForUpdates() {
    try {
      const path = await findLauncherInstancesPath();
      // Check for manifest-based updates
      const updateInfo = await invoke("check_manifest_updates", {
        manifestUrl: manifestUrl,
        instanceBase: path,
      });
      setStatusMessage(`${updateInfo}`);
    } catch (error) {
      setStatusMessage(`Error checking for updates: ${error}`);
    }
  }

  async function downloadFromManifest() {
    setIsDownloading(true);
    setStatusMessage("Downloading from manifest...");
    setDownloadProgress(0);
    setCurrentFile("");

    try {
      const path = await findLauncherInstancesPath();
      const result = await invoke("download_from_manifest", {
        manifestUrl: manifestUrl,
        instanceBase: path,
      });
      console.log("Manifest download result:", result);
      setStatusMessage("Manifest download complete!");
      setDownloadProgress(100);
      setCurrentFile("");
      // Re-check after download
      await runCheck();
    } catch (error) {
      setStatusMessage(`Error downloading from manifest: ${error}`);
    } finally {
      setIsDownloading(false);
    }
  }


  async function findLauncherInstancesPath(): Promise<string> {
    const home = await homeDir();
    const platform = navigator.platform.toLowerCase();
    
    // Check if user has selected a specific launcher
    if (selectedLauncher) {
      console.log(`Using selected launcher: ${selectedLauncher.name} at ${selectedLauncher.path}`);
      return selectedLauncher.path;
    }
    
    // Check if user has already set a custom path in settings
    const currentSettings = useSettingsStore.getState();
    if (currentSettings.instancesPath && currentSettings.instancesPath !== getDefaultInstancesPath()) {
      // Check if it's already an absolute path
      if (currentSettings.instancesPath.startsWith('/') || currentSettings.instancesPath.includes(':\\')) {
        console.log(`Using user-set absolute path: ${currentSettings.instancesPath}`);
        return currentSettings.instancesPath;
      } else {
        console.log(`Using user-set relative path: ${home}${currentSettings.instancesPath}`);
        return `${home}${currentSettings.instancesPath}`;
      }
    }
    
    // Define possible paths for different launchers
    const possiblePaths = [];
    
    if (platform.includes('win')) {
      // Windows paths
      possiblePaths.push(
        `${home}\\AppData\\Roaming\\MultiMC\\instances\\`,
        `${home}\\AppData\\Roaming\\PollyMC\\instances\\`,
        `${home}\\AppData\\Roaming\\PrismLauncher\\instances\\`
      );
    } else if (platform.includes('mac')) {
      // macOS paths
      possiblePaths.push(
        `${home}/Library/Application Support/MultiMC/instances/`,
        `${home}/Library/Application Support/PollyMC/instances/`,
        `${home}/Library/Application Support/PrismLauncher/instances/`
      );
    } else {
      // Linux paths
      possiblePaths.push(
        `${home}/.local/share/MultiMC/instances/`,
        `${home}/.local/share/PollyMC/instances/`,
        `${home}/.local/share/PrismLauncher/instances/`
      );
    }
    
    // Check which path exists
    for (const path of possiblePaths) {
      try {
        const result = await invoke("check_path_exists", { path });
        if (result) {
          console.log(`Found launcher instances at: ${path}`);
          return path;
        }
      } catch (error) {
        console.log(`Path ${path} does not exist: ${error}`);
      }
    }
    
    // Return the first path as default if none found
    return possiblePaths[0];
  }


  const runCheck = async () => {
    const path = await findLauncherInstancesPath();
    await checkStoryInstance(path);
  };

  const createInstance = async () => {
    const path = await findLauncherInstancesPath();
    await createStoryInstance(path);
    await checkStoryInstance(path);
  };

  useEffect(() => {
    runCheck();
    checkForUpdates();
  }, []);

  return (
    <main className="overflow-hidden h-screen border-1 border-gray-500">
      <div
        id="titlebar"
        className="flex items-center z-50 justify-between w-full bg-gray-800"
      >
        <h1 className="px-4 mt-0.5">Story</h1>
        {/* Control buttons on the right side */}
        <div className="flex z-50">
          <TitlebarButton
            icon="https://api.iconify.design/mdi:cog.svg"
            alt="settings"
            onClick={() => setIsSettingsOpen(true)}
          />
          <TitlebarButton
            id="titlebar-close"
            icon="https://api.iconify.design/mdi:close.svg"
            alt="close"
            onClick={() => appWindow.close()}
          />
        </div>
      </div>
      <div className="flex flex-col mt-8">
        <SettingsModal
          isOpen={isSettingsOpen}
          onClose={() => setIsSettingsOpen(false)}
        />

        {/* Add a main content area */}
        <div className="flex-1 p-4">
          {/* Image background */}
          <img
            src={Background}
            alt="Background"
            className="absolute inset-0 bg-gray-800 opacity-40 -z-50 object-cover w-full h-full pointer-events-none"
          />

          {statusMessage ? (
            <p className="mb-4 min-h-[200px] max-h-[200px] overflow-scroll overflow-x-hidden">
              {/* List items by ','*/}
              {statusMessage.split(",").map((item, index) => (
                <li key={index}>
                  {item.trim()}
                  {index < statusMessage.split(",").length - 1 ? ", " : ""}
                </li>
              ))}
            </p>
          ) : null}
          {/* Status info - uncomment if you need to display debug info 
          <p>
            Instance exists:{" "}
            {instanceExists ? "Yes" : "No, please create an instance."}
          </p>
          <p>Instance folder: {instanceFolderName}</p>
          <p>Instance path: {folderPath}</p>
          <p>Base installed: {folderPath && baseInstalled ? "Yes" : "No"}</p>
          */}
          {/* Show progress bars when relevant */}
          {isDownloading && !isExtracting && (
            <div className="space-y-4">
              <div>
                <h3 className="font-bold">Download Progress</h3>
                <progress
                  className="w-full"
                  value={downloadProgress}
                  max="100"
                ></progress>

                {totalBytes > 0 && (
                  <p>
                    <span>
                      {" "}
                      - {formatMB(downloadedBytes)} MB / {formatMB(totalBytes)}{" "}
                      MB
                    </span>
                  </p>
                )}

                {currentFile && (
                  <p className="text-sm truncate mt-2">
                    {downloadProgress.toFixed(1)}% -{" "}
                    <span className="font-semibold">Current file:</span>{" "}
                    {currentFile}
                  </p>
                )}
              </div>
            </div>
          )}

          {isExtracting && (
            <div>
              <h3 className="font-bold">Extraction Progress</h3>
              <progress
                className="w-full"
                value={extractionProgress}
                max="100"
              ></progress>
              <p>
                {extractionProgress.toFixed(1)}% - {extractedFiles} /{" "}
                {totalFiles} files
              </p>
              <p className="text-sm truncate">{currentFile}</p>
            </div>
          )}
        </div>

        {/* Sidebar */}
        {isDownloading || isExtracting ? (
          <aside className="flex items-center flex-col py-2 text-white"></aside>
        ) : (
          <aside className="flex items-center flex-col py-2 text-white">
            <div className="">
              {!instanceExists ? (
                <Button onClick={createInstance} color="green">
                  Create Instance
                </Button>
              ) : (
                <div className="gap-2 flex min-w-[550px] flex-col justify-center items-center w-full">
                  <Button onClick={downloadFromManifest} color="blue">
                    Update
                  </Button>
                  <Button onClick={checkForUpdates} color="gray" size="small">
                    Check for Updates
                  </Button>
                </div>
              )}
            </div>
          </aside>
        )}

        <footer>
          <div className="flex justify-center items-center text-white py-2">
            <p className="text-sm">story-launcher 0.1.0 - canefe</p>
          </div>
        </footer>
      </div>
    </main>
  );
}

const Button = ({
  children,
  onClick,
  color = "blue",
  className = "",
  size = "large",
}: {
  children: React.ReactNode;
  onClick: () => void;
  color?: "blue" | "green" | "gray";
  className?: string;
  size?: "small" | "large";
}) => {
  return (
    <button
      className={`w-full ${
        size === "small" ? "max-w-[150px]" : "max-w-[300px]"
      }  ${
        color === "blue"
          ? "bg-blue-600 hover:bg-blue-700"
          : color === "green"
          ? "bg-green-600 hover:bg-green-700"
          : "bg-gray-600 hover:bg-gray-700"
      } text-white ${size === "small" ? "text-sm" : "text-xl"} font-bold ${
        size === "small" ? "py-1 px-2 h-8" : "py-2 px-4 h-12"
      } hover:cursor-pointer transition-colors duration-200 ${className}`}
      onClick={onClick}
    >
      {children}
    </button>
  );
};
export default App;

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import { listen } from "@tauri-apps/api/event";
import { homeDir } from "@tauri-apps/api/path";

function App() {
  const [megaLink] = useState("https://story.idealcanayavefe.com/update.zip");
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

  const [instanceExists, setInstanceExists] = useState(false);

  useEffect(() => {
    // Existing download progress listener
    const unlistenDownload = listen("download_progress", (event) => {
      const data = event.payload as {
        percent: number;
        downloaded: number;
        total: number;
      };
      setDownloadProgress(data.percent);
      setDownloadedBytes(data.downloaded);
      setTotalBytes(data.total);
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

  // Helper function to format bytes as MB with 2 decimal places
  const formatMB = (bytes: number) => {
    return (bytes / (1024 * 1024)).toFixed(2);
  };

  async function checkStoryInstance(folderBase: string) {
    const result = await invoke("check_story_instance", {
      instanceBase: folderBase,
    });
    console.log("Instance check result:", result);
    setInstanceExists(result as boolean);
    setFolderPath(folderBase + "Story\\.minecraft");
  }

  async function createStoryInstance(folderBase: string) {
    const result = await invoke("create_story_instance", {
      instanceBase: folderBase,
    });
    console.log("Create instance result:", result);
  }

  async function checkForUpdates() {
    try {
      // Only check if an update is available without downloading
      const updateInfo = await invoke("check_for_updates", {
        downloadUrl: megaLink,
      });
      setStatusMessage(`Update status: ${updateInfo}`);
    } catch (error) {
      setStatusMessage(`Error checking for updates: ${error}`);
    }
  }

  async function getPollyMCInstancePath(): Promise<string> {
    const home = await homeDir();
    console.log("Home directory:", home);
    return `${home}\\AppData\\Roaming\\PollyMC\\instances\\`;
  }

  const runCheck = async () => {
    const path = await getPollyMCInstancePath();
    await checkStoryInstance(path);
  };

  const createInstance = async () => {
    const path = await getPollyMCInstancePath();
    await createStoryInstance(path);
    await checkStoryInstance(path);
  };

  useEffect(() => {
    runCheck();
    checkForUpdates();
  }, []);

  return (
    <main className="container">
      <h1>Story Client Updater</h1>

      <button onClick={runCheck}>Check PollyMC Instance</button>

      {!instanceExists && (
        <button onClick={createInstance}>Create PollyMC Instance</button>
      )}

      {instanceExists ? (
        <p className="status-message success bg-red-500">
          ✅ PollyMC instance found at: {folderPath}
        </p>
      ) : (
        <p className="status-message error">
          ❌ PollyMC instance not found at: {folderPath}. Please create it
          first.
        </p>
      )}

      <button
        onClick={async () => {
          if (!folderPath) {
            setStatusMessage("⚠️ Please select a folder first");
            return;
          }

          setIsDownloading(true);
          setStatusMessage("Starting download...");
          try {
            // Don't append timestamp - it breaks caching logic
            const updates = await invoke("download_and_extract_zip", {
              downloadUrl: megaLink,
              extractPath: folderPath,
              forceDownload: false,
            });
            console.log("Updates available:", updates);
            setStatusMessage(updates as string);
          } catch (error) {
            console.error("Download failed:", error);
            setStatusMessage(`❌ Error: ${error}`);
          } finally {
            setIsDownloading(false);
          }
        }}
        disabled={isDownloading || !folderPath}
      >
        {isDownloading ? "Downloading..." : "Download Updates"}
      </button>

      {statusMessage && <p className="status-message">{statusMessage}</p>}

      <div className="progress-container">
        {isDownloading && (
          <>
            <p className="downloading-message">Downloading updates...</p>

            <h3>Download Progress</h3>
            <progress value={downloadProgress} max="100"></progress>
            <p>
              {downloadProgress.toFixed(0)}% - {formatMB(downloadedBytes)} MB /{" "}
              {formatMB(totalBytes)} MB
            </p>
          </>
        )}

        {isExtracting && (
          <>
            <h3>Extraction Progress</h3>
            <progress value={extractionProgress} max="100"></progress>
            <p>
              {extractionProgress.toFixed(0)}% - {extractedFiles} / {totalFiles}{" "}
              files
            </p>
            <p>Current: {currentFile}</p>
          </>
        )}
      </div>
    </main>
  );
}

export default App;

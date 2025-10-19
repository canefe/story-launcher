use chrono;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::future::Future;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Mutex;
use tauri::Emitter;
use tauri::{Manager, Window};
use zip::ZipArchive;

// Global set to track all downloaded JAR files
lazy_static::lazy_static! {
    static ref DOWNLOADED_FILES: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
}

// Helper function to track downloaded JAR files
fn track_downloaded_file(filename: &str) {
    if let Ok(mut files) = DOWNLOADED_FILES.lock() {
        files.insert(filename.to_string());
        println!("📝 Tracked downloaded file: {}", filename);
    }
}

// Helper function to clear the tracking list (call at start of new download session)
fn clear_downloaded_files() {
    if let Ok(mut files) = DOWNLOADED_FILES.lock() {
        files.clear();
        println!("🧹 Cleared downloaded files tracking");
    }
}
// Add new structs for Modrinth API and manifest handling
#[derive(Serialize, Deserialize)]
pub struct ModrinthVersionResponse {
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub changelog: Option<String>,
    pub files: Vec<ModrinthFile>,
    pub dependencies: Vec<ModrinthDependency>,
}

#[derive(Serialize, Deserialize)]
pub struct ModrinthFile {
    pub hashes: HashMap<String, String>,
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ModrinthDependency {
    pub version_id: Option<String>,
    pub project_id: Option<String>,
    pub file_name: Option<String>,
    pub dependency_type: String,
}

#[derive(Serialize, Deserialize, Default)]
struct HashRegistry {
    files: HashMap<String, String>, // URL -> hash mapping
}

#[derive(Serialize, Deserialize, Default)]
pub struct FileHashRegistry {
    pub files: HashMap<String, FileInfo>, // URL -> file info
}

#[derive(Serialize, Deserialize, Default)]
pub struct FileInfo {
    pub hash: String,
    pub last_modified: String,
}

// Legacy manifest structure for old zip-based downloads
#[derive(Serialize, Deserialize)]
pub struct LegacyManifestFile {
    pub delete: Option<Vec<String>>,
    pub notes: Option<String>,
    pub required_files: Option<Vec<String>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            download_and_extract_zip,
            finalize_instance,
            check_story_instance,
            create_story_instance,
            check_for_updates,
            is_base_installed,
            download_from_manifest,
            download_modrinth_modpack,
            download_modrinth_mod,
            check_manifest_updates,
            check_path_exists, // Add the new command here
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
#[tauri::command]
async fn check_for_updates(window: tauri::Window, download_url: String) -> Result<String, String> {
    // Make a HEAD request to get file info without downloading
    let client = reqwest::Client::new();
    let resp = client
        .head(&download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    // Check Last-Modified header
    let last_modified = resp
        .headers()
        .get("Last-Modified")
        .map(|h| h.to_str().unwrap_or_default())
        .unwrap_or_default();

    // Get app data dir for registry check
    let app_handle = window.app_handle();
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let cache_dir = app_data_dir.join("cache");

    // Check if we have a cached registry
    let hash_registry_path = cache_dir.join("hash_registry.json");
    let registry: FileHashRegistry = if hash_registry_path.exists() {
        let registry_content = std::fs::read_to_string(&hash_registry_path)
            .map_err(|e| format!("Failed to read hash registry: {}", e))?;
        serde_json::from_str(&registry_content).unwrap_or_default()
    } else {
        FileHashRegistry::default()
    };

    // Compare with our saved last-modified date
    let update_available = if let Some(file_info) = registry.files.get(&download_url) {
        if file_info.last_modified != last_modified {
            "Yes, a new version is available"
        } else {
            "No, you have the latest version"
        }
    } else {
        "Yes, file has not been downloaded yet"
    };

    // Return info including the last-modified date
    Ok(format!(
        "{} (Last modified: {})",
        update_available, last_modified
    ))
}

#[tauri::command]
fn check_story_instance(instance_base: String, folder_name: String) -> bool {
    let story_path = Path::new(&instance_base).join(folder_name);
    // Check if the Story instance directory exists
    println!("Checking for Story instance at: {:?}", story_path);
    // also check if the dir has a instance.cfg
    story_path.exists()
}

// Check if Base Is Installed (check for npcmessageparser-1.0-SNAPSHOT.jar)
#[tauri::command]
fn is_base_installed(instance_base: String) -> bool {
    let base_path = Path::new(&instance_base).join("npcmessageparser-1.0-SNAPSHOT.jar");
    base_path.exists()
}

#[tauri::command]
fn check_path_exists(path: String) -> bool {
    let path = Path::new(&path);
    path.exists() && path.is_dir()
}

// create Story instance with configurable folder name
#[tauri::command]
fn create_story_instance(instance_base: String, folder_name: String) -> Result<String, String> {
    let story_path = Path::new(&instance_base).join(&folder_name);
    // Create the Story instance directory
    std::fs::create_dir_all(&story_path).map_err(|e| e.to_string())?;
    println!("Created instance at: {:?}", story_path);
    println!("Finalizing instance at {}", instance_base);

    // Use path joining for cross-platform compatibility
    let full_path = story_path.to_string_lossy().to_string();
    match finalize_instance(full_path) {
        Ok(_) => println!("Instance finalized successfully"),
        Err(e) => {
            println!("Failed to finalize instance: {}", e);
            return Err(format!("Failed to finalize instance: {}", e));
        }
    }
    Ok(story_path.to_string_lossy().into_owned())
}

// Add this function to verify extraction integrity based on manifest requirements
pub fn verify_extraction_integrity(
    extract_path: &Path,
    manifest_data: &Option<LegacyManifestFile>,
) -> Result<bool, String> {
    println!("Verifying extraction integrity");

    // Check if we have manifest requirements to verify
    if let Some(manifest) = manifest_data {
        if let Some(required_files) = &manifest.required_files {
            println!(
                "Checking {} required files from manifest",
                required_files.len()
            );

            for (index, relative_path) in required_files.iter().enumerate() {
                let full_path = extract_path.join(relative_path);
                println!(
                    "Checking required file {}/{}: {}",
                    index + 1,
                    required_files.len(),
                    full_path.display()
                );

                if !full_path.exists() {
                    println!("Missing required file: {}", full_path.display());
                    return Ok(false);
                }
            }
            println!("All required files verified successfully");
        } else {
            println!("No required files specified in manifest");
        }
    } else {
        println!("No manifest data available for verification");
    }

    // If we get here, all required files are present (or none were specified)
    Ok(true)
}

#[tauri::command]
async fn download_and_extract_zip(
    window: Window,
    download_url: String,
    extract_path: String,
    force_download: bool,
) -> Result<String, String> {
    println!(
        "Starting download_and_extract_zip with params: url={}, path={}, force={}",
        download_url, extract_path, force_download
    );

    // Clone values that need to be moved into the task
    let window_clone = window.clone();

    // First, check the Last-Modified header from the server
    println!("Making HEAD request to {}", download_url);
    let client = reqwest::Client::new();
    let resp = client.head(&download_url).send().await.map_err(|e| {
        println!("HEAD request failed: {}", e);
        e.to_string()
    })?;

    let last_modified = resp
        .headers()
        .get("Last-Modified")
        .map(|h| h.to_str().unwrap_or_default())
        .unwrap_or_default()
        .to_string();
    println!("Got Last-Modified header: {}", last_modified);

    // Use tokio's spawn_blocking for file operations that can't be async
    let result = tokio::task::spawn_blocking(move || {
        // Create cache directory inside the app's data directory
        println!("Getting app data directory");
        let app_data_dir = match window_clone.app_handle().path().app_data_dir() {
            Ok(path) => path,
            Err(e) => {
                println!("Failed to get app data directory: {}", e);
                return Err(e.to_string());
            }
        };

        println!("App data directory: {}", app_data_dir.display());
        let cache_dir = app_data_dir.join("cache");
        println!("Cache directory: {}", cache_dir.display());

        match std::fs::create_dir_all(&cache_dir) {
            Ok(_) => println!("Cache directory created/verified"),
            Err(e) => {
                println!(
                    "Failed to create cache directory {}: {}",
                    cache_dir.display(),
                    e
                );
                return Err(e.to_string());
            }
        }

        // Path to the hash registry file
        let hash_registry_path = cache_dir.join("hash_registry.json");
        println!("Hash registry path: {}", hash_registry_path.display());

        // Load existing hash registry or create a new one
        let mut registry: FileHashRegistry = if hash_registry_path.exists() {
            println!("Reading existing hash registry");
            let registry_content = match std::fs::read_to_string(&hash_registry_path) {
                Ok(content) => content,
                Err(e) => {
                    println!("Failed to read hash registry: {}", e);
                    return Err(format!("Failed to read hash registry: {}", e));
                }
            };

            match serde_json::from_str(&registry_content) {
                Ok(reg) => reg,
                Err(e) => {
                    println!("Failed to parse hash registry, using default: {}", e);
                    FileHashRegistry::default()
                }
            }
        } else {
            println!("No existing hash registry found, creating new one");
            FileHashRegistry::default()
        };

        // Generate filename from URL - improve this to handle any zip file cleanly
        let url_parts: Vec<&str> = download_url
            .split('?')
            .next()
            .unwrap_or(&download_url)
            .split('/')
            .collect();

        // Extract the actual filename from the URL, with better fallback handling
        let filename = match url_parts.last() {
            Some(name) if !name.is_empty() => (*name).to_string(),
            _ => {
                // If we can't determine a filename from the URL, generate one from the hash of the URL
                let mut hasher = Sha256::new();
                hasher.update(download_url.as_bytes());
                let hash = format!("{:x}", hasher.finalize());
                format!("download-{}.zip", &hash[0..8])
            }
        };

        println!("Using filename: {}", filename);
        let cached_file_path = cache_dir.join(filename);
        println!("Cached file path: {}", cached_file_path.display());

        // Check if we need to download based on existence, hash, and last-modified
        let file_info = registry.files.get(&download_url);
        let file_exists = cached_file_path.exists()
            && std::fs::metadata(&cached_file_path)
                .map(|m| m.len() > 0)
                .unwrap_or(false);

        println!(
            "File exists: {}, Previous info exists: {}",
            file_exists,
            file_info.is_some()
        );

        // Download is needed if:
        // 1. Force download is true, OR
        // 2. File doesn't exist, OR
        // 3. No previous hash/info, OR
        // 4. Last-modified date is different from what we have stored
        let download_needed = force_download
            || !file_exists
            || file_info.is_none()
            || file_info
                .as_ref()
                .map_or(true, |info| info.last_modified != last_modified);

        println!("Download needed: {}", download_needed);
        let mut file_hash = String::new();

        if download_needed {
            // Download the file to cache
            println!("Starting download to cache");
            let download_url_clone = download_url.clone();
            let mut resp = match reqwest::blocking::get(&download_url_clone) {
                Ok(resp) => resp,
                Err(e) => {
                    println!("Failed to start download: {}", e);
                    return Err(e.to_string());
                }
            };

            let total_size = match resp.content_length() {
                Some(size) => size,
                None => {
                    println!("Couldn't get content length");
                    return Err("Couldn't get content length".to_string());
                }
            };
            println!("Download size: {} bytes", total_size);

            // Create file and prepare for download
            let mut file = match File::create(&cached_file_path) {
                Ok(file) => file,
                Err(e) => {
                    println!("Failed to create cache file: {}", e);
                    return Err(e.to_string());
                }
            };

            // Calculate hash while downloading
            let mut hasher = Sha256::new();
            let mut downloaded = 0u64;
            let mut buffer = [0u8; 8192];
            let mut last_update = std::time::Instant::now();
            let update_frequency = std::time::Duration::from_millis(100);

            // Download in chunks and report progress
            while let Ok(n) = resp.read(&mut buffer) {
                if n == 0 {
                    break;
                }

                // Update hash calculation
                hasher.update(&buffer[..n]);

                // Write chunk to file
                file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;

                // Update progress
                downloaded += n as u64;

                // Throttle progress updates to avoid overwhelming the UI
                if last_update.elapsed() >= update_frequency {
                    let pct = (downloaded as f64 / total_size as f64) * 100.0;
                    let _ = window_clone.emit(
                        "download_progress",
                        serde_json::json!({
                            "percent": pct as u32,
                            "downloaded": downloaded,
                            "total": total_size
                        }),
                    );
                    last_update = std::time::Instant::now();
                }
            }

            // Final progress update
            let _ = window_clone.emit(
                "download_progress",
                serde_json::json!({
                    "percent": 100,
                    "downloaded": downloaded,
                    "total": total_size
                }),
            );

            // Finalize file and hash
            file.flush().map_err(|e| e.to_string())?;
            file_hash = format!("{:x}", hasher.finalize());

            // Update registry with new hash and last-modified date
            registry.files.insert(
                download_url.clone(),
                FileInfo {
                    hash: file_hash.clone(),
                    last_modified: last_modified.clone(),
                },
            );

            // Save updated registry
            let registry_json = serde_json::to_string(&registry)
                .map_err(|e| format!("Failed to serialize registry: {}", e))?;
            std::fs::write(&hash_registry_path, registry_json)
                .map_err(|e| format!("Failed to write hash registry: {}", e))?;
        } else if let Some(file_info) = registry.files.get(&download_url) {
            // Use cached file
            println!("Using cached file with hash {}", file_info.hash);
            file_hash = file_info.hash.clone();

            // Report 100% progress for existing file
            let size = std::fs::metadata(&cached_file_path)
                .map(|m| m.len())
                .unwrap_or(0);
            println!("Cached file size: {} bytes", size);
        }

        // Now extract from the cached file
        println!(
            "Opening cached file for extraction: {}",
            cached_file_path.display()
        );
        let file = match File::open(&cached_file_path) {
            Ok(file) => file,
            Err(e) => {
                println!("Failed to open cached file: {}", e);
                return Err(e.to_string());
            }
        };

        println!("Ensuring extract path exists: {}", extract_path);
        match std::fs::create_dir_all(&extract_path) {
            Ok(_) => println!("Created extract directory"),
            Err(e) => {
                println!("Failed to create extract directory: {}", e);
                return Err(format!("Failed to create extract directory: {}", e));
            }
        }

        // Now try to get the canonical path
        println!("Canonicalizing extract path: {}", extract_path);
        let extract_path = match dunce::canonicalize(&extract_path) {
            Ok(path) => path,
            Err(e) => {
                println!("Failed to canonicalize extract path: {}", e);
                // Use the original path as fallback if canonicalization fails
                println!("Using original path instead");
                PathBuf::from(&extract_path)
            }
        };
        println!("Final extract path: {}", extract_path.display());

        // Create a hash file at the extract location to track what version is installed
        let extract_hash_path = extract_path.join(".installed_hash");
        println!("Hash marker path: {}", extract_hash_path.display()); // Initialize manifest_data earlier in the code flow
        let mut manifest_data: Option<LegacyManifestFile> = None;

        // Try to find and parse the manifest file from the zip before extraction
        println!("Looking for manifest.json in zip for verification");
        let file_for_manifest = match File::open(&cached_file_path) {
            Ok(file) => file,
            Err(e) => {
                println!("Failed to open cached file for manifest check: {}", e);
                return Err(e.to_string());
            }
        };

        // Try to read the manifest to use for verification
        if let Ok(mut zip) = ZipArchive::new(file_for_manifest) {
            if let Ok(mut manifest_file) = zip.by_name("manifest.json") {
                println!("Found manifest.json for verification, reading content");
                let mut manifest_content = String::new();
                if manifest_file.read_to_string(&mut manifest_content).is_ok() {
                    match serde_json::from_str::<LegacyManifestFile>(&manifest_content) {
                        Ok(manifest) => {
                            println!("Successfully parsed manifest.json for verification");
                            manifest_data = Some(manifest);
                        }
                        Err(e) => println!("Failed to parse manifest.json for verification: {}", e),
                    }
                } else {
                    println!("Failed to read manifest.json content for verification");
                }
            } else {
                println!("No manifest.json found for verification");
            }
        } else {
            println!("Failed to open zip for manifest verification");
        }

        let current_hash = if extract_hash_path.exists() {
            match std::fs::read_to_string(&extract_hash_path) {
                Ok(hash) => {
                    println!("Found existing installation hash: {}", hash);
                    hash
                }
                Err(e) => {
                    println!("Failed to read installation hash: {}", e);
                    String::new()
                }
            }
        } else {
            println!("No existing installation hash found");
            String::new()
        };

        // Check if extraction integrity is maintained - we need to verify files are present
        // even if the hash hasn't changed
        let files_verified = if !current_hash.is_empty() && current_hash == file_hash {
            println!("Hash matches, checking file integrity...");
            match verify_extraction_integrity(&extract_path, &manifest_data) {
                Ok(true) => {
                    println!("File integrity verified successfully");
                    true
                }
                Ok(false) => {
                    println!("File integrity check failed - some required files are missing");
                    false
                }
                Err(e) => {
                    println!("Error during file verification: {}", e);
                    false
                }
            }
        } else {
            println!("Hash mismatch or no previous hash, skipping integrity check");
            false
        };

        // Need extraction if:
        // 1. Forced download is enabled
        // 2. Hash is different from current
        // 3. Files failed verification check
        let need_extraction = force_download || current_hash != file_hash || !files_verified;
        println!("Extraction needed: {}", need_extraction);

        let mut notes_text = String::new();

        if need_extraction {
            // Extract files
            println!("Creating ZipArchive from file");
            let mut zip = match ZipArchive::new(file) {
                Ok(zip) => zip,
                Err(e) => {
                    println!("Failed to open zip archive: {}", e);
                    return Err(e.to_string());
                }
            };

            let total_files = zip.len();
            println!("Zip archive contains {} files", total_files); // Check for manifest.json again, but no need to re-initialize
            println!("Looking for manifest.json in zip");

            // Only re-read manifest if we couldn't read it earlier
            if manifest_data.is_none() {
                // Try to find and parse the manifest file
                match zip.by_name("manifest.json") {
                    Ok(mut manifest_file) => {
                        println!("Found manifest.json, reading content");
                        let mut manifest_content = String::new();
                        if manifest_file.read_to_string(&mut manifest_content).is_ok() {
                            match serde_json::from_str::<LegacyManifestFile>(&manifest_content) {
                                Ok(manifest) => {
                                    println!("Successfully parsed manifest.json");
                                    manifest_data = Some(manifest);
                                }
                                Err(e) => println!("Failed to parse manifest.json: {}", e),
                            }
                        } else {
                            println!("Failed to read manifest.json content");
                        }
                    }
                    Err(e) => println!("No manifest.json found: {}", e),
                }
            } else {
                println!("Using manifest data from prior verification step");
            }

            // Process deletion requests from manifest
            if let Some(manifest) = &manifest_data {
                if let Some(delete_list) = &manifest.delete {
                    println!("Processing {} deletion requests", delete_list.len());

                    for (index, file_path) in delete_list.iter().enumerate() {
                        let full_path = extract_path.join(file_path);
                        println!(
                            "Deletion {}/{}: {}",
                            index + 1,
                            delete_list.len(),
                            full_path.display()
                        );

                        // Security check - prevent path traversal
                        if !full_path.starts_with(&extract_path) {
                            println!(
                                "Security warning: Attempted deletion outside extract path: {}",
                                file_path
                            );
                            continue;
                        }

                        // Delete file if it exists
                        if full_path.exists() {
                            println!("File exists, deleting");
                            if full_path.is_dir() {
                                match fs::remove_dir_all(&full_path) {
                                    Ok(_) => println!("Deleted directory"),
                                    Err(e) => println!("Failed to delete directory: {}", e),
                                }
                            } else {
                                match fs::remove_file(&full_path) {
                                    Ok(_) => println!("Deleted file"),
                                    Err(e) => println!("Failed to delete file: {}", e),
                                }
                            }
                        } else {
                            println!("File doesn't exist, skipping deletion");
                        }
                    }
                }
            }

            // Extract all files from the zip archive
            let total_zip_files = zip.len();
            println!("Starting extraction of {} files", total_zip_files);
            let mut last_progress_update = std::time::Instant::now();
            let update_frequency = std::time::Duration::from_millis(100);

            for i in 0..total_zip_files {
                let mut file = match zip.by_index(i) {
                    Ok(file) => file,
                    Err(e) => {
                        println!("Failed to get file at index {}: {}", i, e);
                        return Err(e.to_string());
                    }
                };

                let file_name = file.name().to_string();
                println!("Extracting {}/{}: {}", i + 1, total_zip_files, file_name);

                // Skip manifest.json if it exists
                if file_name == "manifest.json" {
                    println!("Skipping manifest.json");
                    continue;
                }

                let file_path = Path::new(file.name());

                // Security checks
                if file_path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    println!("Security error: zip contains directory traversal pattern");
                    return Err(
                        "Invalid zip file: contains directory traversal patterns".to_string()
                    );
                }

                // Report extraction progress if it's time
                if last_progress_update.elapsed() >= update_frequency {
                    let _ = window_clone.emit(
                        "extraction_progress",
                        serde_json::json!({
                            "percent": ((i + 1) as f64 / total_zip_files as f64 * 100.0) as u32,
                            "current": i + 1,
                            "total": total_zip_files,
                            "filename": file_name
                        }),
                    );
                    last_progress_update = std::time::Instant::now();
                }

                let out_path = extract_path.join(file_path);
                println!("Output path: {}", out_path.display());

                if !out_path.starts_with(&extract_path) {
                    println!("Security error: zip would extract outside target directory");
                    return Err(
                        "Invalid zip file: path would extract outside target directory".to_string(),
                    );
                }

                if file.is_dir() {
                    println!("Creating directory: {}", out_path.display());
                    match std::fs::create_dir_all(&out_path) {
                        Ok(_) => println!("Created directory successfully"),
                        Err(e) => {
                            println!("Failed to create directory {}: {}", out_path.display(), e);
                            return Err(e.to_string());
                        }
                    }
                } else {
                    if let Some(parent) = out_path.parent() {
                        println!("Ensuring parent directory exists: {}", parent.display());
                        match std::fs::create_dir_all(parent) {
                            Ok(_) => println!("Created parent directory successfully"),
                            Err(e) => {
                                println!(
                                    "Failed to create parent directory {}: {}",
                                    parent.display(),
                                    e
                                );
                                return Err(e.to_string());
                            }
                        }
                    }

                    println!("Creating file: {}", out_path.display());
                    let mut outfile = match File::create(&out_path) {
                        Ok(file) => file,
                        Err(e) => {
                            println!("Failed to create file {}: {}", out_path.display(), e);
                            return Err(e.to_string());
                        }
                    };

                    println!("Copying file content");
                    match std::io::copy(&mut file, &mut outfile) {
                        Ok(bytes) => println!("Copied {} bytes", bytes),
                        Err(e) => {
                            println!("Failed to copy file content: {}", e);
                            return Err(e.to_string());
                        }
                    }
                }
            }

            // Extract notes from manifest
            if let Some(manifest) = &manifest_data {
                if let Some(notes) = &manifest.notes {
                    println!("Found notes in manifest: {}", notes);
                    notes_text = format!(" Notes: {}", notes);
                }
            }

            // Save the hash to track this installation
            println!(
                "Writing installation hash to {}",
                extract_hash_path.display()
            );
            match std::fs::write(&extract_hash_path, &file_hash) {
                Ok(_) => println!("Installation hash written successfully"),
                Err(e) => {
                    println!("Failed to write installation hash: {}", e);
                    return Err(format!("Failed to write installation hash: {}", e));
                }
            }

            // Final extraction progress update
            let _ = window_clone.emit(
                "extraction_progress",
                serde_json::json!({
                    "percent": 100,
                    "current": total_zip_files,
                    "total": total_zip_files,
                    "filename": "Extraction complete"
                }),
            );
        }

        // Return successful result
        let status = if download_needed {
            if need_extraction {
                format!("✅ Downloaded and extracted new version.{}", notes_text)
            } else {
                "✅ Downloaded but extraction skipped (files up to date)".to_string()
            }
        } else if need_extraction {
            format!("✅ Used cached file and extracted.{}", notes_text)
        } else {
            "✅ All files already up to date".to_string()
        };

        println!("Operation completed: {}", status);
        Ok(format!("{} (Hash: {})", status, file_hash))
    })
    .await
    .map_err(|e| {
        println!("Task join error: {}", e);
        format!("Task join error: {}", e)
    })?;

    println!("Returning final result");
    result
}

#[tauri::command]
fn finalize_instance(instance_path: String) -> Result<(), String> {
    let instance_dir = PathBuf::from(instance_path);
    let _mrpack_dir = instance_dir.join("mrpack");
    let mc_dir = instance_dir.join(".minecraft");
    let mods_dir = mc_dir.join("mods");

    // Ensure mods dir exists
    fs::create_dir_all(&mods_dir).map_err(|e| e.to_string())?;

    // Write instance.cfg
    let instance_cfg = r#"[General]
ConfigVersion=1.2
ManagedPack=true
iconKey=modrinth_fabulously-optimized
ManagedPackID=1KVo5zza
ManagedPackType=modrinth
ManagedPackName=Fabulously Optimized
ManagedPackVersionID=iRJMsGhm
ManagedPackVersionName=6.4.0
name=Story
InstanceType=OneSix
"#;
    fs::write(instance_dir.join("instance.cfg"), instance_cfg)
        .map_err(|e| format!("Failed to write instance.cfg: {}", e))?;

    // Write mmc-pack.json
    let mmc_pack_json = r#"{
    "components": [
        {
            "cachedName": "Minecraft",
            "cachedRequires": [
                { "suggests": "3.3.3", "uid": "org.lwjgl3" }
            ],
            "cachedVersion": "1.21.1",
            "important": true,
            "uid": "net.minecraft",
            "version": "1.21.1"
        },
        {
            "cachedName": "Fabric Loader",
            "cachedRequires": [
                { "uid": "net.fabricmc.intermediary" }
            ],
            "cachedVersion": "0.16.14",
            "uid": "net.fabricmc.fabric-loader",
            "version": "0.16.14"
        }
    ],
    "formatVersion": 1
}"#;
    fs::write(instance_dir.join("mmc-pack.json"), mmc_pack_json)
        .map_err(|e| format!("Failed to write mmc-pack.json: {}", e))?;

    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct ModrinthIndex {
    pub files: Vec<ModrinthIndexFile>,
}

#[derive(Serialize, Deserialize)]
pub struct ModrinthIndexFile {
    pub path: String,
    pub hashes: HashMap<String, String>,
    pub downloads: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct StoryManifest {
    pub instance: InstanceConfig,
    pub extra_mods: Option<Vec<ExtraMod>>,
    pub overrides: Option<Vec<Override>>,
}

#[derive(Serialize, Deserialize)]
pub struct InstanceConfig {
    pub name: String,
    pub version: String,
    pub minecraft_version: Option<String>,
    pub loader: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ExtraMod {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Override {
    pub name: String,
    pub url: String,
}

#[tauri::command]
async fn download_from_manifest(
    window: Window,
    manifest_url: String,
    instance_base: String,
) -> Result<String, String> {
    println!("=== DOWNLOAD_FROM_MANIFEST START ===");
    println!("Manifest URL: {}", manifest_url);
    println!("Instance base path: {}", instance_base);
    
    // Clear the tracking list for this download session
    clear_downloaded_files();

    // Validate instance_base path exists
    let instance_base_path = Path::new(&instance_base);
    println!(
        "Checking if instance_base exists: {}",
        instance_base_path.display()
    );
    if !instance_base_path.exists() {
        let error_msg = format!(
            "Instance base path does not exist: {}",
            instance_base_path.display()
        );
        println!("ERROR: {}", error_msg);
        return Err(error_msg);
    }

    // Check if Story instance already exists and has content
    let story_path = instance_base_path.join("Story");
    if story_path.exists() {
        let minecraft_dir = story_path.join(".minecraft");
        let mods_dir = minecraft_dir.join("mods");
        let config_dir = minecraft_dir.join("config");
        
        // Check if instance already has mods and config
        let has_mods = mods_dir.exists() && 
            std::fs::read_dir(&mods_dir).map(|mut dir| dir.next().is_some()).unwrap_or(false);
        let has_config = config_dir.exists() && 
            std::fs::read_dir(&config_dir).map(|mut dir| dir.next().is_some()).unwrap_or(false);
        
        if has_mods || has_config {
            println!("Instance already has content, checking if update is needed...");
            // We'll still proceed to check for updates, but this helps with logging
        }
    }

    // Download and parse the manifest
    println!("Downloading manifest from: {}", manifest_url);
    let client = reqwest::Client::new();
    let manifest_response = client.get(&manifest_url).send().await.map_err(|e| {
        let error_msg = format!("Failed to download manifest: {}", e);
        println!("ERROR: {}", error_msg);
        error_msg
    })?;

    println!("Successfully downloaded manifest, reading content...");
    let manifest_text = manifest_response.text().await.map_err(|e| {
        let error_msg = format!("Failed to read manifest text: {}", e);
        println!("ERROR: {}", error_msg);
        error_msg
    })?;

    println!(
        "Manifest content length: {} characters",
        manifest_text.len()
    );
    println!("Parsing manifest JSON...");
    let manifest: StoryManifest = serde_json::from_str(&manifest_text).map_err(|e| {
        let error_msg = format!("Failed to parse manifest JSON: {}", e);
        println!("ERROR: {}", error_msg);
        error_msg
    })?;

    println!(
        "Successfully parsed manifest for instance: {} v{}",
        manifest.instance.name, manifest.instance.version
    );

    // Step 1: Download the modpack
    println!("=== STEP 1: DOWNLOADING MODPACK ===");
    println!("Calling download_modrinth_modpack with:");
    println!("  - project_name: {}", manifest.instance.name);
    println!("  - version: {}", manifest.instance.version);
    println!("  - instance_base: {}", instance_base);

    // Emit initial progress for modpack download
    let _ = window.emit(
        "download_progress",
        serde_json::json!({
            "percent": 0,
            "current": 0,
            "total": 1,
            "filename": "Starting modpack download...",
            "stage": "modpack"
        }),
    );

    let modpack_result = download_modrinth_modpack(
        window.clone(),
        manifest.instance.name.clone(),
        manifest.instance.version.clone(),
        instance_base.clone(),
    )
    .await
    .map_err(|e| {
        let error_msg = format!("Modpack download failed: {}", e);
        println!("ERROR: {}", error_msg);
        error_msg
    })?;

    println!("Modpack download result: {}", modpack_result);

    // Wait a moment for file operations to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Emit completion for modpack
    let _ = window.emit(
        "download_progress",
        serde_json::json!({
            "percent": 50,
            "current": 1,
            "total": 1,
            "filename": "Modpack download completed",
            "stage": "modpack"
        }),
    );

    // Step 2: Download extra mods if any
    let mut skipped_count = 0;
    if let Some(extra_mods) = &manifest.extra_mods {
        println!(
            "=== STEP 2: DOWNLOADING {} EXTRA MODS ===",
            extra_mods.len()
        );

        // Emit initial progress for extra mods
        let _ = window.emit(
            "download_progress",
            serde_json::json!({
                "percent": 50,
                "current": 0,
                "total": extra_mods.len(),
                "filename": format!("Starting download of {} extra mods...", extra_mods.len()),
                "stage": "extra_mods"
            }),
        );

        let story_path = Path::new(&instance_base).join("Story");
        println!("Story path: {}", story_path.display());

        // Check if Story directory exists
        if !story_path.exists() {
            let error_msg = format!("Story directory does not exist: {}", story_path.display());
            println!("ERROR: {}", error_msg);
            return Err(error_msg);
        }

        let mods_dir = story_path.join(".minecraft").join("mods");
        println!("Mods directory path: {}", mods_dir.display());

        // Check if mods directory exists, create if it doesn't
        if !mods_dir.exists() {
            println!("Mods directory doesn't exist, creating it...");
            std::fs::create_dir_all(&mods_dir).map_err(|e| {
                let error_msg = format!(
                    "Failed to create mods directory {}: {}",
                    mods_dir.display(),
                    e
                );
                println!("ERROR: {}", error_msg);
                error_msg
            })?;
            println!("Successfully created mods directory");
        }

        // Get list of existing mods to avoid duplicates (scan once, not for each mod)
        println!("Scanning existing mods once to avoid duplicates...");
        let existing_mods = get_existing_mod_names(&mods_dir).unwrap_or_else(|e| {
            println!("Warning: Failed to scan existing mods: {}", e);
            HashSet::new()
        });
        println!("Found {} existing mods", existing_mods.len());
        
        // Also check against our tracked files to see if they actually exist
        let tracked_files = if let Ok(files) = DOWNLOADED_FILES.lock() {
            files.clone()
        } else {
            HashSet::new()
        };
        println!("Tracked files from previous sessions: {:?}", tracked_files);

        for (index, extra_mod) in extra_mods.iter().enumerate() {
            let version_display = extra_mod
                .version
                .as_ref()
                .map(|v| v.as_str())
                .unwrap_or("auto-detect");

            // Check if this mod already exists
            let normalized_mod_name = normalize_mod_name(&extra_mod.name);
            println!(
                "Checking if mod '{}' (normalized: '{}') exists in: {:?}",
                extra_mod.name, normalized_mod_name, existing_mods
            );

            // Check for exact match or intelligent partial matching
            let mod_exists = existing_mods.contains(&normalized_mod_name)
                || existing_mods.iter().any(|existing| {
                    // Allow partial matching in both directions for better compatibility
                    // Check if either name contains the other (with minimum length requirement)
                    let min_len = 3; // Reduced from 4 to 3 for better matching
                    let matches = if normalized_mod_name.len() >= min_len && existing.len() >= min_len {
                        // More flexible matching: check if either contains the other
                        // or if they share a significant portion of characters
                        let contains_match = normalized_mod_name.contains(existing) || existing.contains(&normalized_mod_name);
                        let similarity_match = {
                            let shorter = if normalized_mod_name.len() < existing.len() { &normalized_mod_name } else { existing };
                            let longer = if normalized_mod_name.len() >= existing.len() { &normalized_mod_name } else { existing };
                            // If the shorter name is at least 70% of the longer name, consider it a match
                            shorter.len() as f32 / longer.len() as f32 >= 0.7
                        };
                        contains_match || similarity_match
                    } else {
                        // For short names, require exact match
                        normalized_mod_name == *existing
                    };
                    
                    if matches {
                        println!("  → Found match: '{}' matches existing '{}'", normalized_mod_name, existing);
                    }
                    matches
                });
            
            // Additional check: verify the file actually exists on disk
            // This prevents issues when files were deleted but the scanning still finds them
            let file_actually_exists = if mod_exists {
                // Check if any of the matching files actually exist on disk
                let mut found_existing_file = false;
                for existing in &existing_mods {
                    if normalized_mod_name.contains(existing) || existing.contains(&normalized_mod_name) {
                        // Look for a file that matches this pattern
                        let entries = std::fs::read_dir(&mods_dir).unwrap_or_else(|_| {
                            std::fs::read_dir(&mods_dir).unwrap_or_else(|_| {
                                panic!("Cannot read mods directory")
                            })
                        });
                        
                        for entry in entries {
                            if let Ok(entry) = entry {
                                let path = entry.path();
                                if path.is_file() && path.extension().map_or(false, |ext| ext == "jar") {
                                    let filename = path.file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("");
                                    let normalized_filename = normalize_mod_name(filename);
                                    
                                    if normalized_filename.contains(existing) || existing.contains(&normalized_filename) {
                                        found_existing_file = true;
                                        break;
                                    }
                                }
                            }
                        }
                        if found_existing_file {
                            break;
                        }
                    }
                }
                found_existing_file
            } else {
                false
            };
            
            // Use the more accurate check
            let final_mod_exists = mod_exists && file_actually_exists;

            if final_mod_exists {
                println!(
                    "Skipping extra mod {}/{}: {} v{} (already exists)",
                    index + 1,
                    extra_mods.len(),
                    extra_mod.name,
                    version_display
                );
                skipped_count += 1;

                // Emit progress for skipped mod
                let _ = window.emit(
                    "download_progress",
                    serde_json::json!({
                        "percent": 50 + ((index as f64 / extra_mods.len() as f64) * 50.0) as u32,
                        "current": index + 1,
                        "total": extra_mods.len(),
                        "filename": format!("Skipping extra mod ({}/{}): {} (already exists)", index + 1, extra_mods.len(), extra_mod.name),
                        "stage": "extra_mods"
                    }),
                );
                continue;
            }

            println!(
                "Downloading extra mod {}/{}: {} v{}",
                index + 1,
                extra_mods.len(),
                extra_mod.name,
                version_display
            );

            // Emit progress for this extra mod
            let version_text = extra_mod
                .version
                .as_ref()
                .map(|v| format!(" v{}", v))
                .unwrap_or_else(|| " (auto-detect)".to_string());
            let _ = window.emit(
                "download_progress",
                serde_json::json!({
                    "percent": 50 + ((index as f64 / extra_mods.len() as f64) * 50.0) as u32,
                    "current": index + 1,
                    "total": extra_mods.len(),
                    "filename": format!("Downloading extra mod ({}/{}): {}{}", index + 1, extra_mods.len(), extra_mod.name, version_text),
                    "stage": "extra_mods"
                }),
            );

            // Get minecraft version and loader from manifest
            let minecraft_version = manifest
                .instance
                .minecraft_version
                .as_ref()
                .unwrap_or(&"1.21.1".to_string())
                .clone();

            let loader = manifest
                .instance
                .loader
                .as_ref()
                .unwrap_or(&"fabric".to_string())
                .clone();

            let mod_result = download_modrinth_mod(
                window.clone(),
                extra_mod.name.clone(),
                extra_mod.version.clone(),
                minecraft_version,
                loader,
                mods_dir.to_string_lossy().to_string(),
            )
            .await;

            match mod_result {
                Ok(result) => println!("Extra mod downloaded: {}", result),
                Err(e) => {
                    println!("Failed to download extra mod {}: {}", extra_mod.name, e);
                    // Continue with other mods instead of failing completely
                }
            }
        }
    } else {
        println!("=== STEP 2: NO EXTRA MODS TO DOWNLOAD ===");
    }

    // Step 3: Download and extract override files if any
    if let Some(overrides) = &manifest.overrides {
        println!(
            "=== STEP 3: DOWNLOADING {} OVERRIDE FILES ===",
            overrides.len()
        );

        let story_path = Path::new(&instance_base).join("Story");
        let minecraft_dir = story_path.join(".minecraft");

        // Ensure .minecraft directory exists
        std::fs::create_dir_all(&minecraft_dir).map_err(|e| {
            let error_msg = format!(
                "Failed to create .minecraft directory {}: {}",
                minecraft_dir.display(),
                e
            );
            println!("ERROR: {}", error_msg);
            error_msg
        })?;

        for (index, override_item) in overrides.iter().enumerate() {
            println!(
                "Downloading override {}/{}: {} from {}",
                index + 1,
                overrides.len(),
                override_item.name,
                override_item.url
            );

            // Emit progress for this override
            let _ = window.emit(
                "download_progress",
                serde_json::json!({
                    "percent": 75 + ((index as f64 / overrides.len() as f64) * 20.0) as u32,
                    "current": index + 1,
                    "total": overrides.len(),
                    "filename": format!("Downloading override ({}/{}): {}", index + 1, overrides.len(), override_item.name),
                    "stage": "overrides"
                }),
            );

            // Use the existing download_and_extract_zip function
            let extract_result = download_and_extract_zip(
                window.clone(),
                override_item.url.clone(),
                minecraft_dir.to_string_lossy().to_string(),
                false, // Don't force download unless needed
            )
            .await;

            match extract_result {
                Ok(result) => println!("Override extracted: {}", result),
                Err(e) => {
                    println!(
                        "Warning: Failed to download override {}: {}",
                        override_item.name, e
                    );
                    // Continue with other overrides instead of failing completely
                }
            }
        }

        // Emit completion for overrides
        let _ = window.emit(
            "download_progress",
            serde_json::json!({
                "percent": 95,
                "current": overrides.len(),
                "total": overrides.len(),
                "filename": "Override downloads completed",
                "stage": "overrides"
            }),
        );
    } else {
        println!("=== STEP 3: NO OVERRIDE FILES TO DOWNLOAD ===");
    }

    // Step 4: Save version tracking information
    println!("=== STEP 4: SAVING VERSION TRACKING ===");
    let story_path = Path::new(&instance_base).join("Story");
    println!("Story path for version tracking: {}", story_path.display());

    if !story_path.exists() {
        let error_msg = format!(
            "Story directory does not exist for version tracking: {}",
            story_path.display()
        );
        println!("ERROR: {}", error_msg);
        return Err(error_msg);
    }

    let version_file = story_path.join(".current_version.json");
    println!("Version file path: {}", version_file.display());

    let version_info = serde_json::json!({
        "instance_name": manifest.instance.name,
        "instance_version": manifest.instance.version,
        "extra_mods": manifest.extra_mods.as_ref().map(|mods| {
            mods.iter().map(|m| {
                serde_json::json!({
                    "name": m.name,
                    "version": m.version
                })
            }).collect::<Vec<_>>()
        }).unwrap_or_default(),
        "overrides": manifest.overrides.as_ref().map(|overrides| {
            overrides.iter().map(|o| {
                serde_json::json!({
                    "name": o.name,
                    "url": o.url
                })
            }).collect::<Vec<_>>()
        }).unwrap_or_default(),
        "last_updated": chrono::Utc::now().to_rfc3339()
    });

    println!("Writing version info to file...");
    std::fs::write(
        &version_file,
        serde_json::to_string_pretty(&version_info).unwrap(),
    )
    .map_err(|e| {
        let error_msg = format!(
            "Failed to save version info to {}: {}",
            version_file.display(),
            e
        );
        println!("ERROR: {}", error_msg);
        error_msg
    })?;

    println!(
        "Successfully saved version tracking information to: {}",
        version_file.display()
    );

    // Emit final completion event
    let _ = window.emit(
        "download_progress",
        serde_json::json!({
            "percent": 100,
            "current": 1,
            "total": 1,
            "filename": "All downloads completed successfully!",
            "stage": "complete"
        }),
    );

    let total_extra_mods = manifest.extra_mods.as_ref().map_or(0, |m| m.len());
    let total_overrides = manifest.overrides.as_ref().map_or(0, |o| o.len());
    let downloaded_mods = total_extra_mods - skipped_count;

    let final_result = if total_overrides > 0 {
        if skipped_count > 0 {
            format!("✅ Successfully downloaded modpack, {} extra mods ({} downloaded, {} skipped), and {} override files", 
                    total_extra_mods, downloaded_mods, skipped_count, total_overrides)
        } else {
            format!(
                "✅ Successfully downloaded modpack, {} extra mods, and {} override files",
                total_extra_mods, total_overrides
            )
        }
    } else {
        if skipped_count > 0 {
            format!("✅ Successfully downloaded modpack and {} extra mods ({} downloaded, {} skipped as already present)", 
                    total_extra_mods, downloaded_mods, skipped_count)
        } else {
            format!(
                "✅ Successfully downloaded modpack and {} extra mods",
                total_extra_mods
            )
        }
    };

    // Step 4: Cleanup extra JAR files not in manifest
    println!("=== STEP 4: CLEANUP EXTRA JAR FILES ===");
    let cleanup_result = cleanup_extra_jars(&story_path, &manifest).await;
    match cleanup_result {
        Ok(cleaned_count) => {
            if cleaned_count > 0 {
                println!("✅ Cleaned up {} extra JAR files", cleaned_count);
            } else {
                println!("✅ No extra JAR files found to clean up");
            }
        }
        Err(e) => {
            println!("⚠️ Warning: Failed to cleanup extra JAR files: {}", e);
            // Don't fail the entire operation for cleanup issues
        }
    }

    println!("=== DOWNLOAD_FROM_MANIFEST COMPLETE ===");
    println!("Final result: {}", final_result);

    Ok(final_result)
}

#[tauri::command]
async fn download_modrinth_modpack(
    window: Window,
    project_name: String,
    version: String,
    instance_base: String,
) -> Result<String, String> {
    println!(
        "Downloading Modrinth modpack: {} v{}",
        project_name, version
    );

    // Construct the Modrinth API URL
    let api_url = format!(
        "https://api.modrinth.com/v2/project/{}/version/{}",
        project_name, version
    );
    println!("API URL: {}", api_url);

    // Get version info from Modrinth API
    let client = reqwest::Client::new();
    let response = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch modpack info: {}", e))?;

    let version_info: ModrinthVersionResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse modpack info: {}", e))?;

    println!("Found modpack: {}", version_info.name);

    // Find the primary .mrpack file
    let mrpack_file = version_info
        .files
        .iter()
        .find(|f| f.primary && f.filename.ends_with(".mrpack"))
        .ok_or("No primary .mrpack file found")?;

    println!(
        "Found mrpack file: {} ({} bytes)",
        mrpack_file.filename, mrpack_file.size
    );

    // Create the Story instance directory
    let story_path = Path::new(&instance_base).join("Story");
    println!("=== MODPACK: CREATING DIRECTORIES ===");
    println!("Instance base: {}", instance_base);
    println!("Story path to create: {}", story_path.display());

    // Check if instance_base exists and is accessible
    let instance_base_path = Path::new(&instance_base);
    if !instance_base_path.exists() {
        let error_msg = format!(
            "Instance base directory does not exist: {}",
            instance_base_path.display()
        );
        println!("ERROR: {}", error_msg);
        return Err(error_msg);
    }

    println!("Instance base exists, creating Story directory...");
    std::fs::create_dir_all(&story_path).map_err(|e| {
        let error_msg = format!(
            "Failed to create Story directory {}: {}",
            story_path.display(),
            e
        );
        println!("ERROR: {}", error_msg);
        error_msg
    })?;
    println!("Successfully created Story directory");

    // Download the mrpack file
    println!("Downloading mrpack file from: {}", mrpack_file.url);
    let mrpack_response = client
        .get(&mrpack_file.url)
        .send()
        .await
        .map_err(|e| format!("Failed to download mrpack: {}", e))?;

    let mrpack_bytes = mrpack_response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read mrpack bytes: {}", e))?;

    // Extract the mrpack (it's a zip file)
    let cursor = Cursor::new(&mrpack_bytes);
    let mut zip =
        ZipArchive::new(cursor).map_err(|e| format!("Failed to open mrpack as zip: {}", e))?;

    println!("Extracting mrpack with {} files", zip.len());

    // Create necessary directories
    let mrpack_dir = story_path.join("mrpack");
    let minecraft_dir = story_path.join(".minecraft");
    println!("Creating mrpack directory at: {}", mrpack_dir.display());
    std::fs::create_dir_all(&mrpack_dir).map_err(|e| {
        let error_msg = format!(
            "Failed to create mrpack directory {}: {}",
            mrpack_dir.display(),
            e
        );
        println!("{}", error_msg);
        error_msg
    })?;
    println!(
        "Creating minecraft directory at: {}",
        minecraft_dir.display()
    );
    std::fs::create_dir_all(&minecraft_dir).map_err(|e| {
        let error_msg = format!(
            "Failed to create minecraft directory {}: {}",
            minecraft_dir.display(),
            e
        );
        println!("{}", error_msg);
        error_msg
    })?;

    let mut modrinth_index_content = String::new();

    // Extract files from mrpack
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| e.to_string())?;
        let file_name = file.name();

        println!("Extracting: {}", file_name);

        if file_name == "modrinth.index.json" {
            // Save modrinth.index.json to mrpack folder
            file.read_to_string(&mut modrinth_index_content)
                .map_err(|e| e.to_string())?;
            let index_path = mrpack_dir.join("modrinth.index.json");
            std::fs::write(index_path, &modrinth_index_content).map_err(|e| e.to_string())?;
        } else if file_name.starts_with("overrides/") {
            // Extract overrides to .minecraft folder
            let relative_path = file_name.strip_prefix("overrides/").unwrap_or(file_name);
            let output_path = minecraft_dir.join(relative_path);

            if file.is_dir() {
                std::fs::create_dir_all(&output_path).map_err(|e| e.to_string())?;
            } else {
                if let Some(parent) = output_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }

                let mut output_file = File::create(&output_path).map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut output_file).map_err(|e| e.to_string())?;
            }
        }
    }

    // Parse modrinth.index.json and download mods
    if !modrinth_index_content.is_empty() {
        let modrinth_index: ModrinthIndex = serde_json::from_str(&modrinth_index_content)
            .map_err(|e| format!("Failed to parse modrinth.index.json: {}", e))?;

        let mods_dir = minecraft_dir;
        println!("Creating mods directory at: {}", mods_dir.display());
        std::fs::create_dir_all(&mods_dir).map_err(|e| {
            let error_msg = format!(
                "Failed to create mods directory {}: {}",
                mods_dir.display(),
                e
            );
            println!("{}", error_msg);
            error_msg
        })?;

        println!("Downloading {} mod files", modrinth_index.files.len());

        for (index, mod_file) in modrinth_index.files.iter().enumerate() {
            println!(
                "Downloading mod {}/{}: {}",
                index + 1,
                modrinth_index.files.len(),
                mod_file.path
            );

            // Emit progress update to frontend
            let _ = window.emit(
                "download_progress",
                serde_json::json!({
                    "percent": ((index as f64 / modrinth_index.files.len() as f64) * 100.0) as u32,
                    "current": index + 1,
                    "total": modrinth_index.files.len(),
                    "filename": format!("Downloading mod: {}", mod_file.path),
                    "stage": "mods"
                }),
            );

            // Try each download URL until one works
            let mut downloaded = false;
            for url in &mod_file.downloads {
                match client.get(url).send().await {
                    Ok(response) => {
                        let mod_bytes = response.bytes().await.map_err(|e| e.to_string())?;
                        let mod_path = mods_dir.join(&mod_file.path);

                        // Ensure parent directory exists before writing the file
                        if let Some(parent) = mod_path.parent() {
                            println!("Ensuring parent directory exists: {}", parent.display());
                            std::fs::create_dir_all(parent).map_err(|e| {
                                let error_msg = format!(
                                    "Failed to create parent directory {}: {}",
                                    parent.display(),
                                    e
                                );
                                println!("ERROR: {}", error_msg);
                                error_msg
                            })?;
                        }

                        println!("Writing mod file to: {}", mod_path.display());
                        std::fs::write(&mod_path, &mod_bytes).map_err(|e| {
                            let error_msg =
                                format!("Failed to write mod file {}: {}", mod_path.display(), e);
                            println!("ERROR: {}", error_msg);
                            error_msg
                        })?;
                        
                        // Track the downloaded JAR file
                        if let Some(filename) = mod_path.file_name().and_then(|n| n.to_str()) {
                            track_downloaded_file(filename);
                        }
                        downloaded = true;
                        break;
                    }
                    Err(e) => {
                        println!("Failed to download from {}: {}", url, e);
                        continue;
                    }
                }
            }

            if !downloaded {
                println!("Warning: Failed to download mod: {}", mod_file.path);
            }
        }

        // Final progress update for mods download
        let _ = window.emit(
            "download_progress",
            serde_json::json!({
                "percent": 100,
                "current": modrinth_index.files.len(),
                "total": modrinth_index.files.len(),
                "filename": "Mod downloads completed",
                "stage": "mods"
            }),
        );
    }

    // Create instance configuration files
    create_instance_config(&story_path, &version_info)?;

    Ok(format!(
        "✅ Successfully downloaded and extracted modpack: {} v{}",
        project_name, version
    ))
}

#[tauri::command]
async fn download_modrinth_mod(
    window: Window,
    mod_name: String,
    version: Option<String>,
    minecraft_version: String,
    loader: String,
    mods_dir: String,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let mut downloaded_mods = std::collections::HashSet::new();

    let version_info = if let Some(version) = version {
        println!("Downloading mod: {} v{}", mod_name, version);

        // Construct the Modrinth API URL for the specific version
        let api_url = format!(
            "https://api.modrinth.com/v2/project/{}/version/{}",
            mod_name, version
        );
        println!("Mod API URL: {}", api_url);

        // Get version info from Modrinth API
        let response = client
            .get(&api_url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch mod info: {}", e))?;

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse mod info: {}", e))?
    } else {
        println!(
            "Finding best version for mod: {} with Minecraft {} and loader {}",
            mod_name, minecraft_version, loader
        );

        // Find the best version for this Minecraft version and loader
        find_best_mod_version(&client, &mod_name, &minecraft_version, &loader).await?
    };

    println!("Found mod: {}", version_info.name);

    // Mark this mod as downloaded to prevent cycles
    downloaded_mods.insert(version_info.project_id.clone());

    // Download the main mod file
    let main_result = download_single_mod_file(&window, &client, &version_info, &mods_dir).await?;

    // Download dependencies
    println!("Checking dependencies for mod: {}", mod_name);
    if !version_info.dependencies.is_empty() {
        println!("Found {} dependencies", version_info.dependencies.len());

        if let Err(e) = download_mod_dependencies(
            window.clone(),
            client.clone(),
            version_info.dependencies.clone(),
            minecraft_version.clone(),
            loader.clone(),
            mods_dir.clone(),
            downloaded_mods.clone(),
        )
        .await
        {
            println!("Warning: Failed to download some dependencies: {}", e);
        }
    } else {
        println!("No dependencies found for mod: {}", mod_name);
    }

    // Emit completion progress
    let _ = window.emit(
        "download_progress",
        serde_json::json!({
            "percent": 100,
            "current": 1,
            "total": 1,
            "filename": format!("Completed: {}", mod_name),
            "stage": "extra_mods"
        }),
    );

    Ok(format!(
        "✅ Downloaded mod: {} with dependencies",
        main_result
    ))
}

pub fn create_instance_config(
    story_path: &Path,
    version_info: &ModrinthVersionResponse,
) -> Result<(), String> {
    println!("Creating instance configuration files");

    // Create instance.cfg
    let instance_cfg = format!(
        r#"[General]
ConfigVersion=1.2
ManagedPack=true
iconKey=modrinth_{0}
ManagedPackID={1}
ManagedPackType=modrinth
ManagedPackName={2}
ManagedPackVersionID={3}
ManagedPackVersionName={4}
name=Story
InstanceType=OneSix
"#,
        version_info.project_id,
        version_info.project_id,
        version_info.name,
        version_info.id,
        version_info.version_number
    );

    std::fs::write(story_path.join("instance.cfg"), instance_cfg)
        .map_err(|e| format!("Failed to write instance.cfg: {}", e))?;

    // Determine Minecraft version and loader from the version info
    let minecraft_version = version_info
        .game_versions
        .first()
        .ok_or("No game version found")?;
    let loader = version_info.loaders.first().ok_or("No loader found")?;

    // Create mmc-pack.json
    let mmc_pack_json = if loader == "fabric" {
        format!(
            r#"{{
    "components": [
        {{
            "cachedName": "Minecraft",
            "cachedRequires": [
                {{ "suggests": "3.3.3", "uid": "org.lwjgl3" }}
            ],
            "cachedVersion": "{0}",
            "important": true,
            "uid": "net.minecraft",
            "version": "{0}"
        }},
        {{
            "cachedName": "Fabric Loader",
            "cachedRequires": [
                {{ "uid": "net.fabricmc.intermediary" }}
            ],
            "cachedVersion": "0.16.14",
            "uid": "net.fabricmc.fabric-loader",
            "version": "0.16.14"
        }}
    ],
    "formatVersion": 1
}}"#,
            minecraft_version
        )
    } else {
        // Default/NeoForge configuration
        format!(
            r#"{{
    "components": [
        {{
            "cachedName": "Minecraft",
            "cachedVersion": "{0}",
            "important": true,
            "uid": "net.minecraft",
            "version": "{0}"
        }}
    ],
    "formatVersion": 1
}}"#,
            minecraft_version
        )
    };

    std::fs::write(story_path.join("mmc-pack.json"), mmc_pack_json)
        .map_err(|e| format!("Failed to write mmc-pack.json: {}", e))?;

    Ok(())
}

#[tauri::command]
async fn check_manifest_updates(
    _window: Window,
    manifest_url: String,
    instance_base: String,
) -> Result<String, String> {
    println!("Checking for manifest updates from: {}", manifest_url);

    // Download and parse the manifest
    let client = reqwest::Client::new();
    let manifest_response = client
        .get(&manifest_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download manifest: {}", e))?;

    let manifest_text = manifest_response
        .text()
        .await
        .map_err(|e| format!("Failed to read manifest text: {}", e))?;

    let manifest: StoryManifest = serde_json::from_str(&manifest_text)
        .map_err(|e| format!("Failed to parse manifest JSON: {}", e))?;

    println!(
        "Checking updates for: {} v{}",
        manifest.instance.name, manifest.instance.version
    );

    // Check if the Story instance exists
    let story_path = Path::new(&instance_base).join("Story");
    if !story_path.exists() {
        return Ok("Instance not found - needs to be created".to_string());
    }

    // Check if we have a version tracking file
    let version_file = story_path.join(".current_version.json");
    let current_version_info = if version_file.exists() {
        match std::fs::read_to_string(&version_file) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(version_data) => Some(version_data),
                Err(_) => None,
            },
            Err(_) => None,
        }
    } else {
        None
    };

    // If no version file exists, check if files already exist before saying updates are needed
    let current_version_info = match current_version_info {
        Some(info) => info,
        None => {
            // Check if the instance already has the required files
            let minecraft_dir = story_path.join(".minecraft");
            let mods_dir = minecraft_dir.join("mods");
            let config_dir = minecraft_dir.join("config");
            
            // If these directories exist and have content, assume it's already installed
            let has_mods = mods_dir.exists() && 
                std::fs::read_dir(&mods_dir).map(|mut dir| dir.next().is_some()).unwrap_or(false);
            let has_config = config_dir.exists() && 
                std::fs::read_dir(&config_dir).map(|mut dir| dir.next().is_some()).unwrap_or(false);
            
            if has_mods || has_config {
                return Ok("No updates available - files already exist (no version tracking)".to_string());
            } else {
                return Ok(format!("Updates available - no version tracking found"));
            }
        }
    };

    // Extract current version info
    let current_instance_name = current_version_info
        .get("instance_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let current_instance_version = current_version_info
        .get("instance_version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let current_extra_mods = current_version_info
        .get("extra_mods")
        .and_then(|v| v.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);

    // Compare with manifest
    let manifest_extra_mods = manifest.extra_mods.as_ref().map(|m| m.len()).unwrap_or(0);

    let mut update_reasons = Vec::new();

    // Check if instance name or version changed
    if current_instance_name != manifest.instance.name {
        update_reasons.push(format!(
            "Instance name changed: {} -> {}",
            current_instance_name, manifest.instance.name
        ));
    }

    if current_instance_version != manifest.instance.version {
        update_reasons.push(format!(
            "Instance version changed: {} -> {}",
            current_instance_version, manifest.instance.version
        ));
    }

    // Check if extra mods count changed
    if current_extra_mods != manifest_extra_mods {
        update_reasons.push(format!(
            "Extra mods count changed: {} -> {}",
            current_extra_mods, manifest_extra_mods
        ));
    }

    // Check individual mod versions if we have detailed info
    if let (Some(current_mods_array), Some(manifest_mods)) = (
        current_version_info
            .get("extra_mods")
            .and_then(|v| v.as_array()),
        &manifest.extra_mods,
    ) {
        for manifest_mod in manifest_mods {
            let mod_found = current_mods_array.iter().any(|current_mod| {
                let current_name = current_mod.get("name").and_then(|v| v.as_str());
                let current_version = current_mod.get("version").and_then(|v| v.as_str());
                let manifest_name = &manifest_mod.name;
                let manifest_version = manifest_mod.version.as_ref().map(|v| v.as_str());

                current_name == Some(manifest_name) && current_version == manifest_version
            });

            if !mod_found {
                let version_text = manifest_mod
                    .version
                    .as_ref()
                    .map(|v| format!(" v{}", v))
                    .unwrap_or_else(|| " (auto-detect)".to_string());
                update_reasons.push(format!(
                    "Mod update needed: {}{}",
                    manifest_mod.name, version_text
                ));
            }
        }
    }

    if update_reasons.is_empty() {
        Ok("No updates available - everything is up to date".to_string())
    } else {
        Ok(format!("Updates available: {}", update_reasons.join(", ")))
    }
}

// Function to find the best version for a mod given a Minecraft version and loader
async fn find_best_mod_version(
    client: &reqwest::Client,
    mod_name: &str,
    minecraft_version: &str,
    loader: &str,
) -> Result<ModrinthVersionResponse, String> {
    println!(
        "Finding best version for mod {} with Minecraft {} and loader {}",
        mod_name, minecraft_version, loader
    );

    let api_url = format!("https://api.modrinth.com/v2/project/{}/version", mod_name);
    println!("Fetching versions from: {}", api_url);

    let response = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch mod versions: {}", e))?;

    let versions: Vec<ModrinthVersionResponse> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse mod versions: {}", e))?;

    println!("Found {} versions for mod {}", versions.len(), mod_name);

    // Find the first version that supports our Minecraft version and loader
    for version in versions {
        let supports_minecraft = version
            .game_versions
            .contains(&minecraft_version.to_string());
        let supports_loader = version.loaders.contains(&loader.to_string());

        println!(
            "Checking version {} ({}): MC={}, Loader={}, Supports MC={}, Supports Loader={}",
            version.version_number,
            version.id,
            version.game_versions.join(","),
            version.loaders.join(","),
            supports_minecraft,
            supports_loader
        );

        if supports_minecraft && supports_loader {
            println!(
                "Found compatible version: {} ({}) for MC {} and loader {}",
                version.version_number, version.id, minecraft_version, loader
            );
            return Ok(version);
        }
    }

    Err(format!(
        "No compatible version found for mod {} with Minecraft {} and loader {}",
        mod_name, minecraft_version, loader
    ))
}

// Function to download dependencies for a mod
fn download_mod_dependencies(
    window: Window,
    client: reqwest::Client,
    dependencies: Vec<ModrinthDependency>,
    minecraft_version: String,
    loader: String,
    mods_dir: String,
    downloaded_mods: std::collections::HashSet<String>,
) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
    Box::pin(async move {
        let mut downloaded_mods = downloaded_mods;

        for dependency in dependencies {
            // Skip if dependency type is not required
            if dependency.dependency_type != "required" {
                println!(
                    "Skipping non-required dependency: {:?}",
                    dependency.project_id
                );
                continue;
            }

            if let Some(project_id) = &dependency.project_id {
                // Skip if we already downloaded this mod
                if downloaded_mods.contains(project_id) {
                    println!("Dependency {} already downloaded, skipping", project_id);
                    continue;
                }

                println!("Downloading required dependency: {}", project_id);

                // Mark as downloaded to prevent cycles
                downloaded_mods.insert(project_id.clone());

                // Find the best version for this dependency
                match find_best_mod_version(&client, project_id, &minecraft_version, &loader).await
                {
                    Ok(dep_version) => {
                        // Download the dependency
                        match download_single_mod_file(&window, &client, &dep_version, &mods_dir)
                            .await
                        {
                            Ok(_) => {
                                println!("Successfully downloaded dependency: {}", project_id);

                                // Recursively download dependencies of this dependency
                                if let Err(e) = download_mod_dependencies(
                                    window.clone(),
                                    client.clone(),
                                    dep_version.dependencies,
                                    minecraft_version.clone(),
                                    loader.clone(),
                                    mods_dir.clone(),
                                    downloaded_mods.clone(),
                                )
                                .await
                                {
                                    println!(
                                        "Warning: Failed to download sub-dependencies for {}: {}",
                                        project_id, e
                                    );
                                }
                            }
                            Err(e) => {
                                println!(
                                    "Warning: Failed to download dependency {}: {}",
                                    project_id, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!(
                            "Warning: Failed to find compatible version for dependency {}: {}",
                            project_id, e
                        );
                    }
                }
            }
        }

        Ok(())
    })
}

// Function to cleanup extra JAR files not in current manifest
async fn cleanup_extra_jars(story_path: &Path, manifest: &StoryManifest) -> Result<usize, String> {
    let mods_dir = story_path.join(".minecraft").join("mods");
    
    if !mods_dir.exists() {
        println!("Mods directory doesn't exist, skipping cleanup");
        return Ok(0);
    }

    // Get the list of all files that were downloaded in this session (including dependencies)
    let current_session_files = if let Ok(files) = DOWNLOADED_FILES.lock() {
        files.clone()
    } else {
        println!("Warning: Could not access tracked files, skipping cleanup");
        return Ok(0);
    };

    // Save the current manifest locally for future comparison
    let manifest_file = story_path.join(".current_manifest.json");
    let manifest_json = serde_json::to_string_pretty(manifest)
        .map_err(|e| format!("Failed to serialize manifest: {}", e))?;
    
    std::fs::write(&manifest_file, manifest_json)
        .map_err(|e| format!("Failed to save manifest: {}", e))?;
    
    println!("💾 Saved current manifest to: {}", manifest_file.display());

    // Save the complete list of downloaded files (including dependencies) for future comparison
    let downloaded_files_list = story_path.join(".downloaded_files.json");
    let files_json = serde_json::to_string_pretty(&current_session_files)
        .map_err(|e| format!("Failed to serialize downloaded files: {}", e))?;
    
    std::fs::write(&downloaded_files_list, files_json)
        .map_err(|e| format!("Failed to save downloaded files list: {}", e))?;
    
    println!("💾 Saved downloaded files list to: {}", downloaded_files_list.display());
    
    println!("Current session downloaded files ({}): {:?}", current_session_files.len(), current_session_files);
    
    // For now, let's be conservative and only delete files that are clearly problematic
    // We'll implement a more sophisticated cleanup later that compares against previous manifests
    println!("⚠️ Cleanup disabled for now - preserving all existing files");
    println!("📝 All downloaded files in this session are tracked and will be preserved");
    
    Ok(0)
}

// Helper function to extract JAR files from a ZIP file
fn extract_jar_files_from_zip(zip_path: &Path) -> Result<Vec<String>, String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| format!("Failed to open zip file: {}", e))?;
    
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Failed to read zip archive: {}", e))?;
    
    let mut jar_files = Vec::new();
    
    for i in 0..archive.len() {
        let file = archive.by_index(i)
            .map_err(|e| format!("Failed to read zip entry {}: {}", i, e))?;
        
        let name = file.name();
        if name.ends_with(".jar") {
            // Extract just the filename without path
            let filename = std::path::Path::new(name)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(name);
            jar_files.push(filename.to_string());
        }
    }
    
    Ok(jar_files)
}


// Function to download a single mod file
async fn download_single_mod_file(
    window: &Window,
    client: &reqwest::Client,
    version_info: &ModrinthVersionResponse,
    mods_dir: &str,
) -> Result<String, String> {
    // Find the primary .jar file
    let jar_file = version_info
        .files
        .iter()
        .find(|f| f.primary && f.filename.ends_with(".jar"))
        .ok_or("No primary .jar file found")?;

    println!(
        "Downloading jar file: {} ({} bytes)",
        jar_file.filename, jar_file.size
    );

    /*
    // Emit progress update to frontend
    let _ = window.emit(
        "download_progress",
        serde_json::json!({
            "percent": 50,
            "current": 1,
            "total": 1,
            "filename": format!("Downloading: {}", jar_file.filename),
            "stage": "dependencies"
        }),
    );
    */
    // Download the jar file
    let jar_response = client
        .get(&jar_file.url)
        .send()
        .await
        .map_err(|e| format!("Failed to download jar: {}", e))?;

    let jar_bytes = jar_response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read jar bytes: {}", e))?;

    // Ensure mods directory exists
    std::fs::create_dir_all(mods_dir).map_err(|e| e.to_string())?;

    // Save the jar file
    let jar_path = Path::new(mods_dir).join(&jar_file.filename);
    std::fs::write(&jar_path, &jar_bytes).map_err(|e| e.to_string())?;
    
    // Track the downloaded JAR file
    track_downloaded_file(&jar_file.filename);

    Ok(format!(
        "Downloaded: {} ({})",
        jar_file.filename,
        jar_path.display()
    ))
}

// Helper function to get mod names from existing files
fn get_existing_mod_names(mods_dir: &Path) -> Result<std::collections::HashSet<String>, String> {
    let mut existing_mods = std::collections::HashSet::new();

    if !mods_dir.exists() {
        return Ok(existing_mods);
    }

    println!("Scanning existing mods in: {}", mods_dir.display());

    let entries =
        std::fs::read_dir(mods_dir).map_err(|e| format!("Failed to read mods directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.is_file() && path.extension().map_or(false, |ext| ext == "jar") {
            if let Some(file_name) = path.file_stem() {
                if let Some(file_str) = file_name.to_str() {
                    // Extract mod name from filename (remove version numbers and other suffixes)
                    let mod_name = extract_mod_name_from_filename(file_str);
                    let normalized_name = normalize_mod_name(&mod_name);
                    existing_mods.insert(normalized_name.clone());
                    println!(
                        "Found existing mod: {} -> {} -> {}",
                        file_str, mod_name, normalized_name
                    );
                }
            }
        }
    }

    println!("Found {} existing mod files", existing_mods.len());
    Ok(existing_mods)
}

// Helper function to extract mod name from filename
pub fn extract_mod_name_from_filename(filename: &str) -> String {
    let mut name = filename.to_lowercase();

    // Remove common loader suffixes first (case insensitive)
    let loader_patterns = vec![
        "_fabric_",
        "_forge_",
        "_neoforge_",
        "_quilt_",
        "-fabric-",
        "-forge-",
        "-neoforge-",
        "-quilt-",
        "_fabric",
        "_forge",
        "_neoforge",
        "_quilt",
        "-fabric",
        "-forge",
        "-neoforge",
        "-quilt",
    ];

    for pattern in loader_patterns {
        if let Some(pos) = name.find(pattern) {
            name = name[..pos].to_string();
            break;
        }
    }

    // Remove version patterns by looking for common patterns
    // Look for patterns like _v6.1.12_, _mc1.21_, etc.
    let version_patterns = vec!["_v", "_mc", "-v", "-mc", "+v", "+mc"];

    for pattern in version_patterns {
        if let Some(pos) = name.find(pattern) {
            name = name[..pos].to_string();
            break;
        }
    }

    // Also look for pure version numbers at the end
    let parts: Vec<&str> = name.split(&['-', '_', '+'][..]).collect();
    let mut clean_parts = Vec::new();

    for part in parts {
        // Skip if this looks like a version number
        if part.is_empty() {
            continue;
        }

        // Check if this part looks like a version
        if part.chars().next().map_or(false, |c| c.is_ascii_digit()) ||
           part.starts_with("1.") || // Minecraft versions like 1.21.1
           part.chars().all(|c| c.is_ascii_digit() || c == '.')
        {
            // Pure version numbers
            break; // Stop processing once we hit a version-like part
        }

        clean_parts.push(part);
    }

    // Join the clean parts back together
    let result = clean_parts
        .join("-")
        .trim_end_matches('-')
        .trim_end_matches('_')
        .to_string();

    // Final cleanup - normalize separators
    result.replace("_", "-")
}

// Helper function to normalize mod names for comparison
pub fn normalize_mod_name(name: &str) -> String {
    name.to_lowercase()
        .replace("_", "-")
        .replace(" ", "-")
        .replace("--", "-") // Remove double dashes
        .replace("-", "") // Remove all dashes for better matching
        .trim_matches('-') // Remove leading/trailing dashes
        .to_string()
}

// Public wrapper functions for testing
pub fn test_check_story_instance(instance_base: String, folder_name: String) -> bool {
    check_story_instance(instance_base, folder_name)
}

pub fn test_is_base_installed(instance_base: String) -> bool {
    is_base_installed(instance_base)
}

pub fn test_check_path_exists(path: String) -> bool {
    check_path_exists(path)
}

pub fn test_create_story_instance(instance_base: String, folder_name: String) -> Result<String, String> {
    create_story_instance(instance_base, folder_name)
}

pub fn test_finalize_instance(instance_path: String) -> Result<(), String> {
    finalize_instance(instance_path)
}


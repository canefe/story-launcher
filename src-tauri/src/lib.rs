use fs_extra::dir::{copy as copy_dir, CopyOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::copy;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use tauri::Emitter;
use tauri::{Event, Manager, Runtime, Window};
use zip::ZipArchive;
// Add this struct to store file hash information
#[derive(Serialize, Deserialize, Default)]
struct HashRegistry {
    files: HashMap<String, String>, // URL -> hash mapping
}

#[derive(Serialize, Deserialize, Default)]
struct FileHashRegistry {
    files: HashMap<String, FileInfo>, // URL -> file info
}

#[derive(Serialize, Deserialize, Default)]
struct FileInfo {
    hash: String,
    last_modified: String,
}

#[derive(serde::Deserialize)]
struct ModFile {
    path: String,
    downloads: Vec<String>,
}

#[derive(serde::Deserialize)]
struct ModrinthIndex {
    files: Vec<ModFile>,
}

#[derive(Deserialize)]
struct ModpackIndex {
    files: Vec<ModFile>,
}

#[derive(Deserialize)]
struct ManifestFile {
    delete: Option<Vec<String>>,
    notes: Option<String>,
    required_files: Option<Vec<String>>,
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

// create Story instance with configurable folder name
#[tauri::command]
fn create_story_instance(instance_base: String, folder_name: String) -> Result<String, String> {
    let story_path = Path::new(&instance_base).join(&folder_name);
    // Create the Story instance directory
    std::fs::create_dir_all(&story_path).map_err(|e| e.to_string())?;
    println!("Created instance at: {:?}", story_path);
    println!("Finalizing instance at {}", instance_base);
    
    // Use path joining for cross-platform compatibility
    let full_path = format!("{}\\{}", instance_base, folder_name);
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
fn verify_extraction_integrity(extract_path: &Path, manifest_data: &Option<ManifestFile>) -> Result<bool, String> {
    println!("Verifying extraction integrity");
    
    // Check if we have manifest requirements to verify
    if let Some(manifest) = manifest_data {
        if let Some(required_files) = &manifest.required_files {
            println!("Checking {} required files from manifest", required_files.len());
            
            for (index, relative_path) in required_files.iter().enumerate() {
                let full_path = extract_path.join(relative_path);
                println!("Checking required file {}/{}: {}", index + 1, required_files.len(), full_path.display());
                
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
        println!("Hash marker path: {}", extract_hash_path.display());        // Initialize manifest_data earlier in the code flow
        let mut manifest_data: Option<ManifestFile> = None;

        // Try to find and parse the manifest file from the zip before extraction
        println!("Looking for manifest.json in zip for verification");
        let mut file_for_manifest = match File::open(&cached_file_path) {
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
                    match serde_json::from_str::<ManifestFile>(&manifest_content) {
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
            println!("Zip archive contains {} files", total_files);            // Check for manifest.json again, but no need to re-initialize
            println!("Looking for manifest.json in zip");
            
            // Only re-read manifest if we couldn't read it earlier
            if manifest_data.is_none() {
                // Try to find and parse the manifest file
                match zip.by_name("manifest.json") {
                    Ok(mut manifest_file) => {
                        println!("Found manifest.json, reading content");
                        let mut manifest_content = String::new();
                        if manifest_file.read_to_string(&mut manifest_content).is_ok() {
                            match serde_json::from_str::<ManifestFile>(&manifest_content) {
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
    let mrpack_dir = instance_dir.join("mrpack");
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

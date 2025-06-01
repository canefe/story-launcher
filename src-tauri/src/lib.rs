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
        "Update available: {} (Last modified: {})",
        update_available, last_modified
    ))
}

#[tauri::command]
fn check_story_instance(instance_base: String) -> bool {
    let story_path = Path::new(&instance_base).join("Story");
    // Check if the Story instance directory exists
    // log
    println!("Checking for Story instance at: {:?}", story_path);
    story_path.exists()
}

// create Story instance
#[tauri::command]
fn create_story_instance(instance_base: String) -> Result<String, String> {
    let story_path = Path::new(&instance_base).join("Story");
    // Create the Story instance directory
    std::fs::create_dir_all(&story_path).map_err(|e| e.to_string())?;
    println!("Created Story instance at: {:?}", story_path);
    Ok(story_path.to_string_lossy().into_owned())
}

#[tauri::command]
async fn download_and_extract_zip(
    window: Window,
    download_url: String,
    extract_path: String,
    force_download: bool,
) -> Result<String, String> {
    // Clone values that need to be moved into the task
    let window_clone = window.clone();

    // First, check the Last-Modified header from the server
    let client = reqwest::Client::new();
    let resp = client
        .head(&download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let last_modified = resp
        .headers()
        .get("Last-Modified")
        .map(|h| h.to_str().unwrap_or_default())
        .unwrap_or_default()
        .to_string();

    // Use tokio's spawn_blocking for file operations that can't be async
    let result = tokio::task::spawn_blocking(move || {
        // Create cache directory inside the app's data directory
        let app_data_dir = window_clone
            .app_handle()
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?;
        let cache_dir = app_data_dir.join("cache");
        std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

        // Path to the hash registry file
        let hash_registry_path = cache_dir.join("hash_registry.json");

        // Load existing hash registry or create a new one
        let mut registry: FileHashRegistry = if hash_registry_path.exists() {
            let registry_content = std::fs::read_to_string(&hash_registry_path)
                .map_err(|e| format!("Failed to read hash registry: {}", e))?;
            serde_json::from_str(&registry_content).unwrap_or_default()
        } else {
            FileHashRegistry::default()
        };

        // Generate filename from URL
        let url_parts: Vec<&str> = download_url
            .split('?')
            .next()
            .unwrap_or(&download_url)
            .split('/')
            .collect();
        let filename = url_parts.last().unwrap_or(&"update.zip");
        let cached_file_path = cache_dir.join(filename);

        // Check if we need to download based on existence, hash, and last-modified
        let file_info = registry.files.get(&download_url);
        let file_exists = cached_file_path.exists()
            && std::fs::metadata(&cached_file_path)
                .map(|m| m.len() > 0)
                .unwrap_or(false);

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

        let mut file_hash = String::new();

        if download_needed {
            // Download the file to cache
            let download_url_clone = download_url.clone();
            let mut resp =
                reqwest::blocking::get(&download_url_clone).map_err(|e| e.to_string())?;
            let total_size = resp.content_length().ok_or("Couldn't get content length")?;

            let mut file = File::create(&cached_file_path).map_err(|e| e.to_string())?;
            let mut downloaded = 0u64;
            let mut buffer = [0u8; 8192];
            let mut hasher = Sha256::new();

            while let Ok(n) = resp.read(&mut buffer) {
                if n == 0 {
                    break;
                }

                // Update hash calculation
                hasher.update(&buffer[..n]);

                // Write to file
                file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
                downloaded += n as u64;

                // Send progress updates
                let pct = (downloaded as f64 / total_size as f64) * 100.0;
                let _ = window.emit(
                    "download_progress",
                    serde_json::json!({
                        "percent": pct as u32,
                        "downloaded": downloaded,
                        "total": total_size
                    }),
                );
            }

            // Finalize hash and save it
            file_hash = format!("{:x}", hasher.finalize());
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

            file.flush().map_err(|e| e.to_string())?;
        } else if let Some(file_info) = registry.files.get(&download_url) {
            // Use cached file, verify its hash
            file_hash = file_info.hash.clone();

            // Report 100% progress for existing file
            let size = std::fs::metadata(&cached_file_path)
                .map(|m| m.len())
                .unwrap_or(0);

            let _ = window.emit(
                "download_progress",
                serde_json::json!({
                    "percent": 100,
                    "downloaded": size,
                    "total": size
                }),
            );
        }

        // Now extract from the cached file
        let file = File::open(&cached_file_path).map_err(|e| e.to_string())?;
        let extract_path = dunce::canonicalize(extract_path).map_err(|e| e.to_string())?;

        // Create a hash file at the extract location to track what version is installed
        let extract_hash_path = extract_path.join(".installed_hash");
        let need_extraction = force_download
            || !extract_hash_path.exists()
            || std::fs::read_to_string(&extract_hash_path).unwrap_or_default() != file_hash;

        let mut notes_text = String::new();

        if need_extraction {
            // Extract files
            let mut zip = ZipArchive::new(file).map_err(|e| e.to_string())?;
            let total_files = zip.len();

            // Check for manifest.json first
            let mut manifest_data: Option<ManifestFile> = None;
            
            // Try to find and parse the manifest file
            if let Ok(mut manifest_file) = zip.by_name("manifest.json") {
                let mut manifest_content = String::new();
                if manifest_file.read_to_string(&mut manifest_content).is_ok() {
                    if let Ok(manifest) = serde_json::from_str::<ManifestFile>(&manifest_content) {
                        manifest_data = Some(manifest);
                        
                        // Log that we found a manifest
                        println!("Found manifest.json with delete instructions");
                    }
                }
            }
            
            // Process deletion requests from manifest
            if let Some(manifest) = &manifest_data {
                if let Some(delete_list) = &manifest.delete {
                    let _ = window.emit(
                        "extraction_progress",
                        serde_json::json!({
                            "percent": 5,
                            "current": 0,
                            "total": delete_list.len(),
                            "filename": "Processing deletion requests..."
                        }),
                    );
                    
                    for (index, file_path) in delete_list.iter().enumerate() {
                        let full_path = extract_path.join(file_path);
                        
                        // Security check - prevent path traversal
                        if !full_path.starts_with(&extract_path) {
                            println!("Security warning: Attempted deletion outside extract path: {}", file_path);
                            continue;
                        }
                        
                        // Log deletion attempt
                        println!("Deleting file: {}", full_path.display());
                        
                        // Delete file if it exists
                        if full_path.exists() {
                            if full_path.is_dir() {
                                let _ = fs::remove_dir_all(&full_path);
                            } else {
                                let _ = fs::remove_file(&full_path);
                            }
                        }
                        
                        // Report progress
                        let _ = window.emit(
                            "deletion_progress",
                            serde_json::json!({
                                "percent": ((index + 1) as f64 / delete_list.len() as f64 * 100.0) as u32,
                                "current": index + 1,
                                "total": delete_list.len(),
                                "filename": file_path
                            }),
                        );
                    }
                    
                    // Report completion of deletion phase
                    let _ = window.emit(
                        "extraction_progress",
                        serde_json::json!({
                            "percent": 10,
                            "current": 0,
                            "total": total_files,
                            "filename": "Deletions complete, starting extraction..."
                        }),
                    );
                }
            }

            // Extract all files from the zip archive
            for i in 0..zip.len() {
                let mut file = zip.by_index(i).map_err(|e| e.to_string())?;
                let file_name = file.name().to_string();

                // Skip manifest.json if it exists
                if file_name == "manifest.json" {
                    continue;
                }
                
                let file_path = Path::new(file.name());

                // Emit extraction progress event
                let _ = window.emit(
                    "extraction_progress",
                    serde_json::json!({
                        "percent": ((i + 1) as f64 / total_files as f64 * 100.0) as u32,
                        "current": i + 1,
                        "total": total_files,
                        "filename": file.name()
                    }),
                );

                // Security checks
                if file_path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(
                        "Invalid zip file: contains directory traversal patterns".to_string()
                    );
                }

                let out_path = extract_path.join(file_path);

                if !out_path.starts_with(&extract_path) {
                    return Err(
                        "Invalid zip file: path would extract outside target directory".to_string(),
                    );
                }

                if file.is_dir() {
                    std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
                } else {
                    if let Some(parent) = out_path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }

                    let mut outfile = File::create(&out_path).map_err(|e| e.to_string())?;
                    std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
                }
            }

            if let Some(manifest) = &manifest_data {
                if let Some(notes) = &manifest.notes {
                    notes_text = format!(" Notes: {}", notes);
                }
            }


            // Save the hash to track this installation
            std::fs::write(&extract_hash_path, &file_hash)
                .map_err(|e| format!("Failed to write installation hash: {}", e))?;
        } else {
            // Files already extracted with correct version
            let _ = window.emit(
                "extraction_progress",
                serde_json::json!({
                    "percent": 100,
                    "current": 1,
                    "total": 1,
                    "filename": "Files already up to date"
                }),
            );
        }

        finalize_instance(extract_path.to_string_lossy().into_owned())
            .map_err(|e| format!("Failed to finalize instance: {}", e))?;

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

        Ok(format!("{} (Hash: {})", status, file_hash))
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?;

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

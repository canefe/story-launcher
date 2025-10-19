use story_launcher_lib::*;
use std::path::Path;
use tempfile::TempDir;
use std::fs;

// Integration tests for complex workflows
#[test]
fn test_manifest_parsing_workflow() {
    let manifest_json = r#"{
        "instance": {
            "name": "Test Pack",
            "version": "1.0.0",
            "minecraft_version": "1.21.1",
            "loader": "fabric"
        },
        "extra_mods": [
            {
                "name": "jei",
                "version": "12.3.0.0"
            },
            {
                "name": "modmenu"
            }
        ],
        "overrides": [
            {
                "name": "config",
                "url": "https://example.com/config.zip"
            }
        ]
    }"#;
    
    let manifest: StoryManifest = serde_json::from_str(manifest_json).unwrap();
    
    assert_eq!(manifest.instance.name, "Test Pack");
    assert_eq!(manifest.instance.version, "1.0.0");
    assert_eq!(manifest.instance.minecraft_version, Some("1.21.1".to_string()));
    assert_eq!(manifest.instance.loader, Some("fabric".to_string()));
    
    assert_eq!(manifest.extra_mods.as_ref().unwrap().len(), 2);
    assert_eq!(manifest.extra_mods.as_ref().unwrap()[0].name, "jei");
    assert_eq!(manifest.extra_mods.as_ref().unwrap()[0].version, Some("12.3.0.0".to_string()));
    assert_eq!(manifest.extra_mods.as_ref().unwrap()[1].name, "modmenu");
    assert_eq!(manifest.extra_mods.as_ref().unwrap()[1].version, None);
    
    assert_eq!(manifest.overrides.as_ref().unwrap().len(), 1);
    assert_eq!(manifest.overrides.as_ref().unwrap()[0].name, "config");
    assert_eq!(manifest.overrides.as_ref().unwrap()[0].url, "https://example.com/config.zip");
}

#[test]
fn test_modrinth_version_response_parsing() {
    let version_json = r#"{
        "game_versions": ["1.21.1"],
        "loaders": ["fabric"],
        "id": "test-version-id",
        "project_id": "test-project",
        "name": "Test Mod",
        "version_number": "1.0.0",
        "changelog": "Test changelog",
        "files": [
            {
                "hashes": {
                    "sha256": "abc123def456"
                },
                "url": "https://example.com/mod.jar",
                "filename": "test-mod.jar",
                "primary": true,
                "size": 1024
            }
        ],
        "dependencies": [
            {
                "version_id": "dep-version-id",
                "project_id": "dep-project",
                "file_name": "dep-mod.jar",
                "dependency_type": "required"
            }
        ]
    }"#;
    
    let version: ModrinthVersionResponse = serde_json::from_str(version_json).unwrap();
    
    assert_eq!(version.game_versions, vec!["1.21.1"]);
    assert_eq!(version.loaders, vec!["fabric"]);
    assert_eq!(version.id, "test-version-id");
    assert_eq!(version.project_id, "test-project");
    assert_eq!(version.name, "Test Mod");
    assert_eq!(version.version_number, "1.0.0");
    assert_eq!(version.changelog, Some("Test changelog".to_string()));
    
    assert_eq!(version.files.len(), 1);
    assert_eq!(version.files[0].filename, "test-mod.jar");
    assert!(version.files[0].primary);
    assert_eq!(version.files[0].size, 1024);
    
    assert_eq!(version.dependencies.len(), 1);
    assert_eq!(version.dependencies[0].dependency_type, "required");
}

#[test]
fn test_modrinth_index_parsing() {
    let index_json = r#"{
        "files": [
            {
                "path": "mods/test-mod.jar",
                "hashes": {
                    "sha256": "def456ghi789"
                },
                "downloads": [
                    "https://example.com/download1",
                    "https://example.com/download2"
                ]
            },
            {
                "path": "mods/another-mod.jar",
                "hashes": {
                    "sha256": "ghi789jkl012"
                },
                "downloads": [
                    "https://example.com/download3"
                ]
            }
        ]
    }"#;
    
    let index: ModrinthIndex = serde_json::from_str(index_json).unwrap();
    
    assert_eq!(index.files.len(), 2);
    assert_eq!(index.files[0].path, "mods/test-mod.jar");
    assert_eq!(index.files[0].downloads.len(), 2);
    assert_eq!(index.files[1].path, "mods/another-mod.jar");
    assert_eq!(index.files[1].downloads.len(), 1);
}

#[test]
fn test_file_hash_registry_workflow() {
    let mut registry = FileHashRegistry::default();
    
    // Add a file to the registry
    let file_info = FileInfo {
        hash: "abc123def456".to_string(),
        last_modified: "Wed, 21 Oct 2015 07:28:00 GMT".to_string(),
    };
    registry.files.insert("https://example.com/file.zip".to_string(), file_info);
    
    // Serialize and deserialize
    let json = serde_json::to_string(&registry).unwrap();
    let deserialized: FileHashRegistry = serde_json::from_str(&json).unwrap();
    
    assert_eq!(registry.files.len(), deserialized.files.len());
    assert!(deserialized.files.contains_key("https://example.com/file.zip"));
    
    let stored_info = deserialized.files.get("https://example.com/file.zip").unwrap();
    assert_eq!(stored_info.hash, "abc123def456");
    assert_eq!(stored_info.last_modified, "Wed, 21 Oct 2015 07:28:00 GMT");
}

#[test]
fn test_legacy_manifest_workflow() {
    let manifest = LegacyManifestFile {
        delete: Some(vec![
            "old-config.json".to_string(),
            "outdated-mod.jar".to_string(),
        ]),
        notes: Some("This is a test manifest with cleanup instructions".to_string()),
        required_files: Some(vec![
            "essential-mod.jar".to_string(),
            "config/settings.json".to_string(),
        ]),
    };
    
    // Test serialization
    let json = serde_json::to_string(&manifest).unwrap();
    let deserialized: LegacyManifestFile = serde_json::from_str(&json).unwrap();
    
    assert_eq!(manifest.delete, deserialized.delete);
    assert_eq!(manifest.notes, deserialized.notes);
    assert_eq!(manifest.required_files, deserialized.required_files);
    
    // Verify the content
    assert_eq!(deserialized.delete.as_ref().unwrap().len(), 2);
    assert!(deserialized.delete.as_ref().unwrap().contains(&"old-config.json".to_string()));
    assert!(deserialized.delete.as_ref().unwrap().contains(&"outdated-mod.jar".to_string()));
    
    assert_eq!(deserialized.notes, Some("This is a test manifest with cleanup instructions".to_string()));
    
    assert_eq!(deserialized.required_files.as_ref().unwrap().len(), 2);
    assert!(deserialized.required_files.as_ref().unwrap().contains(&"essential-mod.jar".to_string()));
    assert!(deserialized.required_files.as_ref().unwrap().contains(&"config/settings.json".to_string()));
}

#[test]
fn test_instance_config_creation() {
    let temp_dir = TempDir::new().unwrap();
    let instance_path = temp_dir.path();
    
    // Create a mock version info
    let version_info = ModrinthVersionResponse {
        game_versions: vec!["1.21.1".to_string()],
        loaders: vec!["fabric".to_string()],
        id: "test-version-id".to_string(),
        project_id: "test-project".to_string(),
        name: "Test Modpack".to_string(),
        version_number: "1.0.0".to_string(),
        changelog: None,
        files: vec![],
        dependencies: vec![],
    };
    
    // Test instance config creation
    let result = create_instance_config(instance_path, &version_info);
    assert!(result.is_ok());
    
    // Verify instance.cfg was created
    let instance_cfg_path = instance_path.join("instance.cfg");
    assert!(instance_cfg_path.exists());
    
    let instance_cfg_content = fs::read_to_string(&instance_cfg_path).unwrap();
    assert!(instance_cfg_content.contains("name=Story"));
    assert!(instance_cfg_content.contains("ManagedPackID=test-project"));
    assert!(instance_cfg_content.contains("ManagedPackName=Test Modpack"));
    
    // Verify mmc-pack.json was created
    let mmc_pack_path = instance_path.join("mmc-pack.json");
    assert!(mmc_pack_path.exists());
    
    let mmc_pack_content = fs::read_to_string(&mmc_pack_path).unwrap();
    assert!(mmc_pack_content.contains("\"cachedVersion\": \"1.21.1\""));
    assert!(mmc_pack_content.contains("\"cachedName\": \"Fabric Loader\""));
}

#[test]
fn test_mod_name_processing_workflow() {
    let test_filenames = vec![
        "fabric-api-0.91.0+1.21.1.jar",
        "jei-12.3.0.0.jar",
        "modmenu-8.0.0+1.21.1.jar",
        "sodium-fabric-mc1.21.1-0.5.8.jar",
        "iris-mc1.21.1-1.6.4.jar",
    ];
    
    for filename in test_filenames {
        // Extract mod name
        let mod_name = extract_mod_name_from_filename(filename);
        
        // Normalize mod name
        let normalized = normalize_mod_name(&mod_name);
        
        // Verify the normalized name is clean
        assert!(!normalized.contains("_"));
        assert!(!normalized.contains(" "));
        assert!(!normalized.contains("--"));
        assert!(!normalized.chars().any(|c| c.is_ascii_digit()));
        
        // Verify it's not empty
        assert!(!normalized.is_empty());
    }
}

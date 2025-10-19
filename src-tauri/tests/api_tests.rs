use story_launcher_lib::*;
use std::collections::HashMap;

// Tests for API-related functionality and data structures
#[test]
fn test_modrinth_version_response_structure() {
    let mut hashes = HashMap::new();
    hashes.insert("sha256".to_string(), "abc123def456".to_string());
    hashes.insert("sha1".to_string(), "def456ghi789".to_string());
    
    let version = ModrinthVersionResponse {
        game_versions: vec!["1.21.1".to_string(), "1.21.0".to_string()],
        loaders: vec!["fabric".to_string(), "quilt".to_string()],
        id: "version-123".to_string(),
        project_id: "project-456".to_string(),
        name: "Test Mod".to_string(),
        version_number: "1.0.0".to_string(),
        changelog: Some("Added new features".to_string()),
        files: vec![
            ModrinthFile {
                hashes: hashes.clone(),
                url: "https://example.com/mod.jar".to_string(),
                filename: "test-mod.jar".to_string(),
                primary: true,
                size: 1024,
            },
            ModrinthFile {
                hashes: hashes.clone(),
                url: "https://example.com/mod-sources.jar".to_string(),
                filename: "test-mod-sources.jar".to_string(),
                primary: false,
                size: 2048,
            },
        ],
        dependencies: vec![
            ModrinthDependency {
                version_id: Some("dep-version-1".to_string()),
                project_id: Some("dep-project-1".to_string()),
                file_name: Some("dep-mod.jar".to_string()),
                dependency_type: "required".to_string(),
            },
            ModrinthDependency {
                version_id: None,
                project_id: Some("optional-dep".to_string()),
                file_name: None,
                dependency_type: "optional".to_string(),
            },
        ],
    };
    
    // Test serialization
    let json = serde_json::to_string(&version).unwrap();
    let deserialized: ModrinthVersionResponse = serde_json::from_str(&json).unwrap();
    
    assert_eq!(version.game_versions, deserialized.game_versions);
    assert_eq!(version.loaders, deserialized.loaders);
    assert_eq!(version.id, deserialized.id);
    assert_eq!(version.project_id, deserialized.project_id);
    assert_eq!(version.name, deserialized.name);
    assert_eq!(version.version_number, deserialized.version_number);
    assert_eq!(version.changelog, deserialized.changelog);
    
    assert_eq!(version.files.len(), deserialized.files.len());
    assert_eq!(version.files[0].filename, deserialized.files[0].filename);
    assert_eq!(version.files[0].primary, deserialized.files[0].primary);
    assert_eq!(version.files[1].filename, deserialized.files[1].filename);
    assert_eq!(version.files[1].primary, deserialized.files[1].primary);
    
    assert_eq!(version.dependencies.len(), deserialized.dependencies.len());
    assert_eq!(version.dependencies[0].dependency_type, deserialized.dependencies[0].dependency_type);
    assert_eq!(version.dependencies[1].dependency_type, deserialized.dependencies[1].dependency_type);
}

#[test]
fn test_modrinth_file_validation() {
    let mut hashes = HashMap::new();
    hashes.insert("sha256".to_string(), "a".repeat(64)); // Valid SHA256
    hashes.insert("sha1".to_string(), "b".repeat(40)); // Valid SHA1
    
    let file = ModrinthFile {
        hashes,
        url: "https://example.com/valid-file.jar".to_string(),
        filename: "valid-file.jar".to_string(),
        primary: true,
        size: 1024,
    };
    
    // Test that the file structure is valid
    assert!(file.url.starts_with("https://"));
    assert!(file.filename.ends_with(".jar"));
    assert!(file.size > 0);
    assert!(file.primary);
    
    // Test serialization
    let json = serde_json::to_string(&file).unwrap();
    let deserialized: ModrinthFile = serde_json::from_str(&json).unwrap();
    
    assert_eq!(file.url, deserialized.url);
    assert_eq!(file.filename, deserialized.filename);
    assert_eq!(file.primary, deserialized.primary);
    assert_eq!(file.size, deserialized.size);
    assert_eq!(file.hashes, deserialized.hashes);
}

#[test]
fn test_modrinth_dependency_types() {
    let dependency_types = vec!["required", "optional", "incompatible", "embedded"];
    
    for dep_type in dependency_types {
        let dependency = ModrinthDependency {
            version_id: Some("v1.0.0".to_string()),
            project_id: Some("test-mod".to_string()),
            file_name: Some("test-mod.jar".to_string()),
            dependency_type: dep_type.to_string(),
        };
        
        let json = serde_json::to_string(&dependency).unwrap();
        let deserialized: ModrinthDependency = serde_json::from_str(&json).unwrap();
        
        assert_eq!(dependency.dependency_type, deserialized.dependency_type);
        assert_eq!(dependency.version_id, deserialized.version_id);
        assert_eq!(dependency.project_id, deserialized.project_id);
        assert_eq!(dependency.file_name, deserialized.file_name);
    }
}

#[test]
fn test_modrinth_index_file_structure() {
    let mut hashes = HashMap::new();
    hashes.insert("sha256".to_string(), "abc123".to_string());
    hashes.insert("sha1".to_string(), "def456".to_string());
    
    let index_file = ModrinthIndexFile {
        path: "mods/test-mod.jar".to_string(),
        hashes: hashes.clone(),
        downloads: vec![
            "https://cdn.modrinth.com/data/test/versions/1.0.0/test-mod.jar".to_string(),
            "https://backup.example.com/test-mod.jar".to_string(),
        ],
    };
    
    // Test structure validation
    assert!(index_file.path.starts_with("mods/"));
    assert!(index_file.path.ends_with(".jar"));
    assert!(!index_file.downloads.is_empty());
    assert!(index_file.downloads[0].contains("modrinth.com"));
    
    // Test serialization
    let json = serde_json::to_string(&index_file).unwrap();
    let deserialized: ModrinthIndexFile = serde_json::from_str(&json).unwrap();
    
    assert_eq!(index_file.path, deserialized.path);
    assert_eq!(index_file.hashes, deserialized.hashes);
    assert_eq!(index_file.downloads, deserialized.downloads);
}

#[test]
fn test_modrinth_index_complete_structure() {
    let mut hashes1 = HashMap::new();
    hashes1.insert("sha256".to_string(), "hash1".to_string());
    
    let mut hashes2 = HashMap::new();
    hashes2.insert("sha256".to_string(), "hash2".to_string());
    
    let index = ModrinthIndex {
        files: vec![
            ModrinthIndexFile {
                path: "mods/mod1.jar".to_string(),
                hashes: hashes1,
                downloads: vec!["https://example.com/mod1.jar".to_string()],
            },
            ModrinthIndexFile {
                path: "mods/mod2.jar".to_string(),
                hashes: hashes2,
                downloads: vec![
                    "https://example.com/mod2.jar".to_string(),
                    "https://backup.example.com/mod2.jar".to_string(),
                ],
            },
        ],
    };
    
    assert_eq!(index.files.len(), 2);
    assert_eq!(index.files[0].path, "mods/mod1.jar");
    assert_eq!(index.files[1].path, "mods/mod2.jar");
    assert_eq!(index.files[1].downloads.len(), 2);
    
    // Test serialization
    let json = serde_json::to_string(&index).unwrap();
    let deserialized: ModrinthIndex = serde_json::from_str(&json).unwrap();
    
    assert_eq!(index.files.len(), deserialized.files.len());
    for (original, deserialized) in index.files.iter().zip(deserialized.files.iter()) {
        assert_eq!(original.path, deserialized.path);
        assert_eq!(original.hashes, deserialized.hashes);
        assert_eq!(original.downloads, deserialized.downloads);
    }
}

#[test]
fn test_story_manifest_complete_workflow() {
    let manifest = StoryManifest {
        instance: InstanceConfig {
            name: "Fabulously Optimized".to_string(),
            version: "6.4.0".to_string(),
            minecraft_version: Some("1.21.1".to_string()),
            loader: Some("fabric".to_string()),
        },
        extra_mods: Some(vec![
            ExtraMod {
                name: "jei".to_string(),
                version: Some("12.3.0.0".to_string()),
            },
            ExtraMod {
                name: "modmenu".to_string(),
                version: None, // Auto-detect version
            },
            ExtraMod {
                name: "wthit".to_string(),
                version: Some("7.2.0".to_string()),
            },
        ]),
        overrides: Some(vec![
            Override {
                name: "config".to_string(),
                url: "https://example.com/config-override.zip".to_string(),
            },
            Override {
                name: "resourcepacks".to_string(),
                url: "https://example.com/resourcepacks.zip".to_string(),
            },
        ]),
    };
    
    // Test serialization
    let json = serde_json::to_string(&manifest).unwrap();
    let deserialized: StoryManifest = serde_json::from_str(&json).unwrap();
    
    // Verify instance config
    assert_eq!(deserialized.instance.name, "Fabulously Optimized");
    assert_eq!(deserialized.instance.version, "6.4.0");
    assert_eq!(deserialized.instance.minecraft_version, Some("1.21.1".to_string()));
    assert_eq!(deserialized.instance.loader, Some("fabric".to_string()));
    
    // Verify extra mods
    let extra_mods = deserialized.extra_mods.unwrap();
    assert_eq!(extra_mods.len(), 3);
    assert_eq!(extra_mods[0].name, "jei");
    assert_eq!(extra_mods[0].version, Some("12.3.0.0".to_string()));
    assert_eq!(extra_mods[1].name, "modmenu");
    assert_eq!(extra_mods[1].version, None);
    assert_eq!(extra_mods[2].name, "wthit");
    assert_eq!(extra_mods[2].version, Some("7.2.0".to_string()));
    
    // Verify overrides
    let overrides = deserialized.overrides.unwrap();
    assert_eq!(overrides.len(), 2);
    assert_eq!(overrides[0].name, "config");
    assert_eq!(overrides[0].url, "https://example.com/config-override.zip");
    assert_eq!(overrides[1].name, "resourcepacks");
    assert_eq!(overrides[1].url, "https://example.com/resourcepacks.zip");
}

#[test]
fn test_file_info_tracking() {
    let file_info = FileInfo {
        hash: "sha256hash1234567890abcdef".to_string(),
        last_modified: "Wed, 21 Oct 2015 07:28:00 GMT".to_string(),
    };
    
    // Test serialization
    let json = serde_json::to_string(&file_info).unwrap();
    let deserialized: FileInfo = serde_json::from_str(&json).unwrap();
    
    assert_eq!(file_info.hash, deserialized.hash);
    assert_eq!(file_info.last_modified, deserialized.last_modified);
    
    // Test that the hash looks like a valid hash
    assert!(file_info.hash.len() > 10);
    assert!(file_info.hash.chars().all(|c| c.is_ascii_alphanumeric()));
    
    // Test that the last_modified looks like a valid HTTP date
    assert!(file_info.last_modified.contains("GMT"));
    assert!(file_info.last_modified.contains("Oct"));
}

use story_launcher_lib::*;
use std::collections::HashMap;
use std::path::Path;
use tempfile::TempDir;

// Test the struct serialization/deserialization
#[test]
fn test_modrinth_file_serialization() {
    let mut hashes = HashMap::new();
    hashes.insert("sha256".to_string(), "abc123".to_string());
    
    let file = ModrinthFile {
        hashes,
        url: "https://example.com/file.jar".to_string(),
        filename: "test.jar".to_string(),
        primary: true,
        size: 1024,
    };
    
    let json = serde_json::to_string(&file).unwrap();
    let deserialized: ModrinthFile = serde_json::from_str(&json).unwrap();
    
    assert_eq!(file.filename, deserialized.filename);
    assert_eq!(file.primary, deserialized.primary);
    assert_eq!(file.size, deserialized.size);
}

#[test]
fn test_modrinth_dependency_serialization() {
    let dependency = ModrinthDependency {
        version_id: Some("v1.0.0".to_string()),
        project_id: Some("test-mod".to_string()),
        file_name: Some("test-mod.jar".to_string()),
        dependency_type: "required".to_string(),
    };
    
    let json = serde_json::to_string(&dependency).unwrap();
    let deserialized: ModrinthDependency = serde_json::from_str(&json).unwrap();
    
    assert_eq!(dependency.version_id, deserialized.version_id);
    assert_eq!(dependency.dependency_type, deserialized.dependency_type);
}

#[test]
fn test_story_manifest_serialization() {
    let manifest = StoryManifest {
        instance: InstanceConfig {
            name: "Test Instance".to_string(),
            version: "1.0.0".to_string(),
            minecraft_version: Some("1.21.1".to_string()),
            loader: Some("fabric".to_string()),
        },
        extra_mods: Some(vec![
            ExtraMod {
                name: "test-mod".to_string(),
                version: Some("1.0.0".to_string()),
            }
        ]),
        overrides: Some(vec![
            Override {
                name: "config".to_string(),
                url: "https://example.com/config.zip".to_string(),
            }
        ]),
    };
    
    let json = serde_json::to_string(&manifest).unwrap();
    let deserialized: StoryManifest = serde_json::from_str(&json).unwrap();
    
    assert_eq!(manifest.instance.name, deserialized.instance.name);
    assert_eq!(manifest.extra_mods.as_ref().unwrap().len(), 1);
    assert_eq!(manifest.overrides.as_ref().unwrap().len(), 1);
}

#[test]
fn test_legacy_manifest_file_serialization() {
    let manifest = LegacyManifestFile {
        delete: Some(vec!["old-file.jar".to_string()]),
        notes: Some("Test notes".to_string()),
        required_files: Some(vec!["required-file.jar".to_string()]),
    };
    
    let json = serde_json::to_string(&manifest).unwrap();
    let deserialized: LegacyManifestFile = serde_json::from_str(&json).unwrap();
    
    assert_eq!(manifest.delete, deserialized.delete);
    assert_eq!(manifest.notes, deserialized.notes);
    assert_eq!(manifest.required_files, deserialized.required_files);
}

#[test]
fn test_file_hash_registry_serialization() {
    let mut registry = FileHashRegistry::default();
    let mut file_info = HashMap::new();
    file_info.insert("https://example.com/file.zip".to_string(), FileInfo {
        hash: "abc123".to_string(),
        last_modified: "Wed, 21 Oct 2015 07:28:00 GMT".to_string(),
    });
    registry.files = file_info;
    
    let json = serde_json::to_string(&registry).unwrap();
    let deserialized: FileHashRegistry = serde_json::from_str(&json).unwrap();
    
    assert_eq!(registry.files.len(), deserialized.files.len());
    assert!(deserialized.files.contains_key("https://example.com/file.zip"));
}

#[test]
fn test_modrinth_index_serialization() {
    let mut hashes = HashMap::new();
    hashes.insert("sha256".to_string(), "def456".to_string());
    
    let index = ModrinthIndex {
        files: vec![ModrinthIndexFile {
            path: "mods/test-mod.jar".to_string(),
            hashes,
            downloads: vec!["https://example.com/download".to_string()],
        }],
    };
    
    let json = serde_json::to_string(&index).unwrap();
    let deserialized: ModrinthIndex = serde_json::from_str(&json).unwrap();
    
    assert_eq!(index.files.len(), deserialized.files.len());
    assert_eq!(index.files[0].path, deserialized.files[0].path);
}

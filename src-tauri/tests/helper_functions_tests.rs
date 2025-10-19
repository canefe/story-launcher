use story_launcher_lib::*;
use std::path::Path;
use tempfile::TempDir;

// Test helper functions for mod name processing
#[test]
fn test_extract_mod_name_from_filename() {
    // Test various filename patterns
    let test_cases = vec![
        ("fabric-api-0.91.0+1.21.1.jar", "fabric-api"),
        ("jei-12.3.0.0.jar", "jei"),
        ("modmenu-8.0.0+1.21.1.jar", "modmenu"),
        ("sodium-fabric-mc1.21.1-0.5.8.jar", "sodium"),
        ("iris-mc1.21.1-1.6.4.jar", "iris"),
        ("lithium-fabric-mc1.21.1-0.12.2.jar", "lithium"),
        ("phosphor-fabric-mc1.21.1-0.9.0.jar", "phosphor"),
        ("mod_name_v1.2.3_mc1.21.1.jar", "mod-name"),
        ("some-mod-1.0.0-fabric.jar", "some-mod"),
        ("another_mod_2.0.0_neoforge.jar", "another-mod"),
    ];
    
    for (filename, expected) in test_cases {
        let result = extract_mod_name_from_filename(filename);
        assert_eq!(result, expected, "Failed for filename: {}", filename);
    }
}

#[test]
fn test_normalize_mod_name() {
    let test_cases = vec![
        ("Fabric API", "fabricapi"),
        ("JEI", "jei"),
        ("Mod Menu", "modmenu"),
        ("sodium-fabric", "sodiumfabric"),
        ("iris_mc1.21.1", "irismc1.21.1"),
        ("lithium_fabric", "lithiumfabric"),
        ("phosphor-fabric", "phosphorfabric"),
        ("some-mod", "somemod"),
        ("another_mod", "anothermod"),
        ("test--mod", "testmod"),
        ("  spaced  mod  ", "spacedmod"),
    ];
    
    for (input, expected) in test_cases {
        let result = normalize_mod_name(input);
        assert_eq!(result, expected, "Failed for input: '{}'", input);
    }
}

#[test]
fn test_check_story_instance_function() {
    let temp_dir = TempDir::new().unwrap();
    let instance_base = temp_dir.path().to_string_lossy().to_string();
    
    // Test with non-existent instance
    let result = test_check_story_instance(instance_base.clone(), "NonExistent".to_string());
    assert!(!result);
    
    // Create a test instance directory
    let story_path = Path::new(&instance_base).join("TestInstance");
    std::fs::create_dir_all(&story_path).unwrap();
    
    // Test with existing instance
    let result = test_check_story_instance(instance_base, "TestInstance".to_string());
    assert!(result);
}

#[test]
fn test_is_base_installed_function() {
    let temp_dir = TempDir::new().unwrap();
    let instance_base = temp_dir.path().to_string_lossy().to_string();
    
    // Test with non-existent base
    let result = test_is_base_installed(instance_base.clone());
    assert!(!result);
    
    // Create the base file
    let base_path = Path::new(&instance_base).join("npcmessageparser-1.0-SNAPSHOT.jar");
    std::fs::write(&base_path, "test content").unwrap();
    
    // Test with existing base
    let result = test_is_base_installed(instance_base);
    assert!(result);
}

#[test]
fn test_check_path_exists_function() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_string_lossy().to_string();
    
    // Test with existing directory
    let result = test_check_path_exists(temp_path.clone());
    assert!(result);
    
    // Test with non-existent path
    let result = test_check_path_exists("/non/existent/path".to_string());
    assert!(!result);
}

#[test]
fn test_create_story_instance_function() {
    let temp_dir = TempDir::new().unwrap();
    let instance_base = temp_dir.path().to_string_lossy().to_string();
    let folder_name = "TestStory".to_string();
    
    // Test successful creation
    let result = test_create_story_instance(instance_base.clone(), folder_name.clone());
    assert!(result.is_ok());
    
    let story_path = Path::new(&instance_base).join(&folder_name);
    assert!(story_path.exists());
    
    // Verify instance.cfg was created
    let instance_cfg_path = story_path.join("instance.cfg");
    assert!(instance_cfg_path.exists());
    
    // Verify mmc-pack.json was created
    let mmc_pack_path = story_path.join("mmc-pack.json");
    assert!(mmc_pack_path.exists());
}

#[test]
fn test_finalize_instance_function() {
    let temp_dir = TempDir::new().unwrap();
    let instance_path = temp_dir.path().to_string_lossy().to_string();
    
    // Test successful finalization
    let result = test_finalize_instance(instance_path.clone());
    assert!(result.is_ok());
    
    let instance_dir = Path::new(&instance_path);
    
    // Verify .minecraft directory was created
    let minecraft_dir = instance_dir.join(".minecraft");
    assert!(minecraft_dir.exists());
    
    // Verify mods directory was created
    let mods_dir = minecraft_dir.join("mods");
    assert!(mods_dir.exists());
    
    // Verify instance.cfg was created
    let instance_cfg_path = instance_dir.join("instance.cfg");
    assert!(instance_cfg_path.exists());
    
    // Verify mmc-pack.json was created
    let mmc_pack_path = instance_dir.join("mmc-pack.json");
    assert!(mmc_pack_path.exists());
}

#[test]
fn test_verify_extraction_integrity() {
    let temp_dir = TempDir::new().unwrap();
    let extract_path = temp_dir.path();
    
    // Test with no manifest (should pass)
    let result = verify_extraction_integrity(extract_path, &None);
    assert!(result.is_ok());
    assert!(result.unwrap());
    
    // Test with manifest but no required files
    let manifest = LegacyManifestFile {
        delete: None,
        notes: None,
        required_files: None,
    };
    let result = verify_extraction_integrity(extract_path, &Some(manifest));
    assert!(result.is_ok());
    assert!(result.unwrap());
    
    // Test with required files that don't exist
    let manifest_with_requirements = LegacyManifestFile {
        delete: None,
        notes: None,
        required_files: Some(vec!["missing-file.jar".to_string()]),
    };
    let result = verify_extraction_integrity(extract_path, &Some(manifest_with_requirements));
    assert!(result.is_ok());
    assert!(!result.unwrap());
    
    // Test with required files that do exist
    let required_file = extract_path.join("existing-file.jar");
    std::fs::write(&required_file, "test content").unwrap();
    
    let manifest_with_existing = LegacyManifestFile {
        delete: None,
        notes: None,
        required_files: Some(vec!["existing-file.jar".to_string()]),
    };
    let result = verify_extraction_integrity(extract_path, &Some(manifest_with_existing));
    assert!(result.is_ok());
    assert!(result.unwrap());
}

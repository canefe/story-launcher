# Story Launcher - Manifest System

## Overview

The Story Launcher now supports a manifest-based download system for Modrinth modpacks and additional mods.

## Manifest Format

The manifest.json file should follow this structure:

```json
{
  "instance": {
    "name": "fabulously-optimized",
    "version": "6.4.0"
  },
  "extra_mods": [
    {
      "name": "sodium",
      "version": "mc1.21.1-0.6.6-fabric"
    },
    {
      "name": "iris", 
      "version": "1.8.4-fabric-mc1.21.1"
    }
  ]
}
```

## How it Works

1. **Instance Download**: The `instance` section specifies a Modrinth modpack name and version. This constructs a URL like:
   ```
   https://api.modrinth.com/v2/project/fabulously-optimized/version/6.4.0
   ```

2. **Modpack Processing**: 
   - Downloads the `.mrpack` file from the API response
   - Extracts it as a zip archive
   - Moves `modrinth.index.json` to `mrpack/` folder
   - Extracts `overrides/` contents to `.minecraft/` folder
   - Downloads all mods listed in `modrinth.index.json`
   - Creates `instance.cfg` and `mmc-pack.json` configuration files

3. **Extra Mods**: The optional `extra_mods` array downloads additional mods using URLs like:
   ```
   https://api.modrinth.com/v2/project/sodium/version/mc1.21.1-0.6.6-fabric
   ```

## New Tauri Commands

- `download_from_manifest(manifest_url, instance_base)` - Main entry point
- `download_modrinth_modpack(project_name, version, instance_base)` - Downloads a modpack
- `download_modrinth_mod(mod_name, version, mods_dir)` - Downloads a single mod

## File Structure

After download, the instance structure will be:
```
Story/
├── instance.cfg
├── mmc-pack.json
├── mrpack/
│   └── modrinth.index.json
└── .minecraft/
    ├── mods/
    │   ├── (modpack mods)
    │   └── (extra mods)
    └── (overrides content)
```

## Usage Example

```javascript
// Download from a remote manifest
await invoke('download_from_manifest', {
  manifestUrl: 'https://example.com/story-manifest.json',
  instanceBase: 'C:\\Users\\username\\AppData\\Roaming\\PollyMC\\instances'
});
```

//! Plugin manager — load/unload/toggle plugins (DLL/WASM).
//!
//! For the MVP, actual dynamic library loading is stubbed.  Plugins are
//! registered by metadata (name, version, path) and can be enabled or
//! disabled.  The `scan_plugins` method walks the plugin directory for
//! `.dll` (Windows) / `.so` (Linux) / `.dylib` (macOS) / `.wasm` files.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

// ════════════════════════════════════════════════════════════════════════════
//  LoadedPlugin
// ════════════════════════════════════════════════════════════════════════════

/// Metadata for a loaded plugin.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    /// Human-readable plugin name.
    pub name: String,
    /// Semantic version string (e.g. `"0.1.0"`).
    pub version: String,
    /// Whether the plugin is currently enabled.
    pub enabled: bool,
    /// Filesystem path to the plugin binary.
    pub path: PathBuf,
}

// ════════════════════════════════════════════════════════════════════════════
//  PluginManager
// ════════════════════════════════════════════════════════════════════════════

/// Manages the plugin lifecycle: scanning, loading, unloading, toggling.
pub struct PluginManager {
    /// List of loaded plugins.
    pub plugins: Vec<LoadedPlugin>,
    /// Directory to scan for plugin binaries.
    pub plugin_dir: PathBuf,
}

impl PluginManager {
    /// Create a new `PluginManager` with the given plugin directory.
    pub fn new(plugin_dir: PathBuf) -> Self {
        Self {
            plugins: Vec::new(),
            plugin_dir,
        }
    }

    /// Scan the plugin directory for plugin files (`.dll`, `.so`, `.dylib`, `.wasm`).
    ///
    /// Newly discovered plugins are added to the `plugins` list with
    /// `enabled = true`.  Already-registered plugins are not duplicated.
    ///
    /// Returns the number of newly discovered plugins.
    pub fn scan_plugins(&mut self) -> Result<usize> {
        if !self.plugin_dir.exists() {
            log::info!(
                "[plugins] Plugin directory does not exist: {:?}",
                self.plugin_dir
            );
            return Ok(0);
        }

        let entries = std::fs::read_dir(&self.plugin_dir)
            .map_err(|e| anyhow!("Failed to read plugin dir {:?}: {e}", self.plugin_dir))?;

        let mut new_count = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_plugin_file(&path) {
                continue;
            }

            // Skip if already registered
            if self.plugins.iter().any(|p| p.path == path) {
                continue;
            }

            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            let plugin = LoadedPlugin {
                name: name.clone(),
                version: "0.1.0".to_string(),
                enabled: true,
                path: path.clone(),
            };

            log::info!("[plugins] Discovered: {} ({:?})", plugin.name, plugin.path);
            self.plugins.push(plugin);
            new_count += 1;
        }

        Ok(new_count)
    }

    /// Load a plugin from the given path (stub for MVP).
    ///
    /// In the MVP, this registers the plugin by metadata without actually
    /// loading the dynamic library.  The plugin name is derived from the
    /// file stem.
    ///
    /// Returns the name of the loaded plugin.
    pub fn load_plugin(&mut self, path: &Path) -> Result<String> {
        if !path.exists() {
            return Err(anyhow!("Plugin file not found: {}", path.display()));
        }

        if !is_plugin_file(path) {
            return Err(anyhow!(
                "Unsupported plugin format: {} (expected .dll/.so/.dylib/.wasm)",
                path.display()
            ));
        }

        // Skip if already loaded
        if self.plugins.iter().any(|p| p.path == path) {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            return Err(anyhow!("Plugin already loaded: {name}"));
        }

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let plugin = LoadedPlugin {
            name: name.clone(),
            version: "0.1.0".to_string(),
            enabled: true,
            path: path.to_path_buf(),
        };

        log::info!("[plugins] Loaded (stub): {} ({:?})", plugin.name, plugin.path);
        self.plugins.push(plugin);
        Ok(name)
    }

    /// Unload a plugin by name (stub for MVP).
    ///
    /// Removes the plugin from the `plugins` list.  In a full
    /// implementation, this would also unload the dynamic library.
    pub fn unload_plugin(&mut self, name: &str) -> Result<()> {
        let initial_len = self.plugins.len();
        self.plugins.retain(|p| p.name != name);

        if self.plugins.len() == initial_len {
            return Err(anyhow!("Plugin not found: {name}"));
        }

        log::info!("[plugins] Unloaded (stub): {name}");
        Ok(())
    }

    /// Toggle a plugin's enabled state by name.
    pub fn toggle_plugin(&mut self, name: &str) -> Result<()> {
        let plugin = self
            .plugins
            .iter_mut()
            .find(|p| p.name == name)
            .ok_or_else(|| anyhow!("Plugin not found: {name}"))?;

        plugin.enabled = !plugin.enabled;
        log::info!(
            "[plugins] Toggled {} → {}",
            plugin.name,
            if plugin.enabled { "enabled" } else { "disabled" }
        );
        Ok(())
    }

    /// Get a reference to a plugin by name.
    #[allow(dead_code)]
    pub fn get_plugin(&self, name: &str) -> Option<&LoadedPlugin> {
        self.plugins.iter().find(|p| p.name == name)
    }

    /// Number of loaded plugins.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Whether there are no loaded plugins.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Helpers
// ════════════════════════════════════════════════════════════════════════════

/// Check whether a path has a plugin file extension.
fn is_plugin_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    matches!(ext.as_str(), "dll" | "so" | "dylib" | "wasm")
}

// ════════════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manager_new() {
        let mgr = PluginManager::new(PathBuf::from("/tmp/plugins"));
        assert!(mgr.plugins.is_empty());
        assert_eq!(mgr.plugin_dir, PathBuf::from("/tmp/plugins"));
    }

    #[test]
    fn test_scan_nonexistent_dir() {
        let mut mgr = PluginManager::new(PathBuf::from("/nonexistent/path/plugins"));
        let result = mgr.scan_plugins();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_scan_empty_dir() {
        let dir = std::env::temp_dir().join("drafftink_plugin_test_empty");
        std::fs::create_dir_all(&dir).ok();
        let mut mgr = PluginManager::new(dir.clone());
        let result = mgr.scan_plugins();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_scan_with_plugin_files() {
        let dir = std::env::temp_dir().join("drafftink_plugin_test_files");
        std::fs::create_dir_all(&dir).ok();

        // Create fake plugin files
        std::fs::write(dir.join("test_plugin.dll"), b"fake").ok();
        std::fs::write(dir.join("another.wasm"), b"fake").ok();
        std::fs::write(dir.join("not_a_plugin.txt"), b"fake").ok();

        let mut mgr = PluginManager::new(dir.clone());
        let result = mgr.scan_plugins();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);
        assert_eq!(mgr.plugins.len(), 2);

        // Verify names
        let names: Vec<&str> = mgr.plugins.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"test_plugin"));
        assert!(names.contains(&"another"));

        // All should be enabled by default
        assert!(mgr.plugins.iter().all(|p| p.enabled));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_scan_does_not_duplicate() {
        let dir = std::env::temp_dir().join("drafftink_plugin_test_dedup");
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(dir.join("dup.dll"), b"fake").ok();

        let mut mgr = PluginManager::new(dir.clone());
        let first = mgr.scan_plugins().unwrap();
        assert_eq!(first, 1);

        let second = mgr.scan_plugins().unwrap();
        assert_eq!(second, 0);
        assert_eq!(mgr.plugins.len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_plugin() {
        let dir = std::env::temp_dir().join("drafftink_plugin_test_load");
        std::fs::create_dir_all(&dir).ok();
        let plugin_path = dir.join("my_plugin.dll");
        std::fs::write(&plugin_path, b"fake").ok();

        let mut mgr = PluginManager::new(dir.clone());
        let result = mgr.load_plugin(&plugin_path);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "my_plugin");
        assert_eq!(mgr.plugins.len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_nonexistent_plugin() {
        let mut mgr = PluginManager::new(PathBuf::from("/tmp"));
        let result = mgr.load_plugin(Path::new("/nonexistent/plugin.dll"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_unsupported_format() {
        let dir = std::env::temp_dir().join("drafftink_plugin_test_unsupp");
        std::fs::create_dir_all(&dir).ok();
        let plugin_path = dir.join("plugin.txt");
        std::fs::write(&plugin_path, b"fake").ok();

        let mut mgr = PluginManager::new(dir.clone());
        let result = mgr.load_plugin(&plugin_path);
        assert!(result.is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_unload_plugin() {
        let dir = std::env::temp_dir().join("drafftink_plugin_test_unload");
        std::fs::create_dir_all(&dir).ok();
        let plugin_path = dir.join("to_unload.dll");
        std::fs::write(&plugin_path, b"fake").ok();

        let mut mgr = PluginManager::new(dir.clone());
        mgr.load_plugin(&plugin_path).unwrap();
        assert_eq!(mgr.plugins.len(), 1);

        let result = mgr.unload_plugin("to_unload");
        assert!(result.is_ok());
        assert_eq!(mgr.plugins.len(), 0);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_unload_nonexistent() {
        let mut mgr = PluginManager::new(PathBuf::from("/tmp"));
        let result = mgr.unload_plugin("nope");
        assert!(result.is_err());
    }

    #[test]
    fn test_toggle_plugin() {
        let dir = std::env::temp_dir().join("drafftink_plugin_test_toggle");
        std::fs::create_dir_all(&dir).ok();
        let plugin_path = dir.join("toggle_me.dll");
        std::fs::write(&plugin_path, b"fake").ok();

        let mut mgr = PluginManager::new(dir.clone());
        mgr.load_plugin(&plugin_path).unwrap();

        // Should start enabled
        assert!(mgr.get_plugin("toggle_me").unwrap().enabled);

        // Toggle off
        mgr.toggle_plugin("toggle_me").unwrap();
        assert!(!mgr.get_plugin("toggle_me").unwrap().enabled);

        // Toggle back on
        mgr.toggle_plugin("toggle_me").unwrap();
        assert!(mgr.get_plugin("toggle_me").unwrap().enabled);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_toggle_nonexistent() {
        let mut mgr = PluginManager::new(PathBuf::from("/tmp"));
        let result = mgr.toggle_plugin("ghost");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_plugin_file() {
        assert!(is_plugin_file(Path::new("/tmp/a.dll")));
        assert!(is_plugin_file(Path::new("/tmp/a.so")));
        assert!(is_plugin_file(Path::new("/tmp/a.dylib")));
        assert!(is_plugin_file(Path::new("/tmp/a.wasm")));
        assert!(!is_plugin_file(Path::new("/tmp/a.txt")));
        assert!(!is_plugin_file(Path::new("/tmp/a.exe")));
        assert!(!is_plugin_file(Path::new("/tmp/noext")));
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut mgr = PluginManager::new(PathBuf::from("/tmp"));
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);

        let dir = std::env::temp_dir().join("drafftink_plugin_test_len");
        std::fs::create_dir_all(&dir).ok();
        let plugin_path = dir.join("len_test.dll");
        std::fs::write(&plugin_path, b"fake").ok();
        mgr.load_plugin(&plugin_path).unwrap();

        assert!(!mgr.is_empty());
        assert_eq!(mgr.len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }
}

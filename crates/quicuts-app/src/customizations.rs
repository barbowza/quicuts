//! Per-app user customization files: `{config}/customizations/<Id>.custom.yml`.
//! Deliberately kept as user-editable YAML separate from the bundled
//! manifests; reads always hit the disk so hand edits show up on the next
//! panel refresh.

use std::path::{Path, PathBuf};

use quicuts_manifest::AppCustomizations;

pub struct CustomStore {
    dir: PathBuf,
}

impl CustomStore {
    pub fn new(config_dir: &Path) -> Self {
        Self { dir: config_dir.join("customizations") }
    }

    fn path(&self, manifest_id: &str) -> PathBuf {
        // Manifest ids are package names ("Google.Chrome"); neutralize any
        // character that would escape the directory.
        let safe: String = manifest_id
            .chars()
            .map(|c| if matches!(c, '/' | '\\' | ':') { '_' } else { c })
            .collect();
        self.dir.join(format!("{safe}.custom.yml"))
    }

    pub fn get(&self, manifest_id: &str) -> AppCustomizations {
        AppCustomizations::load_file(&self.path(manifest_id))
    }

    /// Read-modify-write one app's file; an emptied file is removed.
    pub fn update(&self, manifest_id: &str, f: impl FnOnce(&mut AppCustomizations)) {
        let path = self.path(manifest_id);
        let mut c = AppCustomizations::load_file(&path);
        f(&mut c);
        if let Err(e) = c.save_file(&path) {
            log::error!("failed to save customizations {}: {e}", path.display());
        }
    }
}

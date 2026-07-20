//! Pin persistence: manifestId -> set of entry keys, in pinned.json.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct PinStore {
    map: HashMap<String, HashSet<String>>,
    dir: PathBuf,
}

impl PinStore {
    pub fn load(dir: PathBuf) -> Self {
        let path = dir.join("pinned.json");
        let map = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<HashMap<String, Vec<String>>>(&s).ok())
            .map(|m| m.into_iter().map(|(k, v)| (k, v.into_iter().collect())).collect())
            .unwrap_or_default();
        Self { map, dir }
    }

    pub fn get(&self, manifest_id: &str) -> HashSet<String> {
        self.map.get(manifest_id).cloned().unwrap_or_default()
    }

    pub fn toggle(&mut self, manifest_id: &str, entry_key: &str) {
        let set = self.map.entry(manifest_id.to_string()).or_default();
        if !set.remove(entry_key) {
            set.insert(entry_key.to_string());
        }
        if set.is_empty() {
            self.map.remove(manifest_id);
        }
        let _ = self.save();
    }

    fn save(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let serializable: HashMap<&String, Vec<&String>> = self
            .map
            .iter()
            .map(|(k, v)| (k, v.iter().collect()))
            .collect();
        let tmp = self.dir.join("pinned.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&serializable)?)?;
        std::fs::rename(&tmp, self.dir.join("pinned.json"))?;
        Ok(())
    }
}

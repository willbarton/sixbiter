// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

use crate::lens::mapping::{LensMap, LensMapEntry, load_lens_map, save_lens_map};
use gpui::{App, Global};
use std::path::PathBuf;

/// Application-scoped store for the user's lens mapping rules. Loaded from
/// disk at startup; mutations autosave.
pub struct LensStore {
    map: LensMap,
    path: PathBuf,
}

impl Global for LensStore {}

impl LensStore {
    /// Resolve the path, attempt to load, fall back to empty on any failure.
    /// Failures other than "file doesn't exist" trigger a backup of the bad
    /// file so the user can recover it.
    pub fn load() -> Self {
        Self::load_from(lens_map_path())
    }

    /// Load from an explicit path. Tests use this to stay off the real
    /// `~/Library/Application Support` file.
    pub fn load_from(path: PathBuf) -> Self {
        let map = match load_lens_map(&path) {
            Ok(m) => m,
            Err(_) if !path.exists() => {
                // First launch — empty map, no error.
                LensMap::new()
            }
            Err(err) => {
                tracing::error!("Failed to load {}: {}", path.display(), err);
                back_up(&path);
                LensMap::new()
            }
        };
        Self { map, path }
    }

    pub fn rules(&self) -> &LensMap {
        &self.map
    }

    pub fn add(&mut self, name: String, entry: LensMapEntry) {
        self.map.insert(name, entry);
        self.persist();
    }

    pub fn remove(&mut self, name: &str) {
        self.map.shift_remove(name);
        self.persist();
    }

    /// Apply an in-place edit to the named rule, then persist.
    pub fn update<F>(&mut self, name: &str, f: F)
    where
        F: FnOnce(&mut LensMapEntry),
    {
        if let Some(entry) = self.map.get_mut(name) {
            f(entry);
            self.persist();
        }
    }

    /// Rename a rule, preserving its position in the list. Returns whether the
    /// rename succeeded.
    #[must_use]
    pub fn rename(&mut self, old_name: &str, new_name: String) -> bool {
        if old_name == new_name {
            return true;
        }
        if self.map.contains_key(&new_name) {
            return false;
        }
        let Some((index, _, entry)) = self.map.shift_remove_full(old_name) else {
            return false;
        };
        self.map.shift_insert(index, new_name, entry);
        self.persist();
        true
    }

    fn persist(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(err) = save_lens_map(&self.path, &self.map) {
            tracing::error!("Failed to save lens map: {}", err);
        }
    }

    pub fn unique_name(&self, base: &str) -> String {
        if !self.map.contains_key(base) {
            return base.to_string();
        }
        let mut i = 2u32;
        loop {
            let candidate = format!("{base} {i}");
            if !self.map.contains_key(&candidate) {
                return candidate;
            }
            i += 1;
        }
    }

    #[cfg(test)]
    fn for_tests(map: LensMap) -> Self {
        Self {
            map,
            path: PathBuf::new(),
        }
    }
}

/// `~/Library/Application Support/Sixbiter/lens_map.json` on macOS.
fn lens_map_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("Sixbiter").join("lens_map.json")
}

fn back_up(path: &std::path::Path) {
    let mut backup = path.to_path_buf();
    backup.set_extension("json.bak");
    if let Err(err) = std::fs::rename(path, &backup) {
        tracing::error!("Failed to back up corrupted lens map: {}", err);
    } else {
        tracing::info!("Backed up corrupted lens map to {}", backup.display());
    }
}

pub fn init(cx: &mut App) {
    cx.set_global(LensStore::load());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lens::mapping::LensMapEntry;

    fn store_with_names(names: &[&str]) -> LensStore {
        let mut map = LensMap::new();
        for name in names {
            map.insert(name.to_string(), LensMapEntry::default());
        }
        LensStore::for_tests(map)
    }

    fn names(store: &LensStore) -> Vec<String> {
        store.rules().keys().cloned().collect()
    }

    #[test]
    fn unique_name_returns_base_when_no_collision() {
        let store = store_with_names(&[]);
        assert_eq!(store.unique_name("My Rule"), "My Rule");
    }

    #[test]
    fn unique_name_returns_base_when_other_rules_exist() {
        let store = store_with_names(&["Some Other Rule"]);
        assert_eq!(store.unique_name("My Rule"), "My Rule");
    }

    #[test]
    fn unique_name_appends_2_on_first_collision() {
        let store = store_with_names(&["My Rule"]);
        assert_eq!(store.unique_name("My Rule"), "My Rule 2");
    }

    #[test]
    fn unique_name_skips_taken_suffixes() {
        let store = store_with_names(&["My Rule", "My Rule 2", "My Rule 3"]);
        assert_eq!(store.unique_name("My Rule"), "My Rule 4");
    }

    #[test]
    fn rename_changes_name() {
        let mut store = store_with_names(&["A", "B", "C"]);
        assert!(store.rename("B", "B-renamed".to_string()));
        assert!(store.rules().contains_key("B-renamed"));
        assert!(!store.rules().contains_key("B"));
    }

    #[test]
    fn rename_preserves_position() {
        let mut store = store_with_names(&["A", "B", "C", "D"]);
        assert!(store.rename("B", "B-renamed".to_string()));
        assert_eq!(names(&store), vec!["A", "B-renamed", "C", "D"]);
    }

    #[test]
    fn rename_first_entry_preserves_position() {
        let mut store = store_with_names(&["A", "B", "C"]);
        assert!(store.rename("A", "A-renamed".to_string()));
        assert_eq!(names(&store), vec!["A-renamed", "B", "C"]);
    }

    #[test]
    fn rename_last_entry_preserves_position() {
        let mut store = store_with_names(&["A", "B", "C"]);
        assert!(store.rename("C", "C-renamed".to_string()));
        assert_eq!(names(&store), vec!["A", "B", "C-renamed"]);
    }

    #[test]
    fn rename_to_existing_name_is_noop() {
        let mut store = store_with_names(&["A", "B", "C"]);
        assert!(
            !store.rename("A", "B".to_string()),
            "a taken name must be refused, not silently ignored",
        );
        assert_eq!(names(&store), vec!["A", "B", "C"]);
    }

    #[test]
    fn rename_to_same_name_is_noop() {
        let mut store = store_with_names(&["A", "B", "C"]);
        assert!(store.rename("B", "B".to_string()), "already named that");
        assert_eq!(names(&store), vec!["A", "B", "C"]);
    }

    #[test]
    fn rename_nonexistent_rule_is_noop() {
        let mut store = store_with_names(&["A", "B", "C"]);
        assert!(!store.rename("does-not-exist", "renamed".to_string()));
        assert_eq!(names(&store), vec!["A", "B", "C"]);
    }

    fn temp_store() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lens_map.json");
        (dir, path)
    }

    #[test]
    fn load_from_missing_file_starts_empty_without_a_backup() {
        let (dir, path) = temp_store();
        let store = LensStore::load_from(path);

        assert!(store.rules().is_empty());
        assert!(
            !dir.path().join("lens_map.json.bak").exists(),
            "a first launch is not a corrupt file and must not create a backup",
        );
    }

    #[test]
    fn load_from_corrupt_file_moves_it_aside_and_starts_empty() {
        let (dir, path) = temp_store();
        std::fs::write(&path, "{ not json").unwrap();

        let store = LensStore::load_from(path.clone());
        assert!(store.rules().is_empty());

        // An unparseable rules file is not discarded, it is renamed so it
        // can be recovered by hand.
        let backup = dir.path().join("lens_map.json.bak");
        assert!(backup.exists(), "corrupt file should be preserved");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "{ not json");
        assert!(
            !path.exists(),
            "corrupt file should have been moved, not copied"
        );
    }

    #[test]
    fn add_persists_to_disk() {
        let (_dir, path) = temp_store();
        let mut store = LensStore::load_from(path.clone());
        store.add("Ultron 35".to_string(), LensMapEntry::default());

        let reloaded = LensStore::load_from(path);
        assert!(reloaded.rules().contains_key("Ultron 35"));
    }

    #[test]
    fn remove_persists_to_disk() {
        let (_dir, path) = temp_store();
        let mut store = LensStore::load_from(path.clone());
        store.add("A".to_string(), LensMapEntry::default());
        store.add("B".to_string(), LensMapEntry::default());
        store.remove("A");

        assert_eq!(names(&LensStore::load_from(path)), vec!["B"]);
    }

    #[test]
    fn rename_preserves_position_on_disk() {
        let (_dir, path) = temp_store();
        let mut store = LensStore::load_from(path.clone());
        for name in ["A", "B", "C"] {
            store.add(name.to_string(), LensMapEntry::default());
        }
        assert!(store.rename("A", "A-renamed".to_string()));

        let reloaded = LensStore::load_from(path);
        assert_eq!(names(&reloaded), vec!["A-renamed", "B", "C"]);
    }
}

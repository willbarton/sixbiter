// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

use std::path::PathBuf;

use gpui::{App, BorrowAppContext, Global};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub recursive: bool,
    pub apply_with_backup: bool,
    pub last_folder: Option<PathBuf>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            recursive: false,
            apply_with_backup: true,
            last_folder: None,
        }
    }
}

/// Wrapper struct that serves as the gpui global.
/// Holds the path so saves can find it without re-resolving every time.
pub struct PreferencesStore {
    prefs: Preferences,
    path: PathBuf,
}

impl Global for PreferencesStore {}

impl PreferencesStore {
    pub fn load() -> Self {
        Self::load_from(preferences_path())
    }

    pub fn load_from(path: PathBuf) -> Self {
        let prefs = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { prefs, path }
    }

    pub fn get(&self) -> &Preferences {
        &self.prefs
    }

    pub fn update<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Preferences),
    {
        f(&mut self.prefs);
        self.save();
    }

    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.prefs) {
            let tmp = self.path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &self.path);
            }
        }
    }
}

fn preferences_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("Sixbiter").join("preferences.json")
}

pub fn init(cx: &mut App) {
    cx.set_global(PreferencesStore::load());
}

pub fn update<F>(cx: &mut App, f: F)
where
    F: FnOnce(&mut Preferences),
{
    cx.update_global::<PreferencesStore, _>(|store, _| store.update(f));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_non_recursive_with_backups_on() {
        let prefs = Preferences::default();
        assert!(!prefs.recursive);
        assert!(prefs.apply_with_backup);
        assert!(prefs.last_folder.is_none());
    }

    #[test]
    fn round_trips_through_json() {
        let original = Preferences {
            recursive: true,
            apply_with_backup: false,
            last_folder: Some(PathBuf::from("/tmp/photos")),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: Preferences = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.recursive, original.recursive);
        assert_eq!(parsed.apply_with_backup, original.apply_with_backup);
        assert_eq!(parsed.last_folder, original.last_folder);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let json = r#"{"recursive": true, "future_field": "whatever"}"#;
        let parsed: Preferences = serde_json::from_str(json).unwrap();
        assert!(parsed.recursive);
    }
}

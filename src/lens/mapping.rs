// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2025, 2026 Will Barton

use anyhow::Result;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;

pub const LENS_MAP_FIELDS: &[&str] = &["FocalLength", "LensMake", "LensModel", "LensID"];

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct LensMapEntry {
    #[serde(rename = "match")]
    pub tomatch: IndexMap<String, Value>,
    pub modify: IndexMap<String, Value>,
}

pub type LensMap = IndexMap<String, LensMapEntry>;

pub fn load_lens_map(path: &Path) -> Result<LensMap> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mapping: LensMap = serde_json::from_reader(reader)?;
    Ok(mapping)
}

pub fn save_lens_map(path: &Path, map: &LensMap) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    {
        let file = File::create(&tmp)?;
        let mut writer = std::io::BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, map)?;
        writer.flush()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Coerce a JSON value to its plain-text representation. Strings are
/// returned without quoting; everything else uses serde_json's display.
pub fn json_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_map() -> LensMap {
        let mut map = LensMap::new();
        map.insert(
            "Ultron 35".to_string(),
            LensMapEntry {
                tomatch: IndexMap::from([
                    ("FocalLength".into(), json!("35.0 mm")),
                    ("LensModel".into(), json!("Summicron-M 1:2/35 ASPH.")),
                ]),
                modify: IndexMap::from([(
                    "LensModel".into(),
                    json!("Ultron 35 mm f/2 aspherical VM"),
                )]),
            },
        );
        map.insert("Second rule".to_string(), LensMapEntry::default());
        map
    }

    fn save_sample() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lens_map.json");
        save_lens_map(&path, &sample_map()).unwrap();
        (dir, path)
    }

    #[test]
    fn round_trip_preserves_rule_order() {
        let (_dir, path) = save_sample();
        let loaded = load_lens_map(&path).unwrap();
        assert_eq!(
            loaded.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["Ultron 35", "Second rule"],
        );
    }

    #[test]
    fn round_trip_preserves_field_order() {
        let (_dir, path) = save_sample();
        let loaded = load_lens_map(&path).unwrap();
        let entry = loaded.get("Ultron 35").unwrap();
        assert_eq!(
            entry.tomatch.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["FocalLength", "LensModel"],
        );
    }

    #[test]
    fn on_disk_key_is_match_not_tomatch() {
        let (_dir, path) = save_sample();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains(r#""match""#),
            "expected a `match` key in:\n{text}"
        );
        assert!(!text.contains("tomatch"));
    }

    #[test]
    fn loads_the_documented_on_disk_shape() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lens_map.json");
        std::fs::write(
            &path,
            r#"{
              "My lens": {
                "match": { "LensModel": "Summicron-M 1:2/35 ASPH." },
                "modify": { "LensModel": "Ultron 35" }
              }
            }"#,
        )
        .unwrap();

        let map = load_lens_map(&path).unwrap();
        let entry = map.get("My lens").unwrap();
        assert_eq!(
            entry.tomatch["LensModel"],
            json!("Summicron-M 1:2/35 ASPH.")
        );
        assert_eq!(entry.modify["LensModel"], json!("Ultron 35"));
    }

    #[test]
    fn load_missing_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_lens_map(&dir.path().join("does-not-exist.json")).is_err());
    }

    #[test]
    fn load_corrupt_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lens_map.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(load_lens_map(&path).is_err());
    }

    #[test]
    fn json_to_string_leaves_strings_unquoted() {
        assert_eq!(json_to_string(&json!("Ultron 35")), "Ultron 35");
    }

    #[test]
    fn json_to_string_renders_numbers() {
        assert_eq!(json_to_string(&json!(35.0)), "35.0");
        assert_eq!(json_to_string(&json!(35)), "35");
    }

    #[test]
    fn save_leaves_no_temp_file_behind() {
        let (dir, path) = save_sample();
        assert!(path.exists());
        assert!(
            !dir.path().join("lens_map.json.tmp").exists(),
            "the temp file should have been renamed onto the target",
        );
    }

    #[test]
    fn save_replaces_an_existing_file_completely() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lens_map.json");
        std::fs::write(&path, "{ \"Stale\": { \"match\": {}, \"modify\": {} } }").unwrap();

        save_lens_map(&path, &sample_map()).unwrap();

        let reloaded = load_lens_map(&path).unwrap();
        assert!(!reloaded.contains_key("Stale"));
        assert!(reloaded.contains_key("Ultron 35"));
    }
}

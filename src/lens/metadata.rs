// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2025, 2026 Will Barton

use anyhow::{Result, anyhow};
use exiftool_rs::ExifTool;
use serde_json::{Map, Value};
use std::path::Path;
use std::sync::Mutex;

use crate::lens::mapping::{LensMap, LensMapEntry, json_to_string};

pub struct MetadataReader {
    reader: ExifTool,
    writer: Mutex<ExifTool>,
}

impl MetadataReader {
    pub fn new() -> Self {
        Self {
            reader: ExifTool::new(),
            writer: Mutex::new(ExifTool::new()),
        }
    }

    pub fn read(&self, path: &Path) -> Result<Map<String, Value>> {
        let tags = self
            .reader
            .extract_info(path)
            .map_err(|e| anyhow!("failed to read metadata: {e}"))?;
        Ok(tags
            .into_iter()
            .map(|t| (t.name, Value::String(t.print_value)))
            .collect())
    }

    pub fn write(&self, path: &Path, entry: &LensMapEntry, with_backup: bool) -> Result<()> {
        let mut writer = self.writer.lock().unwrap_or_else(|p| p.into_inner());
        writer.clear_new_values();
        for (key, value) in &entry.modify {
            writer.set_new_value(key, Some(&json_to_string(value)));
        }
        if with_backup {
            let mut backup = path.as_os_str().to_os_string();
            backup.push("_original");
            let backup = std::path::PathBuf::from(backup);

            // Back up once and only once, an existing backup is never overwritten
            if backup.exists() {
                tracing::debug!(
                    backup = %backup.display(),
                    "backup already exists, keeping the earlier one",
                );
            } else {
                std::fs::copy(path, &backup).map_err(|e| anyhow!("backup failed: {e}"))?;
            }
        }
        writer
            .write_info(path, path)
            .map_err(|e| anyhow!("failed to write metadata: {e}"))?;
        Ok(())
    }
}

impl Default for MetadataReader {
    fn default() -> Self {
        Self::new()
    }
}

fn value_as_number(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str()?.parse().ok())
}

/// Compare either string or number values from EXIF JSON and lens mapping JSON.
fn compare_values(actual: &Value, expected: &Value) -> bool {
    tracing::trace!(actual = %actual, expected = %expected, "comparing values");

    // Try numeric comparison, parsing strings as needed.
    let actual_num = value_as_number(actual);
    let expected_num = value_as_number(expected);
    if let (Some(a), Some(e)) = (actual_num, expected_num) {
        return (a - e).abs() < 1e-6;
    }

    // Fall back to string comparison.
    match (actual.as_str(), expected.as_str()) {
        (Some(a), Some(e)) => a == e,
        _ => actual == expected,
    }
}

/// Find the first lens mapping whose `match` fields all match the image's metadata.
pub fn match_metadata<'a>(
    lens_map: &'a LensMap,
    metadata: &Map<String, Value>,
) -> Option<(&'a str, &'a LensMapEntry)> {
    lens_map.iter().find_map(|(name, entry)| {
        if entry.tomatch.is_empty() {
            return None;
        }
        let all_match = entry.tomatch.iter().all(|(k, expected)| {
            metadata
                .get(k)
                .is_some_and(|actual| compare_values(actual, expected))
        });
        all_match.then_some((name.as_str(), entry))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use serde_json::json;

    fn fixture() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.dng")
    }

    fn fixture_copy() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.dng");
        std::fs::copy(fixture(), &path).unwrap();
        (dir, path)
    }

    fn lens_model(reader: &MetadataReader, path: &Path) -> Option<String> {
        reader
            .read(path)
            .unwrap()
            .get("LensModel")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    fn modify_lens_model(to: &str) -> LensMapEntry {
        LensMapEntry {
            tomatch: IndexMap::new(),
            modify: IndexMap::from([("LensModel".into(), json!(to))]),
        }
    }

    fn make_metadata() -> Map<String, Value> {
        json!({
            "LensMake": "Leica Camera AG",
            "LensModel": "Summicron-M 1:2/35 ASPH.",
            "MaxApertureValue": 2.0,
        })
        .as_object()
        .unwrap()
        .clone()
    }

    fn make_lens_map_entry() -> LensMapEntry {
        LensMapEntry {
            tomatch: IndexMap::from([
                ("LensMake".into(), json!("Leica Camera AG")),
                ("LensModel".into(), json!("Summicron-M 1:2/35 ASPH.")),
                ("MaxApertureValue".into(), json!(2.0)),
            ]),
            modify: IndexMap::from([
                ("LensMake".into(), json!("Voigtlander")),
                ("LensModel".into(), json!("Ultron 35 mm f/2 aspherical VM")),
                ("MaxApertureValue".into(), json!(2.0)),
            ]),
        }
    }

    #[test]
    fn compare_values_matches_identical_strings() {
        assert!(compare_values(&json!("foo"), &json!("foo")));
    }

    #[test]
    fn compare_values_tolerates_float_imprecision() {
        assert!(compare_values(&json!(2.0000001), &json!(2.0)));
    }

    #[test]
    fn compare_values_matches_a_number_against_a_string() {
        assert!(compare_values(&json!(2.0), &json!("2.0")));
    }

    #[test]
    fn compare_values_rejects_different_strings() {
        assert!(!compare_values(&json!("foo"), &json!("bar")));
    }

    #[test]
    fn compare_values_matches_int_against_float() {
        assert!(compare_values(&json!(2), &json!(2.0)));
        assert!(compare_values(&json!(2.0), &json!(2)));
    }

    #[test]
    fn match_metadata_returns_the_matching_rule() {
        let metadata = make_metadata();
        let mut lens_map = LensMap::new();
        lens_map.insert(
            "Voigtlander Ultron 35 mm f/2 aspherical VM".to_string(),
            make_lens_map_entry(),
        );

        let matched = match_metadata(&lens_map, &metadata);
        assert_eq!(
            matched.map(|(name, _)| name),
            Some("Voigtlander Ultron 35 mm f/2 aspherical VM"),
        );
    }

    #[test]
    fn match_metadata_returns_none_when_a_field_differs() {
        let mut metadata = make_metadata();
        metadata.insert("LensModel".into(), json!("Wrong Lens"));

        let mut lens_map = LensMap::new();
        lens_map.insert(
            "Voigtlander Ultron 35 mm f/2 aspherical VM".to_string(),
            make_lens_map_entry(),
        );

        assert!(match_metadata(&lens_map, &metadata).is_none());
    }

    #[test]
    fn match_metadata_skips_rules_with_no_match_fields() {
        let metadata = make_metadata();
        let mut lens_map = LensMap::new();
        lens_map.insert("Empty".to_string(), LensMapEntry::default());
        assert!(match_metadata(&lens_map, &metadata).is_none());

        lens_map.insert("Voigtlander".to_string(), make_lens_map_entry());
        let matched = match_metadata(&lens_map, &metadata);
        assert_eq!(matched.map(|(name, _)| name), Some("Voigtlander"));
    }

    #[test]
    fn reads_lens_tags_from_a_dng() {
        let reader = MetadataReader::new();
        let metadata = reader.read(&fixture()).expect("read failed");

        let expectations = [
            ("LensMake", "Leica Camera AG"),
            ("LensModel", "Summilux-M 1:1.5/90 ASPH."),
        ];
        for (key, expected) in expectations {
            assert_eq!(
                metadata.get(key).and_then(|v| v.as_str()),
                Some(expected),
                "unexpected value for {key}",
            );
        }
    }

    #[test]
    fn write_updates_a_tag_in_place() {
        let reader = MetadataReader::new();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::copy(fixture(), tmp.path()).unwrap();

        let entry = LensMapEntry {
            tomatch: IndexMap::new(),
            modify: IndexMap::from([("LensModel".into(), json!("Test Lens 50mm"))]),
        };
        reader.write(tmp.path(), &entry, false).unwrap();

        let metadata = reader.read(tmp.path()).unwrap();
        assert_eq!(
            metadata.get("LensModel").and_then(|v| v.as_str()),
            Some("Test Lens 50mm"),
        );
    }

    #[test]
    fn write_with_backup_keeps_the_untouched_original_alongside() {
        let reader = MetadataReader::new();
        let (_dir, path) = fixture_copy();

        let before = lens_model(&reader, &path).expect("fixture should have a LensModel");
        let entry = modify_lens_model("Ultron 35 mm f/2 aspherical VM");
        reader.write(&path, &entry, true).unwrap();

        // The file itself now carries the corrected lens...
        assert_eq!(
            lens_model(&reader, &path).as_deref(),
            Some("Ultron 35 mm f/2 aspherical VM"),
        );
        assert_ne!(lens_model(&reader, &path), Some(before));

        // ...and the pre-write file is still recoverable
        let backup = path.with_file_name("sample.dng_original");
        assert!(
            backup.exists(),
            "expected a _original backup beside the file"
        );
        assert_eq!(
            std::fs::read(&backup).unwrap(),
            std::fs::read(fixture()).unwrap(),
            "backup should be a byte-for-byte copy of the pre-write file",
        );
    }

    #[test]
    fn write_without_backup_leaves_no_original() {
        let reader = MetadataReader::new();
        let (dir, path) = fixture_copy();

        reader
            .write(&path, &modify_lens_model("Test Lens 50mm"), false)
            .unwrap();

        assert_eq!(
            lens_model(&reader, &path).as_deref(),
            Some("Test Lens 50mm")
        );
        assert!(!dir.path().join("sample.dng_original").exists());
    }

    #[test]
    fn write_never_overwrites_an_existing_backup() {
        let reader = MetadataReader::new();
        let (dir, path) = fixture_copy();
        let before = lens_model(&reader, &path).expect("fixture should have a LensModel");

        reader
            .write(&path, &modify_lens_model("First pass"), true)
            .unwrap();
        reader
            .write(&path, &modify_lens_model("Second pass"), true)
            .unwrap();

        // The file carries the most recent correction...
        assert_eq!(lens_model(&reader, &path).as_deref(), Some("Second pass"));

        // ... and the original still carries pre-edit metadata
        let backup = path.with_file_name("sample.dng_original");
        let recovered = dir.path().join("recovered.dng");
        std::fs::copy(&backup, &recovered).unwrap();
        assert_eq!(
            lens_model(&reader, &recovered),
            Some(before),
            "a second apply must not clobber the original backup",
        );
    }
}

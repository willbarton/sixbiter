// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::lens::mapping::{LENS_MAP_FIELDS, LensMap, LensMapEntry};
use crate::lens::metadata::{MetadataReader, match_metadata};
use crate::views::files::{FileRow, RowStatus};

pub(super) fn is_dng(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("dng"))
}

pub(super) fn format_lens_display(make: &str, model: &str) -> String {
    if model.is_empty() { make } else { model }.to_string()
}

pub(super) fn relativize(abs: &Path, root: &Path) -> PathBuf {
    abs.strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| abs.to_path_buf())
}

fn error_row(rel: PathBuf, abs: PathBuf, err: String) -> FileRow {
    FileRow {
        rel_path: rel,
        abs_path: abs,
        current_lens: String::new(),
        matched_rule: None,
        matched_entry: None,
        error: Some(err),
        selected: false,
        status: RowStatus::Pending,
    }
}

pub(super) fn read_one(
    abs: &Path,
    rel: PathBuf,
    lens_map: &LensMap,
    reader: &MetadataReader,
) -> FileRow {
    let metadata = match reader.read(abs) {
        Ok(m) => m,
        Err(err) => return error_row(rel, abs.to_path_buf(), err.to_string()),
    };

    let make = metadata
        .get("LensMake")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let model = metadata
        .get("LensModel")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let current_lens = format_lens_display(make, model);

    let (matched_rule, matched_entry) = match match_metadata(lens_map, &metadata) {
        Some((name, entry)) => (Some(name.to_string()), Some(entry.clone())),
        _ => (None, None),
    };

    let selected = matched_rule.is_some();

    FileRow {
        rel_path: rel,
        abs_path: abs.to_path_buf(),
        current_lens,
        matched_rule,
        matched_entry,
        error: None,
        selected,
        status: RowStatus::Pending,
    }
}

/// Derive a rule from a file's current lens EXIF
pub(super) fn rule_from_file(
    path: &Path,
    reader: &MetadataReader,
) -> Option<(String, LensMapEntry)> {
    let metadata = reader.read(path).ok()?;

    let mut tomatch = IndexMap::new();
    for field in LENS_MAP_FIELDS {
        if let Some(value) = metadata.get(*field) {
            tomatch.insert(field.to_string(), value.clone());
        }
    }

    let model = metadata
        .get("LensModel")
        .and_then(|v| v.as_str())
        .unwrap_or("New rule");

    let entry = LensMapEntry {
        tomatch,
        modify: IndexMap::new(),
    };
    Some((format!("From {model}"), entry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use serde_json::json;
    use std::path::{Path, PathBuf};

    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.dng")
    }

    fn rule(name: &str, tomatch: IndexMap<String, serde_json::Value>) -> LensMap {
        let mut map = LensMap::new();
        map.insert(
            name.to_string(),
            LensMapEntry {
                tomatch,
                modify: IndexMap::from([("LensModel".into(), json!("Corrected"))]),
            },
        );
        map
    }

    #[test]
    fn format_lens_display_prefers_model_over_make() {
        assert_eq!(
            format_lens_display("Leica Camera AG", "Summicron-M 1:2/35 ASPH."),
            "Summicron-M 1:2/35 ASPH."
        );
    }

    #[test]
    fn format_lens_display_falls_back_to_make() {
        assert_eq!(format_lens_display("Leica", ""), "Leica");
        assert_eq!(format_lens_display("", ""), "");
    }

    #[test]
    fn relativize_strips_root_prefix() {
        let abs = Path::new("/Users/will/Photos/2024/IMG_001.dng");
        let root = Path::new("/Users/will/Photos");
        assert_eq!(relativize(abs, root), PathBuf::from("2024/IMG_001.dng"));
    }

    #[test]
    fn relativize_path_equal_to_root_returns_empty() {
        let path = Path::new("/Users/will/Photos");
        assert_eq!(relativize(path, path), PathBuf::from(""));
    }

    #[test]
    fn relativize_returns_original_when_not_under_root() {
        let abs = Path::new("/etc/hosts");
        let root = Path::new("/Users/will/Photos");
        assert_eq!(relativize(abs, root), PathBuf::from("/etc/hosts"));
    }

    #[test]
    fn is_dng_accepts_any_case() {
        assert!(is_dng(Path::new("a.dng")));
        assert!(is_dng(Path::new("a.DNG")));
        assert!(is_dng(Path::new("a.Dng")));
    }

    #[test]
    fn is_dng_rejects_everything_else() {
        assert!(!is_dng(Path::new("a.jpg")));
        assert!(
            !is_dng(Path::new("a.dng.bak")),
            "only the last extension counts"
        );
        assert!(!is_dng(Path::new("noextension")));
        // `.dng` is a dotfile, and `Path` reports no extension for it at all.
        assert!(!is_dng(Path::new(".dng")));
    }

    #[test]
    fn read_one_matches_a_rule_and_pre_checks_the_row() {
        let reader = MetadataReader::new();
        let map = rule(
            "Summilux 90",
            IndexMap::from([
                ("LensMake".into(), json!("Leica Camera AG")),
                ("LensModel".into(), json!("Summilux-M 1:1.5/90 ASPH.")),
            ]),
        );

        let row = read_one(&fixture(), PathBuf::from("sample.dng"), &map, &reader);

        assert_eq!(row.matched_rule.as_deref(), Some("Summilux 90"));
        assert!(
            row.matched_entry.is_some(),
            "the entry is what gets written"
        );
        assert!(row.selected, "a matched file is armed for apply");
        assert_eq!(row.current_lens, "Summilux-M 1:1.5/90 ASPH.");
        assert!(row.error.is_none());
        assert_eq!(row.status, RowStatus::Pending);
    }

    #[test]
    fn read_one_leaves_an_unmatched_file_unchecked() {
        let reader = MetadataReader::new();
        let map = rule(
            "Some other lens",
            IndexMap::from([("LensModel".into(), json!("Nokton 50mm f/1.5"))]),
        );

        let row = read_one(&fixture(), PathBuf::from("sample.dng"), &map, &reader);

        assert!(row.matched_rule.is_none());
        assert!(row.matched_entry.is_none());
        assert!(
            !row.selected,
            "an unmatched file must not be armed for writing"
        );
        // It still reports the lens it actually has, so the user can build a rule.
        assert_eq!(row.current_lens, "Summilux-M 1:1.5/90 ASPH.");
    }

    #[test]
    fn read_one_reports_an_unreadable_file_as_an_error_row() {
        let reader = MetadataReader::new();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.dng");
        std::fs::write(&path, b"not a real dng").unwrap();

        let row = read_one(&path, PathBuf::from("broken.dng"), &LensMap::new(), &reader);

        assert!(
            row.error.is_some(),
            "a failed read must surface, not vanish"
        );
        assert!(!row.selected);
        assert!(row.matched_rule.is_none());
        assert_eq!(row.current_lens, "");
    }

    #[test]
    fn rule_from_file_matches_on_the_lens_it_found() {
        let reader = MetadataReader::new();
        let (name, entry) = rule_from_file(&fixture(), &reader).expect("the fixture is readable");

        assert_eq!(name, "From Summilux-M 1:1.5/90 ASPH.");
        assert_eq!(entry.tomatch["LensMake"], json!("Leica Camera AG"));
        assert_eq!(
            entry.tomatch["LensModel"],
            json!("Summilux-M 1:1.5/90 ASPH.")
        );
        assert!(
            entry.modify.is_empty(),
            "the corrected values are the user's to fill in",
        );
    }

    #[test]
    fn rule_from_file_gives_up_on_an_unreadable_file() {
        let reader = MetadataReader::new();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.dng");
        std::fs::write(&path, b"not a real dng").unwrap();

        assert!(rule_from_file(&path, &reader).is_none());
    }
}

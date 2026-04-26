// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

//! Helpers for locating files inside the macOS .app bundle (with a
//! Cargo-dev fallback).

use std::path::{Path, PathBuf};

// Resolve the root from which to find resources, either from the macOS
// application bundle, or the cargo dev environment.
fn resolve_root<F>(starting_path: Option<&Path>, exists: F) -> Option<PathBuf>
where
    F: Fn(&Path) -> bool,
{
    let bundle = starting_path
        .and_then(|e| Some(e.parent()?.parent()?.join("Resources")))
        .filter(|p| exists(p));

    #[cfg(debug_assertions)]
    let bundle = bundle.or_else(|| Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("resources")));

    bundle
}

/// Return the path to a subdirectory of the macOS bundle's
/// `Contents/Resources`, if it exists, and the cargo dev `resources/` folder
/// otherwise.
pub fn resources(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    let root = resolve_root(exe.as_deref(), |p| p.exists())?;
    let path = root.join(name);
    path.exists().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn exists_in(set: &HashSet<PathBuf>) -> impl Fn(&Path) -> bool + '_ {
        move |p: &Path| set.contains(p)
    }

    #[test]
    fn resolve_root_returns_bundle_path() {
        let exe = Path::new("/Applications/Foo.app/Contents/MacOS/foo");
        let files: HashSet<_> = [PathBuf::from("/Applications/Foo.app/Contents/Resources")].into();
        assert_eq!(
            resolve_root(Some(exe), exists_in(&files)),
            Some(PathBuf::from("/Applications/Foo.app/Contents/Resources")),
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn resolve_root_falls_back_to_dev_in_debug() {
        let exe = Path::new("/Applications/Foo.app/Contents/MacOS/foo");
        let files: HashSet<PathBuf> = HashSet::new();
        let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources");
        assert_eq!(resolve_root(Some(exe), exists_in(&files)), Some(expected));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn resolve_root_has_no_fallback_in_release() {
        let exe = Path::new("/Applications/Foo.app/Contents/MacOS/foo");
        let files: HashSet<PathBuf> = HashSet::new();
        assert_eq!(resolve_root(Some(exe), exists_in(&files)), None);
    }
}

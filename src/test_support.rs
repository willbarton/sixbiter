// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

//! Shared setup for gpui App tests
use gpui::TestAppContext;
use tempfile::TempDir;

use crate::lens::store::LensStore;
use crate::preferences::PreferencesStore;

/// Initialize a `LensStore` and `PreferencesStore` rooted at a fresh temp dir
pub(crate) fn test_app(cx: &mut TestAppContext) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    cx.skip_drawing();
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.set_global(LensStore::load_from(dir.path().join("lens_map.json")));
        cx.set_global(PreferencesStore::load_from(
            dir.path().join("preferences.json"),
        ));
    });
    dir
}

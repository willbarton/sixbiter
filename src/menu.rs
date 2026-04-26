// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

use gpui::{App, Menu, MenuItem};
use gpui_component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};

use crate::actions::{About, OpenSettings, Quit};
use crate::views::files::Open;

pub fn init(cx: &mut App) {
    cx.set_menus(vec![
        Menu {
            name: "Sixbiter".into(),
            items: vec![
                MenuItem::action("About Sixbiter", About),
                MenuItem::separator(),
                // When this says "Preferences" it gets converted to "Settings" with the gear icon
                MenuItem::action("Preferences…", OpenSettings),
                MenuItem::separator(),
                MenuItem::action("Quit Sixbiter", Quit),
            ],
            disabled: false,
        },
        Menu {
            name: "File".into(),
            items: vec![MenuItem::action("Open Folder…", Open)],
            disabled: false,
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Undo", Undo, gpui::OsAction::Undo),
                MenuItem::os_action("Redo", Redo, gpui::OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", Cut, gpui::OsAction::Cut),
                MenuItem::os_action("Copy", Copy, gpui::OsAction::Copy),
                MenuItem::os_action("Paste", Paste, gpui::OsAction::Paste),
                MenuItem::separator(),
                MenuItem::os_action("Select All", SelectAll, gpui::OsAction::SelectAll),
            ],
            disabled: false,
        },
    ]);
}

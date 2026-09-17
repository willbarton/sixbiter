// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

use gpui_kit::{App, KeyBinding, actions};

use crate::views;

actions!(sixbiter, [Quit, OpenSettings, About]);

pub fn install(cx: &mut App) {
    cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
    cx.on_action(|_: &OpenSettings, cx: &mut App| {
        views::settings::open(cx);
    });
    cx.on_action(|_: &About, cx: &mut App| {
        views::about::open(cx);
    });

    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-o", views::files::Open, None),
    ]);
}

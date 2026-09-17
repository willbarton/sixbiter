// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

use gpui_kit::component::{Theme, ThemeRegistry};
use gpui_kit::{App, Global, SharedString, WindowAppearance};
use std::path::PathBuf;

use crate::bundle;

const LIGHT_THEME: &str = "macOS Classic Light";
const DARK_THEME: &str = "macOS Classic Dark";

#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub enum AppearancePref {
    Light,
    Dark,
    #[default]
    System,
}
impl Global for AppearancePref {}

pub fn appearance_pref(cx: &App) -> AppearancePref {
    *cx.global::<AppearancePref>()
}

pub fn set_appearance_pref(pref: AppearancePref, cx: &mut App) {
    cx.set_global(pref);
    sync_theme_from_appearance(cx);
    // Force a redraw of every open window so they pick up the new theme.
    cx.refresh_windows();
}

fn themes_dir() -> PathBuf {
    bundle::resources("themes").unwrap_or_else(|| PathBuf::from("./themes"))
}

pub fn init_themes(cx: &mut App) {
    cx.set_global(AppearancePref::default());

    let dir = themes_dir();
    if let Err(err) = ThemeRegistry::watch_dir(dir, cx, |cx| {
        sync_theme_from_appearance(cx);
    }) {
        tracing::error!("failed to watch themes directory: {}", err);
    }
}

fn apply_theme(theme_name: &str, cx: &mut App) {
    let name = SharedString::from(theme_name.to_string());
    if let Some(config) = ThemeRegistry::global(cx).themes().get(&name).cloned() {
        Theme::global_mut(cx).apply_config(&config);
    }
}

pub fn sync_theme_from_appearance(cx: &mut App) {
    let name = match resolve(appearance_pref(cx), cx.window_appearance()) {
        ResolvedAppearance::Light => LIGHT_THEME,
        ResolvedAppearance::Dark => DARK_THEME,
    };
    apply_theme(name, cx);
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum ResolvedAppearance {
    Light,
    Dark,
}

fn resolve(pref: AppearancePref, system: WindowAppearance) -> ResolvedAppearance {
    match pref {
        AppearancePref::Light => ResolvedAppearance::Light,
        AppearancePref::Dark => ResolvedAppearance::Dark,
        AppearancePref::System => match system {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => ResolvedAppearance::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => ResolvedAppearance::Light,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_preference_follows_os() {
        assert_eq!(
            resolve(AppearancePref::System, WindowAppearance::Dark),
            ResolvedAppearance::Dark
        );
        assert_eq!(
            resolve(AppearancePref::System, WindowAppearance::Light),
            ResolvedAppearance::Light
        );
    }

    #[test]
    fn explicit_preference_overrides_os() {
        assert_eq!(
            resolve(AppearancePref::Light, WindowAppearance::Dark),
            ResolvedAppearance::Light
        );
        assert_eq!(
            resolve(AppearancePref::Dark, WindowAppearance::Light),
            ResolvedAppearance::Dark
        );
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

mod appearance;
mod mapping;

use gpui_kit::component::{
    ActiveTheme, IconName, Root, StyledExt,
    sidebar::{Sidebar, SidebarMenu, SidebarMenuItem},
};
use gpui_kit::{
    App, AppContext, Bounds, Context, Entity, Global, IntoElement, ParentElement, Render, Styled,
    TitlebarOptions, WeakEntity, Window, WindowBounds, WindowHandle, WindowOptions, div, point, px,
    size,
};

use appearance::AppearanceView;
use mapping::LensMappingView;

/// Which settings section is currently selected in the sidebar.
#[derive(Copy, Clone, PartialEq, Eq)]
enum Section {
    Appearance,
    LensMapping,
}

pub struct SettingsView {
    selected: Section,
    appearance: Entity<AppearanceView>,
    lens_mapping: Entity<LensMappingView>,
}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            selected: Section::Appearance,
            appearance: cx.new(|cx| AppearanceView::new(window, cx)),
            lens_mapping: cx.new(|cx| LensMappingView::new(window, cx)),
        }
    }

    pub fn apply_intent(
        &mut self,
        intent: SettingsIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match intent {
            SettingsIntent::SelectLensRule(name) => {
                self.selected = Section::LensMapping;
                let lens_mapping = self.lens_mapping.clone();
                lens_mapping.update(cx, |section, cx| {
                    section.select_by_name(&name, window, cx);
                });
                cx.notify();
            }
        }
    }
    fn select(&mut self, section: Section, cx: &mut Context<Self>) {
        if self.selected != section {
            self.selected = section;
            cx.notify();
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected;

        let content = match selected {
            Section::Appearance => self.appearance.clone().into_any_element(),
            Section::LensMapping => self.lens_mapping.clone().into_any_element(),
        };

        div()
            .h_flex()
            .size_full()
            .child(
                div()
                    .v_flex()
                    .h_full()
                    .bg(cx.theme().sidebar)
                    .child(div().flex_none().h(px(36.0)))
                    .child(
                        Sidebar::new("settings-sidebar").child(
                            SidebarMenu::new()
                                .child(
                                    SidebarMenuItem::new("Appearance")
                                        .icon(IconName::Palette)
                                        .active(selected == Section::Appearance)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.select(Section::Appearance, cx);
                                        })),
                                )
                                .child(
                                    SidebarMenuItem::new("Lens Rules")
                                        .icon(IconName::Map)
                                        .active(selected == Section::LensMapping)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.select(Section::LensMapping, cx);
                                        })),
                                ),
                        ),
                    ),
            )
            .child(div().flex_1().size_full().child(content))
    }
}

#[derive(Clone, Debug)]
pub enum SettingsIntent {
    SelectLensRule(String),
}

#[derive(Default)]
struct SettingsWindow {
    window: Option<WindowHandle<Root>>,
    view: Option<WeakEntity<SettingsView>>,
}

impl Global for SettingsWindow {}

pub fn init(cx: &mut App) {
    cx.set_global(SettingsWindow::default());
}

pub fn open(cx: &mut App) {
    if let Some(handle) = cx.global::<SettingsWindow>().window
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }

    // `open_window` only gives us back the Root entity, but apply_intent needs
    // the inner SettingsView to dispatch into. Capture a weak handle to it
    // during construction so we can address it later. The handle is stored on
    // the SettingsWindow global alongside the window handle.
    let mut stashed_view: Option<WeakEntity<SettingsView>> = None;

    let handle = cx
        .open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(12.0), px(12.0))),
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(960.0), px(600.0)),
                    cx,
                ))),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| SettingsView::new(window, cx));
                stashed_view = Some(view.downgrade());
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("Failed to open settings window");

    cx.set_global(SettingsWindow {
        window: Some(handle),
        view: stashed_view,
    });
}

pub fn open_with_intent(intent: SettingsIntent, cx: &mut App) {
    open(cx);
    apply_intent(intent, cx);
}

fn apply_intent(intent: SettingsIntent, cx: &mut App) {
    // We need a window to construct the editor. The settings window is the
    // one we just opened/focused. Get it via the handle.
    let window_handle = cx.global::<SettingsWindow>().window;
    let Some(window_handle) = window_handle else {
        return;
    };

    window_handle
        .update(cx, |_, window, cx| {
            let view = cx
                .global::<SettingsWindow>()
                .view
                .as_ref()
                .and_then(|v| v.upgrade());
            if let Some(view) = view {
                view.update(cx, |view, cx| {
                    view.apply_intent(intent, window, cx);
                });
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lens::mapping::LensMapEntry;
    use crate::lens::store::LensStore;
    use crate::test_support::test_app;
    use gpui_kit::TestAppContext;

    /// `apply_intent` is where the files table's "Edit matching rule…" and
    /// "Create rule from this file…" actions land
    #[gpui_kit::test]
    fn an_intent_switches_to_the_lens_rules_section(cx: &mut TestAppContext) {
        let _tmp = test_app(cx);
        cx.update_global::<LensStore, _>(|store, _| {
            store.add("Ultron 35".to_string(), LensMapEntry::default());
        });

        let window = cx.add_window(SettingsView::new);
        window
            .update(cx, |view, window, cx| {
                view.apply_intent(
                    SettingsIntent::SelectLensRule("Ultron 35".to_string()),
                    window,
                    cx,
                );
            })
            .unwrap();

        // Appearance is the default section, so this only holds if the intent
        // was actually applied.
        let section = window.read_with(cx, |view, _| view.selected).unwrap();
        assert!(matches!(section, Section::LensMapping));
    }
}

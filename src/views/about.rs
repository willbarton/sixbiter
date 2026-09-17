// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

use gpui_kit::component::{ActiveTheme, Root, StyledExt};
use gpui_kit::{
    App, AppContext, Bounds, Context, Global, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, TitlebarOptions, Window, WindowBounds,
    WindowHandle, WindowOptions, div, px, size,
};

pub struct AboutView;

impl AboutView {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for AboutView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_3()
            .size_full()
            .items_center()
            .justify_center()
            .p_6()
            .child(div().text_2xl().font_semibold().child("Sixbiter"))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "Version {} ({})",
                        env!("CARGO_PKG_VERSION"),
                        env!("GIT_SHA"),
                    )),
            )
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .items_center()
                    .mt_4()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("© 2026 Will Barton"),
                    )
                    .child(
                        div()
                            .text_center()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Licensed under the GNU General Public License v3 or later."),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_3()
                            .child(
                                div()
                                    .id("homepage-link")
                                    .text_xs()
                                    .text_color(cx.theme().link)
                                    .child("Homepage")
                                    .on_click(|_, _, cx| {
                                        cx.open_url(env!("CARGO_PKG_HOMEPAGE"));
                                    }),
                            )
                            .child(
                                div()
                                    .id("source-link")
                                    .text_xs()
                                    .text_color(cx.theme().link)
                                    .child("Source code")
                                    .on_click(|_, _, cx| {
                                        cx.open_url(env!("CARGO_PKG_REPOSITORY"));
                                    }),
                            ),
                    ),
            )
    }
}

/// Singleton handle to the about window.
#[derive(Default)]
struct AboutWindow(Option<WindowHandle<Root>>);

impl Global for AboutWindow {}

pub fn init(cx: &mut App) {
    cx.set_global(AboutWindow::default());
}

pub fn open(cx: &mut App) {
    if let Some(handle) = cx.global::<AboutWindow>().0
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }

    let handle = cx
        .open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("About Sixbiter".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(360.0), px(240.0)),
                    cx,
                ))),
                is_resizable: false,
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| AboutView::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("Failed to open about window");

    cx.set_global(AboutWindow(Some(handle)));
}

// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

use gpui_kit::component::{
    ActiveTheme, StyledExt,
    button::{Button, ButtonVariants},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{Context, IntoElement, ParentElement, Render, Styled, Window, div};

use crate::appearance::{self, AppearancePref};

pub struct AppearanceView;

impl AppearanceView {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self
    }

    fn set_pref(&mut self, pref: AppearancePref, cx: &mut Context<Self>) {
        appearance::set_appearance_pref(pref, cx);
        cx.notify();
    }
}

impl Render for AppearanceView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = appearance::appearance_pref(cx);

        let option = |id: &'static str, label: &'static str, pref: AppearancePref| {
            let selected = current == pref;
            Button::new(id)
                .label(label)
                .when(selected, |b| b.primary())
                .when(!selected, |b| b.outline())
                .on_click(cx.listener(move |this, _, _, cx| this.set_pref(pref, cx)))
        };

        div()
            .v_flex()
            .gap_4()
            .p_6()
            .size_full()
            .child(div().text_xl().child("Appearance"))
            .child(
                div()
                    .v_flex()
                    .gap_2()
                    .child(div().text_sm().child("Theme"))
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .child(option("appearance-light", "Light", AppearancePref::Light))
                            .child(option("appearance-dark", "Dark", AppearancePref::Dark))
                            .child(option(
                                "appearance-system",
                                "System",
                                AppearancePref::System,
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("System matches your operating system's light/dark setting."),
                    ),
            )
    }
}

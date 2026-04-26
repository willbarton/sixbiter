// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

mod editor;

use gpui::{Context, Entity, IntoElement, SharedString, Subscription, Window, div, prelude::*, px};
use gpui_component::{
    ActiveTheme, Disableable, IconName, StyledExt,
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
};

use crate::lens::mapping::LensMapEntry;
use crate::lens::store::LensStore;

use editor::{LensRuleEditor, LensRuleEditorEvent};

pub struct LensMappingView {
    selected: Option<String>,
    editor: Option<Entity<LensRuleEditor>>,
    _subscription: Option<Subscription>,
}

impl LensMappingView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Auto-select the first rule on open, if any.
        let selected = cx.global::<LensStore>().rules().keys().next().cloned();
        let (editor, subscription) = match &selected {
            Some(name) => {
                let (editor, subscription) = Self::open_editor(name, window, cx);
                (Some(editor), Some(subscription))
            }
            None => (None, None),
        };
        Self {
            selected,
            editor,
            _subscription: subscription,
        }
    }

    fn open_editor(
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<LensRuleEditor>, Subscription) {
        let editor = cx.new(|cx| LensRuleEditor::new(name.to_string(), window, cx));
        let subscription = cx.subscribe(
            &editor,
            |this: &mut Self, _, event: &LensRuleEditorEvent, cx| match event {
                LensRuleEditorEvent::Renamed { from, to } => {
                    if this.selected.as_deref() == Some(from.as_str()) {
                        this.selected = Some(to.clone());
                        cx.notify();
                    }
                }
            },
        );
        (editor, subscription)
    }

    fn select(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.as_ref() == Some(&name) {
            return;
        }

        let (editor, subscription) = Self::open_editor(&name, window, cx);
        self.editor = Some(editor);
        self._subscription = Some(subscription);
        self.selected = Some(name);
        cx.notify();
    }

    fn add_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = cx.update_global::<LensStore, _>(|store, _| {
            let name = store.unique_name("New rule");
            store.add(name.clone(), LensMapEntry::default());
            name
        });
        self.select(name, window, cx);
    }

    fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.selected.take() else {
            return;
        };
        cx.update_global::<LensStore, _>(|store, _| store.remove(&name));
        self.editor = None;

        if let Some(next) = cx.global::<LensStore>().rules().keys().next().cloned() {
            self.select(next, window, cx);
        } else {
            cx.notify();
        }
    }

    pub fn select_by_name(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        // Verify the name exists in the store.
        if !cx.global::<LensStore>().rules().contains_key(name) {
            tracing::warn!(rule_name = name, "select_by_name called with unknown rule");
            return;
        }
        self.select(name.to_string(), window, cx);
    }
}

impl Render for LensMappingView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rule_names: Vec<String> = cx.global::<LensStore>().rules().keys().cloned().collect();
        let selected = self.selected.clone();

        let list = div()
            .v_flex()
            .w(px(260.0))
            .h_full()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .text_sm()
                    .font_semibold()
                    .child("Rules"),
            )
            .child(div().v_flex().flex_1().overflow_y_scrollbar().children(
                rule_names.into_iter().map(|name| {
                    let is_selected = selected.as_deref() == Some(name.as_str());
                    let click_name = name.clone();
                    div()
                        .id(SharedString::from(format!("rule-{name}")))
                        .px_3()
                        .py_2()
                        .text_sm()
                        .when(is_selected, |d| {
                            d.bg(cx.theme().accent)
                                .text_color(cx.theme().accent_foreground)
                        })
                        .when(!is_selected, |d| d.hover(|d| d.bg(cx.theme().muted)))
                        .child(name)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select(click_name.clone(), window, cx)
                        }))
                }),
            ))
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .p_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("add-rule")
                            .ghost()
                            .icon(IconName::Plus)
                            .label("Add")
                            .on_click(cx.listener(|this, _, window, cx| this.add_rule(window, cx))),
                    )
                    .child(
                        Button::new("delete-rule")
                            .ghost()
                            .icon(IconName::Delete)
                            .label("Delete")
                            .disabled(selected.is_none())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.delete_selected(window, cx)),
                            ),
                    ),
            );

        let editor = match &self.editor {
            Some(editor) => editor.clone().into_any_element(),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child("Select a rule, or add one to get started.")
                .into_any_element(),
        };

        div()
            .h_flex()
            .size_full()
            .child(list)
            .child(div().flex_1().h_full().child(editor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_app;
    use gpui::TestAppContext;

    fn add_rules(cx: &mut TestAppContext, names: &[&str]) {
        cx.update_global::<LensStore, _>(|store, _| {
            for name in names {
                store.add(name.to_string(), LensMapEntry::default());
            }
        });
    }

    #[gpui::test]
    fn renaming_the_auto_selected_rule_moves_the_selection(cx: &mut TestAppContext) {
        let _tmp = test_app(cx);
        add_rules(cx, &["A", "B"]);

        let window = cx.add_window(LensMappingView::new);
        let (selected, editor) = window
            .read_with(cx, |view, _| (view.selected.clone(), view.editor.clone()))
            .unwrap();
        assert_eq!(selected.as_deref(), Some("A"), "first rule should open");
        let editor = editor.expect("an auto-selected rule should have an editor");

        editor.update(cx, |_, cx| {
            cx.emit(LensRuleEditorEvent::Renamed {
                from: "A".to_string(),
                to: "A-renamed".to_string(),
            });
        });
        cx.run_until_parked();

        let selected = window
            .read_with(cx, |view, _| view.selected.clone())
            .unwrap();
        assert_eq!(selected.as_deref(), Some("A-renamed"));
    }

    #[gpui::test]
    fn renaming_a_different_rule_leaves_the_selection_alone(cx: &mut TestAppContext) {
        let _tmp = test_app(cx);
        add_rules(cx, &["A", "B"]);

        let window = cx.add_window(LensMappingView::new);
        let editor = window
            .read_with(cx, |view, _| view.editor.clone())
            .unwrap()
            .expect("an auto-selected rule should have an editor");

        editor.update(cx, |_, cx| {
            cx.emit(LensRuleEditorEvent::Renamed {
                from: "B".to_string(),
                to: "B-renamed".to_string(),
            });
        });
        cx.run_until_parked();

        let selected = window
            .read_with(cx, |view, _| view.selected.clone())
            .unwrap();
        assert_eq!(selected.as_deref(), Some("A"));
    }
}

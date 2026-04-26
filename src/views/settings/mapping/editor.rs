// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

use gpui::{
    App, Context, Entity, EventEmitter, SharedString, Subscription, Window, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme, IconName, StyledExt,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
};
use indexmap::IndexMap;
use serde_json::Value;

use crate::lens::mapping::json_to_string;
use crate::lens::store::LensStore;

/// One editable row in either the match or modify pane.
struct FieldRow {
    name: Entity<InputState>,
    value: Entity<InputState>,
    /// The key as it currently exists in the store. None for not-yet-saved rows.
    committed_key: Option<String>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Pane {
    Match,
    Modify,
}

#[derive(Clone, Debug)]
pub enum LensRuleEditorEvent {
    Renamed { from: String, to: String },
}

impl EventEmitter<LensRuleEditorEvent> for LensRuleEditor {}

fn subscribe_to_commit(
    input: &Entity<InputState>,
    window: &Window,
    cx: &mut Context<LensRuleEditor>,
) -> Subscription {
    cx.subscribe_in(
        input,
        window,
        |this: &mut LensRuleEditor, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                this.commit(window, cx);
            }
        },
    )
}

pub struct LensRuleEditor {
    rule_name: String,
    name_input: Entity<InputState>,
    name_error: Option<SharedString>,
    match_rows: Vec<FieldRow>,
    modify_rows: Vec<FieldRow>,
    _subscriptions: Vec<Subscription>,
}

impl LensRuleEditor {
    pub fn new(rule_name: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = cx.global::<LensStore>();
        let entry = store.rules().get(&rule_name).cloned().unwrap_or_default();

        let name_input = cx.new(|cx| InputState::new(window, cx).default_value(rule_name.as_str()));

        let make_row = |key: &str, value: &Value, window: &mut Window, cx: &mut App| -> FieldRow {
            let value_str = json_to_string(value);
            FieldRow {
                name: cx.new(|cx| InputState::new(window, cx).default_value(key)),
                value: cx.new(|cx| InputState::new(window, cx).default_value(&value_str)),
                committed_key: Some(key.to_string()),
            }
        };

        let match_rows: Vec<FieldRow> = entry
            .tomatch
            .iter()
            .map(|(k, v)| make_row(k, v, window, cx))
            .collect();
        let modify_rows: Vec<FieldRow> = entry
            .modify
            .iter()
            .map(|(k, v)| make_row(k, v, window, cx))
            .collect();

        // Subscribe to every input's events. Commit on Blur or Enter.
        let subscriptions: Vec<Subscription> = std::iter::once(&name_input)
            .chain(match_rows.iter().flat_map(|r| [&r.name, &r.value]))
            .chain(modify_rows.iter().flat_map(|r| [&r.name, &r.value]))
            .map(|input| subscribe_to_commit(input, window, cx))
            .collect();

        Self {
            rule_name,
            name_input,
            name_error: None,
            match_rows,
            modify_rows,
            _subscriptions: subscriptions,
        }
    }

    fn rows_mut(&mut self, pane: Pane) -> &mut Vec<FieldRow> {
        match pane {
            Pane::Match => &mut self.match_rows,
            Pane::Modify => &mut self.modify_rows,
        }
    }

    fn add_row(&mut self, pane: Pane, window: &mut Window, cx: &mut Context<Self>) {
        let row = FieldRow {
            name: cx.new(|cx| InputState::new(window, cx)),
            value: cx.new(|cx| InputState::new(window, cx)),
            committed_key: None,
        };

        for input in [&row.name, &row.value] {
            self._subscriptions
                .push(subscribe_to_commit(input, window, cx));
        }

        self.rows_mut(pane).push(row);
        cx.notify();
    }

    fn delete_row(&mut self, pane: Pane, index: usize, cx: &mut Context<Self>) {
        let rows = self.rows_mut(pane);
        if index >= rows.len() {
            return;
        }
        let row = rows.remove(index);
        if let Some(key) = row.committed_key {
            let rule_name = self.rule_name.clone();
            cx.update_global::<LensStore, _>(|store, _| {
                store.update(&rule_name, |entry| {
                    let map = match pane {
                        Pane::Match => &mut entry.tomatch,
                        Pane::Modify => &mut entry.modify,
                    };
                    map.shift_remove(&key);
                });
            });
        }
        cx.notify();
    }

    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_name = self.name_input.read(cx).value().to_string();
        let match_map = self.collect_pane(Pane::Match, cx);
        let modify_map = self.collect_pane(Pane::Modify, cx);

        let old_name = self.rule_name.clone();
        let wants_rename = new_name != old_name && !new_name.is_empty();

        let renamed = cx.update_global::<LensStore, _>(|store, _| {
            store.update(&old_name, |entry| {
                entry.tomatch = match_map;
                entry.modify = modify_map;
            });
            wants_rename && store.rename(&old_name, new_name.clone())
        });

        if renamed {
            cx.emit(LensRuleEditorEvent::Renamed {
                from: old_name.clone(),
                to: new_name.clone(),
            });
            self.rule_name = new_name;
            self.name_error = None;
        } else if wants_rename {
            self.name_error = Some(format!("A rule named \"{new_name}\" already exists.").into());
            self.name_input.update(cx, |input, cx| {
                input.set_value(old_name.clone(), window, cx);
            });
            cx.notify();
        } else {
            self.name_error = None;
        }

        // Update committed_key on rows whose name changed.
        for pane in [Pane::Match, Pane::Modify] {
            for row in self.rows_mut(pane) {
                let current = row.name.read(cx).value().to_string();
                row.committed_key = if current.is_empty() {
                    None
                } else {
                    Some(current)
                };
            }
        }
    }

    fn collect_pane(&self, pane: Pane, cx: &App) -> IndexMap<String, Value> {
        let rows = match pane {
            Pane::Match => &self.match_rows,
            Pane::Modify => &self.modify_rows,
        };
        let mut map = IndexMap::new();
        for row in rows {
            let key = row.name.read(cx).value().to_string();
            if key.is_empty() {
                continue;
            }
            let value = Value::String(row.value.read(cx).value().to_string());
            map.insert(key, value);
        }
        map
    }
}

impl Render for LensRuleEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_4()
            .p_6()
            .size_full()
            .overflow_y_scrollbar()
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Rule name"),
                    )
                    .child(Input::new(&self.name_input))
                    .when_some(self.name_error.clone(), |d, message| {
                        d.child(div().text_xs().text_color(cx.theme().danger).child(message))
                    }),
            )
            .child(self.render_pane(Pane::Match, "When EXIF matches", cx))
            .child(self.render_pane(Pane::Modify, "Change EXIF to", cx))
    }
}

impl LensRuleEditor {
    fn render_pane(
        &mut self,
        pane: Pane,
        title: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let rows = match pane {
            Pane::Match => &self.match_rows,
            Pane::Modify => &self.modify_rows,
        };

        let row_views: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(idx, row)| {
                div()
                    .h_flex()
                    .gap_2()
                    .child(div().w(px(180.0)).child(Input::new(&row.name)))
                    .child(div().flex_1().child(Input::new(&row.value)))
                    .child(
                        Button::new(SharedString::from(format!("del-{}-{idx}", pane as u8)))
                            .ghost()
                            .icon(IconName::Delete)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.delete_row(pane, idx, cx);
                                this.commit(window, cx);
                            })),
                    )
                    .into_any_element()
            })
            .collect();

        div()
            .v_flex()
            .gap_2()
            .child(div().text_sm().font_semibold().child(title))
            .children(row_views)
            .child(
                Button::new(SharedString::from(format!("add-{}", pane as u8)))
                    .ghost()
                    .icon(IconName::Plus)
                    .label("Add field")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.add_row(pane, window, cx);
                    })),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lens::mapping::LensMapEntry;
    use crate::test_support::test_app;
    use gpui::{TestAppContext, WindowHandle};
    use serde_json::json;

    fn add_rules(cx: &mut TestAppContext, names: &[&str]) {
        cx.update_global::<LensStore, _>(|store, _| {
            for name in names {
                store.add(name.to_string(), LensMapEntry::default());
            }
        });
    }

    /// Open an editor window on an existing rule.
    fn open_editor(cx: &mut TestAppContext, name: &str) -> WindowHandle<LensRuleEditor> {
        let name = name.to_string();
        cx.add_window(|window, cx| LensRuleEditor::new(name, window, cx))
    }

    fn rule(cx: &TestAppContext, name: &str) -> LensMapEntry {
        cx.read_global::<LensStore, _>(|store, _| {
            store.rules().get(name).cloned().expect("rule should exist")
        })
    }

    /// Add a field row to `pane`, fill it in, and commit as a blur would.
    fn set_field(
        window: &WindowHandle<LensRuleEditor>,
        cx: &mut TestAppContext,
        pane: Pane,
        name: &str,
        value: &str,
    ) {
        window
            .update(cx, |editor, window, cx| {
                editor.add_row(pane, window, cx);
                let row = editor.rows_mut(pane).last().expect("just added");
                let (name_input, value_input) = (row.name.clone(), row.value.clone());
                name_input.update(cx, |i, cx| i.set_value(name.to_string(), window, cx));
                value_input.update(cx, |i, cx| i.set_value(value.to_string(), window, cx));
                editor.commit(window, cx);
            })
            .unwrap();
    }

    /// Type `value` into the editor's name field, then commit as a blur would.
    fn rename_to(window: &WindowHandle<LensRuleEditor>, cx: &mut TestAppContext, value: &str) {
        window
            .update(cx, |editor, window, cx| {
                editor.name_input.update(cx, |input, cx| {
                    input.set_value(value.to_string(), window, cx);
                });
                editor.commit(window, cx);
            })
            .unwrap();
        cx.run_until_parked();
    }

    #[gpui::test]
    fn commit_writes_the_panes_into_the_store(cx: &mut TestAppContext) {
        let _tmp = test_app(cx);
        add_rules(cx, &["A"]);
        let window = open_editor(cx, "A");
        set_field(&window, cx, Pane::Match, "LensModel", "Summicron");

        assert_eq!(rule(cx, "A").tomatch["LensModel"], json!("Summicron"));
    }

    #[gpui::test]
    fn commit_skips_rows_with_no_field_name(cx: &mut TestAppContext) {
        let _tmp = test_app(cx);
        add_rules(cx, &["A"]);
        let window = open_editor(cx, "A");
        // A value with no key -- a half-filled row the user abandoned.
        set_field(&window, cx, Pane::Modify, "", "orphan");

        assert!(rule(cx, "A").modify.is_empty());
    }

    #[gpui::test]
    fn commit_renames_the_rule_and_announces_it(cx: &mut TestAppContext) {
        let _tmp = test_app(cx);
        add_rules(cx, &["A", "B"]);
        let window = open_editor(cx, "A");
        let editor = window.entity(cx).unwrap();
        let mut events = cx.events::<LensRuleEditorEvent, _>(&editor);

        rename_to(&window, cx, "A-renamed");

        cx.read_global::<LensStore, _>(|store, _| {
            assert!(store.rules().contains_key("A-renamed"));
            assert!(!store.rules().contains_key("A"));
        });
        assert_eq!(
            window.read_with(cx, |e, _| e.rule_name.clone()).unwrap(),
            "A-renamed",
        );

        // The announcement is what moves the sidebar selection. `mapping.rs`
        // tests the listener; this is the half that takes it on trust.
        let event = events
            .try_recv()
            .expect("a successful rename should emit Renamed");
        let LensRuleEditorEvent::Renamed { from, to } = event;
        assert_eq!(from, "A");
        assert_eq!(to, "A-renamed");
    }

    #[gpui::test]
    fn an_empty_name_is_not_a_rename(cx: &mut TestAppContext) {
        let _tmp = test_app(cx);
        add_rules(cx, &["A"]);
        let window = open_editor(cx, "A");
        rename_to(&window, cx, "");

        cx.read_global::<LensStore, _>(|store, _| assert!(store.rules().contains_key("A")));
        assert_eq!(
            window.read_with(cx, |e, _| e.rule_name.clone()).unwrap(),
            "A"
        );
    }

    /// Typing a name another rule already owns is refused, said so, and undone.
    #[gpui::test]
    fn a_colliding_rename_is_refused_and_reverts_the_field(cx: &mut TestAppContext) {
        let _tmp = test_app(cx);
        add_rules(cx, &["A", "B"]);
        let window = open_editor(cx, "A");
        let editor = window.entity(cx).unwrap();
        let mut events = cx.events::<LensRuleEditorEvent, _>(&editor);

        rename_to(&window, cx, "B");

        let (rule_name, error, field) = window
            .read_with(cx, |e, cx| {
                (
                    e.rule_name.clone(),
                    e.name_error.clone(),
                    e.name_input.read(cx).value().to_string(),
                )
            })
            .unwrap();

        // `rule_name` is the key every later commit writes to: had the editor
        // adopted "B", the next edit would have overwritten the real rule B.
        assert_eq!(rule_name, "A", "the editor still edits A");
        assert!(error.is_some(), "the refusal is surfaced, not silent");
        assert_eq!(field, "A", "and the field is put back to match");
        assert!(
            events.try_recv().is_err(),
            "no Renamed event -- the sidebar would have followed it",
        );
    }

    #[gpui::test]
    fn deleting_a_row_removes_the_field_from_the_store(cx: &mut TestAppContext) {
        let _tmp = test_app(cx);
        cx.update_global::<LensStore, _>(|store, _| {
            let mut entry = LensMapEntry::default();
            entry.tomatch.insert("LensModel".into(), json!("Summicron"));
            entry.tomatch.insert("LensMake".into(), json!("Leica"));
            store.add("A".to_string(), entry);
        });

        let window = open_editor(cx, "A");
        window
            .update(cx, |editor, window, cx| {
                editor.delete_row(Pane::Match, 0, cx);
                editor.commit(window, cx);
            })
            .unwrap();

        let entry = rule(cx, "A");
        assert!(!entry.tomatch.contains_key("LensModel"));
        assert!(entry.tomatch.contains_key("LensMake"));
    }
}

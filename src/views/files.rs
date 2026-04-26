// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

mod delegate;
pub mod helpers;

use futures::StreamExt;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement,
    PathPromptOptions, SharedString, Task, Window, actions, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme, Disableable, IconName, Sizable, StyledExt,
    button::{Button, ButtonVariants, DropdownButton},
    menu::PopupMenuItem,
    switch::Switch,
    table::TableState,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use walkdir::WalkDir;

use crate::lens::mapping::{LensMap, LensMapEntry};
use crate::lens::metadata::MetadataReader;
use crate::lens::store::LensStore;
use crate::preferences::{self, PreferencesStore};

use delegate::{FileRow, FilesDelegate, RowStatus};
use helpers::{is_dng, read_one, relativize, rule_from_file};

actions!(files, [Open]);

fn muted_label(cx: &App, text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .mr_2()
        .child(text.into())
}

pub struct FilesView {
    recursive: bool,
    apply_with_backup: bool,
    matches_only: bool,
    has_scanned: bool,
    root: Option<PathBuf>,
    busy: bool,
    table: Entity<TableState<FilesDelegate>>,
    reader: Arc<MetadataReader>,
    focus_handle: FocusHandle,
    cancel_flag: Arc<AtomicBool>,
    _pick_task: Option<Task<()>>,
    _scan_task: Option<Task<()>>,
    _apply_task: Option<Task<()>>,
}

impl FilesView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let reader = Arc::new(MetadataReader::new());

        let table = cx.new(|cx| {
            TableState::new(FilesDelegate::new(), window, cx)
                .row_selectable(false)
                .col_selectable(false)
        });

        // Set the parent handle so the delegate can dispatch back to FilesView.
        let weak = cx.weak_entity();
        table.update(cx, |state, _| {
            state.delegate_mut().set_parent(weak);
        });

        let prefs = cx.global::<PreferencesStore>().get().clone();

        Self {
            recursive: prefs.recursive,
            apply_with_backup: prefs.apply_with_backup,
            matches_only: false,
            has_scanned: false,
            root: prefs.last_folder,
            busy: false,
            table,
            reader,
            focus_handle: cx.focus_handle(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            _pick_task: None,
            _scan_task: None,
            _apply_task: None,
        }
    }

    fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.busy = busy;
        self.table
            .update(cx, |state, _| state.delegate_mut().set_busy(busy));
        cx.notify();
    }

    fn scan(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        self.has_scanned = true;

        self.cancel_flag.store(false, Ordering::Relaxed);
        let cancel = self.cancel_flag.clone();

        let recursive = self.recursive;
        let reader = self.reader.clone();
        let table = self.table.clone();

        tracing::info!(?root, recursive, "starting scan");

        let task = cx.spawn(async move |this, cx| {
            let lens_map: LensMap = cx.update(|cx| cx.global::<LensStore>().rules().clone());

            this.update(cx, |this, cx| this.set_busy(true, cx)).ok();

            table.update(cx, |state, cx| {
                state.delegate_mut().clear();
                cx.notify();
            });

            let (tx, mut rx) = futures::channel::mpsc::unbounded::<FileRow>();
            let walk_root = root.clone();

            cx.background_executor()
                .spawn(async move {
                    let max_depth = if recursive { usize::MAX } else { 1 };
                    let walker = WalkDir::new(&walk_root)
                        .max_depth(max_depth)
                        .follow_links(false)
                        .sort_by(|a, b| a.file_name().cmp(b.file_name()))
                        .into_iter()
                        .filter_map(Result::ok)
                        .filter(|e| e.file_type().is_file())
                        .filter(|e| is_dng(e.path()));

                    for entry in walker {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }

                        let abs = entry.into_path();
                        let rel = relativize(&abs, &walk_root);
                        let row = read_one(&abs, rel, &lens_map, &reader);

                        tracing::trace!(file = %abs.display(), "scanning file");

                        if tx.unbounded_send(row).is_err() {
                            break;
                        }
                    }
                })
                .detach();

            while let Some(row) = rx.next().await {
                table.update(cx, |state, cx| {
                    state.delegate_mut().push(row);
                    cx.notify();
                });
            }

            this.update(cx, |this, cx| this.set_busy(false, cx)).ok();
        });

        self._scan_task = Some(task);
    }

    fn pick_folder(&mut self, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| {
            let receiver = cx.update(|cx| {
                cx.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: Some("Choose a folder".into()),
                })
            });

            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(root) = paths.into_iter().next() else {
                return;
            };

            this.update(cx, |this, cx| {
                this.root = Some(root.clone());
                let folder = root.clone();
                preferences::update(cx, |prefs| prefs.last_folder = Some(folder));
                this.scan(root, cx);
            })
            .ok();
        });

        self._pick_task = Some(task);
    }

    fn apply(&mut self, with_backup: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        let table = self.table.clone();
        let reader = self.reader.clone();

        // Snapshot which rows to apply, with their data, before kicking off the
        // task. The task can't read the table directly (not Send).
        let work: Vec<(usize, PathBuf, LensMapEntry)> = {
            let delegate = table.read(cx).delegate();
            delegate
                .pending_apply_indices()
                .into_iter()
                .filter_map(|i| {
                    let row = &delegate.rows[i];
                    row.matched_entry
                        .as_ref()
                        .map(|e| (i, row.abs_path.clone(), e.clone()))
                })
                .collect()
        };

        if work.is_empty() {
            return;
        }

        tracing::info!(rules = work.len(), with_backup, "starting apply");

        self.set_busy(true, cx);

        let task = cx.spawn(async move |this, cx| {
            // Channel for streaming status updates back to the UI thread.
            let (tx, mut rx) = futures::channel::mpsc::unbounded::<(usize, RowStatus)>();

            cx.background_executor()
                .spawn(async move {
                    for (idx, path, entry) in work {
                        let _ = tx.unbounded_send((idx, RowStatus::Applying));
                        let status = match reader.write(&path, &entry, with_backup) {
                            Ok(()) => {
                                tracing::debug!(path = %path.display(), "applied rule");
                                RowStatus::Applied
                            },
                            Err(err) => {
                                tracing::warn!(path = %path.display(), error = %err, "apply failed");
                                RowStatus::Failed(err.to_string())
                            },
                        };
                        if tx.unbounded_send((idx, status)).is_err() {
                            break;
                        }
                    }
                })
                .detach();

            while let Some((idx, status)) = rx.next().await {
                table.update(cx, |state, cx| {
                    state.delegate_mut().set_status(idx, status);
                    cx.notify();
                });
            }

            this.update(cx, |this, cx| this.set_busy(false, cx)).ok();
        });

        self._apply_task = Some(task);
    }

    fn reveal_folder(&mut self, _cx: &mut Context<Self>) {
        let Some(_root) = self.root.as_ref() else {
            return;
        };
        // macOS-only for now; on Linux/Windows you'd shell out differently.
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg(_root).spawn();
        }
    }

    fn rescan(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        if self.busy {
            return;
        }
        self.scan(root, cx);
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.root = None;
        self.has_scanned = false;
        preferences::update(cx, |prefs| prefs.last_folder = None);
        let table = self.table.clone();
        table.update(cx, |state, cx| {
            state.delegate_mut().clear();
            cx.notify();
        });
        cx.notify();
    }

    fn cancel_current(&mut self, cx: &mut Context<Self>) {
        self.cancel_flag.store(true, Ordering::Relaxed);
        self._pick_task = None;
        self._scan_task = None;
        self._apply_task = None;
        self.set_busy(false, cx);
    }

    fn toggle_recursive(&mut self, cx: &mut Context<Self>) {
        self.recursive = !self.recursive;
        preferences::update(cx, |prefs| prefs.recursive = self.recursive);
        cx.notify();
    }

    fn toggle_matches_only(&mut self, cx: &mut Context<Self>) {
        self.matches_only = !self.matches_only;
        let value = self.matches_only;
        self.table.update(cx, |state, cx| {
            state.delegate_mut().set_matches_only(value);
            cx.notify();
        });
        cx.notify();
    }

    fn toggle_backup(&mut self, cx: &mut Context<Self>) {
        self.apply_with_backup = !self.apply_with_backup;
        preferences::update(cx, |prefs| prefs.apply_with_backup = self.apply_with_backup);
        cx.notify();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let recursive = self.recursive;
        let has_root = self.root.is_some();
        let is_busy = self.busy;

        let folder_button = Button::new("open-folder-main")
            .primary()
            .small()
            .icon(IconName::Folder)
            .label(if recursive { "Open recursive" } else { "Open" })
            .disabled(is_busy)
            .on_click(cx.listener(|this, _, _, cx| this.pick_folder(cx)));

        let open_dropdown = DropdownButton::new("open-folder-dropdown")
            .primary()
            .small()
            .button(folder_button)
            .dropdown_menu(move |menu, window, _cx| {
                let mut menu = menu.item(
                    PopupMenuItem::new("Recurse subfolders")
                        .when(recursive, |item| item.icon(IconName::Check))
                        .on_click(window.listener_for(&view, |this, _, _w, cx| {
                            this.toggle_recursive(cx);
                        })),
                );
                if has_root {
                    menu = menu
                        .separator()
                        .item(
                            PopupMenuItem::new("Reveal in Finder")
                                .icon(IconName::ExternalLink)
                                .on_click(window.listener_for(&view, |this, _, _, cx| {
                                    this.reveal_folder(cx);
                                })),
                        )
                        .item(
                            PopupMenuItem::new("Rescan folder")
                                .disabled(is_busy)
                                .on_click(window.listener_for(&view, |this, _, _, cx| {
                                    this.rescan(cx);
                                })),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("Close folder")
                                .disabled(is_busy)
                                .on_click(window.listener_for(&view, |this, _, _, cx| {
                                    this.reset(cx);
                                })),
                        );
                }
                if is_busy {
                    menu = menu.separator().item(
                        PopupMenuItem::new("Cancel").icon(IconName::Close).on_click(
                            window.listener_for(&view, |this, _, _w, cx| {
                                this.cancel_current(cx);
                            }),
                        ),
                    );
                }
                menu
            });

        div()
            .h_flex()
            .items_center()
            .justify_between()
            .pl(px(80.0))
            .pr_3()
            .py_2()
            .gap_1()
            .bg(cx.theme().secondary)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(match self.root.as_deref() {
                Some(path) => div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(path.display().to_string())
                    .into_any_element(),
                None => div().into_any_element(),
            })
            .when(is_busy, |d| d.child(muted_label(cx, "Working…")))
            .when(!is_busy && has_root, |d| {
                let count = self.table.read(cx).delegate().len();
                let suffix = if count == 1 { "" } else { "s" };
                d.child(muted_label(cx, format!("{count} file{suffix}")))
            })
            .child(open_dropdown)
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_busy = self.busy;
        let with_backup = self.apply_with_backup;
        let pending_apply_count = self.table.read(cx).delegate().pending_apply_indices().len();

        let apply_button = Button::new("apply-main")
            .icon(IconName::Check)
            .label(if with_backup {
                "Apply"
            } else {
                "Apply (no backup)"
            })
            .disabled(is_busy || pending_apply_count == 0)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.apply(this.apply_with_backup, cx);
            }));

        let apply_dropdown = DropdownButton::new("apply-dropdown")
            .primary()
            .small()
            .disabled(is_busy || pending_apply_count == 0)
            .button(apply_button)
            .dropdown_menu(move |menu, window, _cx| {
                menu.item(
                    PopupMenuItem::new("Create backup files")
                        .when(with_backup, |item| item.icon(IconName::Check))
                        .on_click(window.listener_for(&view, |this, _, _w, cx| {
                            this.toggle_backup(cx);
                        })),
                )
            });

        let matches_only = self.matches_only;
        let matches_switch = Switch::new("filter-matches-only")
            .small()
            .checked(matches_only)
            .label("Hide unmatched")
            .on_click(cx.listener(|this, _, _, cx| this.toggle_matches_only(cx)));

        div()
            .h_flex()
            .items_center()
            .justify_end()
            .px_3()
            .py_2()
            .gap_3()
            .bg(cx.theme().secondary)
            .border_t_1()
            .border_color(cx.theme().border)
            .when(is_busy, |d| d.child(muted_label(cx, "Working…")))
            .when(!is_busy && pending_apply_count > 0, |d| {
                d.child(muted_label(cx, format!("{pending_apply_count} ready")))
            })
            .child(matches_switch)
            .child(apply_dropdown)
            .into_any_element()
    }

    fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        match (&self.root, self.has_scanned) {
            (None, _) => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted)
                .child("Open a folder to scan for DNG files.")
                .into_any_element(),
            (Some(root), false) => {
                let folder_name = root.display().to_string();
                div()
                    .flex_1()
                    .v_flex()
                    .gap_2()
                    .items_center()
                    .justify_center()
                    .text_color(muted)
                    .child(format!("Restore {folder_name}?"))
                    .child(
                        Button::new("rescan-from-empty")
                            .primary()
                            .small()
                            .icon(IconName::Redo2)
                            .label("Rescan folder")
                            .on_click(cx.listener(|this, _, _, cx| this.rescan(cx))),
                    )
                    .into_any_element()
            }
            (Some(_), true) => div().flex_1().child(self.table.clone()).into_any_element(),
        }
    }

    fn reveal_file(&mut self, _path: &std::path::Path, _cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open")
                .arg("-R") // -R reveals the file in Finder rather than opening it
                .arg(_path)
                .spawn();
        }
    }

    fn edit_matching_rule(&mut self, rule_name: String, cx: &mut Context<Self>) {
        crate::views::settings::open_with_intent(
            crate::views::settings::SettingsIntent::SelectLensRule(rule_name),
            cx,
        );
    }

    fn create_rule_from_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let Some((base_name, entry)) = rule_from_file(path, &self.reader) else {
            return;
        };

        let name = cx.update_global::<LensStore, _>(|store, _| {
            let name = store.unique_name(&base_name);
            store.add(name.clone(), entry);
            name
        });

        crate::views::settings::open_with_intent(
            crate::views::settings::SettingsIntent::SelectLensRule(name),
            cx,
        );
    }
}

impl Focusable for FilesView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FilesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .size_full()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &Open, _, cx| {
                this.pick_folder(cx);
            }))
            .child(self.render_header(cx))
            .child(self.render_body(cx))
            .child(self.render_footer(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_app;
    use gpui::{TestAppContext, WindowHandle};
    use std::path::Path;

    /// A `FilesView` in a test window, with the app's globals rooted at a
    /// fresh temp dir.
    fn files_view(cx: &mut TestAppContext) -> (tempfile::TempDir, WindowHandle<FilesView>) {
        let tmp = test_app(cx);
        let window = cx.add_window(FilesView::new);
        (tmp, window)
    }

    fn stub_file(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"not a real dng").unwrap();
    }

    fn real_dng(dir: &Path) -> PathBuf {
        let path = dir.join("sample.dng");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.dng"),
            &path,
        )
        .unwrap();
        path
    }

    fn matched_row(path: &Path, new_model: &str) -> FileRow {
        FileRow {
            rel_path: PathBuf::from(path.file_name().unwrap()),
            abs_path: path.to_path_buf(),
            current_lens: "Original".to_string(),
            matched_rule: Some("rule".to_string()),
            matched_entry: Some(LensMapEntry {
                tomatch: indexmap::IndexMap::new(),
                modify: indexmap::IndexMap::from([(
                    "LensModel".into(),
                    serde_json::json!(new_model),
                )]),
            }),
            error: None,
            selected: true,
            status: RowStatus::Pending,
        }
    }

    #[gpui::test]
    fn pick_folder_remembers_the_chosen_folder(cx: &mut TestAppContext) {
        let (tmp, window) = files_view(cx);

        let chosen = tmp.path().join("photos");
        std::fs::create_dir(&chosen).unwrap();

        window
            .update(cx, |view, _window, cx| view.pick_folder(cx))
            .unwrap();

        // Let the spawned task reach `prompt_for_paths` before answering it
        cx.run_until_parked();
        assert!(cx.did_prompt_for_paths());

        let selected = chosen.clone();
        cx.simulate_path_prompt_response(move |options| {
            assert!(options.directories, "should ask for a folder");
            assert!(!options.files, "should not offer individual files");
            Some(vec![selected])
        });
        cx.run_until_parked();

        let root = window.read_with(cx, |view, _| view.root.clone()).unwrap();
        assert_eq!(root.as_deref(), Some(chosen.as_path()));

        let remembered =
            cx.read_global::<PreferencesStore, _>(|store, _| store.get().last_folder.clone());
        assert_eq!(remembered.as_deref(), Some(chosen.as_path()));
    }

    #[gpui::test]
    fn scan_adds_a_row_for_each_dng(cx: &mut TestAppContext) {
        let (tmp, window) = files_view(cx);

        let photos = tmp.path().join("photos");
        std::fs::create_dir(&photos).unwrap();
        stub_file(&photos, "a.dng");
        stub_file(&photos, "b.DNG");
        stub_file(&photos, "notes.txt");

        window
            .update(cx, |view, _window, cx| view.scan(photos.clone(), cx))
            .unwrap();
        cx.run_until_parked();

        let (rows, errors, busy) = window
            .read_with(cx, |view, cx| {
                let delegate = view.table.read(cx).delegate();
                let errors = delegate.rows.iter().filter(|r| r.error.is_some()).count();
                (delegate.len(), errors, view.busy)
            })
            .unwrap();

        // Both .dng spellings are picked up, the .txt is not.
        assert_eq!(rows, 2);
        // Neither is a real DNG, so both should have read errors
        assert_eq!(errors, 2);
        assert!(!busy, "scan should clear the busy flag when it finishes");
    }

    #[gpui::test]
    fn reset_clears_the_folder_and_forgets_it(cx: &mut TestAppContext) {
        let (tmp, window) = files_view(cx);

        let photos = tmp.path().join("photos");
        std::fs::create_dir(&photos).unwrap();
        stub_file(&photos, "a.dng");

        window
            .update(cx, |view, _window, cx| {
                view.root = Some(photos.clone());
                preferences::update(cx, |prefs| prefs.last_folder = Some(photos.clone()));
                view.scan(photos.clone(), cx);
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |view, _window, cx| view.reset(cx))
            .unwrap();
        cx.run_until_parked();

        let (root, rows) = window
            .read_with(cx, |view, cx| {
                (view.root.clone(), view.table.read(cx).delegate().len())
            })
            .unwrap();
        assert_eq!(root, None);
        assert_eq!(rows, 0);

        let remembered =
            cx.read_global::<PreferencesStore, _>(|store, _| store.get().last_folder.clone());
        assert_eq!(remembered, None);
    }

    #[gpui::test]
    fn apply_writes_the_matched_rule_and_marks_the_row_applied(cx: &mut TestAppContext) {
        let (tmp, window) = files_view(cx);
        let dng = real_dng(tmp.path());

        window
            .update(cx, |view, _window, cx| {
                view.table.update(cx, |state, _| {
                    state
                        .delegate_mut()
                        .push(matched_row(&dng, "Ultron 35 mm f/2 aspherical VM"));
                });
                view.apply(false, cx);
            })
            .unwrap();
        cx.run_until_parked();

        let (status, busy) = window
            .read_with(cx, |view, cx| {
                let delegate = view.table.read(cx).delegate();
                (delegate.rows[0].status.clone(), view.busy)
            })
            .unwrap();
        assert_eq!(status, RowStatus::Applied);
        assert!(!busy, "apply should clear the busy flag when it finishes");

        // The rule reached the file, not just the row.
        let written = MetadataReader::new().read(&dng).unwrap();
        assert_eq!(
            written.get("LensModel").and_then(|v| v.as_str()),
            Some("Ultron 35 mm f/2 aspherical VM"),
        );
    }

    #[gpui::test]
    fn apply_marks_unwritable_files_as_failed(cx: &mut TestAppContext) {
        let (tmp, window) = files_view(cx);
        stub_file(tmp.path(), "broken.dng");
        let broken = tmp.path().join("broken.dng");

        window
            .update(cx, |view, _window, cx| {
                view.table.update(cx, |state, _| {
                    state.delegate_mut().push(matched_row(&broken, "Whatever"));
                });
                view.apply(false, cx);
            })
            .unwrap();
        cx.run_until_parked();

        let (status, busy) = window
            .read_with(cx, |view, cx| {
                let delegate = view.table.read(cx).delegate();
                (delegate.rows[0].status.clone(), view.busy)
            })
            .unwrap();
        assert!(
            matches!(status, RowStatus::Failed(_)),
            "expected a per-row failure, got {status:?}",
        );
        assert!(!busy, "a failed row should still clear the busy flag");
    }

    #[gpui::test]
    fn apply_leaves_unselected_rows_pending(cx: &mut TestAppContext) {
        let (tmp, window) = files_view(cx);
        stub_file(tmp.path(), "a.dng");
        stub_file(tmp.path(), "b.dng");

        let mut skipped = matched_row(&tmp.path().join("b.dng"), "X");
        skipped.selected = false;

        window
            .update(cx, |view, _window, cx| {
                view.table.update(cx, |state, _| {
                    let delegate = state.delegate_mut();
                    delegate.push(matched_row(&tmp.path().join("a.dng"), "X"));
                    delegate.push(skipped);
                });
                view.apply(false, cx);
            })
            .unwrap();
        cx.run_until_parked();

        let statuses = window
            .read_with(cx, |view, cx| {
                let delegate = view.table.read(cx).delegate();
                delegate
                    .rows
                    .iter()
                    .map(|r| r.status.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap();

        assert_ne!(statuses[0], RowStatus::Pending, "checked row was attempted");
        assert_eq!(statuses[1], RowStatus::Pending, "unchecked row was skipped");
    }

    /// Test rule matching end-to-end
    #[gpui::test]
    fn scan_matches_stored_rules_against_real_files(cx: &mut TestAppContext) {
        let (tmp, window) = files_view(cx);

        let photos = tmp.path().join("photos");
        std::fs::create_dir(&photos).unwrap();
        real_dng(&photos);
        stub_file(&photos, "unreadable.dng");

        cx.update_global::<LensStore, _>(|store, _| {
            store.add(
                "Summilux 90".to_string(),
                LensMapEntry {
                    tomatch: indexmap::IndexMap::from([(
                        "LensModel".into(),
                        serde_json::json!("Summilux-M 1:1.5/90 ASPH."),
                    )]),
                    modify: indexmap::IndexMap::from([(
                        "LensModel".into(),
                        serde_json::json!("Corrected 90"),
                    )]),
                },
            );
        });

        window
            .update(cx, |view, _window, cx| view.scan(photos.clone(), cx))
            .unwrap();
        cx.run_until_parked();

        let (matched, ready) = window
            .read_with(cx, |view, cx| {
                let delegate = view.table.read(cx).delegate();
                let matched: Vec<String> = delegate
                    .rows
                    .iter()
                    .filter_map(|r| r.matched_rule.clone())
                    .collect();
                (matched, delegate.pending_apply_indices().len())
            })
            .unwrap();

        assert_eq!(
            matched,
            ["Summilux 90"],
            "only the real DNG matched the rule"
        );
        assert_eq!(ready, 1, "and it is checked, ready to apply");
    }
}

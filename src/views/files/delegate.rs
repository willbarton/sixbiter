// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Will Barton

use gpui::{App, Context, SharedString, WeakEntity, Window, div, prelude::*};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Sizable, StyledExt,
    checkbox::Checkbox,
    menu::{PopupMenu, PopupMenuItem},
    table::{Column, ColumnSort, TableDelegate, TableState},
    tooltip::Tooltip,
};
use std::cmp::Ordering;
use std::path::PathBuf;

use crate::lens::mapping::LensMapEntry;
use crate::views::files::FilesView;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HeaderSelection {
    /// No row can be applied, so the header checkbox is disabled.
    NothingEligible,
    /// At least one eligible row is unselected.
    NotAll,
    /// Every eligible row is selected.
    All,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RowStatus {
    #[default]
    Pending,
    Applying,
    Applied,
    Failed(String),
}

pub struct FileRow {
    pub rel_path: PathBuf,
    pub abs_path: PathBuf,
    pub current_lens: String,
    pub matched_rule: Option<String>,
    pub matched_entry: Option<LensMapEntry>,
    pub error: Option<String>,
    pub selected: bool,
    pub status: RowStatus,
}

/// Delegate for the files table. Owns the row data.
pub struct FilesDelegate {
    pub(super) rows: Vec<FileRow>,
    pub(super) columns: Vec<Column>,
    pub(super) parent: Option<WeakEntity<FilesView>>,
    pub(super) matches_only: bool,
    /// True while a scan or apply is in flight
    busy: bool,
    visible: Vec<usize>,
}

const SORTABLE_COLUMNS: &[&str] = &["path", "lens", "rule"];

impl FilesDelegate {
    pub(super) fn new() -> Self {
        Self {
            rows: Vec::new(),
            columns: vec![
                Column::new("selected", "").width(36.),
                Column::new("path", "Path").width(320.),
                Column::new("lens", "Current lens").width(220.),
                Column::new("rule", "Matched rule").width(220.),
                Column::new("status", "Status").width(120.),
            ],
            parent: None,
            matches_only: false,
            busy: false,
            visible: Vec::new(),
        }
    }

    pub(super) fn set_parent(&mut self, parent: WeakEntity<FilesView>) {
        self.parent = Some(parent);
    }

    pub(super) fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
    }

    /// Northing is sortable while busy
    fn is_sortable(&self, key: &str) -> bool {
        !self.busy && SORTABLE_COLUMNS.contains(&key)
    }

    fn rebuild_visible(&mut self) {
        self.visible.clear();
        if self.matches_only {
            self.visible.extend(
                self.rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.matched_rule.is_some())
                    .map(|(i, _)| i),
            );
        } else {
            self.visible.extend(0..self.rows.len());
        }
    }

    pub(super) fn set_matches_only(&mut self, value: bool) {
        if self.matches_only == value {
            return;
        }
        self.matches_only = value;
        self.rebuild_visible();
    }

    pub(super) fn clear(&mut self) {
        self.rows.clear();
        self.rebuild_visible();
    }

    pub(super) fn push(&mut self, row: FileRow) {
        let index = self.rows.len();
        let is_visible = !self.matches_only || row.matched_rule.is_some();
        self.rows.push(row);
        if is_visible {
            self.visible.push(index);
        }
    }

    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(super) fn toggle_selected(&mut self, row_ix: usize) {
        if let Some(row) = self.rows.get_mut(row_ix)
            && row.matched_rule.is_some()
            && row.error.is_none()
        {
            row.selected = !row.selected;
        }
    }

    pub(super) fn set_status(&mut self, row_ix: usize, status: RowStatus) {
        if let Some(row) = self.rows.get_mut(row_ix) {
            row.status = status;
        }
    }

    /// Indices of rows the user wants applied: selected, matched, no read error,
    /// not already applied.
    pub(super) fn pending_apply_indices(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.selected
                    && r.matched_entry.is_some()
                    && r.error.is_none()
                    && r.status != RowStatus::Applied
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub(super) fn select_all_eligible(&mut self) {
        for row in &mut self.rows {
            if row.matched_rule.is_some() && row.error.is_none() {
                row.selected = true;
            }
        }
    }

    pub(super) fn deselect_all(&mut self) {
        for row in &mut self.rows {
            row.selected = false;
        }
    }

    /// What the header checkbox should show, based on the rows that can
    /// actually be applied.
    pub(super) fn header_selection_state(&self) -> HeaderSelection {
        let mut eligible = 0;
        let mut selected = 0;
        for row in &self.rows {
            if row.matched_rule.is_some() && row.error.is_none() {
                eligible += 1;
                if row.selected {
                    selected += 1;
                }
            }
        }

        if eligible == 0 {
            HeaderSelection::NothingEligible
        } else if selected == eligible {
            HeaderSelection::All
        } else {
            HeaderSelection::NotAll
        }
    }
}

impl TableDelegate for FilesDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.visible.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let column = self.columns[col_ix].clone();
        if self.is_sortable(column.key.as_ref()) {
            column.sortable()
        } else {
            column
        }
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        if col_ix == 0 {
            let state = self.header_selection_state();
            let (checked, disabled) = match state {
                HeaderSelection::NothingEligible => (false, true),
                HeaderSelection::NotAll => (false, false),
                HeaderSelection::All => (true, false),
            };

            div()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Checkbox::new("select-all")
                        .checked(checked)
                        .disabled(disabled)
                        .on_click(cx.listener(move |state, _, _, cx| {
                            let delegate = state.delegate_mut();
                            if delegate.header_selection_state() == HeaderSelection::All {
                                delegate.deselect_all();
                            } else {
                                delegate.select_all_eligible();
                            }
                            cx.notify();
                        })),
                )
                .into_any_element()
        } else {
            // Default header rendering for other columns: just the column title.
            div()
                .text_sm()
                .font_semibold()
                .child(self.columns[col_ix].name.clone())
                .into_any_element()
        }
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let abs_ix = self.visible[row_ix];
        let row = &self.rows[abs_ix];
        match col_ix {
            0 => render_selected_cell(row, abs_ix, cx).into_any_element(),
            1 => render_path_cell(row, cx).into_any_element(),
            2 => render_lens_cell(row, cx).into_any_element(),
            3 => render_rule_cell(row, cx).into_any_element(),
            4 => render_status_cell(row, cx).into_any_element(),
            _ => div().into_any_element(),
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) {
        fn sort_rows(
            rows: &mut [FileRow],
            sort: ColumnSort,
            compare: impl Fn(&FileRow, &FileRow) -> Ordering,
        ) {
            match sort {
                ColumnSort::Descending => rows.sort_by(|a, b| compare(b, a)),
                _ => rows.sort_by(|a, b| compare(a, b)),
            }
        }

        match self.columns[col_ix].key.as_ref() {
            "path" => sort_rows(&mut self.rows, sort, |a, b| a.rel_path.cmp(&b.rel_path)),
            "lens" => sort_rows(&mut self.rows, sort, |a, b| {
                a.current_lens.cmp(&b.current_lens)
            }),
            "rule" => sort_rows(&mut self.rows, sort, |a, b| {
                a.matched_rule.cmp(&b.matched_rule)
            }),
            _ => {}
        }

        self.rebuild_visible();
    }

    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        let Some(parent) = self.parent.as_ref().and_then(|p| p.upgrade()) else {
            return menu;
        };
        let abs_ix = self.visible[row_ix];
        let row = &self.rows[abs_ix];
        let abs_path = row.abs_path.clone();
        let matched_rule = row.matched_rule.clone();
        let has_error = row.error.is_some();

        let parent_for_reveal = parent.clone();
        let reveal_path = abs_path.clone();
        let mut menu = menu.item(
            PopupMenuItem::new("Reveal in Finder")
                .icon(IconName::ExternalLink)
                .on_click(
                    window.listener_for(&parent_for_reveal, move |this, _, _w, cx| {
                        this.reveal_file(&reveal_path, cx);
                    }),
                ),
        );

        if !has_error {
            menu = menu.separator();

            match matched_rule {
                Some(rule_name) => {
                    let parent_for_edit = parent.clone();
                    menu = menu.item(PopupMenuItem::new("Edit matching rule…").on_click(
                        window.listener_for(&parent_for_edit, move |this, _, _w, cx| {
                            this.edit_matching_rule(rule_name.clone(), cx);
                        }),
                    ));
                }
                None => {
                    let parent_for_create = parent.clone();
                    menu = menu.item(
                        PopupMenuItem::new("Create rule from this file…")
                            .icon(IconName::Plus)
                            .on_click(window.listener_for(
                                &parent_for_create,
                                move |this, _, _, cx| {
                                    this.create_rule_from_file(&abs_path, cx);
                                },
                            )),
                    );
                }
            }
        }

        menu
    }
}

fn render_selected_cell(
    row: &FileRow,
    abs_ix: usize,
    cx: &mut Context<TableState<FilesDelegate>>,
) -> impl IntoElement {
    let can_apply = row.matched_rule.is_some() && row.error.is_none();
    div().flex().items_center().justify_center().child(
        Checkbox::new(SharedString::from(format!("sel-{abs_ix}")))
            .checked(row.selected)
            .disabled(!can_apply)
            .on_click(cx.listener(move |state, _, _, cx| {
                state.delegate_mut().toggle_selected(abs_ix);
                cx.notify();
            })),
    )
}

fn render_rule_cell(row: &FileRow, cx: &mut App) -> impl IntoElement {
    div()
        .text_sm()
        .text_color(match row.matched_rule {
            Some(_) => cx.theme().success,
            None => cx.theme().muted_foreground,
        })
        .child(match &row.matched_rule {
            Some(name) => name.clone(),
            None => "—".to_string(),
        })
}

fn render_status_cell(row: &FileRow, cx: &mut App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let success = cx.theme().success;
    let danger = cx.theme().danger;
    match &row.status {
        RowStatus::Pending => div().into_any_element(),
        RowStatus::Applying => div()
            .h_flex()
            .gap_1()
            .items_center()
            .text_sm()
            .text_color(muted)
            .child("Applying…")
            .into_any_element(),
        RowStatus::Applied => div()
            .h_flex()
            .gap_1()
            .items_center()
            .text_sm()
            .text_color(success)
            .child(Icon::new(IconName::CircleCheck).small())
            .child("Applied")
            .into_any_element(),
        RowStatus::Failed(err) => {
            let err = err.clone();
            div()
                .id(SharedString::from(format!(
                    "status-err-{}",
                    row.rel_path.display()
                )))
                .h_flex()
                .gap_1()
                .items_center()
                .text_sm()
                .text_color(danger)
                .child(Icon::new(IconName::CircleX).small())
                .child("Failed")
                .tooltip(move |window, cx| Tooltip::new(err.clone()).build(window, cx))
                .into_any_element()
        }
    }
}

fn render_path_cell(row: &FileRow, cx: &mut App) -> impl IntoElement {
    let danger = cx.theme().danger;
    let path_str = row.rel_path.display().to_string();
    div()
        .h_flex()
        .gap_2()
        .items_center()
        .when_some(row.error.clone(), |d, err| {
            d.child(
                div()
                    .id("error-icon")
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .text_color(danger)
                            .small(),
                    )
                    .tooltip(move |window, cx| Tooltip::new(err.clone()).build(window, cx)),
            )
        })
        .child(div().text_sm().child(path_str))
}

fn render_lens_cell(row: &FileRow, cx: &mut App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let is_empty = row.current_lens.is_empty();
    let display = if is_empty {
        "—"
    } else {
        row.current_lens.as_str()
    };
    div()
        .text_sm()
        .when(is_empty, |d| d.text_color(muted))
        .child(display.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn row(matched: bool) -> FileRow {
        FileRow {
            rel_path: PathBuf::from("a.dng"),
            abs_path: PathBuf::from("/a.dng"),
            current_lens: String::new(),
            matched_rule: matched.then(|| "rule".to_string()),
            matched_entry: matched.then(LensMapEntry::default),
            error: None,
            selected: false,
            status: RowStatus::Pending,
        }
    }

    /// A row that matched a rule but could not be read.
    fn errored_row() -> FileRow {
        FileRow {
            error: Some("unreadable".to_string()),
            ..row(true)
        }
    }

    #[test]
    fn visible_includes_all_when_filter_off() {
        let mut d = FilesDelegate::new();
        d.push(row(true));
        d.push(row(false));
        d.push(row(true));
        assert_eq!(d.visible, vec![0, 1, 2]);
    }

    #[test]
    fn visible_excludes_unmatched_when_filter_on() {
        let mut d = FilesDelegate::new();
        d.push(row(true));
        d.push(row(false));
        d.push(row(true));
        d.set_matches_only(true);
        assert_eq!(d.visible, vec![0, 2]);
    }

    #[test]
    fn visible_rebuilds_when_filter_toggled() {
        let mut d = FilesDelegate::new();
        d.push(row(true));
        d.push(row(false));
        d.set_matches_only(true);
        d.set_matches_only(false);
        assert_eq!(d.visible, vec![0, 1]);
    }

    #[test]
    fn visible_rebuilds_after_clear() {
        let mut d = FilesDelegate::new();
        d.push(row(true));
        d.push(row(false));
        d.set_matches_only(true);
        d.clear();
        assert!(d.visible.is_empty());
    }

    #[test]
    fn toggle_selected_takes_an_absolute_row_index() {
        let mut d = FilesDelegate::new();
        d.push(row(true));
        d.push(row(false));
        d.push(row(true));
        d.set_matches_only(true);

        // Visible position 1 is absolute row 2, and that is the row that has
        // to flip -- not row 1, which the filter is hiding.
        d.toggle_selected(d.visible[1]);

        assert!(d.rows[2].selected);
        assert!(!d.rows[0].selected);
    }

    #[test]
    fn pending_apply_lists_only_selected_matched_rows() {
        let mut d = FilesDelegate::new();
        d.push(row(true));
        d.push(row(false));
        d.push(row(true));
        d.toggle_selected(0);
        assert_eq!(d.pending_apply_indices(), vec![0]);
    }

    #[test]
    fn pending_apply_skips_rows_already_applied() {
        let mut d = FilesDelegate::new();
        d.push(row(true));
        d.push(row(true));
        d.select_all_eligible();
        d.set_status(0, RowStatus::Applied);
        assert_eq!(d.pending_apply_indices(), vec![1]);
    }

    #[test]
    fn select_all_skips_ineligible_rows() {
        let mut d = FilesDelegate::new();
        d.push(row(true));
        d.push(row(false));
        d.push(errored_row());
        d.select_all_eligible();
        assert_eq!(d.pending_apply_indices(), vec![0]);
    }

    #[test]
    fn deselect_all_clears_every_row() {
        let mut d = FilesDelegate::new();
        d.push(row(true));
        d.push(row(true));
        d.select_all_eligible();
        d.deselect_all();
        assert!(d.pending_apply_indices().is_empty());
    }

    #[test]
    fn header_is_disabled_when_no_row_can_be_applied() {
        let mut d = FilesDelegate::new();
        d.push(row(false));
        d.push(errored_row());
        assert_eq!(d.header_selection_state(), HeaderSelection::NothingEligible);
    }

    #[test]
    fn header_reports_all_only_once_every_eligible_row_is_selected() {
        let mut d = FilesDelegate::new();
        d.push(row(true));
        d.push(row(true));
        assert_eq!(d.header_selection_state(), HeaderSelection::NotAll);
        d.toggle_selected(0);
        assert_eq!(d.header_selection_state(), HeaderSelection::NotAll);
        d.toggle_selected(1);
        assert_eq!(d.header_selection_state(), HeaderSelection::All);
    }

    #[test]
    fn header_ignores_unmatched_rows_when_deciding_all() {
        let mut d = FilesDelegate::new();
        d.push(row(true));
        d.push(row(false));
        d.toggle_selected(0);
        assert_eq!(d.header_selection_state(), HeaderSelection::All);
    }

    #[test]
    fn only_the_data_columns_are_sortable() {
        let d = FilesDelegate::new();
        assert!(d.is_sortable("path"));
        assert!(d.is_sortable("lens"));
        assert!(d.is_sortable("rule"));
        assert!(!d.is_sortable("selected"));
        assert!(!d.is_sortable("status"));
    }

    #[test]
    fn nothing_is_sortable_while_a_task_is_running() {
        let mut d = FilesDelegate::new();
        d.set_busy(true);
        assert!(!d.is_sortable("path"));

        d.set_busy(false);
        assert!(d.is_sortable("path"));
    }
}

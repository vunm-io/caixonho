//! What one location holds, as a table.
//!
//! Folders first, then objects — the order a file explorer has used for forty
//! years, and the one that puts what can be entered above what cannot.
//!
//! The columns that describe an object stay **empty** for a folder. That is
//! the honest rendering rather than a gap to fill: S3 stores no directories,
//! so a folder has no size, no modification time and no storage class, and
//! substituting a zero or a dash-shaped guess would state something the
//! service never said (`object-browsing`, "Folders are inferred, and the
//! inference is not disguised").

use std::collections::BTreeSet;

use caixonho_core::{Folder, Object, Prefix};
use gpui::{
    App, Context, IntoElement, ParentElement, SharedString, Styled, WeakEntity, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName,
    checkbox::Checkbox,
    menu::{PopupMenu, PopupMenuItem},
    table::{Column, TableDelegate, TableState},
};

use crate::app::CaixonhoApp;

use crate::theme::space;

/// One row: something you can enter, or something you cannot.
#[derive(Debug, Clone)]
pub(crate) enum Entry {
    Folder(Folder),
    Object(Object),
}

/// What names a row, wherever it currently sits (`XONHO-0030`).
///
/// A tick has to survive the listing being read again or extended, and rows
/// move when it is: [`ObjectsDelegate::extend`] **inserts** a further page's
/// folders above the objects, so every object below them shifts down. A
/// selection held as row indices would quietly slide onto different objects,
/// which for a delete means removing something nobody ticked.
///
/// Folder and object are separate arms rather than one string because a
/// zero-length object whose key ends in a separator is how several tools write
/// a folder — so the two *can* collide, and the arm is what keeps "the folder
/// `photos/`" and "the marker object `photos/`" from being the same tick.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RowId {
    Folder(String),
    Object(String),
}

impl Entry {
    /// What names this row, independently of where it sits.
    pub(crate) fn id(&self) -> RowId {
        match self {
            Self::Folder(folder) => RowId::Folder(folder.prefix.as_str().to_owned()),
            Self::Object(object) => RowId::Object(object.key.clone()),
        }
    }

    /// What to call it at the location being shown.
    fn name(&self, at: &Prefix) -> SharedString {
        match self {
            Self::Folder(folder) => folder.name().to_owned().into(),
            Self::Object(object) => object.name_within(at).to_owned().into(),
        }
    }

    /// Whether entering it is a thing that can be done.
    pub(crate) fn is_folder(&self) -> bool {
        matches!(self, Self::Folder(_))
    }

    /// The prefix this row leads to, when it leads anywhere.
    pub(crate) fn into_prefix(self) -> Option<Prefix> {
        match self {
            Self::Folder(folder) => Some(folder.prefix),
            Self::Object(_) => None,
        }
    }
}

/// A size a person can read.
///
/// Binary units, because an object store reports bytes and a file manager is
/// where people compare this number to what their disk says. Exact bytes below
/// a kibibyte: rounding "12" to "0.0 KiB" loses the only interesting thing
/// about a very small object, which is that it is very small.
pub(crate) fn readable(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut size = bytes as f64 / 1024.0;
    let mut unit = 0;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.1} {}", UNITS[unit])
}

/// The table's own state for a location's contents.
pub(crate) struct ObjectsDelegate {
    columns: Vec<Column>,
    /// Folders first, then objects, in the order the service reported each.
    pub(crate) rows: Vec<Entry>,
    /// The location these rows belong to, so a name can be read from a key.
    pub(crate) at: Prefix,
    /// Which rows are ticked, **by identity** rather than by position. See
    /// [`RowId`] for why that distinction is the requirement and not a
    /// detail.
    chosen: BTreeSet<RowId>,
    /// The window, so a row's own menu can act.
    ///
    /// Weak, and the direction matters: the window owns this table, so a
    /// strong handle here would be a cycle. `None` until the window sets it,
    /// and a menu built before that simply has no items — which is the
    /// honest failure, rather than a menu of verbs that do nothing.
    window: Option<WeakEntity<CaixonhoApp>>,
}

impl ObjectsDelegate {
    pub(crate) fn new() -> Self {
        Self {
            columns: vec![
                // The ticks. Its own column rather than something that
                // appears on hover: the table component offers one selected
                // row and reports no modifier keys with a click, so there is
                // no cmd-click to inherit — and a tick that is only there
                // while the pointer is over it is a control nobody finds.
                Column::new("chosen", "").width(px(44.)),
                Column::new("name", "Name").width(px(430.)),
                Column::new("size", "Size").width(px(140.)),
                Column::new("modified", "Last modified").width(px(240.)),
            ],
            rows: Vec::new(),
            at: Prefix::root(),
            chosen: BTreeSet::new(),
            window: None,
        }
    }

    /// Replace what is shown.
    ///
    /// Clears the ticks. This is a fresh reading of the place — entering it,
    /// or re-reading after something changed it — and carrying ticks across
    /// one would leave rows selected that the reading may no longer contain.
    /// [`Self::extend`] is the other half and deliberately does not clear.
    pub(crate) fn show(&mut self, at: Prefix, folders: Vec<Folder>, objects: Vec<Object>) {
        self.chosen.clear();
        self.at = at;
        self.rows = folders
            .into_iter()
            .map(Entry::Folder)
            .chain(objects.into_iter().map(Entry::Object))
            .collect();
    }

    /// Add what a further page brought, keeping folders above objects.
    ///
    /// Keeps the ticks, and this is where holding identities earns itself: the
    /// folders below are **inserted** among the existing rows, so every object
    /// after them changes index. A tick recorded as a position would now be on
    /// a different object, silently.
    pub(crate) fn extend(&mut self, folders: Vec<Folder>, objects: Vec<Object>) {
        let first_object = self
            .rows
            .iter()
            .position(Entry::is_folder)
            .map(|_| self.rows.iter().filter(|row| row.is_folder()).count())
            .unwrap_or(0);
        for (offset, folder) in folders.into_iter().enumerate() {
            self.rows
                .insert(first_object + offset, Entry::Folder(folder));
        }
        self.rows.extend(objects.into_iter().map(Entry::Object));
    }

    /// The row at `index`, if there is one.
    pub(crate) fn row(&self, index: usize) -> Option<&Entry> {
        self.rows.get(index)
    }

    /// Whether the row at `index` is ticked.
    pub(crate) fn is_chosen(&self, index: usize) -> bool {
        self.row(index)
            .is_some_and(|row| self.chosen.contains(&row.id()))
    }

    /// Tick or untick the row at `index`.
    pub(crate) fn toggle(&mut self, index: usize) {
        let Some(id) = self.row(index).map(Entry::id) else {
            return;
        };
        if !self.chosen.remove(&id) {
            self.chosen.insert(id);
        }
    }

    /// Tick every row, or none of them.
    pub(crate) fn tick_every(&mut self, ticked: bool) {
        self.chosen = if ticked {
            self.rows.iter().map(Entry::id).collect()
        } else {
            BTreeSet::new()
        };
    }

    /// How many rows are ticked.
    pub(crate) fn chosen_count(&self) -> usize {
        self.chosen.len()
    }

    /// The ticked rows, in the order they are shown.
    ///
    /// Read back off `rows` rather than out of the set, so what comes back is
    /// what is on screen: a tick for a row this listing no longer holds is
    /// not something the user can see, and must not be something they delete.
    pub(crate) fn chosen_rows(&self) -> Vec<Entry> {
        self.rows
            .iter()
            .filter(|row| self.chosen.contains(&row.id()))
            .cloned()
            .collect()
    }

    /// Tell this table which window its rows can act on.
    pub(crate) fn reachable_from(&mut self, window: WeakEntity<CaixonhoApp>) {
        self.window = Some(window);
    }
}

impl TableDelegate for ObjectsDelegate {
    /// The tick column's header ticks everything, or nothing.
    ///
    /// Reachable without it — tick each row — but a folder of thirty objects
    /// is where a bulk delete is actually wanted, and thirty clicks to get
    /// there is a feature that exists on paper. The counted confirmation is
    /// what makes it safe to offer, and it is the same confirmation whether
    /// the ticks arrived one at a time or all at once.
    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        if col_ix != 0 {
            return div()
                .size_full()
                .child(self.columns[col_ix].name.clone())
                .into_any_element();
        }
        let all = !self.rows.is_empty() && self.chosen.len() == self.rows.len();
        div()
            .flex()
            .items_center()
            .justify_center()
            .child(
                Checkbox::new("chosen-every-row")
                    .checked(all)
                    .on_click(cx.listener(move |state, _, _, cx| {
                        state.delegate_mut().tick_every(!all);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// What a right-click on a row offers (`XONHO-0030`).
    ///
    /// The row under the pointer, always — not the ticked set. Right-clicking
    /// a row nobody ticked and having it delete three other rows would be the
    /// worst kind of surprise, and the ticks have their own button.
    ///
    /// Delete is here and **nowhere a stray click reaches**: no hover control
    /// and no double-click binding. The owner's decision of 2026-08-24 governs
    /// — a stray double-click must not be enough to write company bytes to
    /// disk — and a delete icon sitting under the pointer is easier to hit
    /// than a double-click is. The confirmation that follows is a second step,
    /// not an excuse for a first one that is too easy.
    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        let (Some(row), Some(window)) = (self.rows.get(row_ix), self.window.clone()) else {
            return menu;
        };
        let is_folder = row.is_folder();

        // One shape per item: name it, and say what the window should do.
        let act =
            |label: &str,
             act: fn(&mut CaixonhoApp, usize, &mut Window, &mut Context<CaixonhoApp>)| {
                let window = window.clone();
                PopupMenuItem::new(label.to_owned()).on_click(move |_, window_handle, cx| {
                    let _ = window.update(cx, |app, cx| {
                        act(app, row_ix, window_handle, cx);
                    });
                })
            };

        // A folder has no bytes of its own, so the three verbs that read
        // bytes are simply absent rather than present and disabled: there is
        // nothing to explain, and a greyed row invites a second click.
        let menu = if is_folder {
            menu.item(act("Open folder", |app, index, window, cx| {
                app.enter(index, window, cx)
            }))
        } else {
            menu.item(act("Preview", |app, index, _, cx| {
                app.preview_row(index, cx)
            }))
            .item(act("Open", |app, index, _, cx| app.open_row(index, cx)))
            .item(act("Download…", |app, index, window, cx| {
                app.download_row(index, window, cx)
            }))
        };

        menu.separator().item(act("Delete…", |app, index, _, cx| {
            app.delete_row(index, cx)
        }))
    }

    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(row) = self.rows.get(row_ix) else {
            return div().into_any_element();
        };

        if col_ix == 0 {
            let ticked = self.is_chosen(row_ix);
            return div()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Checkbox::new(("chosen-row", row_ix))
                        .checked(ticked)
                        .on_click(cx.listener(move |state, _, _, cx| {
                            state.delegate_mut().toggle(row_ix);
                            cx.notify();
                        })),
                )
                .into_any_element();
        }

        if col_ix == 1 {
            let icon = if row.is_folder() {
                IconName::Folder
            } else {
                IconName::File
            };
            return div()
                .flex()
                .items_center()
                .gap(space::TIGHT)
                .child(
                    div()
                        .text_color(if row.is_folder() {
                            cx.theme().primary
                        } else {
                            cx.theme().muted_foreground
                        })
                        .child(Icon::new(icon).size_4()),
                )
                .child(row.name(&self.at))
                .into_any_element();
        }

        // Size and modification time belong to objects. A folder leaves them
        // empty because the service said nothing about them, and an em dash
        // here would be a value where there is none.
        let text: SharedString = match row {
            Entry::Folder(_) => "".into(),
            Entry::Object(object) => match col_ix {
                2 => readable(object.size).into(),
                3 => object
                    .last_modified
                    .as_deref()
                    .map(crate::views::format::timestamp)
                    .unwrap_or_else(|| "—".to_owned())
                    .into(),
                _ => "".into(),
            },
        };

        div().child(text).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_named_by_its_last_step_and_an_object_by_its_key_here() {
        let at = Prefix::parse("photos/");
        let folder = Entry::Folder(Folder {
            prefix: Prefix::parse("photos/vacation/"),
        });
        let object = Entry::Object(Object {
            key: "photos/cat.jpg".to_owned(),
            size: 1,
            last_modified: None,
            storage_class: None,
            etag: None,
        });

        assert_eq!(folder.name(&at), "vacation");
        assert_eq!(object.name(&at), "cat.jpg");
    }

    #[test]
    fn only_a_folder_leads_anywhere() {
        let folder = Entry::Folder(Folder {
            prefix: Prefix::parse("photos/vacation/"),
        });
        let object = Entry::Object(Object {
            key: "photos/cat.jpg".to_owned(),
            size: 1,
            last_modified: None,
            storage_class: None,
            etag: None,
        });

        assert!(folder.is_folder());
        assert!(!object.is_folder());
        assert_eq!(
            folder.into_prefix().map(|p| p.as_str().to_owned()),
            Some("photos/vacation/".to_owned())
        );
        assert_eq!(object.into_prefix(), None);
    }

    /// A folder row for `name` directly under the root.
    fn folder_row(name: &str) -> Folder {
        Folder {
            prefix: Prefix::parse(name),
        }
    }

    /// An object row at `key`.
    fn object_row(key: &str) -> Object {
        Object {
            key: key.to_owned(),
            size: 1,
            last_modified: None,
            storage_class: None,
            etag: None,
        }
    }

    #[test]
    fn several_rows_can_be_ticked_and_read_back() {
        let mut delegate = ObjectsDelegate::new();
        delegate.show(
            Prefix::root(),
            vec![folder_row("logs/")],
            vec![object_row("one.txt"), object_row("two.txt")],
        );

        delegate.toggle(0);
        delegate.toggle(2);

        assert_eq!(delegate.chosen_count(), 2);
        assert!(delegate.is_chosen(0), "the folder");
        assert!(!delegate.is_chosen(1));
        assert!(delegate.is_chosen(2), "the second object");
        assert_eq!(
            delegate
                .chosen_rows()
                .iter()
                .map(|row| row.name(&Prefix::root()).to_string())
                .collect::<Vec<_>>(),
            ["logs", "two.txt"],
            "read back in the order they are shown"
        );
    }

    #[test]
    fn ticking_the_same_row_twice_unticks_it() {
        let mut delegate = ObjectsDelegate::new();
        delegate.show(Prefix::root(), Vec::new(), vec![object_row("one.txt")]);

        delegate.toggle(0);
        delegate.toggle(0);

        assert_eq!(delegate.chosen_count(), 0);
    }

    #[test]
    fn a_further_page_does_not_move_the_ticks_onto_other_rows() {
        // The requirement, and the reason the set holds identities rather
        // than positions. `extend` **inserts** the new page's folders above
        // the objects, so `two.txt` moves from index 1 to index 3 — and a
        // tick recorded as "row 1" would now be on a folder nobody chose.
        let mut delegate = ObjectsDelegate::new();
        delegate.show(
            Prefix::root(),
            vec![folder_row("a/")],
            vec![object_row("one.txt"), object_row("two.txt")],
        );
        delegate.toggle(2); // two.txt

        delegate.extend(
            vec![folder_row("b/"), folder_row("c/")],
            vec![object_row("three.txt")],
        );

        assert_eq!(
            delegate
                .chosen_rows()
                .iter()
                .map(|row| row.name(&Prefix::root()).to_string())
                .collect::<Vec<_>>(),
            ["two.txt"],
            "the same object, wherever the further page pushed it"
        );
        assert_eq!(delegate.chosen_count(), 1, "and nothing else came with it");
    }

    #[test]
    fn a_fresh_reading_of_the_place_forgets_what_was_ticked() {
        // The other half of the rule. `show` is entering a location or
        // re-reading one, and a tick carried across would name a row this
        // reading may no longer hold.
        let mut delegate = ObjectsDelegate::new();
        delegate.show(Prefix::root(), Vec::new(), vec![object_row("one.txt")]);
        delegate.toggle(0);

        delegate.show(Prefix::root(), Vec::new(), vec![object_row("one.txt")]);

        assert_eq!(delegate.chosen_count(), 0);
    }

    #[test]
    fn a_folder_and_a_marker_object_of_the_same_name_are_two_different_ticks() {
        // Several tools write a folder as a zero-length object whose key ends
        // in the separator, so both can be listed at once. One string for
        // both would make ticking one tick the other.
        let mut delegate = ObjectsDelegate::new();
        delegate.show(
            Prefix::root(),
            vec![folder_row("photos/")],
            vec![object_row("photos/")],
        );

        delegate.toggle(0);

        assert!(delegate.is_chosen(0), "the folder");
        assert!(!delegate.is_chosen(1), "and not the marker object");
    }

    #[test]
    fn everything_can_be_ticked_and_unticked_at_once() {
        let mut delegate = ObjectsDelegate::new();
        delegate.show(
            Prefix::root(),
            vec![folder_row("a/")],
            vec![object_row("one.txt")],
        );

        delegate.tick_every(true);
        assert_eq!(delegate.chosen_count(), 2);

        delegate.tick_every(false);
        assert_eq!(delegate.chosen_count(), 0);
    }

    #[test]
    fn a_size_is_written_the_way_a_file_manager_writes_it() {
        // Exact bytes below a kibibyte: rounding 12 to "0.0 KiB" loses the
        // only interesting thing about a very small object.
        assert_eq!(readable(0), "0 B");
        assert_eq!(readable(12), "12 B");
        assert_eq!(readable(1023), "1023 B");
        assert_eq!(readable(1024), "1.0 KiB");
        assert_eq!(readable(3 * 1024 * 1024), "3.0 MiB");
    }

    #[test]
    fn a_further_page_keeps_folders_above_objects() {
        // The service returns both in each page, so appending naively would
        // interleave them and the list would stop reading as a hierarchy.
        let mut delegate = ObjectsDelegate::new();
        delegate.show(
            Prefix::root(),
            vec![Folder {
                prefix: Prefix::parse("a/"),
            }],
            vec![Object {
                key: "one.txt".to_owned(),
                size: 1,
                last_modified: None,
                storage_class: None,
                etag: None,
            }],
        );

        delegate.extend(
            vec![Folder {
                prefix: Prefix::parse("b/"),
            }],
            vec![Object {
                key: "two.txt".to_owned(),
                size: 1,
                last_modified: None,
                storage_class: None,
                etag: None,
            }],
        );

        let shape: Vec<bool> = delegate.rows.iter().map(Entry::is_folder).collect();
        assert_eq!(shape, [true, true, false, false]);
    }
}

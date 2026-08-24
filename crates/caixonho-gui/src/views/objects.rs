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

use caixonho_core::{Folder, Object, Prefix};
use gpui::{App, Context, IntoElement, ParentElement, SharedString, Styled, Window, div, px};
use gpui_component::{
    ActiveTheme, Icon, IconName,
    table::{Column, TableDelegate, TableState},
};

use crate::theme::space;

/// One row: something you can enter, or something you cannot.
#[derive(Debug, Clone)]
pub(crate) enum Entry {
    Folder(Folder),
    Object(Object),
}

impl Entry {
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
}

impl ObjectsDelegate {
    pub(crate) fn new() -> Self {
        Self {
            columns: vec![
                Column::new("name", "Name").width(px(460.)),
                Column::new("size", "Size").width(px(140.)),
                Column::new("modified", "Last modified").width(px(240.)),
            ],
            rows: Vec::new(),
            at: Prefix::root(),
        }
    }

    /// Replace what is shown.
    pub(crate) fn show(&mut self, at: Prefix, folders: Vec<Folder>, objects: Vec<Object>) {
        self.at = at;
        self.rows = folders
            .into_iter()
            .map(Entry::Folder)
            .chain(objects.into_iter().map(Entry::Object))
            .collect();
    }

    /// Add what a further page brought, keeping folders above objects.
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
}

impl TableDelegate for ObjectsDelegate {
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
                1 => readable(object.size).into(),
                2 => object
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

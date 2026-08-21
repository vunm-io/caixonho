use std::ops::Range;

use caixonho_core::{
    Bucket, BucketKind, LIST_BUCKET_ACTION, Observation, ProbeTarget, Region, RegionChoice, Scope,
    Session,
};
use gpui::{
    AnyElement, App, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, IconName, h_flex,
    select::{SearchableVec, SelectState},
    skeleton::Skeleton,
    table::{Column, TableDelegate, TableState},
    tooltip::Tooltip,
};

use crate::components::status_badge;
use crate::theme::space;
use crate::views::format::split_zonal_name;

/// Displayed instead of a region the service never stated. A first-class
/// value, not a placeholder: the alternative is showing the connection's own
/// region, which would be a guess that reads as fact.
const UNKNOWN_REGION: &str = "unknown";

/// What is known about entering one bucket, as the table shows it.
///
/// Four states, not three: a row with a probe open is saying something
/// different from a row nobody has asked about yet, and collapsing them makes
/// rows flicker between "unknown" and "denied" as answers land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    /// A probe is open for this bucket.
    Probing,
    /// Observed: its contents can be listed.
    Open,
    /// Observed: listing its contents was refused on authorization grounds.
    /// Only a real denial reaches this — never a guess, and never an expired
    /// session or an unreachable network.
    Denied,
    /// Nothing has been observed yet.
    Unobserved,
}

/// The region selector's own state, kept as `SharedString` because the control
/// selects labels; the choice each one stands for is looked up alongside.
pub(crate) type RegionSelect = SelectState<SearchableVec<SharedString>>;

/// How a choice reads in the selector.
///
/// Deliberately not the bare region: "all regions" and the unstated group need
/// wording of their own, and an empty label would offer a selection nobody can
/// interpret.
pub(crate) fn region_label(choice: &RegionChoice) -> SharedString {
    match choice {
        RegionChoice::All => "All regions".into(),
        RegionChoice::In(region) => region.clone().into(),
        RegionChoice::Unstated => "Region unstated".into(),
    }
}

/// The bucket table.
pub(crate) struct BucketsDelegate {
    columns: Vec<Column>,
    /// Every bucket the listing returned.
    pub(crate) rows: Vec<Bucket>,
    /// Indices into `rows` the current region choice admits, in listing order.
    /// Indices rather than a second copy: narrowing a large account should cost
    /// a filter, not a clone of every bucket in it.
    pub(crate) shown: Vec<usize>,
    /// Held so the rows on screen can be reported for probing, and so each row
    /// can read what has been observed about it. `None` until the session is
    /// built, which is after the table.
    pub(crate) session: Option<Session>,
}

impl BucketsDelegate {
    pub(crate) fn new() -> Self {
        Self {
            columns: vec![
                Column::new("name", "Bucket").width(px(420.)),
                Column::new("created", "Created").width(px(200.)),
                Column::new("region", "Region").width(px(180.)),
                Column::new("access", "Access").width(px(160.)),
            ],
            rows: Vec::new(),
            shown: Vec::new(),
            session: None,
        }
    }

    /// Every bucket the current region choice admits, in listing order, with
    /// what each one is.
    ///
    /// The kind travels with the name because the sidebar renders it, and the
    /// name is not evidence of it — that rule is the whole reason `Bucket`
    /// carries a kind at all.
    pub(crate) fn shown_names(&self) -> Vec<(String, BucketKind)> {
        self.shown
            .iter()
            .map(|index| (self.rows[*index].name.clone(), self.rows[*index].kind))
            .collect()
    }

    /// The one kind every shown bucket is, when they are all the same.
    ///
    /// `None` for a mixed list, and for an empty one. This is what decides
    /// whether a row needs marking at all: a badge repeated down every row of
    /// a list that is entirely one kind says nothing any single row did not
    /// already say, and the eye stops reading it — the same reason an
    /// enterable bucket gets no badge in the access column.
    pub(crate) fn shown_kind(&self) -> Option<BucketKind> {
        let mut kinds = self.shown.iter().map(|index| self.rows[*index].kind);
        let first = kinds.next()?;
        kinds.all(|kind| kind == first).then_some(first)
    }

    /// The bucket a shown row names.
    ///
    /// Through `shown` rather than `rows`: the row the user clicked is a row
    /// of the narrowed list, and reading `rows` directly would open whichever
    /// bucket happened to sit at that index before a region was chosen.
    pub(crate) fn name_at(&self, row: usize) -> Option<String> {
        self.shown
            .get(row)
            .map(|index| self.rows[*index].name.clone())
    }

    /// The probe targets for a range of shown rows.
    pub(crate) fn targets(&self, rows: Range<usize>) -> Vec<ProbeTarget> {
        self.shown
            .get(rows)
            .unwrap_or_default()
            .iter()
            .map(|index| ProbeTarget::from(&self.rows[*index]))
            .collect()
    }

    /// What has been observed about entering `bucket`.
    fn access(&self, bucket: &Bucket) -> Access {
        let Some(session) = &self.session else {
            return Access::Unobserved;
        };
        let scope = Scope::bucket(&bucket.name);
        if session.is_probing(&scope) {
            return Access::Probing;
        }
        let Some(credentials) = session.credentials() else {
            return Access::Unobserved;
        };
        match session.capability(&credentials, &scope).list {
            Observation::Allowed => Access::Open,
            Observation::Denied => Access::Denied,
            Observation::Unknown => Access::Unobserved,
        }
    }

    /// The access cell, which is the only one that explains itself.
    ///
    /// Silence is the good news: a bucket that can be entered gets no badge at
    /// all, because a mark on every row is noise and the eye stops reading it.
    /// Only an observed refusal is marked.
    fn render_access(
        &self,
        row_ix: usize,
        access: Access,
        cx: &mut Context<TableState<Self>>,
    ) -> AnyElement {
        match access {
            Access::Open => div().into_any_element(),
            // A probe in flight is its own state: without it a row would
            // flicker between "nothing known" and "refused" as answers land.
            Access::Probing => Skeleton::new().w(px(72.)).h(px(14.)).into_any_element(),
            Access::Unobserved => div()
                .text_color(cx.theme().muted_foreground)
                .child("—")
                .into_any_element(),
            Access::Denied => div()
                .id(("denied", row_ix))
                .child(status_badge(
                    IconName::CircleX,
                    "No access",
                    cx.theme().danger,
                ))
                .tooltip(|window, cx| {
                    Tooltip::new(format!(
                        "Listing this bucket's contents was denied. It needs \
                         {LIST_BUCKET_ACTION} on this bucket."
                    ))
                    .build(window, cx)
                })
                .into_any_element(),
        }
    }
}

/// The name cell: what the account holder chose, then what the service
/// requires, then what the bucket is.
///
/// A directory bucket's zone suffix is identical on every bucket in that zone,
/// so at full weight it is visual noise that also survives truncation while
/// the distinguishing half is cut. Quietened, the eye lands on the part that
/// differs — and the full name is still there, unaltered, which is what any
/// policy or console will show.
///
/// The badge, not the suffix, is what says this is a directory bucket: reading
/// a name for a suffix is exactly the work it exists to save. It appears only
/// when the list holds more than one kind — `marked` — because a mark on every
/// row of a list that is all one kind is noise, and the list says it once
/// above the table instead.
fn render_name(row: &Bucket, access: Access, marked: bool, cx: &mut App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let name = match (row.kind, split_zonal_name(&row.name)) {
        (BucketKind::Directory, Some((chosen, zone))) => h_flex()
            .child(div().child(chosen.to_owned()))
            .child(div().text_color(muted).child(format!("--{zone}")))
            .into_any_element(),
        _ => div().child(row.name.clone()).into_any_element(),
    };

    let cell = h_flex()
        .gap(space::TIGHT)
        .items_center()
        .child(name)
        .children((marked && row.kind == BucketKind::Directory).then(|| {
            div()
                .debug_selector(|| "directory-badge".into())
                .child(status_badge(
                    IconName::LayoutDashboard,
                    "Directory",
                    cx.theme().primary,
                ))
        }));

    // Dimmed, not hidden: a bucket that cannot be entered stays in the list,
    // because it is still a fact about the account.
    if access == Access::Denied {
        cell.text_color(muted).into_any_element()
    } else {
        cell.into_any_element()
    }
}

impl TableDelegate for BucketsDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.shown.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        self.columns[col_ix].clone()
    }

    /// Report the rows on screen, so only those are probed.
    ///
    /// The table calls this when the range changes rather than every frame,
    /// which is the debounce: scrolling reports each range it passes through
    /// once, and the scheduler drops whatever the previous range had queued.
    fn visible_rows_changed(
        &mut self,
        visible_range: Range<usize>,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) {
        let Some(session) = &self.session else {
            return;
        };
        session.submit_viewport(&self.targets(visible_range));
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[self.shown[row_ix]];
        let access = self.access(row);

        if col_ix == 3 {
            return self.render_access(row_ix, access, cx);
        }

        if col_ix == 0 {
            let marked = self.shown_kind().is_none();
            return render_name(row, access, marked, cx);
        }

        let text: SharedString = match col_ix {
            0 => row.name.clone().into(),
            1 => row
                .created
                .as_deref()
                .map(crate::views::format::timestamp)
                .unwrap_or_else(|| "—".to_owned())
                .into(),
            2 => match &row.region {
                Region::Known(region) => region.clone().into(),
                Region::Unknown => UNKNOWN_REGION.into(),
            },
            _ => "".into(),
        };

        let cell = div().child(text);
        // Dimmed, not hidden: a bucket that cannot be entered stays in the
        // list, because it is still a fact about the account.
        if access == Access::Denied {
            cell.text_color(cx.theme().muted_foreground)
                .into_any_element()
        } else {
            cell.into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    //! `bucket-listing` spec — when a bucket has to be marked as its own kind,
    //! and when marking it says nothing.

    use super::*;

    fn delegate(kinds: &[BucketKind]) -> BucketsDelegate {
        let mut delegate = BucketsDelegate::new();
        delegate.rows = kinds
            .iter()
            .enumerate()
            .map(|(index, kind)| Bucket {
                name: format!("bucket-{index}"),
                created: None,
                region: Region::Unknown,
                kind: *kind,
            })
            .collect();
        delegate.shown = (0..kinds.len()).collect();
        delegate
    }

    #[test]
    fn a_list_that_is_all_one_kind_needs_no_mark_on_any_row() {
        let all_directory = delegate(&[BucketKind::Directory; 3]);

        assert_eq!(all_directory.shown_kind(), Some(BucketKind::Directory));
    }

    #[test]
    fn a_mixed_list_has_no_single_kind_so_rows_must_be_marked() {
        let mixed = delegate(&[BucketKind::General, BucketKind::Directory]);

        assert_eq!(
            mixed.shown_kind(),
            None,
            "with both kinds present, a row that is not marked is ambiguous"
        );
    }

    #[test]
    fn an_empty_list_claims_no_kind() {
        assert_eq!(delegate(&[]).shown_kind(), None);
    }

    #[test]
    fn narrowing_to_one_kind_is_what_decides_it_not_the_whole_account() {
        // `shown`, not `rows`: a region choice that leaves only directory
        // buckets on screen is a list of one kind, whatever else the account
        // holds.
        let mut narrowed = delegate(&[BucketKind::General, BucketKind::Directory]);
        narrowed.shown = vec![1];

        assert_eq!(narrowed.shown_kind(), Some(BucketKind::Directory));
    }
}

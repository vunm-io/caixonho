use std::ops::Range;

use caixonho_core::{
    Bucket, LIST_BUCKET_ACTION, Observation, ProbeTarget, Region, RegionChoice, Scope, Session,
};
use gpui::{
    AnyElement, App, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, IconName,
    select::{SearchableVec, SelectState},
    skeleton::Skeleton,
    table::{Column, TableDelegate, TableState},
    tooltip::Tooltip,
};

use crate::components::status_badge;

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

    /// Every bucket the current region choice admits, in listing order.
    pub(crate) fn shown_names(&self) -> Vec<String> {
        self.shown
            .iter()
            .map(|index| self.rows[*index].name.clone())
            .collect()
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

        let text: SharedString = match col_ix {
            0 => row.name.clone().into(),
            1 => row.created.clone().unwrap_or_else(|| "—".into()).into(),
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

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

/// Which kinds of bucket the user wants to see.
///
/// In the window rather than in core, unlike [`RegionChoice`]: a region choice
/// has to be *derived* from a listing, because only the account knows which
/// regions it uses. There are two kinds of bucket and there always will be, so
/// nothing has to be discovered and core learns nothing by holding this.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum KindChoice {
    #[default]
    Any,
    Directory,
    General,
}

impl KindChoice {
    /// Whether this bucket survives the choice.
    fn matches(&self, bucket: &Bucket) -> bool {
        match self {
            Self::Any => true,
            Self::Directory => bucket.kind == BucketKind::Directory,
            Self::General => bucket.kind == BucketKind::General,
        }
    }

    /// How the choice reads in the selector.
    pub(crate) fn label(&self) -> SharedString {
        match self {
            Self::Any => "All kinds".into(),
            Self::Directory => "Directory buckets".into(),
            Self::General => "General purpose".into(),
        }
    }

    /// Every choice, in the order they are offered.
    pub(crate) fn all() -> [Self; 3] {
        [Self::Any, Self::Directory, Self::General]
    }
}

/// Everything the user has chosen to narrow the account listing by.
///
/// One value rather than four fields scattered over the window, because the
/// count has to be of the *final* set: four narrowings applied in four passes
/// with four counts is how the number comes to disagree with the rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Narrowing {
    pub(crate) region: RegionChoice,
    pub(crate) kind: KindChoice,
    /// Matched case-insensitively against the bucket's name. Empty means no
    /// narrowing at all, which is not the same as matching nothing.
    pub(crate) name: String,
    /// Leave out the buckets an authorization denial has been observed for.
    pub(crate) accessible_only: bool,
    /// The buckets this connection has chosen to show (`XONHO-0027`).
    ///
    /// `None` is *not* an empty choice: it means nobody has chosen, so
    /// everything is listed. An empty `Some` means the user chose nothing and
    /// meant it. Collapsing the two would make an empty choice silently show
    /// every bucket — the one outcome a person would read as the feature
    /// being broken.
    ///
    /// The odd one out among the five: the other four are *reset* when the
    /// connection changes, and this one is *loaded*.
    pub(crate) chosen: Option<Vec<String>>,
    /// Set aside for a moment, without being given up.
    ///
    /// "Show me everything" and "forget what I chose" are different acts, and
    /// the spec requires the choice to be *there to return to* after the
    /// first. A single `Option` cannot say that: setting it to `None` is
    /// indistinguishable from never having chosen, so the screen loses both
    /// the explanation and the way back.
    pub(crate) showing_all: bool,
}

impl Default for Narrowing {
    /// Written out rather than derived, because `RegionChoice` has no
    /// `Default` and giving core one for the window's convenience would be
    /// this change reaching into a crate it has no business changing.
    fn default() -> Self {
        Self {
            region: RegionChoice::All,
            kind: KindChoice::Any,
            name: String::new(),
            accessible_only: false,
            chosen: None,
            showing_all: false,
        }
    }
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

    /// What kind of bucket `name` is, according to the listing already held.
    ///
    /// By name and over `rows` rather than `shown`, because the question is
    /// asked from *inside* a bucket — where the account listing may since have
    /// been narrowed, and a narrowing must not change what a bucket is.
    pub(crate) fn kind_of(&self, name: &str) -> Option<BucketKind> {
        self.rows
            .iter()
            .find(|row| row.name == name)
            .map(|row| row.kind)
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

    /// Say that `bucket` is in `region` after all.
    ///
    /// By name rather than by index: the account listing can be replaced
    /// while a read is in flight, and an index taken before that would land
    /// on whatever row inherited it. A bucket this list no longer holds is
    /// simply not found, which is the right answer rather than a failure.
    ///
    /// The narrowing is deliberately *not* re-applied here. A correction can
    /// move a bucket out of the region choice currently on, and pulling the
    /// screen out from under someone as a reward for learning something true
    /// is worse than a filter that is briefly out of date — the next listing
    /// settles it.
    pub(crate) fn correct_region(&mut self, bucket: &str, region: &Region) {
        if let Some(row) = self.rows.iter_mut().find(|row| row.name == bucket) {
            row.region = region.clone();
        }
    }

    /// Apply every narrowing at once, in one pass.
    ///
    /// Here rather than in the window because one of the four predicates —
    /// accessibility — is an *observation*, and only the delegate can read it.
    /// Moving it up would mean handing the window the capability store.
    pub(crate) fn narrow(&mut self, narrowing: &Narrowing) {
        let name = narrowing.name.trim().to_lowercase();
        self.shown = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, bucket)| {
                narrowing.region.matches(bucket)
                    && narrowing.kind.matches(bucket)
                    && (name.is_empty() || bucket.name.to_lowercase().contains(&name))
                    && (!narrowing.accessible_only || self.access(bucket) != Access::Denied)
                    // A chosen name the account no longer lists simply is not
                    // here; nothing fails and the choice is left alone, because
                    // a bucket can be absent for a session and back the next.
                    && (narrowing.showing_all
                        || narrowing
                            .chosen
                            .as_ref()
                            .is_none_or(|chosen| chosen.contains(&bucket.name)))
            })
            .map(|(index, _)| index)
            .collect();
    }

    /// How many buckets are shown, of how many the listing returned.
    pub(crate) fn shown_of_loaded(&self) -> (usize, usize) {
        (self.shown.len(), self.rows.len())
    }

    /// Every bucket the account listed, by name — what a chooser offers.
    pub(crate) fn all_names(&self) -> Vec<String> {
        self.rows.iter().map(|bucket| bucket.name.clone()).collect()
    }

    /// Whether the account has buckets and the narrowing is hiding all of them.
    ///
    /// The distinction this exists for: an account that holds nothing and an
    /// account whose buckets are all narrowed away must not read the same, or
    /// the only cure for an empty screen is guessing which control emptied it.
    pub(crate) fn hidden_by_narrowing(&self) -> bool {
        self.shown.is_empty() && !self.rows.is_empty()
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
        // Named so a test can find the region a row reports — which is the
        // one cell in this table another part of the application corrects
        // after the fact.
        let cell = if col_ix == 2 {
            cell.debug_selector(|| "bucket-region".into())
        } else {
            cell
        };
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
    fn a_corrected_region_lands_on_the_named_row_and_leaves_the_others_alone() {
        let mut delegate = delegate(&[BucketKind::General; 3]);
        delegate.rows[0].region = Region::Known("us-east-1".to_owned());

        delegate.correct_region("bucket-1", &Region::Known("us-west-2".to_owned()));

        assert_eq!(
            delegate.rows[1].region,
            Region::Known("us-west-2".to_owned()),
            "the bucket that was read from elsewhere now says so"
        );
        assert_eq!(
            delegate.rows[0].region,
            Region::Known("us-east-1".to_owned()),
            "a row nobody read is not touched"
        );
        assert_eq!(
            delegate.rows[2].region,
            Region::Unknown,
            "and neither is a row that never stated one"
        );
    }

    #[test]
    fn correcting_a_bucket_this_list_does_not_hold_changes_nothing() {
        // The account listing can be replaced while a read is in flight, so
        // the page can arrive naming a bucket no row stands for. Searching
        // and finding nothing is the answer; the alternative is an index
        // into a list that has moved on.
        let mut delegate = delegate(&[BucketKind::General; 2]);

        delegate.correct_region("gone", &Region::Known("us-west-2".to_owned()));

        assert!(
            delegate
                .rows
                .iter()
                .all(|row| row.region == Region::Unknown),
            "no row is invented and none is altered"
        );
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

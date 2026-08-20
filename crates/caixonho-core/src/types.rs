//! Domain types crossing the core↔frontend boundary.
//!
//! Hard rule (crate invariant): no `aws-sdk-s3` type appears in any public
//! signature — frontends consume these types only, so the UI stays swappable
//! and the core reusable by the future CLI.

/// A connection profile discovered in the AWS shared config files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// The profile's name as written in the config file (`default`, or the
    /// name inside `[profile <name>]`).
    pub name: String,
    /// Whether this is the `default` profile.
    pub is_default: bool,
}

/// Identifies one opened connection.
///
/// Every request outcome is tagged with the id it belongs to, so a late
/// response from a previous profile is dropped instead of rendering as if it
/// belonged to the new one (design: messages, not shared state).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub u64);

/// A bucket's region, honest about not knowing.
///
/// The bucket listing does report regions, but only when the request carries
/// at least one valid parameter — which is why the adapter always sends a page
/// size. `Unknown` is not a placeholder waiting on a later slice: it is what a
/// bucket the service reported no region for stays, permanently and visibly,
/// because the alternative is a guessed default that reads as fact. The spec
/// makes "unknown" a first-class display value, distinct from every region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Region {
    /// The region is known, e.g. `ap-southeast-1`.
    Known(String),
    /// Not determined yet.
    Unknown,
}

/// One bucket as the domain sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bucket {
    /// The bucket name.
    pub name: String,
    /// Creation timestamp formatted as RFC 3339, when the service reported
    /// one. A `String` on purpose: display needs no date arithmetic, and a
    /// date dependency in core is not warranted by rendering alone.
    pub created: Option<String>,
    /// Where the bucket lives, when known.
    pub region: Region,
}

/// How the bucket list has been narrowed by region.
///
/// The choice is applied to buckets already retrieved: the service can filter a
/// listing by region, but only for a request sent to an endpoint in that same
/// region, which would cost a client per region and a round trip per selection
/// to narrow a list already in hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionChoice {
    /// No restriction.
    All,
    /// Only buckets the service placed in this region.
    In(String),
    /// Only buckets the service stated no region for. Without this, those
    /// buckets would belong to no choice and vanish from every one of them.
    Unstated,
}

impl RegionChoice {
    /// Whether this bucket survives the choice.
    pub fn matches(&self, bucket: &Bucket) -> bool {
        match (self, &bucket.region) {
            (Self::All, _) => true,
            (Self::In(chosen), Region::Known(region)) => chosen == region,
            (Self::Unstated, Region::Unknown) => true,
            // A bucket of unstated region is not in any named region: the
            // connection's own region is not evidence about the bucket's.
            (Self::In(_), Region::Unknown) | (Self::Unstated, Region::Known(_)) => false,
        }
    }

    /// The choice to hold once the listing has been replaced.
    ///
    /// A choice no remaining bucket can satisfy would render an empty table
    /// whose only cure is guessing which control emptied it, so it gives way to
    /// no restriction at all. A choice still in use is kept: changing accounts
    /// is not a reason to widen a deliberate one.
    pub fn retained_for(self, buckets: &[Bucket]) -> Self {
        if buckets.iter().any(|bucket| self.matches(bucket)) {
            self
        } else {
            Self::All
        }
    }
}

/// The choices worth offering for these buckets.
///
/// Only regions the account actually uses, so the control cannot offer a
/// selection that empties the table.
pub fn region_choices(buckets: &[Bucket]) -> Vec<RegionChoice> {
    let mut regions: Vec<&str> = buckets
        .iter()
        .filter_map(|bucket| match &bucket.region {
            Region::Known(region) => Some(region.as_str()),
            Region::Unknown => None,
        })
        .collect();
    regions.sort_unstable();
    regions.dedup();

    let mut choices = vec![RegionChoice::All];
    choices.extend(
        regions
            .into_iter()
            .map(|region| RegionChoice::In(region.to_owned())),
    );
    if buckets
        .iter()
        .any(|bucket| bucket.region == Region::Unknown)
    {
        choices.push(RegionChoice::Unstated);
    }
    choices
}

/// How deep into a bucket a location sits.
///
/// A newtype rather than a `String`, and the reason is the trailing separator.
/// S3 makes `photos` and `photos/` different requests with different answers,
/// so a prefix that is *sometimes* normalised is a defect waiting for whichever
/// call site forgets. Here it is normalised once, on the way in, and every
/// value of this type keeps the same shape: either empty — the bucket's own
/// root — or ending in `/`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Prefix(String);

impl Prefix {
    /// The bucket itself, with nothing narrowed.
    pub fn root() -> Self {
        Self(String::new())
    }

    /// The prefix `raw` names, whatever shape it arrives in.
    ///
    /// Leading separators are dropped and a trailing one is ensured: a key is
    /// addressed from the bucket root, so `/photos` and `photos` mean the same
    /// place and only one of them is a request the service understands.
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim_start_matches('/');
        if trimmed.is_empty() {
            return Self::root();
        }
        if trimmed.ends_with('/') {
            Self(trimmed.to_owned())
        } else {
            Self(format!("{trimmed}/"))
        }
    }

    /// What to send to the service.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is the bucket's own root.
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// The steps from the bucket down to here, in order.
    ///
    /// This is where a breadcrumb trail comes from, which is why the trail is
    /// not stored anywhere: it is a reading of the location rather than a
    /// second record of it that could disagree.
    pub fn segments(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.0.split('/').filter(|segment| !segment.is_empty())
    }

    /// The location one step up, or `None` at the bucket root.
    pub fn parent(&self) -> Option<Self> {
        if self.is_root() {
            return None;
        }
        let without_trailing = self.0.trim_end_matches('/');
        match without_trailing.rfind('/') {
            Some(cut) => Some(Self(self.0[..=cut].to_owned())),
            None => Some(Self::root()),
        }
    }

    /// This prefix with one more step below it.
    pub fn child(&self, segment: &str) -> Self {
        Self::parse(&format!("{}{}", self.0, segment))
    }
}

/// Where the user is.
///
/// One value answers it, and everything shown about position is derived from
/// this rather than kept beside it — a second record of where you are is a
/// second thing that can be wrong.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Location {
    /// The bucket being read.
    pub bucket: String,
    /// How far into it.
    pub prefix: Prefix,
}

impl Location {
    /// The root of `bucket`.
    pub fn bucket(name: impl Into<String>) -> Self {
        Self {
            bucket: name.into(),
            prefix: Prefix::root(),
        }
    }

    /// The same bucket, at `prefix`.
    pub fn at(bucket: impl Into<String>, prefix: Prefix) -> Self {
        Self {
            bucket: bucket.into(),
            prefix,
        }
    }

    /// The location `text` names, if it names one.
    ///
    /// Accepts the service's own addressing — `s3://bucket/prefix/` — and the
    /// same thing without the scheme, because someone who has just read a
    /// bucket name off a console will type the short form and being refused
    /// for it would be pedantry.
    ///
    /// There is exactly one way to fail: naming no bucket. A bucket name this
    /// application dislikes is *not* one of them — what is a valid name is the
    /// service's judgement, not ours, and a client that pre-refuses a name the
    /// service would have accepted is declaring where it should be observing
    /// (`ADR-0002`). A name the service rejects comes back as a service
    /// failure, with its own cause, which is the honest place for it.
    pub fn parse(text: &str) -> Option<Self> {
        let trimmed = text.trim();
        let without_scheme = trimmed
            .strip_prefix("s3://")
            .unwrap_or_else(|| trimmed.strip_prefix("S3://").unwrap_or(trimmed))
            .trim_start_matches('/');

        let (bucket, rest) = match without_scheme.split_once('/') {
            Some((bucket, rest)) => (bucket, rest),
            None => (without_scheme, ""),
        };

        if bucket.is_empty() {
            return None;
        }

        Some(Self::at(bucket, Prefix::parse(rest)))
    }
}

impl std::fmt::Display for Location {
    /// The service's own addressing, which is also what the path bar shows.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "s3://{}/{}", self.bucket, self.prefix.as_str())
    }
}

/// Where a listing left off.
///
/// Opaque on purpose: it is the service's own token and means nothing to this
/// application beyond "hand this back to continue".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor(pub String);

/// A folder, which S3 does not have.
///
/// There are no directories in an object store — only keys that share a
/// beginning. A folder is that shared beginning, reported by the service when
/// asked to group by separator, and it holds none of the things an object has:
/// no size, no modification time, no storage class. The columns that describe
/// an object stay empty for one, and that emptiness is the truth rather than a
/// gap to fill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    /// The full prefix this folder names, from the bucket root.
    pub prefix: Prefix,
}

impl Folder {
    /// What to call it here: the last step, without the separator.
    pub fn name(&self) -> &str {
        self.prefix.segments().next_back().unwrap_or("")
    }
}

/// One object, as the domain sees it.
///
/// Storage class and ETag are carried although nothing renders them yet. They
/// arrive in the same response as everything else at no extra cost, and
/// admitting them now is what keeps the port from changing when the remaining
/// columns are built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    /// The full key, from the bucket root.
    pub key: String,
    /// Size in bytes, as the service reported it.
    pub size: u64,
    /// Last modified, formatted as RFC 3339 when the service reported one.
    /// A `String` for the same reason [`Bucket::created`] is one.
    pub last_modified: Option<String>,
    /// The storage class, when stated.
    pub storage_class: Option<String>,
    /// The entity tag, when stated.
    pub etag: Option<String>,
}

impl Object {
    /// What to call it at `prefix`: the key with the prefix taken off.
    ///
    /// Empty when the key *is* the prefix — which happens, because a
    /// zero-length object whose key ends in a separator is how several tools
    /// write a folder. Such an entry is that folder rather than something
    /// inside it, and dropping it is the caller's job, not this one's.
    pub fn name_within(&self, prefix: &Prefix) -> &str {
        self.key.strip_prefix(prefix.as_str()).unwrap_or(&self.key)
    }
}

/// One page of what a location holds.
///
/// Paging is part of the answer rather than hidden inside the fetching,
/// because the interface has to be able to say that more is still coming. A
/// listing that quietly stops early reads exactly like a small folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Page {
    /// The folders directly beneath the location.
    pub folders: Vec<Folder>,
    /// The objects directly within it.
    pub objects: Vec<Object>,
    /// Where to continue, when the service says there is more.
    pub more: Option<Cursor>,
}

impl Page {
    /// Whether the service said there is more to come.
    pub fn is_truncated(&self) -> bool {
        self.more.is_some()
    }

    /// Whether this page reports nothing at all.
    ///
    /// Emptiness is a fact about a location that was read successfully. It is
    /// never what a refusal turns into — that is an error, and the distinction
    /// is the whole of the `object-browsing` spec's fifth requirement.
    pub fn is_empty(&self) -> bool {
        self.folders.is_empty() && self.objects.is_empty()
    }
}

#[cfg(test)]
mod tests {
    //! `bucket-listing` spec — narrowing the list to one region.

    use super::*;

    fn bucket(name: &str, region: Region) -> Bucket {
        Bucket {
            name: name.to_owned(),
            created: None,
            region,
        }
    }

    fn known(name: &str, region: &str) -> Bucket {
        bucket(name, Region::Known(region.to_owned()))
    }

    #[test]
    fn the_choices_offered_are_the_regions_the_account_actually_uses() {
        let buckets = [
            known("logs", "us-east-1"),
            known("backups", "ap-southeast-1"),
            known("archive", "ap-southeast-1"),
        ];

        assert_eq!(
            region_choices(&buckets),
            vec![
                RegionChoice::All,
                RegionChoice::In("ap-southeast-1".to_owned()),
                RegionChoice::In("us-east-1".to_owned()),
            ],
            "each region once, and none the account has no bucket in"
        );
    }

    #[test]
    fn a_bucket_without_a_stated_region_earns_a_choice_of_its_own() {
        let buckets = [
            known("logs", "us-east-1"),
            bucket("mystery", Region::Unknown),
        ];

        assert_eq!(
            region_choices(&buckets),
            vec![
                RegionChoice::All,
                RegionChoice::In("us-east-1".to_owned()),
                RegionChoice::Unstated,
            ],
            "otherwise that bucket belongs to no choice and silently disappears"
        );
    }

    #[test]
    fn an_account_with_no_buckets_offers_only_the_unrestricted_choice() {
        assert_eq!(region_choices(&[]), vec![RegionChoice::All]);
    }

    #[test]
    fn no_restriction_admits_every_bucket() {
        assert!(RegionChoice::All.matches(&known("logs", "us-east-1")));
        assert!(RegionChoice::All.matches(&bucket("mystery", Region::Unknown)));
    }

    #[test]
    fn a_region_admits_only_the_buckets_stated_to_be_in_it() {
        let choice = RegionChoice::In("us-east-1".to_owned());

        assert!(choice.matches(&known("logs", "us-east-1")));
        assert!(!choice.matches(&known("backups", "ap-southeast-1")));
    }

    #[test]
    fn a_bucket_of_unstated_region_is_not_swept_into_a_named_one() {
        let choice = RegionChoice::In("us-east-1".to_owned());

        assert!(
            !choice.matches(&bucket("mystery", Region::Unknown)),
            "the connection's region is not evidence about a bucket's"
        );
    }

    #[test]
    fn the_unstated_choice_admits_only_buckets_the_service_said_nothing_about() {
        assert!(RegionChoice::Unstated.matches(&bucket("mystery", Region::Unknown)));
        assert!(!RegionChoice::Unstated.matches(&known("logs", "us-east-1")));
    }

    #[test]
    fn a_choice_the_next_account_has_no_bucket_for_gives_way() {
        let choice = RegionChoice::In("us-east-1".to_owned());
        let next_account = [known("backups", "ap-southeast-1")];

        assert_eq!(
            choice.retained_for(&next_account),
            RegionChoice::All,
            "holding it would show an empty table with no way back"
        );
    }

    #[test]
    fn a_prefix_has_one_shape_however_it_was_written() {
        // `photos` and `photos/` are different requests to the service, so a
        // prefix that is only sometimes normalised is a defect waiting for
        // whichever call site forgets.
        for written in ["photos", "photos/", "/photos", "/photos/"] {
            assert_eq!(
                Prefix::parse(written).as_str(),
                "photos/",
                "written as {written:?}"
            );
        }
    }

    #[test]
    fn the_bucket_root_is_empty_however_it_was_written() {
        for written in ["", "/", "///"] {
            let prefix = Prefix::parse(written);
            assert!(prefix.is_root(), "written as {written:?}");
            assert_eq!(prefix.as_str(), "");
        }
    }

    #[test]
    fn the_steps_of_a_prefix_are_what_a_breadcrumb_trail_reads() {
        let deep = Prefix::parse("photos/vacation/2026");

        assert_eq!(
            deep.segments().collect::<Vec<_>>(),
            ["photos", "vacation", "2026"]
        );
        assert_eq!(Prefix::root().segments().count(), 0);
    }

    #[test]
    fn walking_up_ends_at_the_bucket_root_and_stops() {
        let deep = Prefix::parse("photos/vacation/2026");

        let up = deep.parent().expect("a prefix three deep has a parent");
        assert_eq!(up.as_str(), "photos/vacation/");
        let up = up.parent().expect("and so does one two deep");
        assert_eq!(up.as_str(), "photos/");
        let root = up.parent().expect("and one step from the root");
        assert!(root.is_root());
        assert_eq!(root.parent(), None, "the root has nowhere above it");
    }

    #[test]
    fn a_child_is_reached_by_name_and_keeps_the_shape() {
        let photos = Prefix::parse("photos");

        assert_eq!(photos.child("vacation").as_str(), "photos/vacation/");
        assert_eq!(Prefix::root().child("photos").as_str(), "photos/");
    }

    #[test]
    fn a_folder_marker_is_named_by_nothing_at_its_own_location() {
        // The case every console's "create folder" produces: a zero-length
        // object whose key is exactly the prefix being listed. Its name here
        // is the empty string, which is why it must be dropped rather than
        // drawn — a nameless row inside every folder anyone ever made.
        let photos = Prefix::parse("photos/");
        let marker = Object {
            key: "photos/".to_owned(),
            size: 0,
            last_modified: None,
            storage_class: None,
            etag: None,
        };
        let inside = Object {
            key: "photos/cat.jpg".to_owned(),
            ..marker.clone()
        };

        assert_eq!(marker.name_within(&photos), "");
        assert_eq!(inside.name_within(&photos), "cat.jpg");
    }

    #[test]
    fn a_folder_is_called_by_its_last_step() {
        let nested = Folder {
            prefix: Prefix::parse("photos/vacation/"),
        };

        assert_eq!(nested.name(), "vacation");
    }

    #[test]
    fn an_empty_page_and_a_truncated_one_are_different_questions() {
        // Neither of these is a refusal. A refusal never becomes a page at
        // all — it is an error, and keeping that true is the whole of the
        // `object-browsing` spec's fifth requirement.
        let nothing = Page::default();
        assert!(nothing.is_empty());
        assert!(!nothing.is_truncated());

        let more_coming = Page {
            more: Some(Cursor("token".to_owned())),
            ..Page::default()
        };
        assert!(more_coming.is_empty(), "this page carried nothing");
        assert!(more_coming.is_truncated(), "but the listing is not over");
    }

    #[test]
    fn a_location_is_read_from_the_services_own_addressing() {
        let here = Location::parse("s3://holiday/photos/vacation/").expect("a location");

        assert_eq!(here.bucket, "holiday");
        assert_eq!(here.prefix.as_str(), "photos/vacation/");
    }

    #[test]
    fn the_scheme_and_the_trailing_separator_are_both_optional() {
        // Someone who has just read a bucket name off a console types the
        // short form, and being refused for it would be pedantry.
        let written = [
            "s3://holiday/photos/",
            "s3://holiday/photos",
            "holiday/photos/",
            "holiday/photos",
            "  s3://holiday/photos  ",
        ];

        for text in written {
            let here = Location::parse(text).unwrap_or_else(|| panic!("{text:?} names a location"));
            assert_eq!(here.bucket, "holiday", "from {text:?}");
            assert_eq!(here.prefix.as_str(), "photos/", "from {text:?}");
        }
    }

    #[test]
    fn a_bucket_on_its_own_is_that_buckets_root() {
        for text in ["s3://holiday", "s3://holiday/", "holiday"] {
            let here = Location::parse(text).unwrap_or_else(|| panic!("{text:?}"));
            assert_eq!(here.bucket, "holiday");
            assert!(here.prefix.is_root(), "from {text:?}");
        }
    }

    #[test]
    fn text_that_names_no_bucket_names_nowhere() {
        // The one way to fail. The caller keeps the location already open —
        // `object-browsing`, "Text that names nowhere".
        for text in ["", "   ", "s3://", "s3:///", "/", "///"] {
            assert_eq!(Location::parse(text), None, "{text:?} names no bucket");
        }
    }

    #[test]
    fn a_bucket_name_the_service_might_dislike_is_the_services_business() {
        // Not ours to refuse: pre-rejecting a name the service would have
        // accepted is declaring where this project observes (`ADR-0002`), and
        // a name it rejects comes back with a cause of its own.
        let odd = Location::parse("s3://Not_A_Valid_Bucket_Name/").expect("parsed anyway");

        assert_eq!(odd.bucket, "Not_A_Valid_Bucket_Name");
    }

    #[test]
    fn what_is_written_can_be_read_back() {
        let here = Location::at("holiday", Prefix::parse("photos/vacation"));

        assert_eq!(here.to_string(), "s3://holiday/photos/vacation/");
        assert_eq!(Location::parse(&here.to_string()), Some(here));

        let root = Location::bucket("holiday");
        assert_eq!(root.to_string(), "s3://holiday/");
        assert_eq!(Location::parse(&root.to_string()), Some(root));
    }

    #[test]
    fn a_choice_the_next_account_still_uses_is_kept() {
        let choice = RegionChoice::In("ap-southeast-1".to_owned());
        let next_account = [
            known("backups", "ap-southeast-1"),
            known("logs", "us-east-1"),
        ];

        assert_eq!(
            choice.clone().retained_for(&next_account),
            choice,
            "switching accounts is not a reason to widen a deliberate choice"
        );
    }
}

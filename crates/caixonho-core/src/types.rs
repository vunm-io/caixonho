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

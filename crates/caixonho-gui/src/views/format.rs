//! Turning what the service said into what a person reads.
//!
//! Pure functions over strings, so they can be tested without a window — the
//! one property most of this crate's rendering does not have, and the reason
//! every display defect so far has been found by a person looking at a screen
//! rather than by a test.

/// A directory bucket's name, split into the part someone chose and the part
/// the service requires of it.
///
/// `marginx-prod--usw2-lax1-az1--x-s3` becomes `("marginx-prod",
/// "usw2-lax1-az1--x-s3")`. The caller renders the second half quietly; it is
/// the same on every bucket in a zone, so at full weight it is the part that
/// survives truncation while the part that distinguishes one bucket from
/// another is what gets cut.
///
/// **Split at the first `--`, never by counting segments.** A local zone's id
/// has three segments where a plain availability zone's has two, so code that
/// counted would be right until the day it met the other kind — and this is
/// the kind the change was verified against.
///
/// Returns `None` for a name with no separator, or one that is all separator:
/// there is nothing to split, and inventing a division would misrepresent the
/// name.
pub(crate) fn split_zonal_name(name: &str) -> Option<(&str, &str)> {
    name.split_once("--")
        .filter(|(chosen, zone)| !chosen.is_empty() && !zone.is_empty())
}

/// A timestamp a person can read, from the RFC 3339 the service reported.
///
/// `2026-08-20T05:59:25.244000+00:00` becomes `2026-08-20 05:59 UTC`. The
/// seconds and the fraction go: they are never what anyone is looking for in a
/// list, and left in a narrow column they are what gets truncated — the
/// timestamp arrives cut off mid-fraction, which reads as a rendering fault
/// rather than as a date.
///
/// **No conversion to local time.** That needs a timezone database and a
/// decision about which zone a remote object's time belongs in; showing UTC
/// and saying so is honest, and showing a converted time without saying so
/// would not be. The marker is dropped when the service reported an offset
/// that is not UTC, because then the time is not UTC and must not claim to be.
///
/// Anything that is not the expected shape is returned unchanged. A value this
/// cannot read is still the service's own answer, and mangling it would be
/// worse than showing it plainly.
pub(crate) fn timestamp(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let shaped = bytes.len() >= 16
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':';
    if !shaped {
        return raw.to_owned();
    }

    let date = &raw[..10];
    let time = &raw[11..16];
    let rest = &raw[16..];
    // Only these mean UTC. An offset of any other kind is a different moment
    // in the day, and labelling it UTC would state something untrue.
    if rest.ends_with('Z') || rest.ends_with("+00:00") {
        format!("{date} {time} UTC")
    } else {
        format!("{date} {time}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timestamp_is_shortened_to_what_a_list_is_read_for() {
        assert_eq!(
            timestamp("2026-08-20T05:59:25.244000+00:00"),
            "2026-08-20 05:59 UTC"
        );
        assert_eq!(timestamp("2026-07-12T07:56:53Z"), "2026-07-12 07:56 UTC");
    }

    #[test]
    fn an_offset_that_is_not_utc_is_not_labelled_utc() {
        // The same instant written from another zone is a different clock
        // reading, and calling it UTC would state something untrue.
        assert_eq!(timestamp("2026-08-20T12:59:25+07:00"), "2026-08-20 12:59");
    }

    #[test]
    fn anything_this_cannot_read_is_returned_exactly_as_it_arrived() {
        // Still the service's own answer. Mangling it would be worse than
        // showing it plainly, and inventing a placeholder would be worse yet.
        for odd in ["", "soon", "20/08/2026", "2026-08-20"] {
            assert_eq!(timestamp(odd), odd);
        }
    }

    #[test]
    fn a_zonal_name_splits_at_the_first_separator() {
        assert_eq!(
            split_zonal_name("marginx-prod--usw2-lax1-az1--x-s3"),
            Some(("marginx-prod", "usw2-lax1-az1--x-s3"))
        );
    }

    #[test]
    fn a_zone_id_is_not_split_by_counting_its_segments() {
        // Three segments and two, split the same way. Counting would pass on
        // one of these and fail on the other, and which one is an accident of
        // whichever account the author happened to have.
        let local = split_zonal_name("data--usw2-lax1-az1--x-s3");
        let plain = split_zonal_name("data--usw2-az1--x-s3");

        assert_eq!(local, Some(("data", "usw2-lax1-az1--x-s3")));
        assert_eq!(plain, Some(("data", "usw2-az1--x-s3")));
    }

    #[test]
    fn a_name_with_nothing_to_split_is_left_alone() {
        for whole in ["logs", "my--", "--zone--x-s3", "--", ""] {
            assert_eq!(
                split_zonal_name(whole),
                None,
                "{whole}: inventing a division would misrepresent the name"
            );
        }
    }
}

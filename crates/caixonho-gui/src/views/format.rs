//! Turning what the service said into what a person reads.
//!
//! Pure functions over strings, so they can be tested without a window — the
//! one property most of this crate's rendering does not have, and the reason
//! every display defect so far has been found by a person looking at a screen
//! rather than by a test.

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
}

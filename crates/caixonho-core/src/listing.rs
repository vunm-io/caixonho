//! Turning what the service reported into what a person should see.
//!
//! S3 has no directories. Asked to group by separator it answers with two
//! lists — the prefixes that share a beginning, and the keys directly at the
//! one being listed — and the folders a user sees are the first of those,
//! inferred rather than stored.
//!
//! Three facts about that inference are ordinary rather than exotic, and each
//! is a way to render a listing wrongly. They are decided here, as one pure
//! function over the service's own answer, because that is a thing a test can
//! hold and a window is not.

use crate::types::{Cursor, Folder, Object, Page, Prefix, Region};

/// The page a listing of `prefix` should show.
///
/// `common_prefixes` and `contents` are what the service reported, unaltered.
///
/// `served_from` is a parameter rather than something decided here because
/// only the caller knows which region it addressed the request to. This
/// function sees an answer, never where it came from.
pub(crate) fn page_at(
    prefix: &Prefix,
    common_prefixes: Vec<String>,
    contents: Vec<Object>,
    more: Option<Cursor>,
    served_from: Option<Region>,
) -> Page {
    Page {
        folders: common_prefixes
            .into_iter()
            .map(|raw| Folder {
                prefix: Prefix::parse(&raw),
            })
            .collect(),
        // The rule that costs nothing to get right and is ugly to get wrong:
        // an entry whose key is exactly the prefix being listed is that
        // folder, not something inside it. Several tools write one when asked
        // to "create folder", and drawn as a child it is a row with no name
        // and no size in every folder anyone ever made that way.
        objects: contents
            .into_iter()
            .filter(|object| object.key != prefix.as_str())
            .collect(),
        more,
        served_from,
    }
}

#[cfg(test)]
mod tests {
    //! `object-browsing` spec — "Folders are inferred, and the inference is
    //! not disguised".
    //!
    //! The fixtures are the real cases, taken from a bucket built to hold
    //! them: a folder marker, a name shared between an object and a prefix,
    //! and a prefix nothing stands behind.

    use super::*;

    /// Every test here is about what a listing shows, never about where the
    /// answer came from. Named rather than a bare `None` sitting beside the
    /// `more` cursor, which is also `None` in most of them: two anonymous
    /// `None`s in a row tell a reader nothing about which is which.
    const SERVED_FROM_THE_REGION_ASKED: Option<Region> = None;

    fn object(key: &str, size: u64) -> Object {
        Object {
            key: key.to_owned(),
            size,
            last_modified: None,
            storage_class: None,
            etag: None,
        }
    }

    #[test]
    fn a_folder_marker_is_the_folder_rather_than_an_entry_inside_it() {
        // Listing `photos/` returns the marker object `photos/` among the
        // contents, because its key does begin with the prefix. Its name at
        // this location is the empty string; showing it produces a nameless
        // zero-byte row inside the folder it *is*.
        let photos = Prefix::parse("photos/");

        let page = page_at(
            &photos,
            vec!["photos/vacation/".to_owned()],
            vec![
                object("photos/", 0),
                object("photos/cat.jpg", 1),
                object("photos/dog.jpg", 1),
            ],
            None,
            SERVED_FROM_THE_REGION_ASKED,
        );

        assert_eq!(
            page.objects
                .iter()
                .map(|o| o.key.as_str())
                .collect::<Vec<_>>(),
            ["photos/cat.jpg", "photos/dog.jpg"],
            "the marker names this folder, so it is not one of its children"
        );
        assert_eq!(page.folders.len(), 1, "the real child folder survives");
    }

    #[test]
    fn a_prefix_nothing_stands_behind_is_still_a_folder() {
        // `deep/` exists only because a key is nested under it. No object is
        // at `deep/` and none needs to be: the folder is openable, and every
        // column that describes an object is empty for it.
        let root = Prefix::root();

        let page = page_at(
            &root,
            vec!["deep/".to_owned()],
            vec![],
            None,
            SERVED_FROM_THE_REGION_ASKED,
        );

        assert_eq!(page.folders.len(), 1);
        assert_eq!(page.folders[0].name(), "deep");
        assert!(
            page.objects.is_empty(),
            "nothing was reported at this level, and nothing is invented"
        );
        assert!(!page.is_empty(), "a location with a folder is not empty");
    }

    #[test]
    fn an_object_and_a_prefix_may_share_a_name_and_both_survive() {
        // `notes` is a 35-byte object and `notes/` is a prefix, at the same
        // location. Two rows with one name: one openable, one not. Neither
        // conceals the other, because the service reported them in different
        // lists and nothing here merges them.
        let root = Prefix::root();

        let page = page_at(
            &root,
            vec!["notes/".to_owned()],
            vec![object("notes", 35)],
            None,
            SERVED_FROM_THE_REGION_ASKED,
        );

        assert_eq!(page.folders[0].name(), "notes");
        assert_eq!(page.objects[0].key, "notes");
        assert_eq!(page.objects[0].size, 35);
    }

    #[test]
    fn the_marker_rule_does_not_touch_a_key_that_merely_starts_the_same() {
        // Only an exact match is the folder. `photos-old.zip` begins with
        // "photos" and is an ordinary object; a rule written with
        // `starts_with` would eat it.
        let root = Prefix::root();

        let page = page_at(
            &root,
            vec![],
            vec![object("photos-old.zip", 900), object("photos.txt", 12)],
            None,
            SERVED_FROM_THE_REGION_ASKED,
        );

        assert_eq!(page.objects.len(), 2, "neither key is a folder marker");
    }

    #[test]
    fn what_the_service_said_about_more_pages_is_carried_through_untouched() {
        // The interface has to be able to say more is coming; a listing that
        // quietly stops early reads exactly like a small folder.
        let root = Prefix::root();

        let page = page_at(
            &root,
            vec![],
            vec![],
            Some(Cursor("token".to_owned())),
            SERVED_FROM_THE_REGION_ASKED,
        );

        assert!(page.is_truncated());
        assert!(page.is_empty(), "and this page still carried nothing");
    }
}

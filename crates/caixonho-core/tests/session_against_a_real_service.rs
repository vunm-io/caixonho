//! `XONHO-0031` — the async layer the window sits on, against a real service.
//!
//! `Session::open` installs a real adapter as its last act, so every `spawn_*`
//! below reaches the service. This is the seam neither existing tier covers:
//! the port tests answer above it and the replay tests below it, and the
//! window's own 118 tests answer above the session with a double underneath.
//!
//! See `service.rs` for what a local service cannot prove.

mod service;

use caixonho_core::session::{DeleteOutcome, FolderOutcome, Tally};
use caixonho_core::transfer::{Collision, DownloadOutcome, UploadOutcome};
use caixonho_core::{BucketKind, Location, Prefix};
use service::{Connected, Service};

fn at(prefix: &str) -> Location {
    Location::at("reports", Prefix::parse(prefix))
}

#[tokio::test]
async fn a_location_is_read_off_the_calling_thread_and_delivered_once() {
    let service = Service::start().await;
    service
        .with_bucket("reports")
        .with_object("reports", "summary.csv", b"a\n")
        .with_object("reports", "daily/monday.csv", b"1\n");
    let connected = Connected::to(&service).await;

    let page = Connected::settled(|deliver| {
        connected.session.spawn_objects(at(""), None, deliver);
    })
    .await
    .expect("the location is readable");

    assert_eq!(page.folders.len(), 1, "daily/");
    assert_eq!(page.objects.len(), 1, "summary.csv");
}

#[tokio::test]
async fn a_file_goes_up_and_the_listing_shows_it() {
    // The whole upload path as the window drives it — including the key it
    // decides on, which is what `XONHO-0026` made the user's choice.
    let service = Service::start().await;
    service.with_bucket("reports");
    let connected = Connected::to(&service).await;

    let dir = tempfile::tempdir().expect("a temporary directory");
    let file = dir.path().join("summary.csv");
    std::fs::write(&file, b"a,b,c\n").expect("written");

    let outcome = Connected::settled(|deliver| {
        connected.session.spawn_upload(
            "reports".to_owned(),
            "daily/summary.csv".to_owned(),
            file.clone(),
            Collision::Ask,
            deliver,
        );
    })
    .await;

    match outcome {
        UploadOutcome::Finished {
            key,
            stepped_aside,
            bytes,
        } => {
            assert_eq!(key, "daily/summary.csv");
            assert!(!stepped_aside, "nothing was in the way");
            assert_eq!(bytes, 6);
        }
        other => panic!("expected Finished, got {other:?}"),
    }

    let page = Connected::settled(|deliver| {
        connected.session.spawn_objects(at("daily/"), None, deliver);
    })
    .await
    .expect("readable");
    assert_eq!(
        page.objects
            .iter()
            .map(|o| o.key.as_str())
            .collect::<Vec<_>>(),
        ["daily/summary.csv"],
        "the object the service holds is the one the upload said it wrote"
    );
}

#[tokio::test]
async fn uploading_onto_a_taken_key_asks_rather_than_replacing() {
    // `XONHO-0020`'s guarantee, one layer up from where it was proven at the
    // port: the *session* must surface the question rather than deciding.
    let service = Service::start().await;
    service
        .with_bucket("reports")
        .with_object("reports", "summary.csv", b"the original\n");
    let connected = Connected::to(&service).await;

    let dir = tempfile::tempdir().expect("a temporary directory");
    let file = dir.path().join("summary.csv");
    std::fs::write(&file, b"the replacement\n").expect("written");

    let outcome = Connected::settled(|deliver| {
        connected.session.spawn_upload(
            "reports".to_owned(),
            "summary.csv".to_owned(),
            file.clone(),
            Collision::Ask,
            deliver,
        );
    })
    .await;

    match outcome {
        UploadOutcome::KeyTaken { key } => assert_eq!(key, "summary.csv"),
        other => panic!("expected KeyTaken, got {other:?}"),
    }
}

#[tokio::test]
async fn a_download_writes_the_object_to_disk_byte_for_byte() {
    let service = Service::start().await;
    let bytes = "một hai ba — ✓\n".as_bytes();
    service
        .with_bucket("reports")
        .with_object("reports", "notes.md", bytes);
    let connected = Connected::to(&service).await;

    let into = tempfile::tempdir().expect("a temporary directory");
    let outcome = Connected::settled(|deliver| {
        connected.session.spawn_download(
            "reports".to_owned(),
            "notes.md".to_owned(),
            into.path().to_owned(),
            Collision::Ask,
            |_, _| {},
            deliver,
        );
    })
    .await;

    match outcome {
        DownloadOutcome::Finished { name, .. } => {
            let written = std::fs::read(into.path().join(&name)).expect("the file is there");
            assert_eq!(written, bytes, "the bytes on disk are not the object's");
        }
        other => panic!("expected Finished, got {other:?}"),
    }
}

#[tokio::test]
async fn a_delete_removes_the_object_from_the_service() {
    let service = Service::start().await;
    service
        .with_bucket("reports")
        .with_object("reports", "summary.csv", b"a\n");
    let connected = Connected::to(&service).await;

    let outcome = Connected::settled(|deliver| {
        connected
            .session
            .spawn_delete("reports".to_owned(), "summary.csv".to_owned(), deliver);
    })
    .await;
    assert!(
        matches!(outcome, DeleteOutcome::Gone { .. }),
        "expected Gone, got {outcome:?}"
    );

    let page = Connected::settled(|deliver| {
        connected.session.spawn_objects(at(""), None, deliver);
    })
    .await
    .expect("readable");
    assert!(page.objects.is_empty(), "the service still holds it");
}

#[tokio::test]
async fn a_folder_is_walked_to_its_end_and_the_count_is_what_a_delete_removes() {
    // `XONHO-0030`'s count, against a real service. The number and the keys
    // come from one pass on purpose — two passes over a live bucket can
    // disagree, and a confirmation stating one number followed by a delete of
    // a different set has told the user something that was never true.
    let service = Service::start().await;
    service.with_bucket("reports");
    for n in 0..7 {
        service.with_object("reports", &format!("daily/{n}.csv"), b"x");
    }
    service.with_object("reports", "daily/deep/nested.csv", b"y");
    let connected = Connected::to(&service).await;

    let tally = Connected::settled(|deliver| {
        connected.session.spawn_walk_under(at("daily/"), deliver);
    })
    .await;

    let keys = match tally {
        Tally::All(keys) => keys,
        other => panic!("expected All, got {other:?}"),
    };
    assert_eq!(keys.len(), 8, "seven at the top and one a level down");

    // And deleting exactly those empties the prefix — which is the claim the
    // confirmation makes on the user's behalf.
    for key in &keys {
        let outcome = Connected::settled(|deliver| {
            connected
                .session
                .spawn_delete("reports".to_owned(), key.clone(), deliver);
        })
        .await;
        assert!(
            matches!(outcome, DeleteOutcome::Gone { .. }),
            "{key} survived: {outcome:?}"
        );
    }

    let left = Connected::settled(|deliver| {
        connected.session.spawn_walk_under(at("daily/"), deliver);
    })
    .await;
    match left {
        Tally::All(keys) => assert!(keys.is_empty(), "still there: {keys:?}"),
        other => panic!("expected All, got {other:?}"),
    }
}

#[tokio::test]
async fn a_folder_marker_is_written_on_a_general_purpose_bucket() {
    // The half of `XONHO-0024` a general purpose bucket answers. The
    // directory-bucket half — a refusal that spends no request — is decided
    // from the listed kind and never reaches a service, so it stays covered
    // where it always was, above the port.
    //
    // What is asserted is that the marker is **written**: the request left,
    // the service accepted it, and the key is what the name became. That a
    // later listing then *shows* the folder is not assertable here, for the
    // reason `an_empty_folder_is_invisible_to_this_service` records.
    let service = Service::start().await;
    service.with_bucket("reports");
    let connected = Connected::to(&service).await;

    let outcome = Connected::settled(|deliver| {
        connected.session.spawn_create_folder(
            "reports".to_owned(),
            Prefix::root(),
            BucketKind::General,
            "archive".to_owned(),
            deliver,
        );
    })
    .await;

    match outcome {
        FolderOutcome::Made { key } => assert_eq!(key, "archive/"),
        other => panic!("expected Made, got {other:?}"),
    }
}

// ---- What this service cannot prove (`XONHO-0031` task 4.1) ----
//
// A test per exclusion rather than a comment. A comment saying "versioning is
// not supported" becomes folklore the day the dependency gains it; a test that
// fails when the reason stops being true does not.

#[tokio::test]
async fn an_empty_folder_is_invisible_to_this_service() {
    // **Not a defect in this application.** Real S3 stores a folder marker as
    // a zero-length object whose key ends in the separator and returns it as a
    // common prefix. `s3s-fs` instead calls `create_dir_all` for such a key
    // (`s3.rs:895`) and derives common prefixes only from *files*
    // (`s3.rs:1712`), so an empty directory produces neither an object nor a
    // prefix.
    //
    // So `XONHO-0024`'s own scenario — a made folder appears in the listing —
    // cannot be proven here and stays with the owner. Asserting the limitation
    // rather than describing it: the day `s3s-fs` starts listing these, this
    // test fails and the exclusion is removed rather than outliving its reason.
    let service = Service::start().await;
    service.with_bucket("reports");
    let connected = Connected::to(&service).await;

    Connected::settled(|deliver| {
        connected.session.spawn_create_folder(
            "reports".to_owned(),
            Prefix::root(),
            BucketKind::General,
            "archive".to_owned(),
            deliver,
        );
    })
    .await;

    let page = Connected::settled(|deliver| {
        connected.session.spawn_objects(at(""), None, deliver);
    })
    .await
    .expect("readable");

    assert!(
        page.folders.is_empty() && page.objects.is_empty(),
        "this service now lists an empty folder — real S3 does too, so remove \
         this exclusion and assert the listing in the test above instead. \
         Got folders={:?} objects={:?}",
        page.folders
            .iter()
            .map(|f| f.prefix.as_str())
            .collect::<Vec<_>>(),
        page.objects
            .iter()
            .map(|o| o.key.as_str())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn this_service_keeps_no_versions_so_undo_cannot_be_proven_here() {
    // `XONHO-0021` offers Undo exactly when a delete's own response reports a
    // marker. `s3s-fs` has no versioning — no `put_bucket_versioning`, no
    // delete markers — so every delete here answers without one and the path
    // that offers Undo is never entered.
    //
    // That flow stays the owner's, on a versioned bucket. What this asserts is
    // the *reason*: a delete here coming back with a marker means versioning
    // has arrived and Undo becomes testable.
    let service = Service::start().await;
    service
        .with_bucket("reports")
        .with_object("reports", "summary.csv", b"a\n");
    let connected = Connected::to(&service).await;

    let outcome = Connected::settled(|deliver| {
        connected
            .session
            .spawn_delete("reports".to_owned(), "summary.csv".to_owned(), deliver);
    })
    .await;

    match outcome {
        DeleteOutcome::Gone { marker } => assert!(
            marker.is_none(),
            "this service reported a delete marker — versioning has arrived, \
             so XONHO-0021's Undo can and should be tested here now"
        ),
        other => panic!("expected Gone, got {other:?}"),
    }
}

#[tokio::test]
async fn this_service_refuses_nothing_so_denials_cannot_be_proven_here() {
    // No IAM here, so reading a bucket nobody granted anything for is not a
    // denial — it is a missing bucket. Telling those two apart is the whole
    // point of this project, and it is covered where it belongs: below the
    // adapter, by the replay tests, which can hand it a real `AccessDenied`
    // body.
    let service = Service::start().await;
    let connected = Connected::to(&service).await;

    let outcome = Connected::settled(|deliver| {
        connected.session.spawn_objects(
            Location::at("nobody-granted-this", Prefix::root()),
            None,
            deliver,
        );
    })
    .await;

    if let Err(caixonho_core::Error::AccessDenied { .. }) = outcome {
        panic!(
            "this service refused a listing — it has grown some notion of \
             permission, so denials can be tested here now"
        );
    }
}

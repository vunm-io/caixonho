//! `XONHO-0031` — the adapter, over real HTTP.

mod service;

use service::Service;

#[tokio::test]
async fn the_service_starts_and_stops() {
    // The smallest thing that proves the harness itself: a port was chosen,
    // and it is not the one anybody hard-coded.
    let service = Service::start().await;
    let url = service.base_url().to_owned();
    // An IP literal, not a name, and the assertion says so on purpose: the
    // host is what keeps this tier reachable on Windows, and a rewrite back
    // to `localhost` should fail here rather than five runs later in CI.
    assert!(url.starts_with("http://127.0.0.1:"), "got {url}");
    assert!(
        !url.ends_with(":0") && !url.ends_with(":8014"),
        "the OS was asked to choose a port, and this is either unbound or the default: {url}"
    );
}

#[tokio::test]
async fn a_connection_opens_against_the_local_service_and_lists_its_buckets() {
    // The whole wiring, in one assertion: a config file with an endpoint in
    // it, the SDK's own loader, the real adapter, and a request that reaches
    // a real service and comes back.
    let service = Service::start().await;
    let connected = service::Connected::to(&service).await;

    let listing = connected
        .store
        .list_buckets()
        .await
        .expect("an empty account is a result, never an error");

    assert!(
        listing.buckets.is_empty(),
        "a service with nothing in it lists nothing, and that is not a refusal"
    );
}

/// A location in the bucket the tests below seed.
fn at(prefix: &str) -> caixonho_core::Location {
    caixonho_core::Location::at("reports", caixonho_core::Prefix::parse(prefix))
}

/// A service holding a small tree: two objects at the root, three a level
/// down, and one two levels down. Enough for the grouped and the flat listing
/// to disagree, which is the whole point of having both.
async fn a_small_tree() -> (Service, service::Connected) {
    let service = Service::start().await;
    service
        .with_bucket("reports")
        .with_object("reports", "summary.csv", b"a,b,c\n")
        .with_object("reports", "notes.md", b"# notes\n")
        .with_object("reports", "daily/monday.csv", b"1\n")
        .with_object("reports", "daily/tuesday.csv", b"2\n")
        .with_object("reports", "daily/deep/wednesday.csv", b"3\n");
    let connected = service::Connected::to(&service).await;
    (service, connected)
}

#[tokio::test]
async fn a_grouped_listing_infers_folders_and_a_flat_one_does_not() {
    // The same tree through both listings, so what is asserted is the
    // *difference*. `XONHO-0030` added the flat one precisely because the
    // grouped one hides everything a level down, and until now nothing has
    // confirmed that a real service agrees.
    let (_service, connected) = a_small_tree().await;

    let page = connected
        .store
        .list_objects(&at(""), None)
        .await
        .expect("the root is readable");

    let mut folders: Vec<&str> = page.folders.iter().map(|f| f.name()).collect();
    folders.sort_unstable();
    let mut objects: Vec<&str> = page.objects.iter().map(|o| o.key.as_str()).collect();
    objects.sort_unstable();

    assert_eq!(
        folders,
        ["daily"],
        "the delimiter is what makes folders exist"
    );
    assert_eq!(
        objects,
        ["notes.md", "summary.csv"],
        "and it hides everything below them"
    );

    let flat = connected
        .store
        .list_keys_under(&at(""), None)
        .await
        .expect("the prefix is walkable");
    let mut keys = flat.keys.clone();
    keys.sort();

    assert_eq!(
        keys,
        [
            "daily/deep/wednesday.csv",
            "daily/monday.csv",
            "daily/tuesday.csv",
            "notes.md",
            "summary.csv",
        ],
        "the flat listing is every key at every depth — which is what deleting \
         a folder has to mean"
    );
    assert!(flat.more.is_none(), "five keys is one page");
}

#[tokio::test]
async fn a_folders_size_is_its_whole_subtree_and_not_its_top_level() {
    // The defect this guards: counting `daily/` by its grouped listing gives
    // two, and deleting it removes three.
    let (_service, connected) = a_small_tree().await;

    let grouped = connected
        .store
        .list_objects(&at("daily/"), None)
        .await
        .expect("readable");
    let flat = connected
        .store
        .list_keys_under(&at("daily/"), None)
        .await
        .expect("walkable");

    assert_eq!(grouped.objects.len(), 2, "the top level of `daily/`");
    assert_eq!(
        flat.keys.len(),
        3,
        "and everything under it — the number a confirmation must state"
    );
}

#[tokio::test]
async fn a_walk_past_one_page_follows_the_services_own_continuation_token() {
    // `XONHO-0030` proved the walk continues against a scripted double whose
    // tokens this project invented. This proves it round-trips a token a real
    // service minted — which is the half a double cannot reach, and the half
    // that decides whether a folder's count is right.
    //
    // **1001, not 25.** The adapter sets no `max_keys`, so the service's own
    // default of 1000 is what decides where a page ends — and at 25 objects
    // this test passed while fetching everything in one page and never
    // touching a continuation token at all. A test named for a requirement it
    // does not exercise is worse than no test: it reports the requirement as
    // covered. `pages > 1` below is the assertion that keeps it honest.
    let service = Service::start().await;
    service.with_bucket("reports");
    for n in 0..1001 {
        service.with_object("reports", &format!("many/{n:04}.txt"), b"x");
    }
    let connected = service::Connected::to(&service).await;

    let mut keys = Vec::new();
    let mut cursor = None;
    let mut pages = 0;
    loop {
        let page = connected
            .store
            .list_keys_under(&at("many/"), cursor.as_ref())
            .await
            .expect("walkable");
        pages += 1;
        keys.extend(page.keys);
        match page.more {
            Some(next) => cursor = Some(next),
            None => break,
        }
        assert!(pages < 30, "the walk is not converging");
    }

    assert!(
        pages > 1,
        "one page: the service's token was never exercised, so this test \
         proved nothing about continuing a walk"
    );
    assert_eq!(keys.len(), 1001, "every page, not the first");
    keys.sort();
    assert_eq!(keys.first().map(String::as_str), Some("many/0000.txt"));
    assert_eq!(keys.last().map(String::as_str), Some("many/1000.txt"));
}

#[tokio::test]
async fn a_taken_key_is_refused_by_the_service_and_not_by_a_check_here() {
    // The whole of `XONHO-0020`'s promise, and the first time anything has
    // checked that a real service makes it. A client-side "does it exist?"
    // followed by a write is a race; a conditional write is not, and this is
    // what proves the condition actually reaches the service and is honoured.
    use caixonho_core::store::{IfAbsent, PutOutcome};

    let service = Service::start().await;
    service.with_bucket("reports");
    let connected = service::Connected::to(&service).await;

    let dir = tempfile::tempdir().expect("a temporary directory");
    let file = dir.path().join("summary.csv");
    std::fs::write(&file, b"a,b,c\n").expect("written");

    let first = connected
        .store
        .put_object("reports", "summary.csv", &file, IfAbsent::Refuse)
        .await
        .expect("a free key accepts the write");
    assert!(
        matches!(first, PutOutcome::Created),
        "expected Created, got {first:?}"
    );

    let second = connected
        .store
        .put_object("reports", "summary.csv", &file, IfAbsent::Refuse)
        .await
        .expect("a taken key is an outcome, never a failure");
    assert!(
        matches!(second, PutOutcome::KeyTaken),
        "the service must be the one that refuses; got {second:?}"
    );

    // And unconditionally, it lands — because replacing is a thing the user
    // may choose, and only then.
    let third = connected
        .store
        .put_object("reports", "summary.csv", &file, IfAbsent::Replace)
        .await
        .expect("replacing is allowed when asked for");
    assert!(
        matches!(third, PutOutcome::Created),
        "expected Created, got {third:?}"
    );
}

#[tokio::test]
async fn bytes_written_come_back_the_same_and_a_delete_removes_the_row() {
    let service = Service::start().await;
    service.with_bucket("reports");
    let connected = service::Connected::to(&service).await;

    let dir = tempfile::tempdir().expect("a temporary directory");
    let file = dir.path().join("notes.md");
    // Not ASCII-only: an object is bytes, and a round trip that only ever
    // carries ASCII proves nothing about the ones that are not.
    let written = "# ghi chú\nmột hai ba — ✓\n".as_bytes();
    std::fs::write(&file, written).expect("written");

    connected
        .store
        .put_object(
            "reports",
            "notes.md",
            &file,
            caixonho_core::store::IfAbsent::Replace,
        )
        .await
        .expect("the write lands");

    let mut content = connected
        .store
        .get_object("reports", "notes.md")
        .await
        .expect("the object is readable");
    let mut read = Vec::new();
    while let Some(chunk) = content.body.next_chunk().await.expect("the stream holds") {
        read.extend(chunk);
    }
    assert_eq!(read, written, "the bytes are not the bytes");

    connected
        .store
        .delete_object("reports", "notes.md")
        .await
        .expect("the delete lands");

    let page = connected
        .store
        .list_objects(&at(""), None)
        .await
        .expect("readable");
    assert!(
        page.objects.is_empty(),
        "the row leaves because the service says so, and it did not: {:?}",
        page.objects.iter().map(|o| &o.key).collect::<Vec<_>>()
    );
}

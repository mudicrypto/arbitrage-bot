use mm_market_data::{
    book::Book,
    model::{Depth, Snapshot},
    recording::{Event, Limits, Recorder, replay, segment_paths},
};
use std::fs;

#[test]
fn bounded_segments_are_independently_deterministic_after_retention() {
    let dir = tempfile::tempdir().unwrap();
    let limits = Limits {
        segment_records: 3,
        max_segments: 2,
        ..Limits::default()
    };
    let mut recorder = Recorder::open(dir.path(), "BTCUSDT", limits.clone()).unwrap();
    assert!(Recorder::open(dir.path(), "BTCUSDT", limits).is_err());
    let snapshot =
        Snapshot::parse(r#"{"lastUpdateId":100,"bids":[["100","1"]],"asks":[["102","1"]]}"#)
            .unwrap();
    let mut book = Book::default();
    recorder
        .append(
            "session-1",
            1000,
            Some("snapshot raw".into()),
            Event::Snapshot {
                snapshot: snapshot.clone(),
            },
            &book,
        )
        .unwrap();
    book.snapshot(&snapshot, 10_000).unwrap();
    for id in 101..121 {
        let depth = Depth::parse(&format!(r#"{{"e":"depthUpdate","E":{id},"s":"BTCUSDT","U":{id},"u":{id},"b":[["100","2"]],"a":[]}}"#),"BTCUSDT").unwrap();
        recorder
            .append(
                "session-1",
                id,
                None,
                Event::AppliedDepth {
                    depth: depth.clone(),
                },
                &book,
            )
            .unwrap();
        book.apply(&depth, 10_000).unwrap();
        assert!(recorder.buffered_bytes() < 3 * 1024 * 1024);
    }
    recorder.flush().unwrap();
    let files = segment_paths(dir.path()).unwrap();
    assert_eq!(files.len(), 2);
    for path in &files {
        assert_eq!(
            replay(path, 10_000, 8 * 1024 * 1024).unwrap(),
            replay(path, 10_000, 8 * 1024 * 1024).unwrap()
        );
    }
    assert_eq!(
        replay(files.last().unwrap(), 10_000, 8 * 1024 * 1024)
            .unwrap()
            .book,
        book
    );
    let path = &files[0];
    let mut bytes = fs::read(path).unwrap();
    bytes[8] ^= 1;
    fs::write(path, bytes).unwrap();
    assert!(
        replay(path, 10_000, 8 * 1024 * 1024)
            .unwrap_err()
            .to_string()
            .contains("checksum")
    );
}
#[test]
fn recording_limits_and_filesystem_failure_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let limits = Limits {
        max_file_bytes: 1,
        ..Limits::default()
    };
    let mut recorder = Recorder::open(dir.path(), "ETHUSDT", limits).unwrap();
    let book = Book::default();
    assert!(
        recorder
            .append(
                "session",
                0,
                Some("x".repeat(3 * 1024 * 1024)),
                Event::Connected,
                &book
            )
            .is_err()
    );
    recorder
        .append("session", 0, None, Event::Connected, &book)
        .unwrap();
    assert!(recorder.flush().is_err());
    drop(recorder);
    let blocked = dir.path().join("file");
    fs::write(&blocked, "not a directory").unwrap();
    assert!(Recorder::open(&blocked, "BTCUSDT", Limits::default()).is_err());
}
#[test]
fn session_invalidation_is_replayed() {
    let dir = tempfile::tempdir().unwrap();
    let mut recorder = Recorder::open(dir.path(), "ETHUSDT", Limits::default()).unwrap();
    recorder
        .append(
            "s",
            1,
            None,
            Event::Invalidated {
                reason: "disconnect".into(),
            },
            &Book::default(),
        )
        .unwrap();
    recorder.flush().unwrap();
    let result = replay(
        &segment_paths(dir.path()).unwrap()[0],
        10_000,
        8 * 1024 * 1024,
    )
    .unwrap();
    assert_eq!(result.book, Book::default());
    assert_eq!(result.records, 2);
}

#[test]
fn retention_preserves_unrelated_local_files() {
    let dir = tempfile::tempdir().unwrap();
    let symbol = dir.path().join("BTCUSDT");
    fs::create_dir_all(&symbol).unwrap();
    fs::write(symbol.join("notes.tmp"), "user data").unwrap();
    fs::write(symbol.join("unrelated.parquet"), "user data").unwrap();
    let mut recorder = Recorder::open(
        dir.path(),
        "BTCUSDT",
        Limits {
            max_segments: 1,
            ..Limits::default()
        },
    )
    .unwrap();
    for _ in 0..3 {
        recorder
            .append("s", 1, None, Event::Connected, &Book::default())
            .unwrap();
        recorder.flush().unwrap();
    }
    assert_eq!(
        fs::read_to_string(symbol.join("notes.tmp")).unwrap(),
        "user data"
    );
    assert_eq!(
        fs::read_to_string(symbol.join("unrelated.parquet")).unwrap(),
        "user data"
    );
    assert_eq!(segment_paths(dir.path()).unwrap().len(), 2);
}

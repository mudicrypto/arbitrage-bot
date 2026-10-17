use mm_backtest::{
    data::load_symbol,
    readiness::{FrozenFile, inspect_frozen},
};
use mm_market_data::{
    book::Book,
    model::{Depth, Snapshot, Trade},
    recording::{Event, Limits, Recorder, segment_paths},
};
fn frozen(root: &std::path::Path) -> Vec<FrozenFile> {
    segment_paths(root)
        .unwrap()
        .into_iter()
        .map(|p| FrozenFile {
            bytes: std::fs::metadata(&p).unwrap().len(),
            sha256_sidecar: std::fs::read_to_string(p.with_extension("sha256")).unwrap(),
            path: p,
        })
        .collect()
}
fn snapshot() -> Snapshot {
    Snapshot::parse(r#"{"lastUpdateId":100,"bids":[["100","2"]],"asks":[["100.02","2"]]}"#).unwrap()
}
fn depth(id: u64, time: u64) -> Depth {
    Depth {
        symbol: "BTCUSDT".into(),
        first_id: id,
        last_id: id,
        exchange_ms: time,
        bids: vec![],
        asks: vec![],
    }
}
#[test]
fn frozen_replay_is_repeatable_and_identifies_freshness_and_trade_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = Recorder::open(dir.path(), "BTCUSDT", Limits::default()).unwrap();
    let mut b = Book::default();
    let s = snapshot();
    r.append(
        "s",
        1000,
        None,
        Event::Snapshot {
            snapshot: s.clone(),
        },
        &b,
    )
    .unwrap();
    b.snapshot(&s, 10000).unwrap();
    for (id, time, trade_id) in [(101, 1002, 1), (102, 4004, 3)] {
        let d = depth(id, time);
        let before = b.clone();
        b.apply(&d, 10000).unwrap();
        r.append("s", time, None, Event::AppliedDepth { depth: d }, &before)
            .unwrap();
        r.append(
            "s",
            time + 1,
            None,
            Event::Trade {
                trade: Trade {
                    symbol: "BTCUSDT".into(),
                    aggregate_id: trade_id,
                    first_id: trade_id,
                    last_id: trade_id,
                    exchange_ms: time + 1,
                    price: 10_000_000_000,
                    quantity: 100_000_000,
                    buyer_is_maker: true,
                },
            },
            &b,
        )
        .unwrap();
        r.flush().unwrap();
    }
    let files = frozen(dir.path());
    let a = inspect_frozen(&files).unwrap();
    assert_eq!(a, inspect_frozen(&files).unwrap());
    let c = &a.symbols["BTCUSDT"];
    assert!(c.prefix_complete && a.errors.is_empty());
    assert_eq!(c.record_sequence_gaps, 0);
    assert_eq!(c.book_updates, 2);
    assert_eq!(c.aggregate_trades, 2);
    assert_eq!(c.trade_id_gaps, 1);
    assert_eq!(c.book_intervals_over_2s, vec![(1002, 4004)]);
}
#[test]
fn vanished_or_modified_frozen_files_never_pass_reproducibility() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = Recorder::open(dir.path(), "BTCUSDT", Limits::default()).unwrap();
    for time in [1000, 1001, 1002] {
        r.append("s", time, None, Event::Connected, &Book::default())
            .unwrap();
        r.flush().unwrap();
    }
    let files = frozen(dir.path());
    let before = inspect_frozen(&files).unwrap();
    assert!(before.errors.is_empty());
    std::fs::remove_file(&files[1].path).unwrap();
    let missing = inspect_frozen(&files).unwrap();
    assert!(!missing.errors.is_empty());
    assert!(missing.symbols["BTCUSDT"].record_sequence_gaps > 0);
    std::fs::write(&files[0].path, b"corrupt").unwrap();
    let bad = inspect_frozen(&files).unwrap();
    assert!(bad.errors.len() >= 2);
    assert_ne!(before, bad);
}
#[test]
fn invalid_recorded_depth_sequence_cannot_produce_execution_tape() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = Recorder::open(dir.path(), "BTCUSDT", Limits::default()).unwrap();
    let mut b = Book::default();
    let s = snapshot();
    r.append(
        "s",
        1000,
        None,
        Event::Snapshot {
            snapshot: s.clone(),
        },
        &b,
    )
    .unwrap();
    b.snapshot(&s, 10000).unwrap();
    r.append(
        "s",
        1010,
        None,
        Event::AppliedDepth {
            depth: depth(102, 1010),
        },
        &b,
    )
    .unwrap();
    r.flush().unwrap();
    assert!(load_symbol(&dir.path().join("BTCUSDT"), "BTCUSDT").is_err());
}

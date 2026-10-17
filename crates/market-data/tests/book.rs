use mm_market_data::{
    book::{Applied, Book},
    model::{Depth, Snapshot, decimal, units},
};
use proptest::prelude::*;

fn snapshot() -> Snapshot {
    Snapshot::parse(
        r#"{"lastUpdateId":100,"bids":[["100","2"],["99","3"]],"asks":[["102","4"],["103","5"]]}"#,
    )
    .unwrap()
}
fn event(first: u64, last: u64, bids: &str, asks: &str) -> Depth {
    Depth::parse(&format!(r#"{{"e":"depthUpdate","E":1000,"s":"BTCUSDT","U":{first},"u":{last},"b":{bids},"a":{asks}}}"#), "BTCUSDT").unwrap()
}
#[test]
fn synchronize_overlap_delete_and_ignore_duplicates_and_old_events() {
    let mut book = Book::default();
    book.snapshot(&snapshot(), 10).unwrap();
    assert!(!book.synced);
    assert_eq!(
        book.apply(
            &event(99, 101, r#"[["100","0"],["101","1.25000000"]]"#, "[]"),
            10
        )
        .unwrap(),
        Applied::Updated
    );
    assert!(book.synced);
    assert!(!book.bids.contains_key(&units("100").unwrap()));
    assert_eq!(book.bids[&units("101").unwrap()], units("1.25").unwrap());
    let saved = book.clone();
    assert_eq!(
        book.apply(&event(99, 101, "[]", "[]"), 10).unwrap(),
        Applied::Duplicate
    );
    assert_eq!(
        book.apply(&event(90, 98, "[]", "[]"), 10).unwrap(),
        Applied::Duplicate
    );
    assert_eq!(saved, book);
    book.apply(&event(102, 102, "[]", r#"[["102","0"]]"#), 10)
        .unwrap();
    assert_eq!(
        *book.asks.first_key_value().unwrap().0,
        units("103").unwrap()
    );
}
#[test]
fn gaps_crosses_empty_books_and_limits_fail_atomically() {
    let mut book = Book::default();
    book.snapshot(&snapshot(), 10).unwrap();
    let original = book.clone();
    for bad in [
        event(102, 103, "[]", "[]"),
        event(101, 101, r#"[["103","1"]]"#, "[]"),
        event(101, 101, r#"[["100","0"],["99","0"]]"#, "[]"),
    ] {
        assert!(book.apply(&bad, 10).is_err());
        assert_eq!(book, original);
    }
    assert!(
        book.apply(&event(101, 101, r#"[["98","1"]]"#, "[]"), 2)
            .is_err()
    );
    assert_eq!(book, original);
    book.invalidate();
    assert!(!book.synced && book.bids.is_empty() && book.last_id.is_none());
}
#[test]
fn malformed_decimals_messages_and_snapshots_rejected() {
    for bad in [
        "-1",
        "NaN",
        "1e2",
        "0.000000001",
        "184467440737.09551616",
        ".1",
        "1.2.3",
    ] {
        assert!(units(bad).is_err(), "{bad}");
    }
    assert!(
        Depth::parse(
            r#"{"e":"depthUpdate","E":1,"s":"ETHUSDT","U":1,"u":2,"b":[],"a":[]}"#,
            "BTCUSDT"
        )
        .is_err()
    );
    assert!(
        Depth::parse(
            r#"{"e":"depthUpdate","E":1,"s":"BTCUSDT","U":2,"u":1,"b":[],"a":[]}"#,
            "BTCUSDT"
        )
        .is_err()
    );
    let mut bad = snapshot();
    bad.asks[0].price = units("99").unwrap();
    assert!(Book::default().snapshot(&bad, 10).is_err());
    let mut bad = snapshot();
    bad.bids.push(bad.bids[0].clone());
    assert!(Book::default().snapshot(&bad, 10).is_err());
}
#[test]
fn exhausted_snapshot_coverage_requires_rebuild() {
    let mut book = Book::default();
    book.snapshot(&snapshot(), 10).unwrap();
    assert!(
        book.apply(
            &event(101, 101, r#"[["100","0"],["99","0"],["98","1"]]"#, "[]"),
            10
        )
        .is_err()
    );
}
proptest! {
    #[test]
    fn decimal_roundtrip_is_exact(value in any::<u64>()) { prop_assert_eq!(units(&decimal(value)).unwrap(), value); }
    #[test]
    fn any_missing_sequence_is_rejected(gap in 2u64..100_000) {
        let mut book = Book::default(); book.snapshot(&snapshot(),10).unwrap();
        prop_assert!(book.apply(&event(100+gap,101+gap,"[]","[]"),10).is_err());
    }
}

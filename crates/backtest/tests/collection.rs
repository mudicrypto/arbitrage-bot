use mm_backtest::collection::CollectionConfig;
use mm_market_data::{
    book::Book,
    recording::{Event, Limits, Recorder, replay, segment_paths},
};
#[test]
fn byte_retention_is_bounded_across_restarts_and_preserves_replayable_checkpoints() {
    let dir = tempfile::tempdir().unwrap();
    let limits = Limits {
        max_segments: 100,
        max_file_bytes: 4096,
        max_total_bytes: Some(8192),
        ..Limits::default()
    };
    for _ in 0..2 {
        let mut recorder = Recorder::open(dir.path(), "BTCUSDT", limits.clone()).unwrap();
        for _ in 0..20 {
            recorder
                .append("session", 1, None, Event::Connected, &Book::default())
                .unwrap();
            recorder.flush().unwrap();
        }
    }
    let files = segment_paths(dir.path()).unwrap();
    let bytes: u64 = files
        .iter()
        .map(|p| std::fs::metadata(p).unwrap().len())
        .sum();
    assert!(bytes <= 8192);
    for file in files {
        replay(&file, 10_000, 4096).unwrap();
    }
}
#[test]
fn long_collection_is_explicitly_bounded_and_not_required_by_tests() {
    let c: CollectionConfig =
        serde_json::from_str(include_str!("../../../config/long_collection.json")).unwrap();
    c.validate().unwrap();
    assert_eq!(c.duration_seconds, 86400);
    let mut bad = c.clone();
    bad.duration_seconds = 259201;
    assert!(bad.validate().is_err());
    let mut bad = c;
    bad.symbols.push("../../BTCUSDT".into());
    assert!(bad.validate().is_err());
}

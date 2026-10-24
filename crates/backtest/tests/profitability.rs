//! Synthetic orchestration fixtures, never market observations or profitability evidence.
use mm_backtest::profitability::{Protocol, run};
use mm_market_data::{
    book::Book,
    model::{Depth, Snapshot, Trade},
    recording::{Event, Limits, Recorder, segment_paths},
};
use serde_json::{Value, json};
use std::path::Path;
fn fixture(root: &Path) {
    std::fs::create_dir_all(root).unwrap();
    let mut r = Recorder::open(
        &root.join("recordings"),
        "UNITUSDT",
        Limits {
            max_segments: 1000,
            segment_records: 4096,
            ..Default::default()
        },
    )
    .unwrap();
    let snapshot=Snapshot::parse(r#"{"lastUpdateId":100,"bids":[["100","2"],["99","10"]],"asks":[["100.02","2"],["101","10"]]}"#).unwrap();
    let mut b = Book::default();
    r.append(
        "synthetic",
        999990,
        None,
        Event::Snapshot {
            snapshot: snapshot.clone(),
        },
        &b,
    )
    .unwrap();
    b.snapshot(&snapshot, 10000).unwrap();
    for i in 0..900 {
        let time = 1_000_000 + i * 1000;
        let d = Depth {
            symbol: "UNITUSDT".into(),
            first_id: 101 + i,
            last_id: 101 + i,
            exchange_ms: time,
            bids: vec![],
            asks: vec![],
        };
        let before = b.clone();
        b.apply(&d, 10000).unwrap();
        r.append(
            "synthetic",
            time,
            None,
            Event::AppliedDepth { depth: d },
            &before,
        )
        .unwrap();
        if i % 5 == 0 {
            let id = i / 5 + 1;
            r.append(
                "synthetic",
                time + 100,
                None,
                Event::Trade {
                    trade: Trade {
                        symbol: "UNITUSDT".into(),
                        aggregate_id: id,
                        first_id: id,
                        last_id: id,
                        exchange_ms: time + 100,
                        price: 10000000000,
                        quantity: 30000000,
                        buyer_is_maker: true,
                    },
                },
                &b,
            )
            .unwrap();
        }
    }
    r.flush().unwrap();
    drop(r);
    let instrument = json!({"symbol":"UNITUSDT","base_asset":"UNIT","price_units":10001000000u64,"tick_units":1000000,"step_units":100000,"min_qty_units":100000,"max_qty_units":100000000000u64,"min_notional_units":500000000,"max_notional_units":null,"clip_quantity_units":99900000,"clip_notional_units":9990999000u64,"volume_24h_usdt":100000000.0,"trades_24h":100000});
    let research: Value =
        serde_json::from_str(include_str!("../../../config/research.json")).unwrap();
    std::fs::write(root.join("manifest.json"),serde_json::to_vec(&json!({"config":research,"instruments":[instrument],"bounds":{"UNITUSDT":[1000000,1900000]}})).unwrap()).unwrap();
    std::fs::write(
        root.join("collection_finished.json"),
        br#"{"reason":"duration_complete","fixture_type":"synthetic_only"}"#,
    )
    .unwrap();
    let info = json!({"symbols":[{"symbol":"UNITUSDT","baseAsset":"UNIT","quoteAsset":"USDT","status":"TRADING","isSpotTradingAllowed":true,"filters":[{"filterType":"PRICE_FILTER","minPrice":"0.01","maxPrice":"1000000","tickSize":"0.01"},{"filterType":"LOT_SIZE","minQty":"0.001","maxQty":"1000","stepSize":"0.001"},{"filterType":"MIN_NOTIONAL","minNotional":"5"}]}]});
    std::fs::write(
        root.join("exchange-info.json"),
        serde_json::to_vec(&info).unwrap(),
    )
    .unwrap();
}
fn protocol(root: &Path) -> std::path::PathBuf {
    let mut p: Protocol =
        serde_json::from_str(include_str!("../../../config/milestone6.json")).unwrap();
    p.symbols = vec!["UNITUSDT".into()];
    p.frozen_at_ms = 1;
    let path = root.join("protocol.json");
    std::fs::write(&path, serde_json::to_vec(&p).unwrap()).unwrap();
    path
}
#[test]
fn chronological_study_reuses_recording_and_is_byte_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fixture(&source);
    let p = protocol(dir.path());
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    run(&source, &p, &a).unwrap();
    run(&source, &p, &b).unwrap();
    assert_eq!(
        std::fs::read(a.join("results.json")).unwrap(),
        std::fs::read(b.join("results.json")).unwrap()
    );
    assert_eq!(
        std::fs::read(a.join("execution_traces.jsonl")).unwrap(),
        std::fs::read(b.join("execution_traces.jsonl")).unwrap()
    );
    let report: Value =
        serde_json::from_slice(&std::fs::read(a.join("results.json")).unwrap()).unwrap();
    let rows = report["results"].as_array().unwrap();
    assert_eq!(rows.len(), 72);
    assert!(
        rows.iter()
            .any(|r| r["strategy"] == "research_touch_control" && r["fills"].as_u64().unwrap() > 0)
    );
    assert!(
        rows.iter().all(
            |r| r["actual_exchange_executions"] == 0 && r["eligible_positive_oos_row"] == false
        )
    );
    assert!(a.join("complete.json").exists() && !a.join("INCOMPLETE").exists());
}
#[test]
fn incomplete_and_corrupt_capture_never_produce_valid_results() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fixture(&source);
    let p = protocol(dir.path());
    std::fs::remove_file(source.join("collection_finished.json")).unwrap();
    assert!(run(&source, &p, &dir.path().join("a")).is_err());
    std::fs::write(
        source.join("collection_finished.json"),
        br#"{"reason":"duration_complete"}"#,
    )
    .unwrap();
    let paths = segment_paths(&source.join("recordings")).unwrap();
    std::fs::write(&paths[0], b"corrupt synthetic source").unwrap();
    let out = dir.path().join("b");
    assert!(run(&source, &p, &out).is_err());
    assert!(out.join("INCOMPLETE").exists() && !out.join("complete.json").exists());
}

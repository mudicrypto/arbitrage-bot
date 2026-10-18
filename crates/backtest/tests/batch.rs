use mm_backtest::{
    batch::{Config, Source, run},
    config::Experiment,
};
use mm_market_data::{
    book::Book,
    model::{Depth, Snapshot, Trade, units},
    recording::{Event, Limits, Recorder},
};
use mm_research::{
    discovery::Instrument,
    run::{Manifest, ResearchConfig},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
fn p(s: &str) -> u64 {
    units(s).unwrap()
}
fn fixture(root: &Path) -> PathBuf {
    let source = root.join("source");
    std::fs::create_dir_all(&source).unwrap();
    let mut recorder = Recorder::open(
        &source.join("recordings"),
        "UNITUSDT",
        Limits {
            segment_records: 4096,
            max_segments: 100,
            ..Default::default()
        },
    )
    .unwrap();
    let mut book = Book::default();
    let snapshot=Snapshot::parse(r#"{"lastUpdateId":100,"bids":[["100","2"],["99","10"]],"asks":[["100.02","2"],["101.02","10"]]}"#).unwrap();
    recorder
        .append(
            "session",
            99990,
            None,
            Event::Snapshot {
                snapshot: snapshot.clone(),
            },
            &book,
        )
        .unwrap();
    book.snapshot(&snapshot, 10000).unwrap();
    for i in 0..20 {
        let time = 100000 + i * 1000;
        let depth = Depth {
            symbol: "UNITUSDT".into(),
            first_id: 101 + i,
            last_id: 101 + i,
            exchange_ms: time,
            bids: vec![],
            asks: vec![],
        };
        let before = book.clone();
        book.apply(&depth, 10000).unwrap();
        recorder
            .append(
                "session",
                time,
                None,
                Event::AppliedDepth { depth },
                &before,
            )
            .unwrap();
        if i == 0 {
            for (id, t, q) in [(1, 100020, "0.4"), (2, 100040, "0.6")] {
                recorder
                    .append(
                        "session",
                        t,
                        None,
                        Event::Trade {
                            trade: Trade {
                                symbol: "UNITUSDT".into(),
                                aggregate_id: id,
                                first_id: id,
                                last_id: id,
                                exchange_ms: t,
                                price: p("99.98"),
                                quantity: p(q),
                                buyer_is_maker: true,
                            },
                        },
                        &book,
                    )
                    .unwrap();
            }
        }
    }
    recorder.flush().unwrap();
    drop(recorder);
    let instrument = Instrument {
        symbol: "UNITUSDT".into(),
        base_asset: "UNIT".into(),
        price_units: p("100.01"),
        tick_units: p("0.01"),
        step_units: p("0.001"),
        min_qty_units: p("0.001"),
        max_qty_units: p("1000"),
        min_notional_units: p("5"),
        max_notional_units: None,
        clip_quantity_units: p("0.999"),
        clip_notional_units: 9990999000,
        volume_24h_usdt: 1e8,
        trades_24h: 100000,
    };
    let research: ResearchConfig =
        serde_json::from_str(include_str!("../../../config/research.json")).unwrap();
    std::fs::write(
        source.join("manifest.json"),
        serde_json::to_vec(&Manifest {
            config: research,
            instruments: vec![instrument],
            bounds: BTreeMap::from([("UNITUSDT".into(), (100000, 120000))]),
        })
        .unwrap(),
    )
    .unwrap();
    std::fs::write(source.join("collection_finished.json"),r#"{"reason":"synthetic_fixture_completed","start_ms":100000,"end_ms":120000,"observed_seconds":20}"#).unwrap();
    let mut e: Experiment =
        serde_json::from_str(include_str!("../../../config/backtest.json")).unwrap();
    e.scenarios.truncate(1);
    let s = &mut e.scenarios[0];
    s.hypothetical = true;
    s.strategy.spread_bps = 2;
    s.strategy.required_edge_bps = 0;
    s.strategy.inventory_skew_bps = 0;
    s.strategy.refresh_ms = 100;
    s.entry_latency_ms = 1;
    s.cancel_latency_ms = 1;
    s.latency_jitter_ms = 0;
    s.fees.periods[0].standard_maker_microbps = 0;
    let experiment = root.join("experiment.json");
    std::fs::write(&experiment, serde_json::to_vec(&e).unwrap()).unwrap();
    let metadata = root.join("metadata.json");
    std::fs::write(&metadata,serde_json::to_vec(&serde_json::json!({"result_type":"synthetic_metadata_fixture","exchange_info":{"symbols":[{"symbol":"UNITUSDT","baseAsset":"UNIT","quoteAsset":"USDT","status":"TRADING","isSpotTradingAllowed":true,"filters":[{"filterType":"PRICE_FILTER","minPrice":"0.01","maxPrice":"1000000","tickSize":"0.01"},{"filterType":"LOT_SIZE","minQty":"0.001","maxQty":"1000","stepSize":"0.001"},{"filterType":"MIN_NOTIONAL","minNotional":"5"}]}]}})).unwrap()).unwrap();
    let config = Config {
        capital_scope: "isolated_alternatives_5000_each_do_not_sum".into(),
        shared_market_capital_usdt: BTreeMap::new(),
        sources: vec![Source {
            root: source,
            legacy_completed_research: false,
        }],
        experiment,
        current_metadata: metadata,
        base_allocations_ppm: vec![0, 200000, 500000, 800000],
        execution_models: vec![
            mm_backtest::portfolio::ExecutionModel::ExactPrice,
            mm_backtest::portfolio::ExecutionModel::ConservativeThrough,
        ],
        scenario_names: vec!["standard_conservative".into()],
        fee_buffer_ppm: 100000,
        max_output_bytes: 16 * 1024 * 1024,
    };
    let path = root.join("batch.json");
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    path
}
#[test]
fn batch_replay_conserves_equity_has_known_fills_and_is_byte_deterministic() {
    let root = tempfile::tempdir().unwrap();
    let config = fixture(root.path());
    let a = root.path().join("a");
    let b = root.path().join("b");
    run(&config, &a).unwrap();
    run(&config, &b).unwrap();
    assert_eq!(
        std::fs::read(a.join("comparisons.json")).unwrap(),
        std::fs::read(b.join("comparisons.json")).unwrap()
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(a.join("comparisons.json")).unwrap()).unwrap();
    let rows = report["results"].as_array().unwrap();
    assert_eq!(rows.len(), 8);
    for row in rows {
        assert_eq!(row["accounting"]["reconciled"], true);
        assert_eq!(row["initialization"]["initial_equity_atoms"], p("5000"));
        if row["execution_model"] == "exact_price" {
            assert_eq!(row["fill_count"], 0);
        } else {
            assert_eq!(row["partial_fill_events"], 1);
            assert_eq!(row["complete_fill_events"], 1);
            assert_eq!(row["through_fill_events"], 2);
        }
        assert_eq!(row["data_quality_valid"], true);
    }
    for row in rows {
        let model = if row["execution_model"] == "exact_price" {
            "ExactPrice"
        } else {
            "ConservativeThrough"
        };
        let name = format!(
            "0-UNITUSDT-standard_conservative-{}-{model}.jsonl",
            row["base_allocation_ppm"].as_u64().unwrap()
        );
        let text = std::fs::read_to_string(a.join("traces").join(name)).unwrap();
        let mut sums: BTreeMap<String, i64> = BTreeMap::new();
        let mut deposits = 0;
        for line in text.lines() {
            let record: serde_json::Value = serde_json::from_str(line).unwrap();
            if record["kind"] == "ledger" {
                let value = &record["value"];
                *sums
                    .entry(value["asset"].as_str().unwrap().into())
                    .or_default() += value["delta"].as_i64().unwrap();
                deposits += u64::from(value["reason"] == "simulated_initial_capital");
                assert_ne!(value["reason"], "reconciliation_checkpoint_not_deposit");
            }
        }
        assert_eq!(deposits, 1);
        assert_eq!(
            sums["USDT"],
            row["end_balances"]["quote_total"].as_i64().unwrap()
        );
        assert_eq!(
            sums.get("UNIT").copied().unwrap_or(0),
            row["end_balances"]["base_total"].as_i64().unwrap()
        );
    }
    for entry in std::fs::read_dir(a.join("traces")).unwrap() {
        let p = entry.unwrap().path();
        assert_eq!(
            std::fs::read(&p).unwrap(),
            std::fs::read(b.join("traces").join(p.file_name().unwrap())).unwrap()
        );
    }
    assert!(a.join("batch_complete.json").exists() && !a.join("INCOMPLETE").exists());
}
#[test]
fn incomplete_capture_is_rejected_without_strategy_evaluation() {
    let root = tempfile::tempdir().unwrap();
    let config = fixture(root.path());
    std::fs::remove_file(root.path().join("source/collection_finished.json")).unwrap();
    let out = root.path().join("out");
    run(&config, &out).unwrap();
    let r: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("comparisons.json")).unwrap()).unwrap();
    assert!(r["results"].as_array().unwrap().is_empty());
    assert_eq!(
        r["rejected_sessions_or_symbols"][0]["reason"],
        "capture_active_or_completion_unverified"
    );
}
#[test]
fn corrupt_input_is_rejected_and_ambiguous_capital_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let config = fixture(root.path());
    let mut parsed: Config = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    parsed.capital_scope = "shared_without_allocations".into();
    assert!(parsed.validate().is_err());
    let files =
        mm_market_data::recording::segment_paths(&root.path().join("source/recordings")).unwrap();
    std::fs::write(&files[0], b"corrupt").unwrap();
    let out = root.path().join("out");
    run(&config, &out).unwrap();
    let r: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("comparisons.json")).unwrap()).unwrap();
    assert!(r["results"].as_array().unwrap().is_empty());
    assert_eq!(
        r["rejected_sessions_or_symbols"][0]["reason"],
        "checksum_or_read_failure"
    );
}
#[test]
fn declared_completed_session_with_missing_tail_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let config = fixture(root.path());
    let path = root.path().join("source/manifest.json");
    let mut m: Manifest = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    m.bounds.get_mut("UNITUSDT").unwrap().1 = 200000;
    std::fs::write(path, serde_json::to_vec(&m).unwrap()).unwrap();
    let out = root.path().join("out");
    run(&config, &out).unwrap();
    let r: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("comparisons.json")).unwrap()).unwrap();
    assert!(r["results"].as_array().unwrap().is_empty());
    assert_eq!(
        r["rejected_sessions_or_symbols"][0]["reason"],
        "incomplete_recording_tail"
    );
}

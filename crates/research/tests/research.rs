use mm_market_data::{
    book::Book,
    model::{Depth, Snapshot, Trade, units},
    recording::{Event, Limits, Recorder},
};
use mm_research::{
    discovery::{Policy, discover, eligible},
    fees::{FeeAsset, FeePeriod, FeeScenario, ceil_div, notional},
    report::{Economics, net_edge, rank, sort_rankings},
    run::{ResearchConfig, replay_series},
    stats::{Series, bps, horizon, quantile, sample},
};
use proptest::prelude::*;
use serde_json::json;
fn period() -> FeePeriod {
    FeePeriod {
        from_ms: 0,
        until_ms: None,
        symbol: None,
        standard_maker_microbps: 10_000_000,
        standard_taker_microbps: 20_000_000,
        standard_buy_microbps: 0,
        standard_sell_microbps: 0,
        buy_asset: None,
        sell_asset: None,
        buy_extra_microbps: 1_000_000,
        sell_extra_microbps: 2_000_000,
        bnb_discount_ppm: 0,
        asset: FeeAsset::Quote,
        bnb_quote_price_units: None,
    }
}
fn scenario() -> FeeScenario {
    FeeScenario {
        name: "manual".into(),
        account_verified: false,
        description: "test manual assumption".into(),
        periods: vec![period()],
    }
}
fn book() -> Book {
    let s=Snapshot::parse(r#"{"lastUpdateId":100,"bids":[["100.00","10"],["99.99","20"],["99.90","50"]],"asks":[["100.02","15"],["100.03","25"],["100.10","60"]]}"#).unwrap();
    let mut book = Book::default();
    book.snapshot(&s, 10_000).unwrap();
    book.apply(
        &Depth {
            symbol: "BTCUSDT".into(),
            first_id: 101,
            last_id: 101,
            exchange_ms: 0,
            bids: vec![],
            asks: vec![],
        },
        10_000,
    )
    .unwrap();
    book
}
fn metadata() -> serde_json::Value {
    json!({"symbol":"TESTUSDT","baseAsset":"TEST","quoteAsset":"USDT","status":"TRADING","isSpotTradingAllowed":true,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.01"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"100000"},{"filterType":"NOTIONAL","minNotional":"5","maxNotional":"100000"}]})
}
fn ticker() -> serde_json::Value {
    json!({"symbol":"TESTUSDT","lastPrice":"100","quoteVolume":"10000000","count":100000})
}
fn series() -> Series {
    let mut s = Series {
        start_ms: 0,
        end_ms: 60_000,
        ..Series::default()
    };
    for i in 0..60 {
        s.observe_book(&book(), i * 1000, i * 1000);
        s.observe_trade(
            &Trade {
                symbol: "TESTUSDT".into(),
                aggregate_id: i,
                first_id: i * 2,
                last_id: i * 2 + 1,
                exchange_ms: i * 1000,
                price: units("100.01").unwrap(),
                quantity: units("1").unwrap(),
                buyer_is_maker: false,
            },
            i * 1000,
        );
    }
    s
}
#[test]
fn spread_depth_imbalance_and_bps_are_exact() {
    let s = sample(&book(), 1000, 1000).unwrap();
    assert!((s.spread_bps - 1.999800019998).abs() < 1e-8);
    assert!((s.depth_usdt[0][0] - 2999.8).abs() < 1e-8);
    assert!((s.depth_usdt[0][1] - 4001.05).abs() < 1e-8);
    assert!((s.imbalance - (2999.8 - 10007.05) / (2999.8 + 10007.05)).abs() < 1e-8);
    assert_eq!(bps(1, 10000).unwrap(), 1.0);
    assert!(bps(1, 0).is_err());
    assert_eq!(notional(100_000_000, 100_000_000).unwrap(), 100_000_000);
}
#[test]
fn fees_handle_sides_assets_discount_and_rounding() {
    let mut p = period();
    let price = units("100").unwrap();
    let qty = units("2").unwrap();
    assert_eq!(
        p.charge(price, qty, true, true).unwrap().amount_atoms,
        22_000_000
    );
    assert_eq!(
        p.charge(price, qty, false, true).unwrap().amount_atoms,
        24_000_000
    );
    p.asset = FeeAsset::Base;
    assert_eq!(
        p.charge(price, qty, true, true).unwrap().amount_atoms,
        220_000
    );
    p.asset = FeeAsset::Bnb;
    p.bnb_discount_ppm = 250_000;
    p.bnb_quote_price_units = Some(units("1000").unwrap());
    assert_eq!(p.rate(true, true).unwrap(), 8_500_000);
    assert_eq!(
        p.charge(price, qty, true, true).unwrap().amount_atoms,
        17_000
    );
    assert_eq!(ceil_div(1, 100).unwrap(), 1);
    assert!(ceil_div(1, 0).is_err());
    let mut bad = scenario();
    bad.periods[0].asset = FeeAsset::Bnb;
    assert!(bad.validate().is_err());
    bad.account_verified = true;
    assert!(bad.validate().is_err());
}
#[test]
fn fee_changes_and_symbol_specific_overrides_are_time_exact() {
    let mut f = scenario();
    f.periods[0].until_ms = Some(1000);
    let mut later = period();
    later.from_ms = 1000;
    later.standard_maker_microbps = 0;
    f.periods.push(later);
    let mut specific = period();
    specific.symbol = Some("BTCUSDT".into());
    specific.standard_maker_microbps = 2_000_000;
    f.periods.push(specific);
    f.validate().unwrap();
    assert_eq!(
        f.at(999, "ETHUSDT").unwrap().standard_maker_microbps,
        10_000_000
    );
    assert_eq!(f.at(1000, "ETHUSDT").unwrap().standard_maker_microbps, 0);
    assert_eq!(
        f.at(1000, "BTCUSDT").unwrap().standard_maker_microbps,
        2_000_000
    );
    assert!(f.at(0, "ETHUSDT").is_ok());
    f.periods.remove(0);
    assert!(f.at(999, "ETHUSDT").is_err());
}
#[test]
fn eligibility_rejects_inactive_illiquid_bad_tick_lots_and_costly_minimums() {
    let policy = Policy::default();
    assert!(eligible(&metadata(), &ticker(), &policy).is_ok());
    for mutate in 0..6 {
        let mut m = metadata();
        let mut t = ticker();
        match mutate {
            0 => m["status"] = json!("BREAK"),
            1 => m["isSpotTradingAllowed"] = json!(false),
            2 => t["quoteVolume"] = json!("10"),
            3 => m["filters"][0]["tickSize"] = json!("10"),
            4 => m["filters"][1]["minQty"] = json!("10"),
            _ => m["filters"][2]["minNotional"] = json!("90"),
        };
        assert!(eligible(&m, &t, &policy).is_err());
    }
    let info = json!({"symbols":[metadata()]});
    let tickers = json!([ticker()]);
    assert_eq!(discover(&info, &tickers, &policy, 1).unwrap().0.len(), 1);
    assert!(discover(&info, &tickers, &policy, 21).is_err());
}
#[test]
fn invalid_stale_missing_books_and_histories_are_not_valid_observations() {
    assert!(sample(&Book::default(), 0, 0).is_err());
    assert!(sample(&book(), 6000, 0).is_err());
    let empty = Series::default();
    assert!(horizon(&empty, 60).is_none());
    let dir = tempfile::tempdir().unwrap();
    assert!(replay_series(dir.path(), &std::collections::BTreeMap::new()).is_err());
    let instrument = eligible(&metadata(), &ticker(), &Policy::default()).unwrap();
    let ranked = rank(
        &instrument,
        &empty,
        &Policy::default(),
        &Economics::default(),
        &[scenario()],
    )
    .unwrap();
    assert!(
        ranked.baseline_net_bps.is_none()
            && !ranked.plausible_positive_edge
            && !ranked.eligible_for_further_research
    );
}
#[test]
fn robust_statistics_and_ranking_are_deterministic_and_never_create_profit() {
    assert_eq!(quantile(&[1., 1., 1., 1., 1_000_000.], 0.5), Some(1.));
    let data = series();
    let w = horizon(&data, 60).unwrap();
    assert_eq!(w.samples, 60);
    assert_eq!(w.underlying_trade_count, 120);
    assert!(w.quote_stability_fraction > 0.99);
    let instrument = eligible(&metadata(), &ticker(), &Policy::default()).unwrap();
    let a = rank(
        &instrument,
        &data,
        &Policy::default(),
        &Economics::default(),
        &[scenario()],
    )
    .unwrap();
    let b = rank(
        &instrument,
        &data,
        &Policy::default(),
        &Economics::default(),
        &[scenario()],
    )
    .unwrap();
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
    assert!(
        a.baseline_net_bps.unwrap() < 0.
            && !a.plausible_positive_edge
            && !a.executable_edge_validated
    );
    let mut rows = vec![b, a];
    sort_rankings(&mut rows);
    assert_eq!(rows[0].rank, 1);
    assert_eq!(rows[1].rank, 2);
    assert_eq!(net_edge(20., 10., 3., 2., 3., 2.), 0.);
    assert!(net_edge(1., 20., 0., 0., 0., 0.) < 0.);
}
#[test]
fn research_observations_roundtrip_through_existing_parquet_replay() {
    let dir = tempfile::tempdir().unwrap();
    let mut recorder = Recorder::open(dir.path(), "BTCUSDT", Limits::default()).unwrap();
    let snapshot =
        Snapshot::parse(r#"{"lastUpdateId":100,"bids":[["100","10"]],"asks":[["100.02","10"]]}"#)
            .unwrap();
    let mut b = Book::default();
    recorder
        .append(
            "s",
            1000,
            None,
            Event::Snapshot {
                snapshot: snapshot.clone(),
            },
            &b,
        )
        .unwrap();
    b.snapshot(&snapshot, 10_000).unwrap();
    let depth = Depth {
        symbol: "BTCUSDT".into(),
        first_id: 101,
        last_id: 101,
        exchange_ms: 1100,
        bids: vec![],
        asks: vec![],
    };
    recorder
        .append(
            "s",
            1100,
            None,
            Event::AppliedDepth {
                depth: depth.clone(),
            },
            &b,
        )
        .unwrap();
    b.apply(&depth, 10_000).unwrap();
    let trade = Trade {
        symbol: "BTCUSDT".into(),
        aggregate_id: 1,
        first_id: 1,
        last_id: 2,
        exchange_ms: 1200,
        price: units("100.01").unwrap(),
        quantity: units("1").unwrap(),
        buyer_is_maker: false,
    };
    recorder
        .append(
            "s",
            1200,
            None,
            Event::Trade {
                trade: trade.clone(),
            },
            &b,
        )
        .unwrap();
    recorder.flush().unwrap();
    let bounds = std::collections::BTreeMap::from([("BTCUSDT".into(), (1000, 2000))]);
    let replay = replay_series(dir.path(), &bounds).unwrap();
    let mut online = Series {
        start_ms: 1000,
        end_ms: 2000,
        ..Series::default()
    };
    online.observe_book(&b, 1100, 1100);
    online.observe_trade(&trade, 1200);
    assert_eq!(replay["BTCUSDT"], online);
}
#[test]
fn manual_configuration_is_valid_and_bounded() {
    let c: ResearchConfig =
        serde_json::from_str(include_str!("../../../config/research.json")).unwrap();
    c.validate().unwrap();
    let mut bad = c;
    bad.universe_limit = 21;
    assert!(bad.validate().is_err());
}
proptest! {
 #[test]
 fn exact_fixed_point_notional(price in 1u64..1_000_000_000_000,qty in 1u64..1_000_000_000_000) {prop_assert_eq!(notional(price,qty).unwrap(),u128::from(price)*u128::from(qty)/100_000_000);}
 #[test]
 fn extra_cost_never_improves_edge(gross in 0f64..100.,cost in 0f64..100.,extra in 0f64..100.) {prop_assert!(net_edge(gross,cost+extra,1.,1.,1.,1.)<=net_edge(gross,cost,1.,1.,1.,1.)+1e-10);}
}

#[test]
fn per_side_fee_assets_and_standard_components_are_preserved() {
    let mut p = period();
    p.buy_asset = Some(FeeAsset::Base);
    p.sell_asset = Some(FeeAsset::Quote);
    p.standard_buy_microbps = 2_000_000;
    p.standard_sell_microbps = 3_000_000;
    let buy = p
        .charge(units("100").unwrap(), units("1").unwrap(), true, true)
        .unwrap();
    let sell = p
        .charge(units("100").unwrap(), units("1").unwrap(), false, true)
        .unwrap();
    assert_eq!(buy.asset, FeeAsset::Base);
    assert_eq!(buy.amount_atoms, 130_000);
    assert_eq!(sell.asset, FeeAsset::Quote);
    assert_eq!(sell.amount_atoms, 15_000_000);
    p.buy_asset = Some(FeeAsset::Bnb);
    p.bnb_quote_price_units = Some(units("1000").unwrap());
    p.bnb_discount_ppm = 250_000;
    assert_eq!(p.rate(true, true).unwrap(), 10_000_000); // (10+2)*.75 + tax/special 1
    assert_eq!(p.rate(false, true).unwrap(), 15_000_000); // no BNB discount on quote-paid side
}
#[test]
fn truncated_windows_and_trade_gaps_are_explicit() {
    let mut s = series();
    let w = horizon(&s, 600).unwrap();
    assert!(!w.requested_window_complete);
    assert_eq!(w.observed_duration_seconds, 60.);
    let t = Trade {
        symbol: "TESTUSDT".into(),
        aggregate_id: 1000,
        first_id: 1000,
        last_id: 1000,
        exchange_ms: 59000,
        price: units("100").unwrap(),
        quantity: units("1").unwrap(),
        buyer_is_maker: true,
    };
    s.observe_trade(&t, 59000);
    assert_eq!(s.aggregate_gaps, 1);
    s.observe_trade(&t, 59000);
    assert_eq!(s.duplicate_trades, 1);
    let i = eligible(&metadata(), &ticker(), &Policy::default()).unwrap();
    let r = rank(
        &i,
        &s,
        &Policy::default(),
        &Economics::default(),
        &[scenario()],
    )
    .unwrap();
    assert!(!r.eligible_for_further_research && !r.plausible_positive_edge);
}
#[test]
fn sample_history_survives_json_persistence_exactly() {
    let s = series();
    let encoded = serde_json::to_vec(&s).unwrap();
    let decoded: Series = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(s, decoded);
}

#[test]
fn single_wide_spread_outlier_cannot_manufacture_a_winner() {
    let mut s = series();
    s.samples[30].spread_bps = 1000.;
    s.samples[31].concentration = 0.99;
    let i = eligible(&metadata(), &ticker(), &Policy::default()).unwrap();
    let r = rank(
        &i,
        &s,
        &Policy::default(),
        &Economics::default(),
        &[scenario()],
    )
    .unwrap();
    assert!(
        r.gross_theoretical_bps.unwrap() < 3.
            && r.baseline_net_bps.unwrap() < 0.
            && !r.plausible_positive_edge
    );
    for p in &mut s.samples {
        p.concentration = 0.99;
    }
    let r = rank(
        &i,
        &s,
        &Policy::default(),
        &Economics::default(),
        &[scenario()],
    )
    .unwrap();
    assert!(r.exclusions.iter().any(|s| s.contains("concentration")));
}

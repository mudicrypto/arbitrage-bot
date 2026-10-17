use mm_backtest::{
    config::{Experiment, QueueModel, Scenario},
    data::{Input, Tick, View},
    engine::{Engine, S},
    experiment::simulate,
};
use mm_market_data::model::{Trade, units};
use mm_research::{discovery::Instrument, fees::FeeAsset};
use proptest::prelude::*;
fn p(v: &str) -> u64 {
    units(v).unwrap()
}
fn config() -> Experiment {
    serde_json::from_str(include_str!("../../../config/backtest.json")).unwrap()
}
fn scenario() -> Scenario {
    let mut s = config().scenarios.remove(0);
    s.entry_latency_ms = 10;
    s.cancel_latency_ms = 20;
    s.latency_jitter_ms = 0;
    s.report_latency_ms = 500;
    s.strategy.refresh_ms = 60_000;
    s.strategy.spread_bps = 2;
    s.strategy.required_edge_bps = 0;
    s.strategy.stale_ms = 2000;
    for period in &mut s.fees.periods {
        period.standard_maker_microbps = 0;
        period.standard_taker_microbps = 0;
    }
    s
}
fn instrument() -> Instrument {
    Instrument {
        symbol: "TESTUSDT".into(),
        base_asset: "TEST".into(),
        price_units: p("100"),
        tick_units: p("0.01"),
        step_units: p("0.001"),
        min_qty_units: p("0.001"),
        max_qty_units: p("1000"),
        min_notional_units: p("5"),
        max_notional_units: None,
        clip_quantity_units: p("1"),
        clip_notional_units: p("100").into(),
        volume_24h_usdt: 1e8,
        trades_24h: 100000,
    }
}
fn book(time: u64, bid: &str, ask: &str) -> Tick {
    Tick {
        time_ms: time,
        sequence: time,
        input: Input::Book {
            view: View {
                bids: vec![(p(bid), p("2")), (p(bid) - p("1"), p("10"))],
                asks: vec![(p(ask), p("2")), (p(ask) + p("1"), p("10"))],
            },
            exchange_ms: time,
        },
    }
}
fn trade(receive: u64, exchange: u64, price: &str, qty: &str, sell_aggressor: bool) -> Tick {
    Tick {
        time_ms: receive,
        sequence: receive,
        input: Input::Trade {
            trade: Trade {
                symbol: "TESTUSDT".into(),
                aggregate_id: receive,
                first_id: receive,
                last_id: receive,
                exchange_ms: exchange,
                price: p(price),
                quantity: p(qty),
                buyer_is_maker: sell_aggressor,
            },
        },
    }
}
fn engine() -> Engine {
    let mut e = Engine::new(scenario(), instrument(), 5000).unwrap();
    e.step(&book(1000, "100", "100.02")).unwrap();
    e
}
#[test]
fn exact_price_trade_evidence_queue_depletion_and_partial_fills() {
    let mut e = engine();
    assert!(e.submit(true, p("100"), p("1")).unwrap());
    e.step(&book(1011, "100", "100.02")).unwrap();
    assert_eq!(e.orders[0].ahead, u128::from(p("2")));
    e.step(&trade(1020, 1020, "100", "2.4", true)).unwrap();
    assert_eq!(e.fills.len(), 1);
    assert_eq!(e.fills[0].quantity, p("0.4"));
    assert_eq!(e.orders[0].remaining, p("0.6"));
    e.step(&trade(1030, 1030, "100", "0.6", true)).unwrap();
    assert_eq!(e.base, p("1") as i128);
    assert!(e.orders.is_empty());
    e.reconcile().unwrap();
}
#[test]
fn touch_depth_deletion_and_trade_at_other_price_never_fill() {
    let mut e = engine();
    e.submit(true, p("100"), p("1")).unwrap();
    e.step(&book(1011, "100", "100.02")).unwrap();
    let mut changed = book(1020, "100", "100.02");
    if let Input::Book { view, .. } = &mut changed.input {
        view.bids[0].0 = p("100.01");
    }
    e.step(&changed).unwrap();
    assert!(e.fills.is_empty());
    e.step(&trade(1030, 1030, "99.99", "100", true)).unwrap();
    assert!(e.fills.is_empty());
    assert_eq!(e.orders[0].ahead, p("2") as u128);
}
#[test]
fn cancellation_advancement_is_an_alternative_not_a_fill() {
    let mut e = Engine::new(
        {
            let mut s = scenario();
            s.queue = QueueModel::ProportionalCancellation;
            s
        },
        instrument(),
        5000,
    )
    .unwrap();
    e.step(&book(1000, "100", "100.02")).unwrap();
    e.submit(true, p("100"), p("1")).unwrap();
    e.step(&book(1011, "100", "100.02")).unwrap();
    let mut changed = book(1020, "100", "100.02");
    if let Input::Book { view, .. } = &mut changed.input {
        view.bids[0].1 = p("1");
    }
    e.step(&changed).unwrap();
    assert_eq!(e.orders[0].ahead, p("1") as u128);
    assert!(e.fills.is_empty());
    e.step(&trade(1030, 1030, "100", "1.5", true)).unwrap();
    assert_eq!(e.fills[0].quantity, p("0.5"));
}
#[test]
fn entry_latency_and_post_only_rejection_are_enforced() {
    let mut e = engine();
    e.submit(true, p("100.02"), p("1")).unwrap();
    e.step(&book(1011, "100", "100.02")).unwrap();
    assert_eq!(e.rejected, 1);
    assert!(e.orders.is_empty());
    e.submit(true, p("100"), p("1")).unwrap();
    e.step(&trade(1015, 1015, "100", "100", true)).unwrap();
    assert!(e.fills.is_empty());
    e.step(&trade(1025, 1016, "100", "100", true)).unwrap();
    assert!(e.fills.is_empty()); // execution preceded entry
}
#[test]
fn delayed_pre_cancel_fill_race_and_cancel_timestamp_boundary() {
    let mut e = engine();
    e.submit(true, p("100"), p("1")).unwrap();
    e.step(&book(1011, "100", "100.02")).unwrap();
    e.cancel(1).unwrap(); // effective 1031
    e.step(&trade(1040, 1030, "100", "2.4", true)).unwrap();
    assert_eq!(e.fills[0].quantity, p("0.4"));
    e.step(&trade(1050, 1032, "100", "10", true)).unwrap();
    assert_eq!(e.fills.len(), 1);
    assert!(e.available().0 < e.cash);
    e.step(&book(1600, "100", "100.02")).unwrap();
    assert!(e.orders.is_empty());
    e.reconcile().unwrap();
}
#[test]
fn stale_disconnect_and_late_trade_reports_invalidate_fills() {
    let mut e = engine();
    e.submit(true, p("100"), p("1")).unwrap();
    e.step(&book(1011, "100", "100.02")).unwrap();
    e.step(&Tick {
        time_ms: 1020,
        sequence: 1,
        input: Input::Invalid {
            reason: "connection lost".into(),
        },
    })
    .unwrap();
    e.step(&trade(1030, 1030, "100", "100", true)).unwrap();
    assert!(e.fills.is_empty());
    assert!(!e.data_ok);
    let mut e = engine();
    e.submit(true, p("100"), p("1")).unwrap();
    e.step(&book(1011, "100", "100.02")).unwrap();
    e.step(&trade(4000, 4000, "100", "100", true)).unwrap();
    assert!(e.fills.is_empty());
    assert!(!e.data_ok);
    let mut e = engine();
    e.submit(true, p("100"), p("1")).unwrap();
    e.step(&book(1011, "100", "100.02")).unwrap();
    e.step(&trade(1800, 1100, "100", "100", true)).unwrap();
    assert!(!e.data_ok && e.fills.is_empty());
}
#[test]
fn asset_fees_inventory_conservation_and_ledger_reconcile() {
    for asset in [FeeAsset::Quote, FeeAsset::Base, FeeAsset::Bnb] {
        let mut s = scenario();
        for f in &mut s.fees.periods {
            f.standard_maker_microbps = 10_000_000;
            f.asset = asset;
            if asset == FeeAsset::Bnb {
                f.bnb_quote_price_units = Some(p("1000"));
            }
        }
        let mut e = Engine::new(s, instrument(), 5000).unwrap();
        e.step(&book(1000, "100", "100.02")).unwrap();
        e.submit(true, p("100"), p("1")).unwrap();
        e.step(&book(1011, "100", "100.02")).unwrap();
        e.step(&trade(1020, 1020, "100", "3", true)).unwrap();
        assert_eq!(e.fills.len(), 1);
        assert!(e.fees >= p("0.10") as i128);
        e.reconcile().unwrap();
        let (q, b, n) = e.available();
        assert!(q >= 0 && b >= 0 && n >= 0);
        if asset == FeeAsset::Base {
            assert_eq!(e.base, p("0.999") as i128);
        } else {
            assert_eq!(e.base, p("1") as i128);
        }
    }
}
#[test]
fn precision_limits_and_cash_reservations_prevent_shorting_or_overcommit() {
    let mut e = engine();
    assert!(!e.submit(false, p("100.02"), p("1")).unwrap());
    assert!(!e.submit(true, p("100.001"), p("1")).unwrap());
    assert!(!e.submit(true, p("100"), p("0.0001")).unwrap());
    assert!(!e.submit(true, p("100"), p("6")).unwrap());
    assert!(e.submit(true, p("100"), p("4")).unwrap());
    assert!(!e.submit(true, p("100"), p("2")).unwrap());
    e.reconcile().unwrap();
}
#[test]
fn drawdown_stop_cancels_and_never_reopens() {
    let mut s = scenario();
    s.strategy.drawdown_stop_usdt = 1;
    let mut e = Engine::new(s, instrument(), 5000).unwrap();
    e.step(&book(1000, "100", "100.02")).unwrap();
    e.submit(true, p("100"), p("1")).unwrap();
    e.step(&book(1011, "100", "100.02")).unwrap();
    e.step(&trade(1020, 1020, "100", "3", true)).unwrap();
    e.step(&book(1030, "90", "90.02")).unwrap();
    assert!(e.stopped);
    assert!(!e.submit(true, p("90"), p("1")).unwrap());
    let result = e.finish("fixture", 1000, 1031).unwrap();
    assert!(result.max_drawdown_atoms >= S as i128 && result.net_pnl_atoms < 0);
}
#[test]
fn ordering_replay_and_complete_results_are_deterministic() {
    let tape = vec![
        book(100000, "100", "100.02"),
        book(100100, "100", "100.02"),
        trade(100200, 100200, "99.99", "3", true),
        book(102000, "100.01", "100.03"),
    ];
    let a = simulate(&tape, &instrument(), &scenario(), "fixture", 100000, 103000).unwrap();
    let b = simulate(&tape, &instrument(), &scenario(), "fixture", 100000, 103000).unwrap();
    assert!(!a.fills.is_empty());
    assert_eq!(a.logs, b.logs);
    assert_eq!(a.ledger, b.ledger);
    assert_eq!(
        a.finish("fixture", 100000, 103000).unwrap(),
        b.finish("fixture", 100000, 103000).unwrap()
    );
    let mut e = engine();
    assert!(e.step(&book(900, "100", "100.02")).is_err());
    config().validate().unwrap();
}
proptest! {
 #[test]
 fn arbitrary_trade_sizes_preserve_balances_and_cannot_exceed_order(q in 1u64..10000) {
  let mut e=engine();e.submit(true,p("100"),p("1")).unwrap();e.step(&book(1011,"100","100.02")).unwrap();
  let quantity=q*p("0.001");let t=Trade {symbol:"TESTUSDT".into(),aggregate_id:1,first_id:1,last_id:1,exchange_ms:1020,price:p("100"),quantity,buyer_is_maker:true};
  e.step(&Tick {time_ms:1020,sequence:1,input:Input::Trade {trade:t}}).unwrap();e.reconcile().unwrap();prop_assert!(e.base>=0 && e.base<=p("1") as i128);
 }
 #[test]
 fn price_quantity_rounding_never_submits_invalid_increment(ticks in 1u64..10000,lots in 1u64..10000) {
  let mut e=engine();let price=ticks*e.instrument.tick_units;let qty=lots*e.instrument.step_units;
  let _=e.submit(true,price,qty).unwrap();prop_assert!(e.orders.iter().all(|o|o.price.is_multiple_of(e.instrument.tick_units) && o.remaining.is_multiple_of(e.instrument.step_units)));e.reconcile().unwrap();
 }
}

#[test]
fn delayed_trade_uses_execution_time_fees_and_reference_price_without_lookahead() {
    let mut s = scenario();
    s.fees.periods[0].until_ms = Some(1021);
    let mut later = s.fees.periods[0].clone();
    later.from_ms = 1021;
    later.until_ms = None;
    later.standard_maker_microbps = 100_000_000;
    s.fees.periods.push(later);
    let mut e = Engine::new(s, instrument(), 5000).unwrap();
    e.step(&book(1000, "100", "100.02")).unwrap();
    e.submit(true, p("100"), p("1")).unwrap();
    e.step(&book(1011, "100", "100.02")).unwrap();
    e.step(&book(1030, "101", "101.02")).unwrap();
    e.step(&trade(1040, 1020, "100", "3", true)).unwrap();
    assert_eq!(e.fills.len(), 1);
    assert_eq!(e.fees, 0);
    assert_eq!(e.fills[0].reference_mid, p("100.01"));
    assert_eq!(e.fills[0].time_ms, 1020);
    assert_eq!(e.gross, p("0.01") as i128);
    e.reconcile().unwrap();
}
#[test]
fn markout_horizons_use_observed_reference_prices_and_do_not_cross_disconnects() {
    let mut e = engine();
    e.submit(true, p("100"), p("1")).unwrap();
    e.step(&book(1011, "100", "100.02")).unwrap();
    e.step(&trade(1020, 1020, "100", "3", true)).unwrap();
    e.step(&book(2020, "99.98", "100.00")).unwrap();
    let result = e.finish("fixture", 1000, 2021).unwrap();
    assert_eq!(result.markouts[0].completed, 1);
    assert!(result.markouts[0].mean_bps.unwrap() < 0.0);
    assert_eq!(result.markouts[1].completed, 0);
    assert!(result.markouts[1].mean_bps.is_none());
    e.step(&Tick {
        time_ms: 2100,
        sequence: 1,
        input: Input::Invalid {
            reason: "gap".into(),
        },
    })
    .unwrap();
    e.step(&book(6020, "90", "90.02")).unwrap();
    assert_eq!(
        e.finish("fixture", 1000, 6021).unwrap().markouts[1].completed,
        0
    );
}

#[test]
fn favorable_inventory_movement_alone_does_not_validate_profitability() {
    let mut e = engine();
    e.submit(true, p("100"), p("1")).unwrap();
    e.step(&book(1011, "100", "100.02")).unwrap();
    e.step(&trade(1020, 1020, "100", "3", true)).unwrap();
    e.step(&book(1030, "101", "101.02")).unwrap();
    let r = e.finish("synthetic_fixture", 1000, 1031).unwrap();
    assert!(r.net_pnl_atoms > 0 && r.inventory_pnl_atoms > 0);
    assert!(!r.net_positive_after_costs_and_without_favorable_inventory);
}
#[test]
fn synthetic_quiet_trend_and_shock_regimes_preserve_risk_and_ledger() {
    for bid in ["100", "100.10", "90"] {
        let ask = format!(
            "{}.{:08}",
            (p(bid) + p("0.02")) / S,
            (p(bid) + p("0.02")) % S
        );
        let mut e = engine();
        e.submit(true, p("100"), p("1")).unwrap();
        e.step(&book(1011, "100", "100.02")).unwrap();
        e.step(&trade(1020, 1020, "100", "3", true)).unwrap();
        e.step(&book(1030, bid, &ask)).unwrap();
        e.reconcile().unwrap();
        let r = e.finish("synthetic_fixture", 1000, 1031).unwrap();
        assert!(r.max_drawdown_atoms >= 0);
    }
}

#[test]
fn stale_terminal_reference_cannot_qualify_an_experiment() {
    let e = engine();
    assert!(e.finish("fixture", 1000, 2000).unwrap().data_quality_ok);
    let stale = e.finish("fixture", 1000, 5000).unwrap();
    assert!(!stale.data_quality_ok);
    assert!(!stale.net_positive_after_costs_and_without_favorable_inventory);
}

//! Pre-registered research orchestration around the existing funded Engine and replay.
use crate::{
    config::Scenario,
    data::{Input, Tick, View, stream_symbol},
    engine::{Engine, Fill, LedgerEntry, S},
    portfolio::{ExecutionModel, Initialization},
    readiness::{FrozenFile, inspect_frozen},
};
use anyhow::{Result, ensure};
use mm_market_data::recording::segment_paths;
use mm_research::{
    fees::{FeeAsset, ceil_div, notional},
    run::{Manifest, replay_series},
    stats::horizon,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, VecDeque},
    io::Write,
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sensitivity {
    pub name: String,
    pub queue_multiplier_ppm: u64,
    pub entry_latency_ms: u64,
    pub cancel_latency_ms: u64,
    pub model: ExecutionModel,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Protocol {
    pub version: u64,
    pub frozen_at_ms: u64,
    pub capital_scope: String,
    pub symbols: Vec<String>,
    pub train_until_ppm: u64,
    pub validation_until_ppm: u64,
    pub embargo_ms: u64,
    pub min_oos_seconds: u64,
    pub min_oos_fills: u64,
    pub scenario: Scenario,
    pub strategies: Vec<String>,
    pub sensitivities: Vec<Sensitivity>,
    pub fee_assets: Vec<String>,
    pub selection_rule: String,
    pub max_trace_bytes: u64,
}
impl Protocol {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && self.capital_scope == "isolated_5000_alternatives_do_not_sum",
            "unsupported capital/protocol"
        );
        ensure!(
            self.train_until_ppm > 0
                && self.train_until_ppm < self.validation_until_ppm
                && self.validation_until_ppm < 1_000_000
                && self.embargo_ms >= 60_000,
            "chronological split/embargo invalid"
        );
        ensure!(
            !self.symbols.is_empty()
                && self.symbols.len() <= 20
                && self.symbols.iter().collect::<BTreeSet<_>>().len() == self.symbols.len(),
            "invalid symbols"
        );
        ensure!(
            self.strategies.len() == 4
                && self.strategies.iter().all(|s| [
                    "economic_fixed",
                    "adaptive_inventory",
                    "adaptive_filtered",
                    "research_touch_control"
                ]
                .contains(&s.as_str()))
                && self.strategies.iter().collect::<BTreeSet<_>>().len() == 4,
            "invalid strategy set"
        );
        ensure!(
            self.sensitivities.len() == 3
                && self
                    .sensitivities
                    .iter()
                    .map(|s| &s.name)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == 3,
            "invalid sensitivity set"
        );
        ensure!(
            self.fee_assets == ["received", "quote"]
                && self.min_oos_seconds >= 3600
                && self.min_oos_fills >= 100
                && (1024 * 1024..=1024 * 1024 * 1024).contains(&self.max_trace_bytes),
            "invalid fees/evidence/output limits"
        );
        self.scenario.validate()?;
        let risk = &self.scenario.strategy;
        ensure!(
            risk.clip_usdt <= 100
                && risk.inventory_limit_usdt <= 500
                && risk.drawdown_stop_usdt <= 50
                && risk.required_edge_bps >= 7
                && risk.max_spread_bps <= 50
                && risk.stale_ms <= 2000
                && self.scenario.report_latency_ms <= 500
                && self.scenario.queue == crate::config::QueueModel::TradeDepletion,
            "existing paper risk controls may not be relaxed"
        );
        ensure!(
            self.sensitivities.iter().any(|s| s.name == "conservative"
                && s.model == ExecutionModel::ConservativeThrough
                && s.queue_multiplier_ppm == 1_000_000
                && s.entry_latency_ms == 50
                && s.cancel_latency_ms == 100)
                && self.sensitivities.iter().any(|s| s.model
                    == ExecutionModel::ConservativeThrough
                    && s.queue_multiplier_ppm >= 2_000_000
                    && s.entry_latency_ms >= 250
                    && s.cancel_latency_ms >= 250)
                && self
                    .sensitivities
                    .iter()
                    .filter(|s| s.model == ExecutionModel::AlternativeThrough)
                    .count()
                    == 1,
            "required primary/stressed/diagnostic models missing"
        );
        ensure!(
            !self.scenario.hypothetical && self.scenario.maker_rebate_microbps == 0,
            "hypothetical economics cannot qualify"
        );
        for fee in &self.scenario.fees.periods {
            ensure!(
                fee.standard_maker_microbps == 10_000_000
                    && fee.standard_taker_microbps == 10_000_000
                    && fee.bnb_discount_ppm == 0
                    && fee.standard_buy_microbps == 0
                    && fee.standard_sell_microbps == 0
                    && fee.buy_extra_microbps == 0
                    && fee.sell_extra_microbps == 0,
                "standard manual tier only"
            );
        }
        for sensitivity in &self.sensitivities {
            ensure!(
                sensitivity.queue_multiplier_ppm >= 1_000_000
                    && sensitivity.queue_multiplier_ppm <= 10_000_000
                    && sensitivity.entry_latency_ms >= 50
                    && sensitivity.entry_latency_ms <= 5000
                    && sensitivity.cancel_latency_ms >= 100
                    && sensitivity.cancel_latency_ms <= 5000,
                "risk assumptions may not be relaxed"
            );
        }
        Ok(())
    }
    fn scenario(&self, strategy: &str, sensitivity: &Sensitivity, asset: &str) -> Scenario {
        let mut s = self.scenario.clone();
        s.name = format!("{strategy}_{}_{}", sensitivity.name, asset);
        s.queue_multiplier_ppm = sensitivity.queue_multiplier_ppm;
        s.entry_latency_ms = sensitivity.entry_latency_ms;
        s.cancel_latency_ms = sensitivity.cancel_latency_ms;
        if strategy.starts_with("adaptive") {
            s.strategy.spread_bps = 27;
            s.strategy.max_quote_spread_bps = 60;
            s.strategy.inventory_skew_bps = 20;
        }
        if strategy == "research_touch_control" {
            s.strategy.spread_bps = 0;
            s.strategy.max_quote_spread_bps = 100;
        }
        for fee in &mut s.fees.periods {
            fee.buy_asset = Some(if asset == "received" {
                FeeAsset::Base
            } else {
                FeeAsset::Quote
            });
            fee.sell_asset = Some(FeeAsset::Quote);
        }
        s
    }
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Window {
    pub split: String,
    pub start_ms: u64,
    pub end_ms: u64,
}
pub fn windows(p: &Protocol, start: u64, end: u64) -> Result<Vec<Window>> {
    ensure!(end > start, "empty source bounds");
    let duration = end - start;
    let train = start + duration * p.train_until_ppm / 1_000_000;
    let validation = start + duration * p.validation_until_ppm / 1_000_000;
    ensure!(
        p.frozen_at_ms < train,
        "protocol was not frozen before holdout"
    );
    let mut result = vec![];
    for (split, a, b) in [
        ("train", start, train),
        ("validation", train + p.embargo_ms, validation),
        ("test", validation + p.embargo_ms, end),
    ] {
        ensure!(
            b > a + 70_000,
            "split too short after embargo and markout settlement"
        );
        result.push(Window {
            split: split.into(),
            start_ms: a,
            end_ms: b,
        });
    }
    Ok(result)
}
#[derive(Default)]
struct Signals {
    prices: VecDeque<(u64, u64)>,
}
impl Signals {
    fn update(&mut self, time: u64, mid: u64) {
        if self
            .prices
            .back()
            .is_none_or(|(t, _)| t / 1000 != time / 1000)
        {
            self.prices.push_back((time, mid));
        }
        while self
            .prices
            .front()
            .is_some_and(|(t, _)| time.saturating_sub(*t) > 32_000)
        {
            self.prices.pop_front();
        }
    }
    fn features(&self) -> (u64, u64, bool) {
        let mut squared = 0u128;
        let mut count = 0;
        let mut movement = 0;
        let chronological: Vec<_> = self.prices.iter().collect();
        for pair in chronological.windows(2) {
            if pair[1].0 - pair[0].0 <= 2000 {
                let m = u128::from(pair[1].1.abs_diff(pair[0].1)) * 10_000_000_000
                    / u128::from(pair[0].1);
                squared += m * m;
                count += 1;
                movement = m;
            }
        }
        let rms = if count > 0 {
            (squared / count).isqrt()
        } else {
            0
        };
        (
            ceil_div(rms * 3, 1_000_000).unwrap_or(1000) as u64,
            (movement / 1_000_000) as u64,
            count >= 5,
        )
    }
}
fn imbalance(view: &View) -> Result<(u128, u128)> {
    let mut sides = [0u128; 2];
    let mid = u128::from(view.mid());
    for (side, levels) in [&view.bids, &view.asks].iter().enumerate() {
        for (p, q) in *levels {
            if u128::from(p.abs_diff(view.mid())) * 10_000 <= mid * 10 {
                sides[side] += notional(*p, *q)?;
            }
        }
    }
    Ok((sides[0].abs_diff(sides[1]), sides[0] + sides[1]))
}
#[derive(Default, Serialize)]
struct Markout {
    seconds: u64,
    completed: u64,
    missing: u64,
    signed_adverse_atoms: i128,
    unfavorable_adverse_atoms: i128,
}
struct Pending {
    fill: Fill,
    target: u64,
    index: usize,
}
struct Trial {
    engine: Engine,
    strategy: String,
    sensitivity: Sensitivity,
    asset: String,
    window: Window,
    signals: Signals,
    pending: Vec<Pending>,
    markouts: Vec<Markout>,
    fills: u64,
    filtered: u64,
    invalid: u64,
    first_book: Option<u64>,
    last_mark: u64,
    books: u64,
    trades: u64,
}
impl Trial {
    fn new(
        p: &Protocol,
        market: (&mm_research::discovery::Instrument, (u64, u64)),
        window: Window,
        strategy: &str,
        sensitivity: &Sensitivity,
        asset: &str,
        tick: &Tick,
    ) -> Result<Self> {
        let (instrument, bounds) = market;
        let Input::Book { view, .. } = &tick.input else {
            anyhow::bail!("initial book required")
        };
        let mut engine = Engine::funded(
            p.scenario(strategy, sensitivity, asset),
            instrument.clone(),
            Initialization {
                base_allocation_ppm: 500_000,
                valuation_price_units: view.mid(),
                valuation_time_ms: tick.time_ms,
                fee_buffer_ppm: 100_000,
            },
            sensitivity.model,
        )?;
        engine.price_bounds = Some(bounds);
        engine.automatic_quoting = false;
        engine.research_quotes = strategy == "research_touch_control";
        Ok(Self {
            engine,
            strategy: strategy.into(),
            sensitivity: sensitivity.clone(),
            asset: asset.into(),
            window,
            signals: Signals::default(),
            pending: vec![],
            markouts: [1, 5, 30, 60]
                .map(|seconds| Markout {
                    seconds,
                    ..Default::default()
                })
                .into(),
            fills: 0,
            filtered: 0,
            invalid: 0,
            first_book: None,
            last_mark: view.mid(),
            books: 0,
            trades: 0,
        })
    }
    fn step(
        &mut self,
        tick: &Tick,
        writer: &mut impl Write,
        bytes: &mut u64,
        max: u64,
    ) -> Result<()> {
        if matches!(tick.input, Input::Invalid { .. }) {
            self.invalid += 1;
        }
        if tick.time_ms >= self.window.end_ms - 65_000 && !self.engine.settling {
            self.engine.clock = tick.time_ms;
            self.engine.settling = true;
            for id in self.engine.orders.iter().map(|o| o.id).collect::<Vec<_>>() {
                self.engine.cancel(id)?;
            }
        }
        self.engine.step(tick)?;
        if let Input::Trade { .. } = &tick.input {
            self.trades += 1;
        }
        if let Input::Book { exchange_ms, .. } = &tick.input
            && let Some(view) = &self.engine.view
        {
            self.books += 1;
            self.first_book.get_or_insert(tick.time_ms);
            self.last_mark = view.mid();
            self.signals.update(*exchange_ms, view.mid());
            let epoch = self.engine.reference_marks.last().map_or(0, |(_, _, e)| *e);
            self.pending.retain(|pending| {
                if epoch != pending.fill.epoch || *exchange_ms > pending.target + 1500 {
                    self.markouts[pending.index].missing += 1;
                    false
                } else if *exchange_ms >= pending.target {
                    let sign = if pending.fill.buy { 1 } else { -1 };
                    let adverse = (i128::from(pending.fill.reference_mid) - i128::from(view.mid()))
                        * sign
                        * i128::from(pending.fill.quantity)
                        / i128::from(S);
                    let m = &mut self.markouts[pending.index];
                    m.completed += 1;
                    m.signed_adverse_atoms += adverse;
                    m.unfavorable_adverse_atoms += adverse.max(0);
                    false
                } else {
                    true
                }
            });
        }
        for fill in &self.engine.fills {
            self.fills += 1;
            for (index, m) in self.markouts.iter().enumerate() {
                ensure!(self.pending.len() < 100_000, "pending markout bound");
                self.pending.push(Pending {
                    fill: fill.clone(),
                    target: fill.time_ms + m.seconds * 1000,
                    index,
                });
            }
        }
        if self.engine.view.is_some() && !self.engine.settling {
            let (width, movement, warm) = self.signals.features();
            let adaptive = self.strategy.starts_with("adaptive");
            let (pressure, total) = imbalance(self.engine.view.as_ref().unwrap())?;
            let reject = adaptive
                && (!warm
                    || self.strategy == "adaptive_filtered"
                        && (movement >= 5
                            || pressure * 10 > total * 6
                            || total < 1000 * u128::from(S)));
            if reject {
                self.filtered += 1;
                for id in self.engine.orders.iter().map(|o| o.id).collect::<Vec<_>>() {
                    self.engine.cancel(id)?;
                }
            } else {
                if adaptive {
                    self.engine.scenario.strategy.spread_bps = 27.max(width);
                }
                self.engine.quote()?;
            }
        }
        if !self.engine.fills.is_empty() || !self.engine.logs.is_empty() {
            let row = json!({"type":"simulated_not_actual_execution",
            "symbol":self.engine.instrument.symbol,
            "split":self.window.split,
            "strategy":self.strategy,
            "sensitivity":self.sensitivity.name,
            "fee_assets":self.asset,
            "source_sequence":tick.sequence,
            "time_ms":tick.time_ms,
            "fills":self.engine.fills,
            "actions":self.engine.logs});
            let line = serde_json::to_vec(&row)?;
            ensure!(
                *bytes + (line.len() as u64) < max,
                "study trace budget exceeded"
            );
            writer.write_all(&line)?;
            writer.write_all(b"\n")?;
            *bytes += line.len() as u64 + 1;
        }
        compact(&mut self.engine)?;
        Ok(())
    }
    fn finish(mut self, p: &Protocol, source_valid: bool) -> Result<Value> {
        for pending in self.pending {
            self.markouts[pending.index].missing += 1;
        }
        let e = &mut self.engine;
        e.reconcile()?;
        let equity = e.marked_equity(self.last_mark)?;
        let net = equity - e.capital_atoms();
        let hold = e.portfolio.initial_quote_atoms
            + notional(self.last_mark, e.portfolio.initial_base_units)? as i128
            - e.capital_atoms();
        let excess = net - hold;
        let spread_net = e.gross - e.fees;
        let adverse = self.markouts[3].unfavorable_adverse_atoms;
        let markouts_complete = self.markouts[3].completed == self.fills;
        let fee = e.scenario.fees.at(e.clock, &e.instrument.symbol)?;
        let position = u64::try_from(e.base)?;
        let taker = if position > 0 {
            fee.charge(self.last_mark, position, false, false)?
                .quote_equivalent_atoms as i128
        } else {
            0
        };
        // Stress liquidation of the entire funded base, not an invented favorable offset.
        let bid = e.view.as_ref().map_or(self.last_mark, View::bid);
        let closeout = taker
            + notional(self.last_mark, position)?.saturating_sub(notional(bid, position)?) as i128
            + ceil_div(notional(bid, position)? * 2, 10_000)? as i128;
        let allowance = ceil_div(e.turnover * 7, 20_000)? as i128;
        let adjusted = excess.min(spread_net - adverse) - closeout - allowance;
        let valid = source_valid
            && self.invalid == 0
            && e.data_ok
            && self.books > 0
            && self.trades > 0
            && e.last_book_ms
                .is_some_and(|t| self.window.end_ms.saturating_sub(t) <= 2000);
        let duration = self.window.end_ms - self.first_book.unwrap_or(self.window.end_ms);
        let mut reasons = vec![];
        if !valid {
            reasons.push("invalid_or_missing_market_coverage");
        }
        if self.strategy == "research_touch_control" {
            reasons.push("research_only_uneconomic_control");
        }
        if self.sensitivity.model == ExecutionModel::AlternativeThrough {
            reasons.push("optimistic_fill_assumption_diagnostic_only");
        }
        if self.fills < p.min_oos_fills {
            reasons.push("insufficient_simulated_fill_count");
        }
        if duration < p.min_oos_seconds * 1000 {
            reasons.push("insufficient_observation_duration");
        }
        if !markouts_complete {
            reasons.push("missing_60s_fill_markouts");
        }
        if !e.orders.is_empty() {
            reasons.push("unsettled_simulated_orders");
        }
        if adjusted <= 0 {
            reasons.push("no_positive_inventory_neutral_edge_after_fees_and_costs");
        }
        if e.stopped {
            reasons.push("drawdown_stop_triggered");
        }
        let edge = |atoms: i128| {
            if e.turnover > 0 {
                Some(atoms as f64 / e.turnover as f64 * 10_000.)
            } else {
                None
            }
        };
        let mut row = json!({"result_type":"historical_paper_simulation_not_exchange_execution",
            "symbol":e.instrument.symbol,
            "split":self.window.split,
            "window":self.window,
            "strategy":self.strategy,
            "sensitivity":self.sensitivity,
            "fee_asset_assumption":self.asset,
            "capital_scope":p.capital_scope,
            "capital_usdt":5000,
            "initialization":e.portfolio,
            "observed_duration_ms":duration,
            "books":self.books,
            "trades":self.trades,
            "fills":self.fills,
            "submitted":e.submitted,
            "accepted":e.accepted,
            "cancelled":e.cancelled,
            "filtered_events":self.filtered,
            "funnel":e.diagnostics,
            "ending_balances":e.balances(),
            "open_orders":e.orders.len()});
        row.as_object_mut().unwrap().extend(
            json!({"gross_spread_capture_atoms":e.gross,
            "fees_atoms":e.fees,
            "fee_assets":e.fee_asset_totals(),
            "net_mtm_atoms":net,
            "initial_inventory_hold_pnl_atoms":hold,
            "incremental_inventory_pnl_atoms":net+e.fees-e.gross-hold,
            "excess_over_hold_atoms":excess,
            "spread_capture_less_fees_atoms":spread_net,
            "markouts":self.markouts,
            "closeout_stress_atoms":closeout,
            "risk_allowance_atoms":allowance,
            "conservative_trading_pnl_atoms":adjusted,
            "executed_notional_atoms":e.turnover,
            "spread_capture_less_fees_bps_per_executed_notional":edge(spread_net),
            "conservative_net_edge_bps_per_executed_notional":edge(adjusted),
            "max_drawdown_atoms":e.maximum_drawdown(),
            "drawdown_stopped":e.stopped,
            "accounting":e.portfolio_pnl(self.last_mark)?,
            "data_quality_valid":valid,
            "eligible_positive_oos_row":self.window.split!="train" && reasons.is_empty(),
            "rejection_reasons":reasons,
            "actual_exchange_executions":0})
            .as_object()
            .unwrap()
            .clone(),
        );
        Ok(row)
    }
}
fn compact(e: &mut Engine) -> Result<()> {
    e.logs.clear();
    e.fills.clear();
    e.decisions.clear();
    e.equity.clear();
    e.execution_evidence.clear();
    e.ledger = [
        ("USDT".to_string(), e.cash),
        (e.instrument.base_asset.clone(), e.base),
        ("BNB".into(), e.bnb),
    ]
    .into_iter()
    .map(|(asset, delta)| LedgerEntry {
        time_ms: e.clock,
        asset,
        delta,
        reason: "reconciliation_checkpoint_not_deposit".into(),
        order_id: None,
    })
    .collect();
    let ids: BTreeSet<_> = e.orders.iter().map(|o| o.id).collect();
    e.entry_deadlines.retain(|id, _| ids.contains(id));
    e.cancel_deadlines.retain(|id, _| ids.contains(id));
    e.initial_queue_ahead.retain(|id, _| ids.contains(id));
    let cutoff = e.clock.saturating_sub(6000);
    let first = e
        .reference_marks
        .partition_point(|(t, _, _)| *t < cutoff)
        .saturating_sub(1);
    e.reference_marks.drain(..first);
    e.reconcile()
}

pub fn run(source: &Path, protocol_path: &Path, output: &Path) -> Result<()> {
    let protocol_bytes = std::fs::read(protocol_path)?;
    let p: Protocol = serde_json::from_slice(&protocol_bytes)?;
    p.validate()?;
    ensure!(
        source.join("collection_finished.json").exists(),
        "source still running or completion unverified"
    );
    let completion: Value =
        serde_json::from_slice(&std::fs::read(source.join("collection_finished.json"))?)?;
    ensure!(
        completion["reason"] == "duration_complete",
        "capture interrupted; no study on partial duration"
    );
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(source.join("manifest.json"))?)?;
    ensure!(
        manifest
            .instruments
            .iter()
            .map(|i| &i.symbol)
            .collect::<BTreeSet<_>>()
            == p.symbols.iter().collect(),
        "universe differs from frozen protocol"
    );
    ensure!(
        !output.exists() || std::fs::read_dir(output)?.next().is_none(),
        "study output must be new/empty"
    );
    std::fs::create_dir_all(output)?;
    std::fs::write(
        output.join("INCOMPLETE"),
        "Do not interpret before complete.json exists",
    )?;
    std::fs::write(output.join("protocol.json"), &protocol_bytes)?;
    let paths = segment_paths(&source.join("recordings"))?;
    ensure!(paths.len() <= 40_000, "source file bound");
    let frozen: Vec<_> = paths
        .iter()
        .map(|path| {
            Ok(FrozenFile {
                path: path.clone(),
                bytes: std::fs::metadata(path)?.len(),
                sha256_sidecar: std::fs::read_to_string(path.with_extension("sha256"))?,
            })
        })
        .collect::<Result<_>>()?;
    let audit = inspect_frozen(&frozen)?;
    ensure!(audit.errors.is_empty(), "checksum/replay errors");
    std::fs::write(
        output.join("source_audit.json"),
        serde_json::to_vec_pretty(&json!({"source":source,
            "completion":completion,
            "files":frozen,
            "audit":audit}))?,
    )?;
    let info: Value = serde_json::from_slice(&std::fs::read(source.join("exchange-info.json"))?)?;
    let metadata = json!({"exchange_info":info});
    let mut rows = vec![];
    let mut gate_audits = vec![];
    let mut trace_bytes = 0;
    let mut writer = std::io::BufWriter::new(std::fs::File::create(
        output.join("execution_traces.jsonl"),
    )?);
    for old in &manifest.instruments {
        let (instrument, bounds, _) = crate::batch::metadata_instrument(old, &metadata)?;
        let (start, end) = manifest.bounds[&old.symbol];
        let splits = windows(&p, start, end)?;
        let coverage = &audit.symbols[&old.symbol];
        let source_valid = coverage.prefix_complete
            && coverage.record_sequence_gaps == 0
            && coverage.book_exchange_time_reversals == 0
            && coverage.trade_exchange_time_reversals == 0;
        let mut trials: Vec<Trial> = vec![];
        let mut current: Option<usize> = None;
        let mut gate_audited = false;
        let normalization = stream_symbol(
            &source.join("recordings").join(&old.symbol),
            &old.symbol,
            |tick| {
                if !gate_audited
                    && let Input::Book { view, exchange_ms } = &tick.input
                    && view.valid()
                    && tick.time_ms.abs_diff(*exchange_ms) <= 2000
                {
                    gate_audits.push(audit_gate(&instrument, &tick)?);
                    gate_audited = true;
                }
                let index = splits
                    .iter()
                    .position(|w| tick.time_ms >= w.start_ms && tick.time_ms < w.end_ms);
                if current != index {
                    for trial in trials.drain(..) {
                        rows.push(trial.finish(&p, source_valid)?);
                    }
                    current = index;
                }
                let Some(index) = index else { return Ok(()) };
                if trials.is_empty()
                    && let Input::Book { view, exchange_ms } = &tick.input
                    && view.valid()
                    && tick.time_ms.abs_diff(*exchange_ms) <= 2000
                {
                    for strategy in &p.strategies {
                        for sensitivity in &p.sensitivities {
                            for asset in &p.fee_assets {
                                trials.push(Trial::new(
                                    &p,
                                    (&instrument, bounds),
                                    splits[index].clone(),
                                    strategy,
                                    sensitivity,
                                    asset,
                                    &tick,
                                )?);
                            }
                        }
                    }
                }
                for trial in &mut trials {
                    trial.step(&tick, &mut writer, &mut trace_bytes, p.max_trace_bytes)?;
                }
                Ok(())
            },
        )?;
        ensure!(
            normalization.prefix_complete && normalization.sequence_contiguous,
            "normalization missing prefix/segments"
        );
        for trial in trials {
            rows.push(trial.finish(&p, source_valid)?);
        }
    }
    writer.flush()?;
    std::fs::write(
        output.join("economic_gate_audit.json"),
        serde_json::to_vec_pretty(&gate_audits)?,
    )?;
    let series = replay_series(&source.join("recordings"), &manifest.bounds)?;
    let markets: Vec<_> = manifest
        .instruments
        .iter()
        .map(|instrument| {
            json!({"instrument":instrument,
            "windows":([60,300,900].map(|seconds|horizon(&series[&instrument.symbol],seconds))),
            "aggregate_trade_gaps":series[&instrument.symbol].aggregate_gaps,
            "assumed_maker_fee_bps_each_side":10,
            "account_fees_verified":false})
        })
        .collect();
    std::fs::write(
        output.join("market_comparison.json"),
        serde_json::to_vec_pretty(&markets)?,
    )?;
    let mut missing_windows = vec![];
    for instrument in &manifest.instruments {
        for split in ["train", "validation", "test"] {
            if !rows
                .iter()
                .any(|r| r["symbol"] == instrument.symbol && r["split"] == split)
            {
                missing_windows.push(json!({"symbol":instrument.symbol,"split":split,"reason":"no_valid_initial_book_in_fold","financial_results":null}));
            }
        }
    }
    let assessment = assess(&rows, &p);
    // No holdout-based retuning: report all fixed hypotheses; never choose a winner by test P&L.
    let report = json!({"version":1,
            "protocol_sha256":format!("{:x}",Sha256::digest(&protocol_bytes)),
            "source":source,
            "capital_scope":p.capital_scope,
            "actual_exchange_executions":0,
            "results":rows,
            "assessment":assessment,"missing_windows":missing_windows,
            "defensible_positive_simulated_edge":assessment["defensible_positive_simulated_edge"],
            "selection":"All candidates pre-registered. Final test is diagnostic, never a tuning objective. Qualification additionally requires cross-sensitivity, fee-asset and validation/test agreement, not any isolated positive row.",
            "preliminary":true,
            "trace_bytes":trace_bytes});
    std::fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    let mut csv="symbol,split,strategy,sensitivity,fee_asset,fills,gross_capture_atoms,fees_atoms,initial_hold_pnl_atoms,net_mtm_atoms,trading_pnl_atoms,net_edge_bps,max_drawdown_atoms,data_valid\n".to_string();
    for r in report["results"].as_array().unwrap() {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            r["symbol"].as_str().unwrap(),
            r["split"].as_str().unwrap(),
            r["strategy"].as_str().unwrap(),
            r["sensitivity"]["name"].as_str().unwrap(),
            r["fee_asset_assumption"].as_str().unwrap(),
            r["fills"],
            r["gross_spread_capture_atoms"],
            r["fees_atoms"],
            r["initial_inventory_hold_pnl_atoms"],
            r["net_mtm_atoms"],
            r["conservative_trading_pnl_atoms"],
            r["conservative_net_edge_bps_per_executed_notional"],
            r["max_drawdown_atoms"],
            r["data_quality_valid"]
        ));
    }
    std::fs::write(output.join("results.csv"), csv)?;
    std::fs::write(
        output.join("complete.json"),
        serde_json::to_vec_pretty(&json!({"complete":true,
            "results_sha256":format!("{:x}",Sha256::digest(std::fs::read(output.join("results.json"))?))}))?,
    )?;
    std::fs::remove_file(output.join("INCOMPLETE"))?;
    Ok(())
}

/// Training nomination is fixed before interpreting any validation/test scores.
fn assess(rows: &[Value], p: &Protocol) -> Value {
    let conservative: Vec<_> = p
        .sensitivities
        .iter()
        .filter(|s| s.model == ExecutionModel::ConservativeThrough)
        .map(|s| s.name.as_str())
        .collect();
    let mut nominations = vec![];
    let mut positives = vec![];
    for symbol in &p.symbols {
        let mut nominated: Option<(&str, f64)> = None;
        for strategy in p
            .strategies
            .iter()
            .filter(|s| s.as_str() != "research_touch_control")
        {
            let train: Vec<_> = rows
                .iter()
                .filter(|r| {
                    r["symbol"] == *symbol
                        && r["strategy"] == *strategy
                        && r["split"] == "train"
                        && r["sensitivity"]["name"] == "conservative"
                })
                .collect();
            let score = if train.len() == 2
                && train.iter().all(|r| {
                    r["data_quality_valid"] == true && r["fills"].as_u64().unwrap_or(0) > 0
                }) {
                train
                    .iter()
                    .filter_map(|r| r["conservative_net_edge_bps_per_executed_notional"].as_f64())
                    .reduce(f64::min)
            } else {
                None
            };
            if let Some(score) = score
                && score > 0.0
                && nominated.is_none_or(|(_, old)| score > old)
            {
                nominated = Some((strategy, score));
            }
        }
        if let Some((strategy, score)) = nominated {
            let mut passed = conservative.len() == 2;
            for split in ["validation", "test"] {
                for sensitivity in &conservative {
                    for asset in &p.fee_assets {
                        let matching: Vec<_> = rows
                            .iter()
                            .filter(|r| {
                                r["symbol"] == *symbol
                                    && r["strategy"] == strategy
                                    && r["split"] == split
                                    && r["sensitivity"]["name"] == *sensitivity
                                    && r["fee_asset_assumption"] == *asset
                            })
                            .collect();
                        passed &=
                            matching.len() == 1 && matching[0]["eligible_positive_oos_row"] == true;
                    }
                }
            }
            nominations.push(json!({"symbol":symbol,
            "strategy":strategy,
            "training_worst_fee_asset_net_bps":score,
            "all_conservative_holdout_cells_pass":passed}));
            if passed {
                positives.push(json!({"symbol":symbol,
            "strategy":strategy}));
            }
        }
    }
    json!({"training_nominations":nominations,
            "qualifying_candidates":positives,
            "defensible_positive_simulated_edge":!positives.is_empty(),
            "no_final_test_selection":true})
}

fn audit_gate(instrument: &mm_research::discovery::Instrument, tick: &Tick) -> Result<Value> {
    let Input::Book { view, .. } = &tick.input else {
        anyhow::bail!("gate audit requires book")
    };
    let mut probes = vec![];
    for width in [0, 1, 26, 27, 30] {
        let scenario = crate::paper::scenario(crate::paper::Profile::Economic, width)?;
        let roundtrip = scenario
            .fees
            .at(tick.time_ms, &instrument.symbol)?
            .roundtrip_microbps()?;
        let threshold =
            ceil_div(roundtrip, 1_000_000)? as u64 + scenario.strategy.required_edge_bps;
        let mut e = Engine::funded(
            scenario,
            instrument.clone(),
            Initialization {
                base_allocation_ppm: 500_000,
                valuation_price_units: view.mid(),
                valuation_time_ms: tick.time_ms,
                fee_buffer_ppm: 100_000,
            },
            ExecutionModel::ConservativeThrough,
        )?;
        e.step(tick)?;
        probes.push(json!({"requested_bps":width,"required_minimum_bps":threshold,"configured_net_after_cost_allowance_bps":i128::from(width)-i128::from(threshold),"fee_rejections":e.diagnostics.rejected_fee_constraints,"risk_rejections":e.diagnostics.rejected_risk_limits,"exchange_filter_rejections":e.diagnostics.rejected_exchange_filters,"submitted":e.submitted,"open_simulated_orders":e.orders.len(),"funnel":e.diagnostics}));
    }
    Ok(
        json!({"symbol":instrument.symbol,"source_time_ms":tick.time_ms,"source_sequence":tick.sequence,"probes":probes,"interpretation":"fee threshold is a quote-width feasibility gate, never observed executable profitability; default30 passes, widths below27 fail. Volatility above the cap and market/funding/filter limits may independently prevent quotes."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn protocol() -> Protocol {
        serde_json::from_str(include_str!("../../../config/milestone6.json")).unwrap()
    }
    fn instrument() -> mm_research::discovery::Instrument {
        mm_research::discovery::Instrument {
            symbol: "UNITUSDT".into(),
            base_asset: "UNIT".into(),
            price_units: 100 * S,
            tick_units: S / 100,
            step_units: S / 1000,
            min_qty_units: S / 1000,
            max_qty_units: 1000 * S,
            min_notional_units: 5 * S,
            max_notional_units: None,
            clip_quantity_units: S,
            clip_notional_units: u128::from(100 * S),
            volume_24h_usdt: 1e8,
            trades_24h: 100_000,
        }
    }
    fn book(time: u64, mid: u64) -> Tick {
        Tick {
            time_ms: time,
            sequence: time,
            input: Input::Book {
                view: View {
                    bids: vec![(mid - S / 100, 2 * S), (mid - S, 10 * S)],
                    asks: vec![(mid + S / 100, 2 * S), (mid + S, 10 * S)],
                },
                exchange_ms: time,
            },
        }
    }
    #[test]
    fn frozen_windows_are_chronological_embargoed_and_disjoint() {
        let mut p = protocol();
        p.frozen_at_ms = 1;
        let w = windows(&p, 1_000_000, 1_900_000).unwrap();
        assert_eq!(w[0].end_ms, 1_360_000);
        assert_eq!(w[1].start_ms, 1_420_000);
        assert_eq!(w[2].start_ms, 1_690_000);
        assert!(
            w.windows(2)
                .all(|pair| pair[1].start_ms - pair[0].end_ms == 60_000)
        );
        p.frozen_at_ms = 1_360_001;
        assert!(windows(&p, 1_000_000, 1_900_000).is_err());
    }
    #[test]
    fn protocol_rejects_discount_and_relaxed_risk() {
        let mut p = protocol();
        p.validate().unwrap();
        p.scenario.fees.periods[0].standard_maker_microbps = 0;
        assert!(p.validate().is_err());
        let mut p = protocol();
        p.scenario.strategy.inventory_limit_usdt = 1000;
        assert!(p.validate().is_err());
        let mut p = protocol();
        p.sensitivities[1].queue_multiplier_ppm = 500_000;
        assert!(p.validate().is_err());
    }
    #[test]
    fn signals_are_causal_bounded_and_invariant_to_deque_wrap() {
        let mut s = Signals::default();
        for i in 0..100 {
            s.update(1000 * i, 100 * S + i * S / 100);
        }
        assert!(s.prices.len() <= 33);
        let features = s.features();
        assert!(features.2);
        let mut contiguous = Signals {
            prices: s.prices.iter().copied().collect(),
        };
        contiguous.prices.make_contiguous();
        assert_eq!(features, contiguous.features());
        let before = s.features();
        s.update(100_000, 110 * S);
        assert!(s.features().0 > before.0 && s.features().1 >= 5);
    }
    #[test]
    fn depth_pressure_is_exact_and_not_quantity_weighted_across_prices() {
        let Input::Book { view, .. } = book(1000, 100 * S).input else {
            panic!()
        };
        let (pressure, total) = imbalance(&view).unwrap();
        assert_eq!(pressure, 4_000_000);
        assert_eq!(total, 400 * S as u128);
    }
    #[test]
    fn received_and_quote_fees_are_explicit_without_discount() {
        let p = protocol();
        for asset in &p.fee_assets {
            let s = p.scenario("economic_fixed", &p.sensitivities[0], asset);
            let fee = s.fees.at(1000, "UNITUSDT").unwrap();
            let charged = fee.charge(100 * S, S, true, true).unwrap();
            assert_eq!(charged.quote_equivalent_atoms, 10_000_000);
            assert_eq!(
                charged.asset,
                if asset == "received" {
                    FeeAsset::Base
                } else {
                    FeeAsset::Quote
                }
            );
        }
    }
    #[test]
    fn inventory_appreciation_and_zero_fills_never_create_trading_edge() {
        let p = protocol();
        let tick = book(1_000_000, 100 * S);
        let window = Window {
            split: "test".into(),
            start_ms: 1_000_000,
            end_ms: 1_900_000,
        };
        let mut trial = Trial::new(
            &p,
            (&instrument(), (1, 1_000_000 * S)),
            window,
            "economic_fixed",
            &p.sensitivities[0],
            "received",
            &tick,
        )
        .unwrap();
        trial.engine.settling = true;
        let mut bytes = 0;
        let mut sink = vec![];
        trial
            .step(&tick, &mut sink, &mut bytes, p.max_trace_bytes)
            .unwrap();
        trial
            .step(
                &book(1_001_000, 110 * S),
                &mut sink,
                &mut bytes,
                p.max_trace_bytes,
            )
            .unwrap();
        let result = trial.finish(&p, true).unwrap();
        assert!(result["net_mtm_atoms"].as_i64().unwrap() > 0);
        assert_eq!(result["excess_over_hold_atoms"], 0);
        assert!(result["conservative_trading_pnl_atoms"].as_i64().unwrap() < 0);
        assert!(result["conservative_net_edge_bps_per_executed_notional"].is_null());
        assert_eq!(result["eligible_positive_oos_row"], false);
    }
    fn hypothetical_pass_rows(p: &Protocol) -> Vec<Value> {
        let mut rows = vec![];
        for split in ["train", "validation", "test"] {
            for sensitivity in p
                .sensitivities
                .iter()
                .filter(|s| s.model == ExecutionModel::ConservativeThrough)
            {
                for asset in &p.fee_assets {
                    rows.push(json!({"result_type":"synthetic_qualification_test","symbol":p.symbols[0],"strategy":"economic_fixed","split":split,"sensitivity":sensitivity,"fee_asset_assumption":asset,"data_quality_valid":true,"fills":150,"conservative_net_edge_bps_per_executed_notional":1.0,"eligible_positive_oos_row":true}));
                }
            }
        }
        rows
    }
    #[test]
    fn qualification_requires_both_holdouts_stress_and_fee_assets() {
        let p = protocol();
        let mut rows = hypothetical_pass_rows(&p);
        assert_eq!(
            assess(&rows, &p)["defensible_positive_simulated_edge"],
            true
        );
        rows.last_mut().unwrap()["eligible_positive_oos_row"] = json!(false);
        assert_eq!(
            assess(&rows, &p)["defensible_positive_simulated_edge"],
            false
        );
    }
    #[test]
    fn optimistic_fills_and_test_only_winner_cannot_qualify() {
        let p = protocol();
        let mut rows = hypothetical_pass_rows(&p);
        for r in &mut rows {
            if r["split"] == "train" {
                r["conservative_net_edge_bps_per_executed_notional"] = json!(-1.0);
            }
        }
        assert_eq!(
            assess(&rows, &p)["defensible_positive_simulated_edge"],
            false
        );
        let mut rows = hypothetical_pass_rows(&p);
        rows.retain(|r| r["sensitivity"]["name"] == "conservative");
        let mut optimistic = rows[0].clone();
        optimistic["sensitivity"] = json!(p.sensitivities[2]);
        rows.push(optimistic);
        assert_eq!(
            assess(&rows, &p)["defensible_positive_simulated_edge"],
            false
        );
    }
}

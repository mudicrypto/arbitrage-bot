//! Explanations only: counters and retrospective trade reach never create executions.
use crate::{
    data::{Input, Tick},
    engine::{Engine, S},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Funnel {
    pub quote_opportunities: u64,
    pub generated_quote_intents: u64,
    pub existing_quote_retained_or_canceling: u64,
    pub fee_widened_opportunities: u64,
    pub rejected_fee_constraints: u64,
    pub rejected_risk_limits: u64,
    pub rejected_exchange_filters: u64,
    pub rejected_market_data: u64,
    pub submitted_to_simulated_execution: u64,
    pub dependency_failures: BTreeMap<String, u64>,
    pub rejection_reasons: BTreeMap<String, u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Decision {
    pub best_bid: Option<u64>,
    pub best_ask: Option<u64>,
    pub distance_from_same_side_microbps: Option<i128>,
    pub observed_spread_microbps: Option<u128>,
    pub roundtrip_maker_fee_microbps: Option<u128>,
    pub time_ms: u64,
    pub buy: bool,
    pub price: u64,
    pub quantity: u64,
    pub outcome: String,
    pub reason: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionEvidence {
    pub through_eligible_quantity: u128,
    pub through_filled_quantity: u128,
    pub unknown_through_depth_events: u64,
    pub trade_through_valid_window_events: u64,
    pub exact_price_eligible_trade_events: u64,
    pub queue_blocked_trade_events: u64,
    pub quantity_blocked_by_lot_rounding: u128,
    pub observed_eligible_quantity: u128,
    pub queue_consumed_quantity: u128,
    pub filled_quantity: u128,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderExplanation {
    pub end_state: String,
    pub initial_queue_ahead_estimate: Option<u128>,
    pub lifetime_ms: u64,
    pub lifetime_right_censored: bool,
    pub active_lifetime_ms: Option<u64>,
    pub order_id: u64,
    pub buy: bool,
    pub price: u64,
    pub quantity: u64,
    pub submitted_ms: u64,
    pub entry_due_ms: u64,
    pub accepted_ms: Option<u64>,
    pub cancel_requested_ms: Option<u64>,
    pub cancel_effective_ms: Option<u64>,
    pub terminal_ms: Option<u64>,
    pub terminal_reason: Option<String>,
    pub exact_price_directional_trade_events_since_submission: u64,
    pub exact_price_directional_trade_events_before_entry: u64,
    pub exact_price_directional_trade_events_after_cancel: u64,
    pub trade_through_events_ignored_by_exact_price_model: u64,
    pub evidence: ExecutionEvidence,
    pub primary_explanation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub result_type: String,
    pub symbol: String,
    pub scenario: String,
    pub split: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub funnel: Funnel,
    pub no_fill: bool,
    pub no_fill_explanations: BTreeMap<String, u64>,
    pub quotes_never_reached_by_exact_price_directional_trades: u64,
    pub quotes_not_reached_at_or_through_limit_in_valid_window: u64,
    pub quotes_with_live_trade_through_omitted_by_fill_model: u64,
    pub quotes_reached_but_queue_blocked: u64,
    pub quotes_affected_by_entry_latency: u64,
    pub quotes_canceled_before_later_potential_execution: u64,
    pub invalid_market_data: bool,
    pub execution_arrival_rejections: u64,
    pub quotes_end_with_unresolved_latency: u64,
    pub orders: Vec<OrderExplanation>,
    pub definitions: Vec<String>,
    pub lifetime_distribution_ms: BTreeMap<String, u128>,
    pub spread_fee_feasibility: serde_json::Value,
    pub fill_probability_diagnostics: serde_json::Value,
}
impl Engine {
    pub(crate) fn diagnostic_decision(
        &mut self,
        buy: bool,
        price: u64,
        quantity: u64,
        outcome: &str,
        reason: &str,
    ) -> Result<()> {
        ensure!(
            self.decisions.len() < 100_000,
            "bounded diagnostic decision limit exceeded"
        );
        let best_bid = self.view.as_ref().map(|v| v.bid());
        let best_ask = self.view.as_ref().map(|v| v.ask());
        let distance = self.view.as_ref().map(|v| {
            let reference = if buy { v.bid() } else { v.ask() };
            let diff = if buy {
                i128::from(reference) - i128::from(price)
            } else {
                i128::from(price) - i128::from(reference)
            };
            diff * 10_000_000_000 / i128::from(reference)
        });
        self.decisions.push(Decision {
            best_bid,
            best_ask,
            distance_from_same_side_microbps: distance,
            observed_spread_microbps: self
                .view
                .as_ref()
                .map(|v| u128::from(v.ask() - v.bid()) * 10_000_000_000 / u128::from(v.mid())),
            roundtrip_maker_fee_microbps: self
                .scenario
                .fees
                .at(self.clock, &self.instrument.symbol)
                .and_then(|f| f.roundtrip_microbps())
                .ok(),
            time_ms: self.clock,
            buy,
            price,
            quantity,
            outcome: outcome.into(),
            reason: reason.into(),
        });
        if outcome == "rejected" {
            *self
                .diagnostics
                .rejection_reasons
                .entry(reason.into())
                .or_default() += 1;
        }
        Ok(())
    }
    pub fn diagnostic_report(
        &self,
        tape: &[Tick],
        split: &str,
        start: u64,
        end: u64,
    ) -> Result<Report> {
        let mut orders = vec![];
        for submit in self.logs.iter().filter(|l| l.action == "submit") {
            let logs: Vec<_> = self
                .logs
                .iter()
                .filter(|l| l.order_id == submit.order_id)
                .collect();
            let entry_due = self
                .entry_deadlines
                .get(&submit.order_id)
                .copied()
                .expect("submitted deadline recorded");
            let accepted = logs
                .iter()
                .find(|l| l.action == "accept")
                .map(|l| l.time_ms);
            let cancel_request = logs
                .iter()
                .find(|l| l.action == "cancel_request")
                .map(|l| l.time_ms);
            let cancel = self.cancel_deadlines.get(&submit.order_id).copied();
            let mut terminal = logs
                .iter()
                .find(|l| matches!(l.action.as_str(), "reject" | "cancel_ack" | "coverage_lost"));
            let evidence = self
                .execution_evidence
                .get(&submit.order_id)
                .cloned()
                .unwrap_or_default();
            if evidence.filled_quantity >= u128::from(submit.quantity) {
                terminal = logs.iter().rev().find(|l| l.action == "fill");
            }
            let live_end = terminal
                .map(|l| l.time_ms)
                .into_iter()
                .chain(cancel)
                .min()
                .unwrap_or(end)
                .min(end);
            let mut reached = 0;
            let mut before = 0;
            let mut after = 0;
            let mut through = 0;
            for tick in tape
                .iter()
                .filter(|t| t.time_ms >= start && t.time_ms < end)
            {
                if let Input::Trade { trade } = &tick.input {
                    if trade.exchange_ms < submit.time_ms || trade.buyer_is_maker != submit.buy {
                        continue;
                    }
                    if (submit.buy && trade.price < submit.price)
                        || (!submit.buy && trade.price > submit.price)
                    {
                        through += 1;
                    }
                    if trade.price != submit.price {
                        continue;
                    }
                    reached += 1;
                    before += u64::from(trade.exchange_ms <= entry_due);
                    after += u64::from(cancel.is_some_and(|c| trade.exchange_ms > c));
                }
            }
            let primary = if evidence.filled_quantity > 0 {
                "filled"
            } else if terminal.is_some_and(|l| l.action == "reject") {
                "execution_arrival_rejected"
            } else if evidence.queue_blocked_trade_events > 0 {
                "reached_queue_not_depleted"
            } else if evidence.quantity_blocked_by_lot_rounding > 0 {
                "eligible_residual_below_lot"
            } else if evidence.trade_through_valid_window_events > 0 {
                "trade_through_evidence_omitted_by_exact_price_fill_model"
            } else if before > 0 {
                "reached_before_entry_latency_elapsed"
            } else if after > 0 {
                "canceled_before_later_directional_trade_counterfactual"
            } else if terminal.is_some_and(|l| l.action == "coverage_lost") {
                "market_data_coverage_lost"
            } else if reached == 0 {
                "never_reached_by_exact_price_directional_trade"
            } else {
                "directional_trade_outside_valid_execution_window"
            };
            orders.push(OrderExplanation {
                end_state: self
                    .orders
                    .iter()
                    .find(|o| o.id == submit.order_id)
                    .map_or_else(
                        || {
                            terminal
                                .map_or("unknown", |l| l.action.as_str())
                                .to_string()
                        },
                        |o| {
                            if o.cancel_effective_logged {
                                "cancel_effective_awaiting_reports".into()
                            } else if o.cancel_due.is_some() {
                                "cancel_pending".into()
                            } else if o.active {
                                "open".into()
                            } else {
                                "entry_pending".into()
                            }
                        },
                    ),
                initial_queue_ahead_estimate: self
                    .initial_queue_ahead
                    .get(&submit.order_id)
                    .copied(),
                lifetime_ms: live_end.saturating_sub(submit.time_ms),
                lifetime_right_censored: terminal.is_none() && cancel.is_none_or(|t| t >= end),
                active_lifetime_ms: accepted.map(|t| live_end.saturating_sub(t)),
                order_id: submit.order_id,
                buy: submit.buy,
                price: submit.price,
                quantity: submit.quantity,
                submitted_ms: submit.time_ms,
                entry_due_ms: entry_due,
                accepted_ms: accepted,
                cancel_requested_ms: cancel_request,
                cancel_effective_ms: cancel,
                terminal_ms: terminal.map(|l| l.time_ms),
                terminal_reason: terminal.map(|l| l.reason.clone()),
                exact_price_directional_trade_events_since_submission: reached,
                exact_price_directional_trade_events_before_entry: before,
                exact_price_directional_trade_events_after_cancel: after,
                trade_through_events_ignored_by_exact_price_model: through,
                evidence,
                primary_explanation: primary.into(),
            });
        }
        ensure!(
            self.diagnostics.generated_quote_intents
                == self.diagnostics.submitted_to_simulated_execution
                    + self.diagnostics.rejected_fee_constraints
                    + self.diagnostics.rejected_risk_limits
                    + self.diagnostics.rejected_exchange_filters
                    + self.diagnostics.rejected_market_data,
            "diagnostic intent funnel does not reconcile"
        );
        let mut explanations = BTreeMap::new();
        for o in &orders {
            if o.evidence.filled_quantity == 0 {
                *explanations
                    .entry(o.primary_explanation.clone())
                    .or_default() += 1;
            }
        }
        if orders.is_empty() {
            *explanations
                .entry("no_order_passed_intent_gates".into())
                .or_default() += 1;
        }
        let lifetime_distribution_ms =
            distribution(orders.iter().map(|o| u128::from(o.lifetime_ms)).collect());
        let observed = distribution(
            self.decisions
                .iter()
                .filter_map(|d| d.observed_spread_microbps)
                .collect(),
        );
        let fees = distribution(
            self.decisions
                .iter()
                .filter_map(|d| d.roundtrip_maker_fee_microbps)
                .collect(),
        );
        let filled_orders = orders
            .iter()
            .filter(|o| o.evidence.filled_quantity > 0)
            .count();
        let spread_fee_feasibility = serde_json::json!({"observed_quoted_spread_microbps":observed,"roundtrip_maker_fee_microbps":fees,"required_safety_edge_bps":self.scenario.strategy.required_edge_bps,"max_quote_spread_bps":self.scenario.strategy.max_quote_spread_bps,"fee_guard_behavior":"widen to fees + safety, reject only if required width exceeds configured cap","quoted_spread_is_not_execution_edge":true});
        let fill_probability_diagnostics = serde_json::json!({"accepted_orders":self.accepted,"orders_with_at_least_one_fill":filled_orders,"empirical_fill_fraction":if self.accepted>0 {Some(filled_orders as f64/self.accepted as f64)}else{None},"calibrated_fill_probability":null,"probability_not_identifiable_from_this_short_mbp_sample":true});
        Ok(Report {lifetime_distribution_ms,spread_fee_feasibility,fill_probability_diagnostics,result_type:"simulated_execution_diagnostics".into(),symbol:self.instrument.symbol.clone(),scenario:self.scenario.name.clone(),split:split.into(),start_ms:start,end_ms:end,funnel:self.diagnostics.clone(),no_fill:self.fills.is_empty(),no_fill_explanations:explanations,quotes_not_reached_at_or_through_limit_in_valid_window:orders.iter().filter(|o|o.evidence.exact_price_eligible_trade_events==0 && o.evidence.trade_through_valid_window_events==0).count() as u64,quotes_with_live_trade_through_omitted_by_fill_model:orders.iter().filter(|o|o.evidence.trade_through_valid_window_events>0).count() as u64,quotes_never_reached_by_exact_price_directional_trades:orders.iter().filter(|o|o.exact_price_directional_trade_events_since_submission==0).count() as u64,quotes_reached_but_queue_blocked:orders.iter().filter(|o|o.evidence.filled_quantity==0 && o.evidence.queue_blocked_trade_events>0).count() as u64,quotes_affected_by_entry_latency:orders.iter().filter(|o|o.exact_price_directional_trade_events_before_entry>0).count() as u64,quotes_canceled_before_later_potential_execution:orders.iter().filter(|o|o.exact_price_directional_trade_events_after_cancel>0).count() as u64,execution_arrival_rejections:self.logs.iter().filter(|l|l.action=="reject").count() as u64,quotes_end_with_unresolved_latency:self.orders.iter().filter(|o|!o.active || o.cancel_due.is_some()).count() as u64,invalid_market_data:!self.finish(split,start,end)?.data_quality_ok,orders,definitions:vec!["One quote opportunity is one side at a refresh-eligible valid-book evaluation; existing quotes retained/canceling are separate from newly generated submit intents.".into(),"Fee guard widens quotes and rejects only when its required width exceeds max_quote_spread_bps; zero rejections can coexist with fee widening.".into(),"Intent rejections are mutually exclusive, but reach/latency/cancellation/queue flags may overlap and must not be summed.".into(),"Reached means an exact-price trade with the required aggressor direction after submission. Later post-cancel matches are retrospective counterfactual opportunities, not fills or proven executable volume.".into(),"Queue blocking refers only to actual valid-window evaluation. No claims use future validation/test trades. Trade-through counts expose the exact-price model's deliberately omitted evidence.".into(),format!("Prices, quantities and monetary atoms use fixed-point scale {S}.")]})
    }
}

pub fn distribution(mut values: Vec<u128>) -> BTreeMap<String, u128> {
    values.sort_unstable();
    let mut out = BTreeMap::new();
    out.insert("count".into(), values.len() as u128);
    if !values.is_empty() {
        for (key, percent) in [
            ("min", 0),
            ("p10", 10),
            ("p50", 50),
            ("p90", 90),
            ("max", 100),
        ] {
            out.insert(key.into(), values[(values.len() - 1) * percent / 100]);
        }
    }
    out
}

/// Diagnose exactly the pre-existing evaluated partitions; never open a new OOS test.
pub fn diagnose(
    root: &std::path::Path,
    config_path: &std::path::Path,
    baseline_path: &std::path::Path,
    output: &std::path::Path,
) -> Result<()> {
    use anyhow::Context;
    use sha2::{Digest, Sha256};
    use std::io::Write;
    let bytes = std::fs::read(config_path)?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let baseline: serde_json::Value = serde_json::from_slice(&std::fs::read(baseline_path)?)?;
    ensure!(
        baseline["config_sha256"].as_str() == Some(hash.as_str()),
        "configuration differs from frozen M4 experiment"
    );
    ensure!(
        baseline["final_test_evaluated"] == false,
        "diagnostics must not newly evaluate final OOS"
    );
    let source = baseline["source"]
        .as_str()
        .context("baseline source missing")?;
    ensure!(
        std::fs::canonicalize(root)? == std::fs::canonicalize(source)?,
        "baseline source differs"
    );
    ensure!(
        output != baseline_path.parent().context("baseline output missing")?,
        "preserve original M4 reports; use a separate diagnostics directory"
    );
    let config: crate::config::Experiment = serde_json::from_slice(&bytes)?;
    config.validate()?;
    let manifest: mm_research::run::Manifest =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json"))?)?;
    let old: Vec<crate::engine::Summary> = serde_json::from_value(baseline["summaries"].clone())?;
    ensure!(
        !old.is_empty() && old.iter().all(|r| r.split != "test"),
        "no baseline experiments or held-out data requested"
    );
    std::fs::create_dir_all(output.join("decisions"))?;
    let mut reports = vec![];
    let mut comparisons = vec![];
    let mut source_audits = vec![];
    for instrument in &manifest.instruments {
        let (tape, audit) = crate::data::load_symbol(
            &root.join("recordings").join(&instrument.symbol),
            &instrument.symbol,
        )?;
        for previous in old.iter().filter(|r| r.symbol == instrument.symbol) {
            let scenario = config
                .scenarios
                .iter()
                .find(|s| s.name == previous.scenario)
                .context("baseline scenario missing")?;
            let engine = crate::experiment::simulate(
                &tape,
                instrument,
                scenario,
                &previous.split,
                previous.start_ms,
                previous.end_ms,
            )?;
            ensure!(
                engine.diagnostics.quote_opportunities
                    == engine.diagnostics.generated_quote_intents
                        + engine.diagnostics.existing_quote_retained_or_canceling,
                "strategy opportunity funnel does not reconcile"
            );
            let result = engine.finish(&previous.split, previous.start_ms, previous.end_ms)?;
            let mut report = engine.diagnostic_report(
                &tape,
                &previous.split,
                previous.start_ms,
                previous.end_ms,
            )?;
            if !audit.prefix_complete || !audit.sequence_contiguous || audit.trade_id_gaps > 0 {
                report.invalid_market_data = true;
                report
                    .funnel
                    .dependency_failures
                    .insert("incomplete_source_recording".into(), 1);
            }
            let name = format!(
                "{}-{}-{}",
                previous.symbol, previous.scenario, previous.split
            );
            let mut file = std::io::BufWriter::new(std::fs::File::create(
                output.join("decisions").join(format!("{name}.jsonl")),
            )?);
            for decision in &engine.decisions {
                let mut row = serde_json::to_value(decision)?;
                row.as_object_mut()
                    .unwrap()
                    .insert("result_type".into(), "simulated_quote_decision".into());
                serde_json::to_writer(&mut file, &row)?;
                file.write_all(b"\n")?;
            }
            file.flush()?;
            comparisons.push(serde_json::json!({"symbol":previous.symbol,"scenario":previous.scenario,"split":previous.split,"legacy_summary_identical":result==*previous,"previous_fill_count":previous.fill_count,"diagnostic_fill_count":result.fill_count,"synthetic_fills_included":false,"actual_exchange_executions":0}));
            reports.push(report);
        }
        source_audits.push(audit);
    }
    ensure!(
        reports.len() == old.len(),
        "not all baseline rows diagnosed"
    );
    let mut csv="result_type,symbol,scenario,split,quote_opportunities,intents,fee_rejected,risk_rejected,filter_rejected,data_rejected,submitted,never_reached,queue_blocked,entry_latency,canceled_before_later_match,live_trade_through_omissions,never_at_or_through_in_valid_window,unresolved_latency_at_end,invalid_data\n".to_string();
    let mut md="# M4.1 execution diagnostics — historical simulations, preliminary\n\nThese diagnostics rerun only the previously evaluated partitions with the frozen M4 configuration. No synthetic fills or actual exchange executions are included. Retrospective post-cancel matches are not fills. Counts across independent scenarios are not unique market events.\n\n| Market | Scenario | Opportunities | Intents | Risk rejections | Submitted | Never reached | Queue blocked | No-fill explanation |\n|---|---|---:|---:|---:|---:|---:|---:|---|\n".to_string();
    for r in &reports {
        let f = &r.funnel;
        csv.push_str(&format!(
            "simulated,{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            r.symbol,
            r.scenario,
            r.split,
            f.quote_opportunities,
            f.generated_quote_intents,
            f.rejected_fee_constraints,
            f.rejected_risk_limits,
            f.rejected_exchange_filters,
            f.rejected_market_data,
            f.submitted_to_simulated_execution,
            r.quotes_never_reached_by_exact_price_directional_trades,
            r.quotes_reached_but_queue_blocked,
            r.quotes_affected_by_entry_latency,
            r.quotes_canceled_before_later_potential_execution,
            r.quotes_with_live_trade_through_omitted_by_fill_model,
            r.quotes_not_reached_at_or_through_limit_in_valid_window,
            r.quotes_end_with_unresolved_latency,
            r.invalid_market_data
        ));
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            r.symbol,
            r.scenario,
            f.quote_opportunities,
            f.generated_quote_intents,
            f.rejected_risk_limits,
            f.submitted_to_simulated_execution,
            r.quotes_never_reached_by_exact_price_directional_trades,
            r.quotes_reached_but_queue_blocked,
            r.no_fill_explanations
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    md.push_str("\nZero fills provide insufficient evidence of executable edge; they are not evidence that a market never has an edge. Report funnel gates, queue assumptions and missing history separately. A zero empirical fill fraction is not a calibrated probability estimate. Fee guard widening is counted separately from explicit fee-cap rejection. See docs/MILESTONE_4_1.md for methods, tests and data readiness.\n");
    std::fs::write(
        output.join("execution_diagnostics.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"schema_version":1,"result_type":"historical_simulation_diagnostics","source":root,"config_sha256":hash,"original_results":baseline_path,"final_test_evaluated":false,"experiments":reports,"baseline_comparison":comparisons,"source_audits":source_audits,"actual_exchange_executions":0,"synthetic_test_fills_included":false}),
        )?,
    )?;
    std::fs::write(output.join("execution_diagnostics.csv"), csv)?;
    std::fs::write(output.join("execution_diagnostics.md"), md)?;
    std::fs::write(output.join("experiment.json"), bytes)?;
    Ok(())
}

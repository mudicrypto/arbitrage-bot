//! Recorded-order execution regression, not a strategy-return estimate.
use crate::{
    config::Experiment,
    data::{Input, Tick, load_symbol},
    engine::Engine,
    portfolio::ExecutionModel,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Debug, Serialize, Deserialize)]
pub struct Case {
    pub result_type: String,
    pub source: String,
    pub symbol: String,
    pub source_order: serde_json::Value,
    pub source_scenarios: Vec<String>,
    pub instrument: mm_research::discovery::Instrument,
    pub ticks: Vec<Tick>,
    pub provenance: serde_json::Value,
}
pub fn replay(case: &Case, model: ExecutionModel) -> Result<Engine> {
    let config: Experiment = serde_json::from_str(include_str!("../../../config/backtest.json"))?;
    let scenario = config
        .scenarios
        .into_iter()
        .find(|s| s.name == "standard_conservative")
        .unwrap();
    let mut e = Engine::new(scenario, case.instrument.clone(), 5000)?;
    e.execution_model = model;
    e.automatic_quoting = false;
    let order = &case.source_order;
    let submit = order["submitted_ms"].as_u64().context("submit missing")?;
    let cancel = order["cancel_requested_ms"]
        .as_u64()
        .context("cancel missing")?;
    let mut submitted = false;
    let mut canceled = false;
    for tick in &case.ticks {
        if !submitted && tick.time_ms >= submit {
            e.clock = submit;
            ensure!(
                e.submit(
                    true,
                    order["price"].as_u64().unwrap(),
                    order["quantity"].as_u64().unwrap()
                )?,
                "recorded-order replay intent rejected"
            );
            e.orders[0].entry_due = order["entry_due_ms"].as_u64().unwrap();
            submitted = true;
        }
        if submitted && !canceled && tick.time_ms >= cancel {
            e.clock = cancel;
            if let Some(o) = e.orders.first() {
                let id = o.id;
                e.cancel(id)?;
                if let Some(o) = e.orders.first_mut() {
                    o.cancel_due = order["cancel_effective_ms"].as_u64();
                }
            }
            canceled = true;
        }
        e.step(tick)?;
    }
    e.reconcile()?;
    Ok(e)
}
pub fn export(root: &Path, previous: &Path, output: &Path) -> Result<()> {
    let reports: serde_json::Value = serde_json::from_slice(&std::fs::read(previous)?)?;
    let previous = reports["experiments"]
        .as_array()
        .context("diagnostics missing")?
        .iter()
        .find(|r| r["symbol"] == "ZECUSDT" && r["scenario"] == "zero_maker_hypothetical")
        .context("previous ZEC case absent")?;
    let order = previous["orders"]
        .as_array()
        .context("orders missing")?
        .iter()
        .find(|o| o["order_id"] == 4)
        .context("order 4 missing")?
        .clone();
    let manifest: mm_research::run::Manifest =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json"))?)?;
    let instrument = manifest
        .instruments
        .into_iter()
        .find(|i| i.symbol == "ZECUSDT")
        .context("instrument missing")?;
    let (tape, audit) = load_symbol(&root.join("recordings/ZECUSDT"), "ZECUSDT")?;
    let submit = order["submitted_ms"].as_u64().unwrap();
    let end = order["terminal_ms"].as_u64().unwrap() + 200;
    let start = tape
        .iter()
        .rposition(|t| t.time_ms < submit && matches!(t.input, Input::Book { .. }))
        .context("causal pre-submit book missing")?;
    let mut ticks: Vec<_> = tape
        .into_iter()
        .skip(start)
        .take_while(|t| t.time_ms <= end)
        .collect();
    let limit = order["price"].as_u64().unwrap();
    let lowest_trade = ticks
        .iter()
        .filter_map(|t| {
            if let Input::Trade { trade } = &t.input {
                Some(trade.price)
            } else {
                None
            }
        })
        .min()
        .unwrap_or(limit);
    for tick in &mut ticks {
        if let Input::Book { view, .. } = &mut tick.input {
            let first = view.bids.first().copied();
            let last = view.bids.last().copied();
            view.bids.retain(|(p, _)| {
                *p >= lowest_trade && *p <= limit
                    || first.is_some_and(|v| v.0 == *p)
                    || last.is_some_and(|v| v.0 == *p)
            });
            let first = view.asks.first().copied();
            let last = view.asks.last().copied();
            view.asks.retain(|v| Some(*v) == first || Some(*v) == last);
        }
    }
    let case = Case {
        result_type: "recorded_order_execution_regression_not_strategy_profitability".into(),
        source: root.display().to_string(),
        symbol: "ZECUSDT".into(),
        source_order: order,
        source_scenarios: vec![
            "zero_maker_hypothetical".into(),
            "maker_rebate_hypothetical".into(),
        ],
        instrument,
        ticks,
        provenance: serde_json::json!({"audit":audit,"independent_market_evidence_count":1,"source_scenarios_reuse_same_observation":true,"manual_regression_fees":"standard 10bps maker; original hypothetical fee eligibility is not assumed","book_projection":"Original bid levels between order limit and lowest trade, plus observed best/farthest bid/ask anchors; irrelevant outside levels omitted without changing through barriers or reference prices."}),
    };
    let mut comparisons = vec![];
    for model in [
        ExecutionModel::ExactPrice,
        ExecutionModel::ConservativeThrough,
        ExecutionModel::AlternativeThrough,
    ] {
        let e = replay(&case, model)?;
        comparisons.push(serde_json::json!({"model":model,"fills":e.fills,"fee_atoms":e.fees,"balances":e.balances(),"valid_data":e.data_ok,"actual_exchange_executions":0,"strategy_profitability_result":false}));
    }
    std::fs::create_dir_all(output)?;
    std::fs::write(
        output.join("recorded_trade_through_case.json"),
        serde_json::to_vec_pretty(&case)?,
    )?;
    std::fs::write(
        output.join("trade_through_case_results.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"result_type":"historical_order_execution_model_regression","independent_market_case_count":1,"comparisons":comparisons,"actual_exchange_executions":0}),
        )?,
    )?;
    Ok(())
}

use crate::{
    config::Experiment,
    data::{Input, Tick, audit_all, load_symbol},
    engine::{Engine, Summary},
};
use anyhow::{Context, Result, ensure};
use mm_research::run::Manifest;
use serde::Serialize;
use std::{collections::BTreeMap, io::Write, path::Path};
#[derive(Serialize)]
pub struct Results {
    pub schema_version: u16,
    pub label: String,
    pub source: String,
    pub config_sha256: String,
    pub selection_policy: String,
    pub final_test_evaluated: bool,
    pub summaries: Vec<Summary>,
    pub limitations: Vec<String>,
}
pub fn simulate(
    tape: &[Tick],
    instrument: &mm_research::discovery::Instrument,
    scenario: &crate::config::Scenario,
    _split: &str,
    start: u64,
    end: u64,
) -> Result<Engine> {
    let mut engine = Engine::new(scenario.clone(), instrument.clone(), 5000)?;
    let mut prior_book = None;
    for tick in tape.iter().filter(|t| t.time_ms < start) {
        if matches!(tick.input, Input::Book { .. }) {
            prior_book = Some(tick.clone());
        } else if matches!(tick.input, Input::Invalid { .. }) {
            prior_book = None;
        }
    }
    if let Some(mut book) = prior_book {
        // Warm only if the book was available and fresh at the boundary. No orders before the split.
        if start.saturating_sub(book.time_ms) <= scenario.strategy.stale_ms {
            book.time_ms = start;
            engine.step(&book)?;
        }
    }
    for tick in tape
        .iter()
        .filter(|t| t.time_ms >= start && t.time_ms < end)
    {
        let settle_at = end.saturating_sub(
            scenario.cancel_latency_ms
                + scenario.latency_jitter_ms
                + scenario.report_latency_ms
                + 1,
        );
        if tick.time_ms >= settle_at && !engine.settling {
            engine.clock = tick.time_ms;
            engine.settling = true;
            let ids: Vec<_> = engine.orders.iter().map(|o| o.id).collect();
            for id in ids {
                engine.cancel(id)?;
            }
        }
        engine.step(tick)?;
    }
    engine.reconcile()?;
    Ok(engine)
}
fn jsonl<T: Serialize>(path: &Path, rows: &[T]) -> Result<()> {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    for r in rows {
        let mut row = serde_json::to_value(r)?;
        row.as_object_mut()
            .context("report row must be an object")?
            .insert("result_type".into(), "simulated".into());
        serde_json::to_writer(&mut file, &row)?;
        file.write_all(b"\n")?;
    }
    file.flush()?;
    Ok(())
}
pub fn run(root: &Path, config_file: &Path, output: &Path, include_test: bool) -> Result<Results> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(config_file)?;
    let config: Experiment = serde_json::from_slice(&bytes)?;
    config.validate()?;
    if root.join("collection-config.json").exists() {
        ensure!(
            root.join("collection_finished.json").exists(),
            "long capture still active or unclean; audit and close it before backtesting"
        );
    }
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(root.join("manifest.json"))?)?;
    std::fs::create_dir_all(output.join("executions"))?;
    std::fs::create_dir_all(output.join("ledgers"))?;
    std::fs::create_dir_all(output.join("equity"))?;
    let audit = audit_all(&root.join("recordings"))?;
    std::fs::write(
        output.join("data_audit.json"),
        serde_json::to_vec_pretty(&audit)?,
    )?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    std::fs::write(output.join("experiment.json"), &bytes)?;
    let mut summaries = vec![];
    let mut data_audits = vec![];
    let mut split_manifest = BTreeMap::new();
    for instrument in &manifest.instruments {
        let (tape, audit) = load_symbol(
            &root.join("recordings").join(&instrument.symbol),
            &instrument.symbol,
        )?;
        ensure!(
            audit.trades > 0 && audit.book_updates > 0,
            "depth-only history cannot support trade-evidence fills"
        );
        let (start, end) = manifest
            .bounds
            .get(&instrument.symbol)
            .copied()
            .context("symbol observation bounds missing")?;
        let duration = end.saturating_sub(start);
        let train_end = start + duration * config.train_fraction_ppm / 1_000_000;
        let validation_end = train_end + duration * config.validation_fraction_ppm / 1_000_000;
        let validation_start = (train_end + config.split_embargo_ms).min(validation_end);
        let test_start = (validation_end + config.split_embargo_ms).min(end);
        split_manifest.insert(instrument.symbol.clone(),serde_json::json!({"train":[start,train_end],"validation":[validation_start,validation_end],"test":[test_start,end],"embargo_ms":config.split_embargo_ms,"final_test_held_out":!include_test,"short_sample_empty_splits_are_not_validation":true}));
        for scenario in &config.scenarios {
            let mut splits = vec![
                ("train", start, train_end),
                ("validation", validation_start, validation_end),
            ];
            if include_test {
                splits.push(("test", test_start, end));
            }
            for (split, a, b) in splits {
                if b <= a {
                    continue;
                }
                let engine = simulate(&tape, instrument, scenario, split, a, b)?;
                let mut summary = engine.finish(split, a, b)?;
                if !audit.prefix_complete || !audit.sequence_contiguous || audit.trade_id_gaps > 0 {
                    summary.data_quality_ok = false;
                    summary.net_positive_after_costs_and_without_favorable_inventory = false;
                    summary
                        .warnings
                        .push("incomplete/fragmented input coverage".into());
                }
                let name = format!("{}-{}-{}", instrument.symbol, scenario.name, split);
                jsonl(
                    &output.join("executions").join(format!("{name}.jsonl")),
                    &engine.logs,
                )?;
                jsonl(
                    &output.join("ledgers").join(format!("{name}.jsonl")),
                    &engine.ledger,
                )?;
                jsonl(
                    &output.join("equity").join(format!("{name}.jsonl")),
                    &engine.equity,
                )?;
                summaries.push(summary);
            }
        }
        data_audits.push(audit);
    }
    std::fs::write(
        output.join("split_manifest.json"),
        serde_json::to_vec_pretty(&split_manifest)?,
    )?;
    std::fs::write(
        output.join("selected_data_audit.json"),
        serde_json::to_vec_pretty(&data_audits)?,
    )?;
    let results=Results {schema_version:1,label:"PRELIMINARY_SIMULATED_ON_SHORT_OBSERVED_DATA".into(),source:root.display().to_string(),config_sha256:hash,selection_policy:"Predeclared scenario grid; no optimization or selection based on held-out test outcomes. Final test is disabled by default; split embargo is 60 seconds or longer.".into(),final_test_evaluated:include_test,summaries,limitations:vec!["Short M3 history is not representative; empty validation/test intervals after embargo cannot establish out-of-sample performance.".into(),"Independent $5000 one-market simulations, not simultaneously funded multi-market portfolios.".into(),"MBP and aggregate trades cannot identify exact queues, hidden priority or individual counterparties. Results are modeled, never observed own fills.".into(),"No-trade benchmark is $5000 cash, zero exposure/fees. Inventory is acquired only through evidence-based modeled fills, never invented initial stock.".into(),"No account-eligible promotion or rebate has been verified. Hypothetical fees must not be presented as accessible tiers.".into()]};
    std::fs::write(
        output.join("backtest_results.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    let mut csv = String::from(
        "result_type,symbol,scenario,split,fills,net_pnl_usdt,inventory_pnl_usdt,fees_usdt,closeout_adjusted_usdt,max_drawdown_usdt,data_quality_ok,hypothetical\n",
    );
    let mut md = String::from(
        "# Preliminary execution backtest results\n\nAll executions and P&L are **simulated on observed short public data**. Hypothetical fee/stress cases are labeled. No trading, private endpoints or capital allocation occurred.\n\n| Market | Scenario | Split | Fills | Net P&L USDT | Inventory P&L | Fees | Closeout-adjusted P&L |\n|---|---|---|---:|---:|---:|---:|---:|\n",
    );
    for r in &results.summaries {
        csv.push_str(&format!(
            "simulated,{},{},{},{},{:.8},{:.8},{:.8},{:.8},{:.8},{},{}\n",
            r.symbol,
            r.scenario,
            r.split,
            r.fill_count,
            r.net_pnl_atoms as f64 / 1e8,
            r.inventory_pnl_atoms as f64 / 1e8,
            r.maker_fees_atoms as f64 / 1e8,
            r.closeout_adjusted_pnl_atoms as f64 / 1e8,
            r.max_drawdown_atoms as f64 / 1e8,
            r.data_quality_ok,
            r.hypothetical
        ));
        md.push_str(&format!(
            "| {} | {}{} | {} | {} | {:.8} | {:.8} | {:.8} | {:.8} |\n",
            r.symbol,
            r.scenario,
            if r.hypothetical {
                " (hypothetical)"
            } else {
                " (manual unverified fees)"
            },
            r.split,
            r.fill_count,
            r.net_pnl_atoms as f64 / 1e8,
            r.inventory_pnl_atoms as f64 / 1e8,
            r.maker_fees_atoms as f64 / 1e8,
            r.closeout_adjusted_pnl_atoms as f64 / 1e8
        ));
    }
    md.push_str("\nNo conclusion of profitability is justified by this dataset. Zero fills imply no demonstrated executable opportunity, not a profitable strategy. Inventory gains are separated and cannot alone qualify a result. Markouts are reported only with sufficient observed future history and do not cross coverage breaks.\n\nFinal OOS evaluation is disabled by default. The current sample cannot support a 60-second-embargoed test interval; collect longer untouched data before evaluating final test.\n");
    let mut daily = String::from(
        "result_type,symbol,scenario,split,date_Europe_Moscow,pnl_usdt,max_drawdown_usdt,partial_day\n",
    );
    let mut attribution = String::from(
        "result_type,symbol,scenario,split,gross_spread_usdt,maker_fees_usdt,hypothetical_rebates_usdt,inventory_pnl_usdt,net_pnl_usdt,closeout_spread_usdt,stress_slippage_usdt,estimated_taker_usdt\n",
    );
    let mut comparison = String::from(
        "result_type,symbol,scenario,split,strategy_pnl_usdt,no_trade_pnl_usdt,without_positive_inventory_usdt,profitable_after_all_modeled_costs\n",
    );
    let mut queue = String::from(
        "# Queue-model sensitivity — simulated, preliminary\n\nCompare standard_conservative (trade-only depletion) with standard_cancel_credit (proportional cancellation advancement; optimistic modeling alternative). Cancellations never directly create fills. All orders require exact-price aggressive-trade evidence, entry eligibility and depleted queue. Other scenario differences are not isolated queue effects.\n\n| Market | Scenario | Split | Fills | P&L USDT |\n|---|---|---|---:|---:|\n",
    );
    for r in &results.summaries {
        for (day, pnl) in &r.daily_pnl_atoms {
            daily.push_str(&format!(
                "simulated,{},{},{},{},{:.8},{:.8},true\n",
                r.symbol,
                r.scenario,
                r.split,
                day,
                *pnl as f64 / 1e8,
                *r.daily_max_drawdown_atoms.get(day).unwrap_or(&0) as f64 / 1e8
            ));
        }
        attribution.push_str(&format!(
            "simulated,{},{},{},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8}\n",
            r.symbol,
            r.scenario,
            r.split,
            r.gross_spread_capture_atoms as f64 / 1e8,
            r.maker_fees_atoms as f64 / 1e8,
            r.rebate_atoms as f64 / 1e8,
            r.inventory_pnl_atoms as f64 / 1e8,
            r.net_pnl_atoms as f64 / 1e8,
            r.closeout_spread_cost_atoms as f64 / 1e8,
            r.slippage_stress_cost_atoms as f64 / 1e8,
            r.estimated_taker_fee_atoms as f64 / 1e8
        ));
        comparison.push_str(&format!(
            "simulated,{},{},{},{:.8},0,{:.8},{}\n",
            r.symbol,
            r.scenario,
            r.split,
            r.net_pnl_atoms as f64 / 1e8,
            (r.closeout_adjusted_pnl_atoms - r.inventory_pnl_atoms.max(0)) as f64 / 1e8,
            r.net_positive_after_costs_and_without_favorable_inventory
        ));
        if matches!(
            r.scenario.as_str(),
            "standard_conservative" | "standard_cancel_credit"
        ) {
            queue.push_str(&format!(
                "| {} | {} | {} | {} | {:.8} |\n",
                r.symbol,
                r.scenario,
                r.split,
                r.fill_count,
                r.net_pnl_atoms as f64 / 1e8
            ));
        }
    }
    std::fs::write(output.join("daily_pnl.csv"), daily)?;
    std::fs::write(output.join("attribution.csv"), attribution)?;
    std::fs::write(output.join("strategy_vs_no_trade.csv"), comparison)?;
    std::fs::write(output.join("queue_sensitivity.md"), queue)?;
    std::fs::write(output.join("summary.md"), md)?;
    std::fs::write(output.join("results.csv"), csv)?;
    Ok(results)
}

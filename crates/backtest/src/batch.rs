//! Bounded offline sessions, explicitly isolated alternative portfolios; no pooled P&L.
use crate::{
    config::Experiment,
    data::{Input, Tick, stream_symbol},
    engine::{Engine, LedgerEntry, S},
    portfolio::{ExecutionModel, Initialization},
    readiness::{FrozenFile, inspect_frozen},
};
use anyhow::{Context, Result, ensure};
use mm_market_data::recording::segment_paths;
use mm_research::{discovery::Instrument, fees::notional, run::Manifest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub root: PathBuf,
    #[serde(default)]
    pub legacy_completed_research: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub capital_scope: String,
    #[serde(default)]
    pub shared_market_capital_usdt: BTreeMap<String, u64>,
    pub sources: Vec<Source>,
    pub experiment: PathBuf,
    pub current_metadata: PathBuf,
    pub base_allocations_ppm: Vec<u64>,
    pub execution_models: Vec<ExecutionModel>,
    pub scenario_names: Vec<String>,
    pub fee_buffer_ppm: u64,
    pub max_output_bytes: u64,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.capital_scope == "isolated_alternatives_5000_each_do_not_sum"
                || self.capital_scope == "partitioned_single_5000_budget",
            "capital scope must explicitly declare isolated or partitioned portfolios"
        );
        if self.capital_scope == "partitioned_single_5000_budget" {
            crate::portfolio::SharedBudget {
                total_usdt: 5000,
                markets: self.shared_market_capital_usdt.clone(),
            }
            .validate()?;
        } else {
            ensure!(
                self.shared_market_capital_usdt.is_empty(),
                "isolated mode cannot contain shared partitions"
            );
        }
        ensure!(
            !self.sources.is_empty()
                && self.sources.len() <= 32
                && !self.base_allocations_ppm.is_empty()
                && self.base_allocations_ppm.len() <= 4
                && self.base_allocations_ppm.iter().all(|v| *v <= 1_000_000)
                && self.execution_models.len() <= 3
                && !self.execution_models.is_empty()
                && self.scenario_names.len() <= 4
                && !self.scenario_names.is_empty()
                && self.fee_buffer_ppm <= 1_000_000
                && (16 * 1024 * 1024..=1024 * 1024 * 1024).contains(&self.max_output_bytes),
            "invalid bounded batch configuration"
        );
        ensure!(
            self.base_allocations_ppm
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                == self.base_allocations_ppm.len(),
            "duplicate allocation"
        );
        ensure!(
            self.scenario_names.iter().collect::<BTreeSet<_>>().len() == self.scenario_names.len(),
            "duplicate scenario"
        );
        ensure!(
            self.execution_models
                .iter()
                .map(|m| format!("{m:?}"))
                .collect::<BTreeSet<_>>()
                .len()
                == self.execution_models.len(),
            "duplicate execution model"
        );
        Ok(())
    }
}
pub fn metadata_instrument(
    old: &Instrument,
    metadata: &serde_json::Value,
) -> Result<(Instrument, (u64, u64), bool)> {
    use mm_market_data::model::units;
    let s = metadata["exchange_info"]["symbols"]
        .as_array()
        .context("official symbols missing")?
        .iter()
        .find(|s| s["symbol"] == old.symbol)
        .context("symbol absent from current metadata")?;
    ensure!(
        s["status"] == "TRADING"
            && s["quoteAsset"] == "USDT"
            && s["baseAsset"] == old.base_asset
            && s["isSpotTradingAllowed"] == true,
        "current symbol not eligible"
    );
    let filters = s["filters"].as_array().context("filters missing")?;
    let filter = |kind: &str| {
        filters
            .iter()
            .find(|f| f["filterType"] == kind)
            .context("required filter absent")
    };
    let value =
        |f: &serde_json::Value, key: &str| units(f[key].as_str().context("filter decimal absent")?);
    let price = filter("PRICE_FILTER")?;
    let lot = filter("LOT_SIZE")?;
    let nf = filter("NOTIONAL").or_else(|_| filter("MIN_NOTIONAL"))?;
    let mut out = old.clone();
    out.tick_units = value(price, "tickSize")?;
    out.step_units = value(lot, "stepSize")?;
    out.min_qty_units = value(lot, "minQty")?;
    out.max_qty_units = value(lot, "maxQty")?;
    out.min_notional_units = value(nf, "minNotional")?;
    out.max_notional_units = nf
        .get("maxNotional")
        .map(|_| value(nf, "maxNotional"))
        .transpose()?
        .filter(|n| *n > 0);
    ensure!(
        out.tick_units > 0 && out.step_units > 0,
        "unsupported zero filter increments"
    );
    let matches = old.tick_units == out.tick_units
        && old.step_units == out.step_units
        && old.min_qty_units == out.min_qty_units
        && old.max_qty_units == out.max_qty_units
        && old.min_notional_units == out.min_notional_units
        && old.max_notional_units == out.max_notional_units;
    Ok((
        out,
        (value(price, "minPrice")?, value(price, "maxPrice")?),
        matches,
    ))
}
pub async fn fetch_metadata(manifest: &Path, output: &Path) -> Result<()> {
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(manifest)?)?;
    let symbols: Vec<_> = manifest
        .instruments
        .iter()
        .map(|i| i.symbol.clone())
        .collect();
    ensure!(
        !symbols.is_empty() && symbols.len() <= 20,
        "invalid metadata universe"
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut url = reqwest::Url::parse("https://data-api.binance.vision/api/v3/exchangeInfo")?;
    url.query_pairs_mut()
        .append_pair("symbols", &serde_json::to_string(&symbols)?);
    let (raw, info) = mm_research::discovery::public_json(&client, url.as_str()).await?;
    let value = serde_json::json!({"source":"Binance Spot public exchangeInfo","endpoint":"https://data-api.binance.vision/api/v3/exchangeInfo","fetched_at_ms":mm_market_data::runtime::now_ms(),"raw_response_sha256":format!("{:x}",Sha256::digest(raw.as_bytes())),"exchange_info":info,"account_commissions_verified":false});
    for old in &manifest.instruments {
        metadata_instrument(old, &value)?;
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, serde_json::to_vec_pretty(&value)?)?;
    Ok(())
}
#[derive(Default)]
struct Counts {
    fills: u64,
    partial: u64,
    complete: u64,
    exact: u64,
    through: u64,
    exact_opportunities: u64,
    through_opportunities: u64,
    max_active: usize,
    max_exposure: u128,
    daily: BTreeMap<String, i128>,
    previous_equity: i128,
    previous_exposure: u128,
    last_time: Option<u64>,
    exposure_integral: f64,
}
struct Trial {
    engine: Engine,
    allocation: u64,
    model: ExecutionModel,
    writer: BufWriter<std::fs::File>,
    counts: Counts,
    initialized: crate::portfolio::Balances,
}
fn write_row<T: Serialize>(
    writer: &mut BufWriter<std::fs::File>,
    kind: &str,
    value: &T,
    bytes: &mut u64,
    max: u64,
) -> Result<()> {
    let row = serde_json::to_vec(
        &serde_json::json!({"result_type":"historical_simulation_not_actual_execution","kind":kind,"value":value}),
    )?;
    ensure!(
        bytes.saturating_add(row.len() as u64 + 1) <= max,
        "bounded batch output limit exceeded; incomplete output must not be interpreted"
    );
    writer.write_all(&row)?;
    writer.write_all(b"\n")?;
    *bytes += row.len() as u64 + 1;
    Ok(())
}
impl Trial {
    fn observe(&mut self, tick: &Tick, bytes: &mut u64, max: u64) -> Result<()> {
        let e = &self.engine;
        let equity = e.marked_equity(e.current_mid())?;
        let exposure = notional(e.current_mid(), u64::try_from(e.base)?)?;
        if let Some(last) = self.counts.last_time {
            self.counts.exposure_integral += (tick.time_ms - last) as f64
                * self.counts.previous_exposure as f64
                / self.engine.capital_atoms() as f64;
        }
        self.counts.max_exposure = self.counts.max_exposure.max(exposure);
        self.counts.previous_exposure = exposure;
        self.counts.last_time = Some(tick.time_ms);
        let day = chrono::DateTime::from_timestamp_millis(tick.time_ms as i64)
            .context("invalid timestamp")?
            .with_timezone(&chrono::FixedOffset::east_opt(10800).unwrap())
            .format("%Y-%m-%d")
            .to_string();
        *self.counts.daily.entry(day).or_default() += equity - self.counts.previous_equity;
        self.counts.previous_equity = equity;
        self.counts.max_active = self.counts.max_active.max(
            e.orders
                .iter()
                .filter(|o| o.active && !o.cancel_effective_logged)
                .count(),
        );
        self.flush(bytes, max)
    }
    fn flush(&mut self, bytes: &mut u64, max: u64) -> Result<()> {
        let e = &mut self.engine;
        for fill in &e.fills {
            self.counts.fills += 1;
            if e.orders.iter().any(|o| o.id == fill.order_id) {
                self.counts.partial += 1;
            } else {
                self.counts.complete += 1;
            }
            if fill.evidence_kind.starts_with("exact") {
                self.counts.exact += 1;
            } else {
                self.counts.through += 1;
            }
            write_row(&mut self.writer, "fill", fill, bytes, max)?;
        }
        for (kind, rows) in [
            ("execution", serde_json::to_value(&e.logs)?),
            ("decision", serde_json::to_value(&e.decisions)?),
            ("ledger", serde_json::to_value(&e.ledger)?),
            ("equity", serde_json::to_value(&e.equity)?),
        ] {
            for row in rows.as_array().unwrap() {
                if kind == "ledger" && row["reason"] == "reconciliation_checkpoint_not_deposit" {
                    continue;
                }
                write_row(&mut self.writer, kind, row, bytes, max)?;
            }
        }
        for evidence in e.execution_evidence.values() {
            self.counts.exact_opportunities += evidence.exact_price_eligible_trade_events;
            self.counts.through_opportunities += evidence.trade_through_valid_window_events;
        }
        e.execution_evidence.clear();
        e.logs.clear();
        e.decisions.clear();
        e.fills.clear();
        e.equity.clear();
        e.ledger = vec![
            ("USDT".to_string(), e.cash),
            (e.instrument.base_asset.clone(), e.base),
            ("BNB".to_string(), e.bnb),
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
        e.reconcile()?;
        e.portfolio_pnl(e.current_mid())?;
        Ok(())
    }
}
pub fn run(config_path: &Path, output: &Path) -> Result<()> {
    let bytes = std::fs::read(config_path)?;
    let config: Config = serde_json::from_slice(&bytes)?;
    config.validate()?;
    let experiment: Experiment = serde_json::from_slice(&std::fs::read(&config.experiment)?)?;
    experiment.validate()?;
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config.current_metadata)?)?;
    ensure!(
        !output.exists() || std::fs::read_dir(output)?.next().is_none(),
        "batch output must be a new/empty directory; preserve previous runs"
    );
    std::fs::create_dir_all(output.join("traces"))?;
    std::fs::write(
        output.join("INCOMPLETE"),
        "No final results until batch_complete.json exists",
    )?;
    let mut results = vec![];
    let mut rejected = vec![];
    let mut provenance = vec![];
    let mut output_bytes = 0;
    for (source_index, source) in config.sources.iter().enumerate() {
        let complete = source.root.join("collection_finished.json").exists()
            || source.legacy_completed_research
                && source.root.join("online-observations.json").exists()
                && !source.root.join("collection-config.json").exists();
        if !complete {
            rejected.push(serde_json::json!({"source":source.root,"reason":"capture_active_or_completion_unverified","financial_results":null}));
            continue;
        }
        let manifest: Manifest =
            serde_json::from_slice(&std::fs::read(source.root.join("manifest.json"))?)?;
        ensure!(manifest.instruments.len() <= 20, "symbol bound exceeded");
        ensure!(
            manifest
                .instruments
                .iter()
                .map(|i| &i.symbol)
                .collect::<BTreeSet<_>>()
                .len()
                == manifest.instruments.len(),
            "duplicate source instruments would duplicate capital"
        );
        let paths = segment_paths(&source.root.join("recordings"))?;
        ensure!(paths.len() <= 40_000, "file bound exceeded");
        let frozen: Vec<_> = paths
            .iter()
            .map(|p| {
                Ok(FrozenFile {
                    path: p.clone(),
                    bytes: std::fs::metadata(p)?.len(),
                    sha256_sidecar: std::fs::read_to_string(p.with_extension("sha256"))?,
                })
            })
            .collect::<Result<_>>()?;
        let audit = inspect_frozen(&frozen)?;
        provenance.push(serde_json::json!({"source":source.root,"audit":audit,"files":frozen,"manifest_sha256":format!("{:x}",Sha256::digest(std::fs::read(source.root.join("manifest.json"))?))}));
        if !audit.errors.is_empty() {
            rejected.push(serde_json::json!({"source":source.root,"reason":"checksum_or_read_failure","financial_results":null}));
            continue;
        }
        for old in &manifest.instruments {
            let coverage = audit
                .symbols
                .get(&old.symbol)
                .context("symbol coverage missing")?;
            if !coverage.prefix_complete
                || coverage.record_sequence_gaps > 0
                || coverage.trade_id_gaps > 0
                || !coverage.invalidations.is_empty()
                || !coverage.book_intervals_over_2s.is_empty()
                || coverage.book_updates == 0
                || coverage.aggregate_trades == 0
                || coverage.book_exchange_time_reversals > 0
                || coverage.trade_exchange_time_reversals > 0
            {
                rejected.push(serde_json::json!({"source":source.root,"symbol":old.symbol,"reason":"source_has_invalid_intervals_reject_entire_session_symbol","coverage":coverage,"financial_results":null}));
                continue;
            }
            let (instrument, bounds, filter_match) = metadata_instrument(old, &metadata)?;
            let (start, full_end) = *manifest.bounds.get(&old.symbol).context("bounds missing")?;
            if coverage
                .last_book_ms
                .is_none_or(|t| full_end.saturating_sub(t) > 2000)
            {
                rejected.push(serde_json::json!({"source":source.root,"symbol":old.symbol,"reason":"incomplete_recording_tail","declared_end_ms":full_end,"last_book_ms":coverage.last_book_ms,"financial_results":null}));
                continue;
            }
            let end = start + (full_end - start) * experiment.train_fraction_ppm / 1_000_000;
            let mut trials: Vec<Trial> = vec![];
            let mut initialized_at = None;
            let streamed = stream_symbol(
                &source.root.join("recordings").join(&old.symbol),
                &old.symbol,
                |tick| {
                    if tick.time_ms < start || tick.time_ms >= end {
                        return Ok(());
                    }
                    if trials.is_empty() {
                        let Input::Book { view, exchange_ms } = &tick.input else {
                            return Ok(());
                        };
                        if !view.valid() || tick.time_ms.abs_diff(*exchange_ms) > 2000 {
                            return Ok(());
                        }
                        initialized_at = Some(tick.time_ms);
                        for scenario_name in &config.scenario_names {
                            let scenario = experiment
                                .scenarios
                                .iter()
                                .find(|s| &s.name == scenario_name)
                                .context("unknown batch scenario")?;
                            for allocation in &config.base_allocations_ppm {
                                for model in &config.execution_models {
                                    let capital = if config.capital_scope
                                        == "partitioned_single_5000_budget"
                                    {
                                        *config
                                            .shared_market_capital_usdt
                                            .get(&old.symbol)
                                            .context(
                                                "source symbol lacks shared capital allocation",
                                            )?
                                    } else {
                                        5000
                                    };
                                    let mut e = Engine::funded_budget(
                                        scenario.clone(),
                                        instrument.clone(),
                                        Initialization {
                                            base_allocation_ppm: *allocation,
                                            valuation_price_units: view.mid(),
                                            valuation_time_ms: tick.time_ms,
                                            fee_buffer_ppm: config.fee_buffer_ppm,
                                        },
                                        *model,
                                        capital,
                                    )?;
                                    e.price_bounds = Some(bounds);
                                    let initial = e.balances();
                                    let name = format!(
                                        "{source_index}-{}-{}-{allocation}-{model:?}.jsonl",
                                        old.symbol, scenario.name
                                    );
                                    trials.push(Trial {
                                        engine: e,
                                        allocation: *allocation,
                                        model: *model,
                                        writer: BufWriter::new(std::fs::File::create(
                                            output.join("traces").join(name),
                                        )?),
                                        counts: Counts {
                                            previous_equity: i128::from(capital) * i128::from(S),
                                            ..Default::default()
                                        },
                                        initialized: initial,
                                    });
                                }
                            }
                        }
                    }
                    for trial in &mut trials {
                        if tick.time_ms >= end.saturating_sub(2000) && !trial.engine.settling {
                            trial.engine.clock = tick.time_ms;
                            trial.engine.settling = true;
                            let ids: Vec<_> = trial.engine.orders.iter().map(|o| o.id).collect();
                            for id in ids {
                                trial.engine.cancel(id)?;
                            }
                        }
                        trial.engine.step(&tick)?;
                        trial.observe(&tick, &mut output_bytes, config.max_output_bytes)?;
                    }
                    Ok(())
                },
            )?;
            ensure!(
                streamed.prefix_complete
                    && streamed.sequence_contiguous
                    && streamed.trade_id_gaps == 0,
                "source changed between audit and stream"
            );
            for mut trial in trials {
                trial.flush(&mut output_bytes, config.max_output_bytes)?;
                trial.writer.flush()?;
                let e = &trial.engine;
                let mid = e.current_mid();
                let net = e.marked_equity(mid)? - e.capital_atoms();
                let hold = e.portfolio.initial_quote_atoms
                    + notional(mid, e.portfolio.initial_base_units)? as i128
                    + notional(e.bnb_price(), e.portfolio.initial_bnb_units)? as i128
                    - e.capital_atoms();
                let valid = e.data_ok
                    && e.last_book_ms
                        .is_some_and(|t| end.saturating_sub(t) <= e.scenario.strategy.stale_ms);
                let mut row = serde_json::json!({"result_type":"historical_simulation_preliminary","source":source.root,"symbol":old.symbol,"scenario":e.scenario.name,"execution_model":trial.model,"less_conservative_assumption":trial.model==ExecutionModel::AlternativeThrough,"capital_scope":config.capital_scope,"capital_group_id":if config.capital_scope=="partitioned_single_5000_budget" {format!("{source_index}-{}-{}-{:?}",e.scenario.name,trial.allocation,trial.model)}else{format!("{source_index}-{}-{}-{}-{:?}",old.symbol,e.scenario.name,trial.allocation,trial.model)},"capital_usdt":e.capital_atoms()/i128::from(S),"base_allocation_ppm":trial.allocation,"initialization":e.portfolio,"initial_balances":trial.initialized,"end_balances":e.balances(),"start_ms":initialized_at,"end_ms":end,"split":"train_only","current_metadata_matches_historical_filters":filter_match,"quote_intents":e.diagnostics.generated_quote_intents,"funding_and_risk_rejections":e.diagnostics.rejection_reasons,"funnel":e.diagnostics,"active_orders_peak":trial.counts.max_active,"orders_at_end":e.orders.len(),"exact_price_opportunities":trial.counts.exact_opportunities,"trade_through_opportunities":trial.counts.through_opportunities,"fill_count":trial.counts.fills,"partial_fill_events":trial.counts.partial,"complete_fill_events":trial.counts.complete,"exact_fill_events":trial.counts.exact,"through_fill_events":trial.counts.through});
                row.as_object_mut().unwrap().extend(serde_json::json!({"gross_spread_capture_atoms":e.gross,"net_pnl_atoms":if valid {Some(net)}else{None},"accounting":e.portfolio_pnl(mid)?,"fee_expenses_atoms":e.fees,"fee_assets":e.fee_asset_totals(),"gross_inventory_pnl_atoms":net+e.fees-e.rebates-e.gross,"no_trade_initial_holdings_pnl_atoms":hold,"excess_over_initial_holdings_atoms":net-hold,"max_inventory_exposure_atoms":trial.counts.max_exposure,"time_weighted_inventory_exposure_fraction":(trial.counts.exposure_integral+(end.saturating_sub(trial.counts.last_time.unwrap_or(end))) as f64*trial.counts.previous_exposure as f64/e.capital_atoms() as f64)/(end-initialized_at.unwrap_or(start)).max(1) as f64,"max_drawdown_atoms":e.maximum_drawdown(),"daily_pnl_atoms":trial.counts.daily,"data_quality_valid":valid,"executable_edge_validated":false,"actual_exchange_executions":0,"final_oos_evaluated":false,"markouts":"not computed by compact batch sink; legacy runner retains markout reports"}).as_object().unwrap().clone());
                results.push(row);
            }
        }
    }
    let report = serde_json::json!({"schema_version":1,"result_type":"preliminary_historical_simulation_comparisons","capital_scope":config.capital_scope,"config_sha256":format!("{:x}",Sha256::digest(&bytes)),"current_metadata_sha256":format!("{:x}",Sha256::digest(std::fs::read(&config.current_metadata)?)),"results":results,"rejected_sessions_or_symbols":rejected,"source_provenance":provenance,"trace_bytes":output_bytes,"final_oos_evaluated":false,"limitations":["Capital scope is explicit: isolated alternatives must never be summed; partitioned markets share exactly $5000 within the same source/allocation/model/scenario group. Rejected market budgets remain unallocated cash.","Only completed sources accepted. Any invalid interval rejects the entire source-symbol; no invented gap fills or cash resets within a reported trial.","Initial holdings are a priced portfolio reclassification, not historical executed purchases. No conversion fee is charged because no conversion is claimed.","Conservative through debits entry-visible intervening depth and queue ahead; alternative through assumes priority cleared. Neither reconstructs exact FIFO or proves profit.","Training partitions only; existing short source remains preliminary. No live orders, private API or account-specific fee verification.","Trace output is bounded; INCOMPLETE means aborted and must not be interpreted as a valid result."]});
    std::fs::write(
        output.join("comparisons.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    std::fs::write(output.join("experiment.json"), bytes)?;
    let mut csv="result_type,source,symbol,allocation_base_ppm,model,intents,active_peak,exact_opportunities,through_opportunities,partial_fills,complete_fills,gross_capture_atoms,net_pnl_atoms,fees_atoms,max_inventory_atoms,max_drawdown_atoms,capital_scope\n".to_string();
    for r in report["results"].as_array().unwrap() {
        csv.push_str(&format!(
            "simulated,{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{scope}\n",
            r["source"].as_str().unwrap(),
            r["symbol"].as_str().unwrap(),
            r["base_allocation_ppm"],
            r["execution_model"].as_str().unwrap(),
            r["quote_intents"],
            r["active_orders_peak"],
            r["exact_price_opportunities"],
            r["trade_through_opportunities"],
            r["partial_fill_events"],
            r["complete_fill_events"],
            r["gross_spread_capture_atoms"],
            r["net_pnl_atoms"],
            r["fee_expenses_atoms"],
            r["max_inventory_exposure_atoms"],
            r["max_drawdown_atoms"],
            scope = config.capital_scope
        ));
    }
    std::fs::write(output.join("comparisons.csv"), csv)?;
    std::fs::write(
        output.join("batch_complete.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"complete":true,"comparisons_sha256":format!("{:x}",Sha256::digest(std::fs::read(output.join("comparisons.json"))?)),"financial_results_are_simulated":true}),
        )?,
    )?;
    std::fs::remove_file(output.join("INCOMPLETE"))?;
    Ok(())
}

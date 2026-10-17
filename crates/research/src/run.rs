use crate::{
    discovery::{Instrument, Policy, discover, public_json},
    fees::FeeScenario,
    report::{Economics, Report, rank, sort_rankings, write_reports},
    stats::{ResearchObserver, Series},
};
use anyhow::{Context, Result, ensure};
use mm_market_data::{
    recording::{Event, replay_visit, segment_paths},
    runtime::{Config, Observer, SharedStatus, now_ms, run_symbol},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchConfig {
    pub duration_seconds: u64,
    pub universe_limit: usize,
    pub policy: Policy,
    pub economics: Economics,
    pub fees: Vec<FeeScenario>,
}
impl ResearchConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (30..=3600).contains(&self.duration_seconds) && (1..=20).contains(&self.universe_limit),
            "research duration 30..3600s, universe 1..20"
        );
        ensure!(
            self.duration_seconds >= (self.universe_limit as u64 - 1) * 4 + 30,
            "duration must leave at least 30 seconds after staggered startup"
        );
        ensure!(
            self.policy.capital_usdt == 5000
                && self.policy.clip_usdt > 0
                && self.policy.clip_usdt <= 500,
            "research capital must remain $5000; clip 1..500"
        );
        ensure!(
            self.policy.min_uptime > 0.0
                && self.policy.min_uptime <= 1.0
                && self.policy.max_level_concentration > 0.0
                && self.policy.max_level_concentration <= 1.0
                && self.policy.max_tick_bps.is_finite()
                && self.policy.max_tick_bps > 0.0,
            "invalid eligibility policy"
        );
        ensure!(
            self.economics.inventory_bps >= 0.0
                && self.economics.safety_bps >= 0.0
                && (0.0..=1.0).contains(&self.economics.rebalance_fraction)
                && self.economics.rebalance_slippage_bps >= 0.0,
            "invalid cost assumptions"
        );
        ensure!(
            !self.economics.adverse_floors_bps.is_empty()
                && self
                    .economics
                    .adverse_floors_bps
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0.0),
            "invalid adverse floors"
        );
        ensure!(!self.fees.is_empty(), "manual fees required");
        for s in &self.fees {
            s.validate()?;
        }
        let names: std::collections::BTreeSet<_> = self.fees.iter().map(|s| &s.name).collect();
        ensure!(
            names.len() == self.fees.len(),
            "duplicate fee scenario name"
        );
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub config: ResearchConfig,
    pub instruments: Vec<Instrument>,
    pub bounds: BTreeMap<String, (u64, u64)>,
}
pub fn replay_series(
    path: &Path,
    bounds: &BTreeMap<String, (u64, u64)>,
) -> Result<BTreeMap<String, Series>> {
    let observer = ResearchObserver {
        series: Mutex::new(
            bounds
                .iter()
                .map(|(s, (start, end))| {
                    (
                        s.clone(),
                        Series {
                            start_ms: *start,
                            end_ms: *end,
                            ..Series::default()
                        },
                    )
                })
                .collect(),
        ),
    };
    let mut continuity: BTreeMap<String, (String, u64)> = BTreeMap::new();
    let paths = segment_paths(path)?;
    ensure!(!paths.is_empty(), "missing historical data");
    for path in paths {
        let mut first = true;
        replay_visit(&path, 10_000, 8 * 1024 * 1024, |record, book| {
            if first {
                let prior = continuity.get(&record.symbol);
                ensure!(
                    prior.is_none_or(
                        |(id, seq)| id == &record.recording_id && record.sequence == seq + 1
                    ),
                    "historical segment discontinuity"
                );
                ensure!(
                    prior.is_some() || record.sequence == 1,
                    "historical prefix missing (retention eviction)"
                );
                first = false;
            }
            continuity.insert(
                record.symbol.clone(),
                (record.recording_id.clone(), record.sequence),
            );
            match &record.event {
                Event::AppliedDepth { depth } => {
                    observer.book(&record.symbol, book, record.received_ms, depth.exchange_ms)
                }
                Event::Trade { trade } => observer.trade(trade, record.received_ms),
                _ => (),
            }
            Ok(())
        })?;
    }
    ensure!(
        bounds.keys().all(|s| continuity.contains_key(s)),
        "symbol history missing"
    );
    Ok(observer.series.into_inner().unwrap())
}
fn check_resources(pairs: usize) -> Result<()> {
    let output = std::process::Command::new("df")
        .args(["-Pk", "/workspace"])
        .output()?;
    ensure!(output.status.success(), "cannot inspect free storage");
    let text = String::from_utf8(output.stdout)?;
    let free = text
        .lines()
        .last()
        .context("df empty")?
        .split_whitespace()
        .nth(3)
        .context("df available missing")?
        .parse::<u64>()?
        * 1024;
    ensure!(
        free > (pairs as u64 + 1) * 1024 * 1024 * 1024,
        "insufficient free storage for bounded recording reservation; reduce universe"
    );
    let mem = std::fs::read_to_string("/proc/meminfo")?;
    let available = mem
        .lines()
        .find(|l| l.starts_with("MemAvailable:"))
        .context("memory available missing")?
        .split_whitespace()
        .nth(1)
        .context("memory count missing")?
        .parse::<u64>()?
        * 1024;
    ensure!(
        available > (pairs as u64 + 1) * 256 * 1024 * 1024,
        "insufficient memory reservation; reduce universe"
    );
    Ok(())
}
pub async fn execute(config: ResearchConfig, output: &Path) -> Result<Report> {
    config.validate()?;
    check_resources(config.universe_limit)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let (raw_info,info)=public_json(&client,"https://data-api.binance.vision/api/v3/exchangeInfo?permissions=SPOT&showPermissionSets=false").await?;
    let (raw_tickers, tickers) = public_json(
        &client,
        "https://data-api.binance.vision/api/v3/ticker/24hr",
    )
    .await?;
    let (instruments, excluded) = discover(&info, &tickers, &config.policy, config.universe_limit)?;
    ensure!(!instruments.is_empty(), "no symbols pass discovery");
    let start = now_ms();
    let end = start + config.duration_seconds * 1000;
    let root = PathBuf::from(format!(".local/research/run-{start}"));
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("exchange-info.json"), raw_info)?;
    std::fs::write(root.join("tickers-24h.json"), raw_tickers)?;
    let observer = Arc::new(ResearchObserver::default());
    let shared: SharedStatus = Arc::new(Mutex::new(BTreeMap::new()));
    let budget = Arc::new(tokio::sync::Mutex::new(tokio::time::Instant::now()));
    let mut tasks = vec![];
    let mut bounds = BTreeMap::new();
    for (index, instrument) in instruments.iter().enumerate() {
        tokio::time::sleep(Duration::from_millis(if index == 0 { 0 } else { 4000 })).await;
        let mut md = Config {
            include_trades: true,
            snapshot_budget: Some(budget.clone()),
            observer: Some(observer.clone()),
            data_dir: root.join("recordings"),
            ..Config::default()
        };
        md.recording.segment_records = 4096;
        md.recording.max_segments = 128;
        let symbol = instrument.symbol.clone();
        let symbol_start = now_ms();
        observer.series.lock().unwrap().insert(
            symbol.clone(),
            Series {
                start_ms: symbol_start,
                end_ms: end,
                ..Series::default()
            },
        );
        bounds.insert(symbol.clone(), (symbol_start, end));
        let statuses = shared.clone();
        tasks.push(tokio::spawn(async move {
            run_symbol(md, &symbol, statuses).await
        }));
    }
    let manifest = Manifest {
        config: config.clone(),
        instruments: instruments.clone(),
        bounds: bounds.clone(),
    };
    std::fs::write(
        root.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    println!(
        "Public research collecting {} pairs until {}: {}",
        instruments.len(),
        end,
        instruments
            .iter()
            .map(|s| s.symbol.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    tokio::time::sleep(Duration::from_millis(end.saturating_sub(now_ms()))).await;
    for task in &tasks {
        task.abort();
    }
    for task in tasks {
        let _ = task.await;
    }
    let series = observer.series.lock().unwrap().clone();
    std::fs::write(
        root.join("online-observations.json"),
        serde_json::to_vec(&series)?,
    )?;
    let replayed = replay_series(&root.join("recordings"), &bounds);
    let verified = match &replayed {
        Ok(replayed) => *replayed == series,
        Err(_) => false,
    };
    let mut rankings = vec![];
    for instrument in &instruments {
        let mut row = rank(
            instrument,
            &series[&instrument.symbol],
            &config.policy,
            &config.economics,
            &config.fees,
        )?;
        if !verified {
            row.eligible_for_further_research = false;
            row.plausible_positive_edge = false;
            row.exclusions
                .push("recording/replay coverage not verified".into());
            row.explanation
                .push_str(" Recording/replay coverage not verified.");
        }
        rankings.push(row);
    }
    sort_rankings(&mut rankings);
    let positives = rankings
        .iter()
        .filter(|r| r.plausible_positive_edge)
        .count();
    let mut limitations=vec!["PRELIMINARY: a short, single-session sample cannot establish profitability, regime robustness or statistical significance.".into(),"No fills or queue position modeled; displayed/quoted depth is cancellable and not a guarantee of executable liquidity. No capital was allocated.".into(),"Manual fee assumptions are unverified; account-specific commissions, promotions, rebates and tax/special components require independent verification before any later execution.".into(),"Passive-counterparty aggregate-trade markouts are proxies, not adverse selection conditional on this strategy's executions. Aggregate trades and one-second samples omit microstructure detail.".into(),"Volatility is robust one-second RMS return in bps (6-MAD winsorization with 1bps floor); rare extreme returns remain in raw recordings and can be understated by this statistic.".into(),"Observation coverage approximates synchronization uptime from occupied one-second bins, not continuous-time uptime. The universe is prioritized by 24h activity for collection, not economic ranking.".into(),format!("Auditable raw inputs, manifest and online observations: {}. Fixed bounded recorder retention can invalidate full-run replay for long/high-activity runs; such reports exclude every candidate.",root.display())];
    if let Err(error) = replayed {
        limitations.push(format!("Replay problem: {error}"));
    }
    let conclusion = if positives == 0 {
        "No pair has a plausible positive edge under the baseline manual fees and conservative risk assumptions. No winning candidate is manufactured.".into()
    } else {
        format!(
            "{positives} pairs show only a hypothetical positive screening opportunity. No executable edge or profitability has been demonstrated."
        )
    };
    let report = Report {
        schema_version: 1,
        preliminary: true,
        generated_at_ms: now_ms(),
        started_at_ms: start,
        ended_at_ms: end,
        collection_duration_seconds: config.duration_seconds as f64,
        capital_assumption_usdt: config.policy.capital_usdt,
        observations_are_real: true,
        fee_assumptions: config.fees,
        policy: config.policy,
        economics: config.economics,
        conclusion,
        rankings,
        discovery_exclusions: excluded,
        limitations,
        replay_consistency_verified: verified,
    };
    std::fs::create_dir_all(output)?;
    std::fs::write(
        output.join("universe.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    write_reports(output, &report)?;
    capture_audit(&root, output, &report, &series)?;
    println!(
        "{} Replay consistent: {}. Reports: {}",
        report.conclusion,
        verified,
        output.display()
    );
    Ok(report)
}

/// Recompute analytics from the same immutable captured observations; never fetch new observations.
pub fn summarize(root: &Path, output: &Path) -> Result<Report> {
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(root.join("manifest.json"))?)?;
    let online: BTreeMap<String, Series> =
        serde_json::from_slice(&std::fs::read(root.join("online-observations.json"))?)?;
    let replayed = replay_series(&root.join("recordings"), &manifest.bounds)?;
    ensure!(online == replayed, "online/replay mismatch");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("market_rankings.json"))?)?;
    value["rankings"] = serde_json::json!([]);
    let mut report: Report = serde_json::from_value(value)?;
    ensure!(
        serde_json::to_value(&report.fee_assumptions)?
            == serde_json::to_value(&manifest.config.fees)?,
        "fee assumptions changed; rerun with explicit new research config instead"
    );
    let mut rankings = manifest
        .instruments
        .iter()
        .map(|i| {
            rank(
                i,
                &replayed[&i.symbol],
                &manifest.config.policy,
                &manifest.config.economics,
                &manifest.config.fees,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    sort_rankings(&mut rankings);
    report.rankings = rankings;
    report.replay_consistency_verified = true;
    report.generated_at_ms = now_ms();
    report.conclusion = if report.rankings.iter().any(|r| r.plausible_positive_edge) {
        "Hypothetical positive screening opportunities only; no executable edge or profitability demonstrated.".into()
    } else {
        "No pair has a plausible positive edge under the baseline manual fees and conservative risk assumptions. No winning candidate is manufactured.".into()
    };
    let notes = [
        "Requested 30/60/180/600-second statistics include actual observed duration and an explicit requested_window_complete flag. Truncated horizons do not imply longer historical coverage.",
        "Depth statistics sum known reconstructed levels only. Bands extending beyond the finite snapshot coverage can be understated; displayed depth is a quoted lower bound, not guaranteed executable liquidity.",
    ];
    for note in notes {
        if !report.limitations.iter().any(|s| s == note) {
            report.limitations.push(note.into());
        }
    }
    write_reports(output, &report)?;
    capture_audit(root, output, &report, &replayed)?;
    Ok(report)
}

fn capture_audit(
    root: &Path,
    output: &Path,
    report: &Report,
    series: &BTreeMap<String, Series>,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    let mut inputs = BTreeMap::new();
    for name in [
        "manifest.json",
        "exchange-info.json",
        "tickers-24h.json",
        "online-observations.json",
    ] {
        let mut file = std::fs::File::open(root.join(name))?;
        let mut hash = Sha256::new();
        std::io::copy(&mut file, &mut hash)?;
        inputs.insert(name, format!("{:x}", hash.finalize()));
    }
    let mut bytes = 0u64;
    let mut segments = vec![];
    for path in segment_paths(&root.join("recordings"))? {
        let size = std::fs::metadata(&path)?.len();
        bytes += size;
        segments.push(serde_json::json!({"path":path.strip_prefix(root)?,"bytes":size,"sha256":std::fs::read_to_string(path.with_extension("sha256"))?}));
    }
    let samples:BTreeMap<_,_>=series.iter().map(|(symbol,s)| (symbol,serde_json::json!({"book_samples":s.samples.len(),"aggregate_trade_messages":s.trades.values().map(|b|b.aggregate_messages).sum::<u64>(),"underlying_trade_count":s.trades.values().map(|b|b.underlying_trades).sum::<u64>(),"trade_bins":s.trades.len(),"start_ms":s.start_ms,"end_ms":s.end_ms,"aggregate_gaps":s.aggregate_gaps,"capacity_exceeded":s.capacity_exceeded}))).collect();
    let audit = serde_json::json!({"source":"Binance Spot public WebSocket depth and aggregate trades, REST snapshots and discovery","run_directory":root,"replay_consistency_verified":report.replay_consistency_verified,"observation_samples":samples,"input_sha256":inputs,"segments":segments,"total_parquet_bytes":bytes});
    std::fs::write(
        output.join("capture_audit.json"),
        serde_json::to_vec_pretty(&audit)?,
    )?;
    Ok(())
}

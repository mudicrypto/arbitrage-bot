use crate::{
    discovery::{Exclusion, Instrument, Policy},
    fees::FeeScenario,
    stats::{Horizon, Markout, Series, horizon, markout},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Economics {
    pub inventory_bps: f64,
    pub rebalance_fraction: f64,
    pub rebalance_slippage_bps: f64,
    pub safety_bps: f64,
    pub adverse_floors_bps: Vec<f64>,
}
impl Default for Economics {
    fn default() -> Self {
        Self {
            inventory_bps: 2.0,
            rebalance_fraction: 0.1,
            rebalance_slippage_bps: 1.0,
            safety_bps: 2.0,
            adverse_floors_bps: vec![0.0, 1.0, 3.0, 10.0],
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sensitivity {
    pub fee_scenario: String,
    pub adverse_floor_bps: f64,
    pub assumed_adverse_bps: f64,
    pub maker_roundtrip_bps: f64,
    pub rebalancing_bps: f64,
    pub net_theoretical_bps: f64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Ranking {
    pub rank: usize,
    pub symbol: String,
    pub eligible_for_further_research: bool,
    pub plausible_positive_edge: bool,
    pub executable_edge_validated: bool,
    pub explanation: String,
    pub exclusions: Vec<String>,
    pub sample_count: usize,
    pub trade_bins: usize,
    pub aggregate_gaps: u64,
    pub data_quality: f64,
    pub gross_theoretical_bps: Option<f64>,
    pub baseline_net_bps: Option<f64>,
    pub worst_window_net_bps: Option<f64>,
    pub risk_adjusted_score: Option<f64>,
    pub capital_efficiency: f64,
    pub clip_usdt: f64,
    pub quoted_clip_liquidity_usdt: f64,
    pub windows: Vec<Horizon>,
    pub markouts: Vec<Markout>,
    pub sensitivity: Vec<Sensitivity>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u16,
    pub preliminary: bool,
    pub generated_at_ms: u64,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub collection_duration_seconds: f64,
    pub capital_assumption_usdt: u64,
    pub observations_are_real: bool,
    pub fee_assumptions: Vec<FeeScenario>,
    pub policy: Policy,
    pub economics: Economics,
    pub conclusion: String,
    pub rankings: Vec<Ranking>,
    pub discovery_exclusions: Vec<Exclusion>,
    pub limitations: Vec<String>,
    pub replay_consistency_verified: bool,
}
pub fn net_edge(
    gross: f64,
    maker: f64,
    adverse: f64,
    inventory: f64,
    rebalancing: f64,
    safety: f64,
) -> f64 {
    gross - maker - adverse - inventory - rebalancing - safety
}
pub fn rank(
    instrument: &Instrument,
    series: &Series,
    policy: &Policy,
    economics: &Economics,
    fees: &[FeeScenario],
) -> Result<Ranking> {
    ensure!(!fees.is_empty(), "baseline fee scenario missing");
    let windows: Vec<_> = [30, 60, 180, 600]
        .into_iter()
        .filter_map(|h| horizon(series, h))
        .collect();
    let markouts: Vec<_> = [1, 5, 15].into_iter().map(|h| markout(series, h)).collect();
    let longest = windows.last();
    let mut exclusions = vec![];
    let duration = (series.end_ms.saturating_sub(series.start_ms)) as f64 / 1000.0;
    let quality = (series.samples.len() as f64 / duration.max(1.0)).min(1.0);
    if series.samples.len() < 30 {
        exclusions.push("fewer than 30 synchronized observation seconds".into());
    }
    if quality < policy.min_uptime {
        exclusions.push("insufficient synchronized observation coverage".into());
    }
    if series.capacity_exceeded {
        exclusions.push("bounded observation capacity exceeded".into());
    }
    if series.aggregate_gaps > 0 {
        exclusions.push("aggregate-trade sequence gaps; volume/markouts incomplete".into());
    }
    let usable_depth = longest.map_or(0.0, |w| {
        w.depth_median_usdt[1][0].min(w.depth_median_usdt[1][1])
    });
    if usable_depth < policy.min_depth_10bps_usdt {
        exclusions.push("insufficient two-sided 10bps depth".into());
    }
    if longest.is_some_and(|w| w.concentration_p90 > policy.max_level_concentration) {
        exclusions.push("abnormal single-level near-mid liquidity concentration".into());
    }
    if series.trades.is_empty() {
        exclusions.push("no observed public trades".into());
    }
    let observed = markouts
        .iter()
        .filter_map(|m| m.mean_passive_adverse_bps)
        .map(|v| v.max(0.0))
        .fold(0.0_f64, f64::max);
    if markouts.iter().any(|m| m.bins < 10) {
        exclusions.push("insufficient completed adverse-selection horizons".into());
    }
    let gross = longest.map(|w| w.spread_p50_bps);
    let mut sensitivity = vec![];
    for scenario in fees {
        if gross.is_none() {
            continue;
        }
        let periods = series
            .samples
            .iter()
            .map(|s| scenario.at(s.received_ms, &instrument.symbol))
            .collect::<Result<Vec<_>>>()?;
        if periods.is_empty() {
            continue;
        }
        let maker = periods
            .iter()
            .map(|p| p.roundtrip_microbps().map(|r| r as f64 / 1_000_000.0))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .fold(0.0_f64, f64::max);
        let taker = periods
            .iter()
            .map(|p| p.rate(true, false).map(|r| r as f64 / 1_000_000.0))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .fold(0.0_f64, f64::max);
        for floor in &economics.adverse_floors_bps {
            let adverse = observed.max(*floor);
            let rebalance =
                economics.rebalance_fraction * (taker + economics.rebalance_slippage_bps);
            sensitivity.push(Sensitivity {
                fee_scenario: scenario.name.clone(),
                adverse_floor_bps: *floor,
                assumed_adverse_bps: adverse,
                maker_roundtrip_bps: maker,
                rebalancing_bps: rebalance,
                net_theoretical_bps: net_edge(
                    gross.unwrap_or(0.0),
                    maker,
                    adverse,
                    economics.inventory_bps,
                    rebalance,
                    economics.safety_bps,
                ),
            });
        }
    }
    let baseline = sensitivity
        .iter()
        .find(|s| s.fee_scenario == fees[0].name && s.adverse_floor_bps >= 3.0)
        .or_else(|| sensitivity.first());
    let baseline_net = baseline.and_then(|s| gross.map(|_| s.net_theoretical_bps));
    let worst_window = baseline
        .map(|s| {
            windows
                .iter()
                .map(|w| {
                    net_edge(
                        w.spread_p10_bps,
                        s.maker_roundtrip_bps,
                        s.assumed_adverse_bps,
                        economics.inventory_bps,
                        s.rebalancing_bps,
                        economics.safety_bps,
                    )
                })
                .fold(f64::INFINITY, f64::min)
        })
        .filter(|v| v.is_finite());
    let clip = instrument.clip_notional_units as f64 / 100_000_000.0;
    let efficiency = (usable_depth / clip.max(1.0)).min(1.0);
    let vol = longest
        .and_then(|w| w.realized_volatility_bps)
        .unwrap_or(0.0);
    let score = worst_window.map(|v| v - vol - (1.0 - quality) * 100.0 - (1.0 - efficiency) * 10.0);
    let eligible = exclusions.is_empty();
    let plausible =
        eligible && baseline_net.is_some_and(|v| v > 0.0) && worst_window.is_some_and(|v| v > 0.0);
    let explanation = format!(
        "{} synchronized samples; quality {:.1}%; median spread {}; baseline net {}; conservative window floor {}; two-sided 10bps quoted depth ${:.2}; clip ${:.2}; one-second volatility {}. {}. Quoted liquidity is not a fill guarantee; queue position, latency and fill-conditioned selection are unmodeled.",
        series.samples.len(),
        quality * 100.0,
        display(gross),
        display(baseline_net),
        display(worst_window),
        usable_depth,
        clip,
        display(longest.and_then(|w| w.realized_volatility_bps)),
        if eligible {
            "Passed bounded research data/liquidity screens".into()
        } else {
            exclusions.join("; ")
        }
    );
    Ok(Ranking {
        rank: 0,
        symbol: instrument.symbol.clone(),
        eligible_for_further_research: eligible,
        plausible_positive_edge: plausible,
        executable_edge_validated: false,
        explanation,
        exclusions,
        sample_count: series.samples.len(),
        trade_bins: series.trades.len(),
        aggregate_gaps: series.aggregate_gaps,
        data_quality: quality,
        gross_theoretical_bps: gross,
        baseline_net_bps: baseline_net,
        worst_window_net_bps: worst_window,
        risk_adjusted_score: score,
        capital_efficiency: efficiency,
        clip_usdt: clip,
        quoted_clip_liquidity_usdt: usable_depth.min(clip),
        windows,
        markouts,
        sensitivity,
    })
}
fn display(v: Option<f64>) -> String {
    v.map_or_else(|| "unavailable".into(), |v| format!("{v:.4}bps"))
}
pub fn sort_rankings(rankings: &mut [Ranking]) {
    rankings.sort_by(|a, b| {
        b.eligible_for_further_research
            .cmp(&a.eligible_for_further_research)
            .then_with(|| {
                b.risk_adjusted_score
                    .unwrap_or(f64::NEG_INFINITY)
                    .total_cmp(&a.risk_adjusted_score.unwrap_or(f64::NEG_INFINITY))
            })
            .then(a.symbol.cmp(&b.symbol))
    });
    for (index, row) in rankings.iter_mut().enumerate() {
        row.rank = index + 1;
    }
}
fn csv_number(value: Option<f64>) -> String {
    value.map_or_else(String::new, |v| format!("{v:.8}"))
}
pub fn write_reports(path: &std::path::Path, report: &Report) -> Result<()> {
    std::fs::create_dir_all(path)?;
    let json = serde_json::to_vec_pretty(report)?;
    ensure!(json.len() <= 8 * 1024 * 1024, "report too large");
    std::fs::write(path.join("market_rankings.json.tmp"), json)?;
    let mut csv = String::from(
        "rank,symbol,eligible,plausible_positive_edge,samples,quality,gross_bps,baseline_net_bps,worst_window_net_bps,explanation\n",
    );
    for r in &report.rankings {
        csv.push_str(&format!(
            "{},{},{},{},{},{:.6},{},{},{},\"{}\"\n",
            r.rank,
            r.symbol,
            r.eligible_for_further_research,
            r.plausible_positive_edge,
            r.sample_count,
            r.data_quality,
            csv_number(r.gross_theoretical_bps),
            csv_number(r.baseline_net_bps),
            csv_number(r.worst_window_net_bps),
            r.explanation.replace('"', "\"\"")
        ));
    }
    std::fs::write(path.join("market_rankings.csv"), csv)?;
    let mut summary = format!(
        "# Preliminary Binance Spot profitability screening\n\n{}\n\nActual public-data run: {}–{} (Unix milliseconds), {:.1} seconds, {} collected pairs, {} synchronized samples. Capital assumption: ${}; no allocation or trading. All fee scenarios are manual and unverified. Replay consistency: {}.\n\nRankings use conservative net edge, quoted clip liquidity, capital fit, volatility, data quality and cross-window stability. A rank is a research priority, not a trading recommendation.\n\n",
        report.conclusion,
        report.started_at_ms,
        report.ended_at_ms,
        report.collection_duration_seconds,
        report.rankings.len(),
        report
            .rankings
            .iter()
            .map(|r| r.sample_count)
            .sum::<usize>(),
        report.capital_assumption_usdt,
        report.replay_consistency_verified
    );
    for r in &report.rankings {
        summary.push_str(&format!(
            "{}. **{}** — {}\n\n",
            r.rank, r.symbol, r.explanation
        ));
    }
    summary.push_str("## Limitations\n\n");
    for l in &report.limitations {
        summary.push_str(&format!("- {l}\n"));
    }
    std::fs::write(path.join("research_summary.md"), summary)?;
    let mut sensitivity = String::from(
        "# Manual fee and adverse-selection sensitivity\n\nUnverified assumptions, not account commission rates. No rebate or promotion entitlement is assumed. Hypothetical zero fees do not establish eligibility. BNB conversion price is a manual scenario assumption, not an observed market price. Discount applies only to standard commission components. Worst active fee rate across observed timestamps is used for screening.\n\n",
    );
    for s in &report.fee_assumptions {
        sensitivity.push_str(&format!("- **{}**: {}\n", s.name, s.description));
    }
    sensitivity.push_str("\nNet bps = median quoted spread − maker buy/sell fees − max(observed passive markout proxy, adverse floor) − inventory allowance − expected rebalance fraction × (taker fee + slippage) − safety margin. Proxy markouts are not conditional on our fills. No daily P&L or trade count is projected.\n\n| Pair | Manual fee scenario | Adverse floor | Used adverse | Maker roundtrip | Rebalance | Net theoretical bps |\n|---|---|---:|---:|---:|---:|---:|\n");
    for r in &report.rankings {
        for s in &r.sensitivity {
            sensitivity.push_str(&format!(
                "| {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |\n",
                r.symbol,
                s.fee_scenario,
                s.adverse_floor_bps,
                s.assumed_adverse_bps,
                s.maker_roundtrip_bps,
                s.rebalancing_bps,
                s.net_theoretical_bps
            ));
        }
    }
    std::fs::write(path.join("fee_sensitivity.md"), sensitivity)?;
    // JSON is the API generation marker and is published only after companion files.
    std::fs::rename(
        path.join("market_rankings.json.tmp"),
        path.join("market_rankings.json"),
    )?;
    Ok(())
}

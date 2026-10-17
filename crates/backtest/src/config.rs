use anyhow::{Result, ensure};
use mm_research::fees::FeeScenario;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueueModel {
    TradeDepletion,
    ProportionalCancellation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    pub spread_bps: u64,
    pub volatility_multiplier: u64,
    pub inventory_skew_bps: u64,
    pub clip_usdt: u64,
    pub inventory_limit_usdt: u64,
    pub refresh_ms: u64,
    pub refresh_ticks: u64,
    pub required_edge_bps: u64,
    pub max_spread_bps: u64,
    pub stale_ms: u64,
    pub drawdown_stop_usdt: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub name: String,
    pub queue: QueueModel,
    pub queue_multiplier_ppm: u64,
    pub trade_participation_ppm: u64,
    pub report_latency_ms: u64,
    pub entry_latency_ms: u64,
    pub cancel_latency_ms: u64,
    pub latency_jitter_ms: u64,
    pub seed: u64,
    pub strategy: Strategy,
    pub fees: FeeScenario,
    pub maker_rebate_microbps: u64,
    pub hypothetical: bool,
    pub promotion_verified: bool,
    pub verification_reference: Option<String>,
    pub account_access_verified: bool,
    pub closeout_slippage_bps: u64,
    pub adverse_terminal_shock_bps: u64,
    pub outage_every_ms: Option<u64>,
    pub outage_duration_ms: u64,
}
impl Scenario {
    pub fn validate(&self) -> Result<()> {
        self.fees.validate()?;
        ensure!(
            !self.name.is_empty()
                && self
                    .name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_'),
            "unsafe scenario name"
        );
        ensure!(
            self.queue_multiplier_ppm >= 1_000_000
                && self.queue_multiplier_ppm <= 10_000_000
                && self.trade_participation_ppm > 0
                && self.trade_participation_ppm <= 1_000_000,
            "invalid queue assumption"
        );
        ensure!(
            self.report_latency_ms > 0
                && self.report_latency_ms <= 5000
                && self.entry_latency_ms <= 60_000
                && self.cancel_latency_ms <= 60_000
                && self.latency_jitter_ms <= 60_000,
            "invalid latency"
        );
        ensure!(
            self.strategy.clip_usdt > 0
                && self.strategy.clip_usdt <= 500
                && self.strategy.inventory_limit_usdt >= self.strategy.clip_usdt
                && self.strategy.inventory_limit_usdt <= 2500,
            "invalid simulated position budget"
        );
        ensure!(
            self.strategy.spread_bps <= 1000
                && self.strategy.volatility_multiplier <= 20
                && self.strategy.inventory_skew_bps <= 1000
                && self.strategy.required_edge_bps <= 1000
                && self.strategy.max_spread_bps <= 1000
                && self.strategy.stale_ms > 0
                && self.strategy.stale_ms <= 5000
                && self.strategy.refresh_ms >= 100
                && self.strategy.refresh_ms <= 60_000
                && self.strategy.refresh_ticks > 0
                && self.strategy.drawdown_stop_usdt > 0,
            "invalid strategy limits"
        );
        ensure!(
            self.maker_rebate_microbps <= 100_000_000
                && (self.maker_rebate_microbps == 0 || self.hypothetical),
            "rebates must be bounded and hypothetical"
        );
        ensure!(
            !self.promotion_verified
                || (self
                    .verification_reference
                    .as_ref()
                    .is_some_and(|s| !s.trim().is_empty())
                    && self.account_access_verified),
            "verified promotion requires documented external account eligibility evidence"
        );
        ensure!(
            !self.account_access_verified
                || self
                    .verification_reference
                    .as_ref()
                    .is_some_and(|s| !s.trim().is_empty()),
            "account eligibility evidence missing"
        );
        ensure!(
            self.closeout_slippage_bps <= 1000
                && self.adverse_terminal_shock_bps <= 5000
                && self
                    .outage_every_ms
                    .is_none_or(|v| v > self.outage_duration_ms && self.outage_duration_ms > 0),
            "invalid stress assumptions"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub capital_usdt: u64,
    pub train_fraction_ppm: u64,
    pub validation_fraction_ppm: u64,
    pub split_embargo_ms: u64,
    pub scenarios: Vec<Scenario>,
}
impl Experiment {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.capital_usdt == 5000
                && self.train_fraction_ppm > 0
                && self.validation_fraction_ppm > 0
                && self.train_fraction_ppm + self.validation_fraction_ppm < 1_000_000
                && self.split_embargo_ms >= 60_000,
            "invalid capital/splits (at least 60s embargo)"
        );
        ensure!(
            !self.scenarios.is_empty() && self.scenarios.len() <= 32,
            "scenario count out of bounds"
        );
        let mut names = std::collections::BTreeSet::new();
        for s in &self.scenarios {
            s.validate()?;
            ensure!(names.insert(&s.name), "duplicate scenario name");
        }
        Ok(())
    }
}

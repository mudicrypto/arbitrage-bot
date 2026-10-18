//! Explicit initial holdings and average-cost accounting; never actual purchases/deposits.
use crate::engine::{Engine, LedgerEntry, S};
use anyhow::{Context, Result, ensure};
use mm_research::{
    discovery::Instrument,
    fees::{FeeAsset, notional},
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionModel {
    #[default]
    ExactPrice,
    ConservativeThrough,
    AlternativeThrough,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Initialization {
    pub base_allocation_ppm: u64,
    pub valuation_price_units: u64,
    pub valuation_time_ms: u64,
    pub fee_buffer_ppm: u64,
}
impl Initialization {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.base_allocation_ppm <= 1_000_000
                && self.valuation_price_units > 0
                && self.fee_buffer_ppm <= 1_000_000,
            "invalid initial portfolio"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortfolioState {
    pub initial_equity_atoms: i128,
    pub initial_base_units: u64,
    pub initial_quote_atoms: i128,
    pub initial_bnb_units: u64,
    pub initial_valuation_price: u64,
    pub initial_valuation_time_ms: u64,
    pub base_cost_basis_atoms: i128,
    pub bnb_cost_basis_atoms: i128,
    pub realized_gross_atoms: i128,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Balances {
    pub quote_total: i128,
    pub base_total: i128,
    pub bnb_total: i128,
    pub quote_reserved: i128,
    pub base_reserved: i128,
    pub bnb_reserved: i128,
    pub quote_available: i128,
    pub base_available: i128,
    pub bnb_available: i128,
}
impl Engine {
    pub fn funded(
        scenario: crate::config::Scenario,
        instrument: Instrument,
        init: Initialization,
        model: ExecutionModel,
    ) -> Result<Self> {
        Self::funded_budget(scenario, instrument, init, model, 5000)
    }
    pub(crate) fn funded_budget(
        scenario: crate::config::Scenario,
        instrument: Instrument,
        init: Initialization,
        model: ExecutionModel,
        capital_usdt: u64,
    ) -> Result<Self> {
        init.validate()?;
        let mut e = Self::new_budget(scenario, instrument, capital_usdt)?;
        let budget =
            u128::from(capital_usdt) * u128::from(S) * u128::from(init.base_allocation_ppm)
                / 1_000_000;
        let units = budget * u128::from(S)
            / u128::from(init.valuation_price_units)
            / u128::from(e.instrument.step_units)
            * u128::from(e.instrument.step_units);
        let quantity = u64::try_from(units)?;
        let value = notional(init.valuation_price_units, quantity)? as i128;
        ensure!(
            value <= e.cash,
            "base allocation and separately funded fee asset exceed total capital"
        );
        e.cash -= value;
        e.base += i128::from(quantity);
        if value > 0 {
            e.ledger.push(LedgerEntry {
                time_ms: init.valuation_time_ms,
                asset: "USDT".into(),
                delta: -value,
                reason: "initial_portfolio_reclassification_not_execution".into(),
                order_id: None,
            });
            e.ledger.push(LedgerEntry {
                time_ms: init.valuation_time_ms,
                asset: e.instrument.base_asset.clone(),
                delta: quantity.into(),
                reason: "initial_portfolio_reclassification_not_execution".into(),
                order_id: None,
            });
        }
        e.portfolio.initial_base_units = quantity;
        e.portfolio.initial_quote_atoms = e.cash;
        e.portfolio.initial_bnb_units = u64::try_from(e.bnb)?;
        e.portfolio.initial_valuation_price = init.valuation_price_units;
        e.portfolio.initial_valuation_time_ms = init.valuation_time_ms;
        e.portfolio.base_cost_basis_atoms = value;
        e.reservation_buffer_ppm = init.fee_buffer_ppm;
        e.execution_model = model;
        e.clock = init.valuation_time_ms;
        e.reconcile()?;
        ensure!(
            e.marked_equity(init.valuation_price_units)?
                == i128::from(capital_usdt) * i128::from(S),
            "initial equity must equal declared budget"
        );
        Ok(e)
    }
    pub fn balances(&self) -> Balances {
        let (q, b, n) = self.available();
        Balances {
            quote_total: self.cash,
            base_total: self.base,
            bnb_total: self.bnb,
            quote_reserved: self.cash - q,
            base_reserved: self.base - b,
            bnb_reserved: self.bnb - n,
            quote_available: q,
            base_available: b,
            bnb_available: n,
        }
    }
    pub(crate) fn cost_basis_fill(
        &mut self,
        buy: bool,
        quantity: u64,
        quote: i128,
        fee_asset: FeeAsset,
        fee: u128,
        fee_quote: u128,
    ) -> Result<()> {
        let mut base = self.base;
        if buy {
            self.portfolio.base_cost_basis_atoms += quote;
            base += i128::from(quantity);
        } else {
            ensure!(
                base >= i128::from(quantity) && base > 0,
                "cost basis sell exceeds inventory"
            );
            let cost = self.portfolio.base_cost_basis_atoms * i128::from(quantity) / base;
            self.portfolio.base_cost_basis_atoms -= cost;
            self.portfolio.realized_gross_atoms += quote - cost;
            base -= i128::from(quantity);
        }
        if fee_asset == FeeAsset::Base && fee > 0 {
            ensure!(
                base >= fee as i128 && base > 0,
                "base fee exceeds acquired/available quantity"
            );
            let cost = self.portfolio.base_cost_basis_atoms * fee as i128 / base;
            self.portfolio.base_cost_basis_atoms -= cost;
            self.portfolio.realized_gross_atoms += fee_quote as i128 - cost;
        }
        if fee_asset == FeeAsset::Bnb && fee > 0 {
            ensure!(
                self.bnb >= fee as i128 && self.bnb > 0,
                "BNB fee exceeds balance"
            );
            let cost = self.portfolio.bnb_cost_basis_atoms * fee as i128 / self.bnb;
            self.portfolio.bnb_cost_basis_atoms -= cost;
            self.portfolio.realized_gross_atoms += fee_quote as i128 - cost;
        }
        Ok(())
    }
    pub fn portfolio_pnl(&self, mid: u64) -> Result<serde_json::Value> {
        let unrealized = notional(mid, u64::try_from(self.base)?)? as i128
            - self.portfolio.base_cost_basis_atoms
            + notional(self.bnb_price(), u64::try_from(self.bnb)?)? as i128
            - self.portfolio.bnb_cost_basis_atoms;
        let net = self.marked_equity(mid)? - self.portfolio.initial_equity_atoms;
        let explained = self.portfolio.realized_gross_atoms + unrealized - self.fees + self.rebates;
        ensure!(
            net == explained,
            "realized/unrealized P&L fails exact equity reconciliation: {net} != {explained}"
        );
        Ok(
            serde_json::json!({"realized_gross_atoms":self.portfolio.realized_gross_atoms,"unrealized_gross_atoms":unrealized,"fees_atoms":self.fees,"rebates_atoms":self.rebates,"net_pnl_atoms":net,"reconciled":true,"cost_basis":"moving average; fee-asset disposals recognize their valuation less allocated cost basis; fees deducted once"}),
        )
    }
    pub fn current_mid(&self) -> u64 {
        self.view
            .as_ref()
            .map_or(self.portfolio.initial_valuation_price, |v| v.mid())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedBudget {
    pub total_usdt: u64,
    pub markets: std::collections::BTreeMap<String, u64>,
}
impl SharedBudget {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.total_usdt == 5000
                && !self.markets.is_empty()
                && self.markets.len() <= 20
                && self.markets.values().all(|v| *v > 0)
                && self
                    .markets
                    .values()
                    .try_fold(0u64, |a, b| a.checked_add(*b))
                    .is_some_and(|v| v == 5000),
            "market partitions must sum to exactly the one $5000 budget"
        );
        Ok(())
    }
    pub fn initialize(
        &self,
        scenario: crate::config::Scenario,
        instrument: Instrument,
        init: Initialization,
        model: ExecutionModel,
    ) -> Result<Engine> {
        self.validate()?;
        let capital = *self
            .markets
            .get(&instrument.symbol)
            .context("market has no shared budget")?;
        Engine::funded_budget(scenario, instrument, init, model, capital)
    }
}

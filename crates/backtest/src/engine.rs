//! Local-only simulation. No exchange write transport is linked or exposed.
use crate::{
    config::{QueueModel, Scenario},
    data::{Input, Tick, View},
};
use anyhow::{Context, Result, ensure};
use mm_research::{
    discovery::Instrument,
    fees::{FeeAsset, RATE_DENOM, ceil_div, notional},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const S: u64 = 100_000_000;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerEntry {
    pub time_ms: u64,
    pub asset: String,
    pub delta: i128,
    pub reason: String,
    pub order_id: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Execution {
    pub execution_time_ms: Option<u64>,
    pub time_ms: u64,
    pub action: String,
    pub order_id: u64,
    pub buy: bool,
    pub price: u64,
    pub quantity: u64,
    pub fee_asset: Option<String>,
    pub fee_atoms: i128,
    pub quote_fee_atoms: i128,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Fill {
    pub evidence_kind: String,
    pub aggregate_trade_id: u64,
    pub observed_trade_price: u64,
    pub observed_trade_quantity: u64,
    pub observed_at_ms: u64,
    pub epoch: u64,
    pub time_ms: u64,
    pub buy: bool,
    pub price: u64,
    pub quantity: u64,
    pub reference_mid: u64,
    pub order_id: u64,
}
#[derive(Debug, Clone)]
pub struct Order {
    pub id: u64,
    pub buy: bool,
    pub price: u64,
    pub remaining: u64,
    pub ahead: u128,
    pub active: bool,
    pub entry_due: u64,
    pub cancel_due: Option<u64>,
    pub cancel_effective_logged: bool,
    pub reserved_quote: u128,
    pub reserved_base: u128,
    pub reserved_bnb: u128,
    pub entry_book: Option<std::sync::Arc<View>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EquityPoint {
    pub mid: u64,
    pub epoch: u64,
    pub time_ms: u64,
    pub equity_atoms: i128,
    pub base_atoms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Markout {
    pub seconds: u64,
    pub completed: usize,
    pub mean_bps: Option<f64>,
    pub adverse_quote_atoms: Option<i128>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Summary {
    pub symbol: String,
    pub scenario: String,
    pub split: String,
    pub result_type: String,
    pub hypothetical: bool,
    pub account_access_verified: bool,
    pub start_ms: u64,
    pub end_ms: u64,
    pub gross_spread_capture_atoms: i128,
    pub maker_fees_atoms: i128,
    pub rebate_atoms: i128,
    pub closeout_spread_cost_atoms: i128,
    pub slippage_stress_cost_atoms: i128,
    pub estimated_taker_fee_atoms: i128,
    pub inventory_pnl_atoms: i128,
    pub net_pnl_atoms: i128,
    pub estimated_closeout_cost_atoms: i128,
    pub closeout_adjusted_pnl_atoms: i128,
    pub pnl_per_million_usdt: Option<f64>,
    pub executed_notional_atoms: u128,
    pub turnover: f64,
    pub max_drawdown_atoms: i128,
    pub fill_count: usize,
    pub submitted_orders: u64,
    pub accepted_orders: u64,
    pub fill_ratio: Option<f64>,
    pub filled_quantity_ratio: Option<f64>,
    pub cancellation_ratio: Option<f64>,
    pub rejected_orders: u64,
    pub cancelled_orders: u64,
    pub open_orders_at_end: usize,
    pub exposure_utilization: f64,
    pub max_inventory_notional_atoms: u128,
    pub markouts: Vec<Markout>,
    pub data_quality_ok: bool,
    pub coverage_interruptions: u64,
    pub drawdown_stopped: bool,
    pub net_positive_after_costs_and_without_favorable_inventory: bool,
    pub no_trade_pnl_atoms: i128,
    pub no_trade_drawdown_atoms: i128,
    pub daily_pnl_atoms: BTreeMap<String, i128>,
    pub daily_max_drawdown_atoms: BTreeMap<String, i128>,
    pub fee_assets: BTreeMap<String, i128>,
    pub warnings: Vec<String>,
}
pub struct Engine {
    pub automatic_quoting: bool,
    pub price_bounds: Option<(u64, u64)>,
    pub portfolio: crate::portfolio::PortfolioState,
    pub execution_model: crate::portfolio::ExecutionModel,
    pub reservation_buffer_ppm: u64,
    pub diagnostics: crate::diagnostics::Funnel,
    pub decisions: Vec<crate::diagnostics::Decision>,
    pub execution_evidence: BTreeMap<u64, crate::diagnostics::ExecutionEvidence>,
    pub entry_deadlines: BTreeMap<u64, u64>,
    pub initial_queue_ahead: BTreeMap<u64, u128>,
    last_trade_id: Option<u64>,
    pub cancel_deadlines: BTreeMap<u64, u64>,
    pub scenario: Scenario,
    pub instrument: Instrument,
    pub orders: Vec<Order>,
    pub cash: i128,
    pub base: i128,
    pub bnb: i128,
    pub ledger: Vec<LedgerEntry>,
    pub logs: Vec<Execution>,
    pub fills: Vec<Fill>,
    pub equity: Vec<EquityPoint>,
    pub reference_marks: Vec<(u64, u64, u64)>,
    pub gross: i128,
    pub fees: i128,
    pub rebates: i128,
    pub turnover: u128,
    pub submitted: u64,
    pub accepted: u64,
    pub cancelled: u64,
    pub rejected: u64,
    pub data_ok: bool,
    pub interruptions: u64,
    pub stopped: bool,
    pub settling: bool,
    pub clock: u64,
    pub last_book_ms: Option<u64>,
    pub view: Option<View>,
    epoch: u64,
    capital: i128,
    next_id: u64,
    rng: u64,
    last_refresh: u64,
    last_mid: Option<u64>,
    vol_bps: u64,
    peak: i128,
    max_drawdown: i128,
    max_inventory: u128,
    submitted_quantity: u128,
    filled_quantity: u128,
    fee_assets: BTreeMap<String, i128>,
}
impl Engine {
    pub fn new(scenario: Scenario, instrument: Instrument, capital_usdt: u64) -> Result<Self> {
        ensure!(
            capital_usdt == 5000,
            "legacy simulation capital must be $5000"
        );
        Self::new_budget(scenario, instrument, capital_usdt)
    }
    pub(crate) fn new_budget(
        scenario: Scenario,
        instrument: Instrument,
        capital_usdt: u64,
    ) -> Result<Self> {
        scenario.validate()?;
        ensure!(
            instrument.tick_units > 0
                && instrument.step_units > 0
                && instrument.base_asset != "BNB"
                && instrument.base_asset != "USDT",
            "unsupported instrument wallet alias or zero increments"
        );
        ensure!(
            capital_usdt > 0 && capital_usdt <= 5000,
            "subportfolio budget out of bounds"
        );
        let capital = i128::from(capital_usdt) * i128::from(S);
        let mut e = Self {
            automatic_quoting: true,
            price_bounds: None,
            portfolio: Default::default(),
            execution_model: Default::default(),
            reservation_buffer_ppm: 0,
            diagnostics: Default::default(),
            decisions: vec![],
            execution_evidence: BTreeMap::new(),
            entry_deadlines: BTreeMap::new(),
            initial_queue_ahead: BTreeMap::new(),
            last_trade_id: None,
            cancel_deadlines: BTreeMap::new(),
            rng: scenario.seed,
            scenario,
            instrument,
            orders: vec![],
            cash: capital,
            base: 0,
            bnb: 0,
            ledger: vec![],
            logs: vec![],
            fills: vec![],
            equity: vec![],
            reference_marks: vec![],
            gross: 0,
            fees: 0,
            rebates: 0,
            turnover: 0,
            submitted: 0,
            accepted: 0,
            cancelled: 0,
            rejected: 0,
            data_ok: true,
            interruptions: 0,
            stopped: false,
            settling: false,
            clock: 0,
            last_book_ms: None,
            view: None,
            epoch: 0,
            capital,
            next_id: 1,
            last_refresh: 0,
            last_mid: None,
            vol_bps: 0,
            peak: capital,
            max_drawdown: 0,
            max_inventory: 0,
            submitted_quantity: 0,
            filled_quantity: 0,
            fee_assets: BTreeMap::new(),
        };
        e.ledger.push(LedgerEntry {
            time_ms: 0,
            asset: "USDT".into(),
            delta: capital,
            reason: "simulated_initial_capital".into(),
            order_id: None,
        });
        // BNB is an explicit hypothetical starting endowment purchased at the manual conversion assumption.
        if let Some(p) = e
            .scenario
            .fees
            .periods
            .iter()
            .find(|p| p.asset_for(true) == FeeAsset::Bnb || p.asset_for(false) == FeeAsset::Bnb)
        {
            let price = p
                .bnb_quote_price_units
                .context("manual BNB conversion missing")?;
            let qty = (25u128 * u128::from(S) * u128::from(S) / u128::from(price)) as i128;
            let cost = notional(price, u64::try_from(qty)?)? as i128;
            e.cash -= cost;
            e.bnb = qty;
            e.portfolio.bnb_cost_basis_atoms = cost;
            e.ledger.push(LedgerEntry {
                time_ms: 0,
                asset: "USDT".into(),
                delta: -cost,
                reason: "hypothetical_BNB_endowment".into(),
                order_id: None,
            });
            e.ledger.push(LedgerEntry {
                time_ms: 0,
                asset: "BNB".into(),
                delta: qty,
                reason: "hypothetical_BNB_endowment".into(),
                order_id: None,
            });
        }
        e.portfolio.initial_equity_atoms = capital;
        e.portfolio.initial_quote_atoms = e.cash;
        e.portfolio.initial_bnb_units = u64::try_from(e.bnb)?;
        e.portfolio.initial_valuation_price = e.instrument.price_units;
        Ok(e)
    }
    fn log(&mut self, action: &str, order: &Order, quantity: u64, reason: &str) -> Result<()> {
        ensure!(
            self.logs.len() < 200_000,
            "bounded execution-log limit reached"
        );
        self.logs.push(Execution {
            execution_time_ms: None,
            time_ms: self.clock,
            action: action.into(),
            order_id: order.id,
            buy: order.buy,
            price: order.price,
            quantity,
            fee_asset: None,
            fee_atoms: 0,
            quote_fee_atoms: 0,
            reason: reason.into(),
        });
        Ok(())
    }
    fn jitter(&mut self) -> u64 {
        self.rng = self
            .rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.rng % (self.scenario.latency_jitter_ms + 1)
    }
    pub(crate) fn bnb_price(&self) -> u64 {
        self.scenario
            .fees
            .at(self.clock, &self.instrument.symbol)
            .ok()
            .and_then(|p| p.bnb_quote_price_units)
            .unwrap_or(0)
    }
    pub fn marked_equity(&self, mid: u64) -> Result<i128> {
        Ok(self.cash
            + notional(mid, u64::try_from(self.base)?)? as i128
            + notional(self.bnb_price(), u64::try_from(self.bnb)?)? as i128)
    }
    pub fn available(&self) -> (i128, i128, i128) {
        (
            self.cash
                - self
                    .orders
                    .iter()
                    .map(|o| o.reserved_quote as i128)
                    .sum::<i128>(),
            self.base
                - self
                    .orders
                    .iter()
                    .map(|o| o.reserved_base as i128)
                    .sum::<i128>(),
            self.bnb
                - self
                    .orders
                    .iter()
                    .map(|o| o.reserved_bnb as i128)
                    .sum::<i128>(),
        )
    }
    pub fn reconcile(&self) -> Result<()> {
        let balances = |asset: &str| {
            self.ledger
                .iter()
                .filter(|l| l.asset == asset)
                .map(|l| l.delta)
                .sum::<i128>()
        };
        ensure!(
            balances("USDT") == self.cash
                && balances(&self.instrument.base_asset) == self.base
                && balances("BNB") == self.bnb,
            "ledger reconciliation failed"
        );
        let (cash, base, bnb) = self.available();
        ensure!(
            self.cash >= 0 && self.base >= 0 && self.bnb >= 0 && cash >= 0 && base >= 0 && bnb >= 0,
            "negative available balances"
        );
        Ok(())
    }
    fn limits(&self, price: u64, qty: u64) -> Result<()> {
        let i = &self.instrument;
        ensure!(
            self.price_bounds
                .is_none_or(|(min, max)| (min == 0 || price >= min) && (max == 0 || price <= max)),
            "price filter bounds"
        );
        ensure!(
            price > 0
                && qty >= i.min_qty_units
                && qty <= i.max_qty_units
                && price.is_multiple_of(i.tick_units)
                && qty.is_multiple_of(i.step_units),
            "tick/lot constraint"
        );
        let exact_product = u128::from(price) * u128::from(qty);
        ensure!(
            exact_product >= u128::from(i.min_notional_units) * u128::from(S)
                && i.max_notional_units
                    .is_none_or(|m| exact_product <= u128::from(m) * u128::from(S)),
            "notional constraint"
        );
        Ok(())
    }
    fn reserve(&self, buy: bool, price: u64, qty: u64) -> Result<(u128, u128, u128)> {
        // Reserve for known future fee changes and a configured fee-only buffer.
        self.scenario.fees.at(self.clock, &self.instrument.symbol)?;
        let mut quote_fee = 0;
        let mut base_fee = 0;
        let mut bnb_fee = 0;
        for period in self.scenario.fees.periods.iter().filter(|p| {
            p.until_ms.is_none_or(|t| t > self.clock)
                && p.symbol
                    .as_ref()
                    .is_none_or(|s| s == &self.instrument.symbol)
        }) {
            let fee = period.charge(price, qty, buy, true)?;
            let buffered = ceil_div(
                fee.amount_atoms * u128::from(1_000_000 + self.reservation_buffer_ppm),
                1_000_000,
            )?;
            match fee.asset {
                FeeAsset::Quote => quote_fee = quote_fee.max(buffered),
                FeeAsset::Base => base_fee = base_fee.max(buffered),
                FeeAsset::Bnb => bnb_fee = bnb_fee.max(buffered),
            }
        }
        let quote = if buy {
            ceil_div(u128::from(price) * u128::from(qty), S.into())?
        } else {
            0
        };
        let base = if buy { 0 } else { u128::from(qty) };
        Ok((
            quote + if buy { quote_fee } else { 0 },
            base + if !buy { base_fee } else { 0 },
            bnb_fee,
        ))
    }
    pub fn submit(&mut self, buy: bool, price: u64, qty: u64) -> Result<bool> {
        self.diagnostics.generated_quote_intents += 1;
        if self.stopped
            || self.settling
            || self.view.is_none()
            || self
                .last_book_ms
                .is_none_or(|t| self.clock.saturating_sub(t) > self.scenario.strategy.stale_ms)
        {
            if self.stopped || self.settling {
                self.diagnostics.rejected_risk_limits += 1;
            } else {
                self.diagnostics.rejected_market_data += 1;
            }
            self.diagnostic_decision(
                buy,
                price,
                qty,
                "rejected",
                if self.stopped {
                    "drawdown_stopped"
                } else if self.settling {
                    "settling"
                } else {
                    "missing_or_stale_book"
                },
            )?;
            return Ok(false);
        }
        if self.limits(price, qty).is_err() {
            self.rejected += 1;
            self.diagnostics.rejected_exchange_filters += 1;
            self.diagnostic_decision(buy, price, qty, "rejected", "tick_lot_or_notional_filter")?;
            return Ok(false);
        }
        if self.execution_model != crate::portfolio::ExecutionModel::ExactPrice
            && self.orders.iter().any(|o| o.buy == buy)
        {
            self.diagnostics.rejected_risk_limits += 1;
            self.diagnostic_decision(
                buy,
                price,
                qty,
                "rejected",
                "through_model_single_order_per_side",
            )?;
            return Ok(false);
        }
        let (rq, rb, rn) = self.reserve(buy, price, qty)?;
        let (cash, base, bnb) = self.available();
        let inventory = notional(self.view.as_ref().unwrap().mid(), u64::try_from(self.base)?)?;
        let pending_buys: u128 = self
            .orders
            .iter()
            .filter(|o| o.buy)
            .map(|o| notional(o.price, o.remaining).unwrap_or(u128::MAX))
            .sum();
        let mid = self.view.as_ref().unwrap().mid();
        let target = notional(mid, self.portfolio.initial_base_units)?;
        let lower = target.saturating_sub(
            u128::from(self.scenario.strategy.inventory_limit_usdt) * u128::from(S),
        );
        let pending_sells = self
            .orders
            .iter()
            .filter(|o| !o.buy)
            .try_fold(0u128, |sum, o| -> Result<u128> {
                Ok(sum + notional(mid, u64::try_from(o.reserved_base)?)?)
            })?;
        let sell_limit = !buy
            && inventory.saturating_sub(pending_sells + notional(mid, u64::try_from(rb)?)?) < lower;
        if cash < rq as i128
            || base < rb as i128
            || bnb < rn as i128
            || sell_limit
            || buy
                && inventory + pending_buys + notional(price, qty)?
                    > notional(
                        self.view.as_ref().unwrap().mid(),
                        self.portfolio.initial_base_units,
                    )? + u128::from(self.scenario.strategy.inventory_limit_usdt) * u128::from(S)
        {
            self.diagnostics.rejected_risk_limits += 1;
            let reason = if cash < rq as i128 {
                "insufficient_available_quote"
            } else if base < rb as i128 {
                "insufficient_available_base"
            } else if bnb < rn as i128 {
                "insufficient_available_bnb"
            } else if sell_limit {
                "position_limit_including_pending_sells"
            } else {
                "position_limit_including_pending_buys"
            };
            self.diagnostic_decision(buy, price, qty, "rejected", reason)?;
            return Ok(false);
        }
        ensure!(
            self.entry_deadlines.len() < 100_000,
            "bounded diagnostic order limit exceeded"
        );
        let due = self.clock + self.scenario.entry_latency_ms + self.jitter();
        let order = Order {
            id: self.next_id,
            buy,
            price,
            remaining: qty,
            ahead: 0,
            active: false,
            entry_due: due,
            cancel_due: None,
            cancel_effective_logged: false,
            reserved_quote: rq,
            reserved_base: rb,
            reserved_bnb: rn,
            entry_book: None,
        };
        self.entry_deadlines.insert(order.id, due);
        self.diagnostics.submitted_to_simulated_execution += 1;
        self.diagnostic_decision(buy, price, qty, "submitted", "passed_all_intent_gates")?;
        self.next_id += 1;
        self.submitted += 1;
        self.submitted_quantity += u128::from(qty);
        self.log("submit", &order, qty, "simulated_post_only_intent")?;
        self.orders.push(order);
        self.reconcile()?;
        Ok(true)
    }
    pub fn cancel(&mut self, id: u64) -> Result<()> {
        let due = self.clock + self.scenario.cancel_latency_ms + self.jitter();
        if let Some(index) = self
            .orders
            .iter()
            .position(|o| o.id == id && o.cancel_due.is_none())
        {
            self.orders[index].cancel_due = Some(due);
            self.cancel_deadlines.insert(id, due);
            self.log(
                "cancel_request",
                &self.orders[index].clone(),
                0,
                "cancellation_not_yet_effective",
            )?;
        }
        Ok(())
    }
    fn due(&mut self, time: u64, inclusive: bool) -> Result<()> {
        let ready = |t: u64| if inclusive { t <= time } else { t < time };
        loop {
            let next = self
                .orders
                .iter()
                .enumerate()
                .flat_map(|(i, o)| {
                    let mut actions = vec![];
                    if !o.active && o.cancel_due.is_none_or(|c| o.entry_due < c) {
                        actions.push((o.entry_due, 0u8, i));
                    }
                    if let Some(cancel) = o.cancel_due {
                        if !o.cancel_effective_logged {
                            actions.push((cancel, 1, i));
                        } else {
                            actions.push((cancel + self.scenario.report_latency_ms, 2, i));
                        }
                    }
                    actions
                })
                .filter(|(t, _, _)| ready(*t))
                .min();
            let Some((due, action, index)) = next else {
                break;
            };
            self.clock = due;
            let mut order = self.orders[index].clone();
            if action == 1 {
                order.cancel_effective_logged = true;
                for later in &mut self.orders {
                    if later.active
                        && later.buy == order.buy
                        && later.price == order.price
                        && (later.entry_due, later.id) > (order.entry_due, order.id)
                    {
                        later.ahead = later.ahead.saturating_sub(u128::from(order.remaining));
                    }
                }
                self.cancelled += 1;
                self.log(
                    "cancel_effective",
                    &order,
                    0,
                    "awaiting_delayed_fill_reports_reservations_retained",
                )?;
                self.orders[index] = order;
                continue;
            }
            if action == 2 {
                self.log("cancel_ack", &order, 0, "fill_report_grace_elapsed")?;
                self.orders.remove(index);
                continue;
            }
            let valid = self.view.as_ref().is_some_and(|v| {
                if order.buy {
                    order.price < v.ask() && order.price >= v.bids.last().unwrap().0
                } else {
                    order.price > v.bid() && order.price <= v.asks.last().unwrap().0
                }
            }) && self
                .last_book_ms
                .is_some_and(|t| due.saturating_sub(t) <= self.scenario.strategy.stale_ms);
            if !valid {
                self.rejected += 1;
                self.log(
                    "reject",
                    &order,
                    0,
                    "post_only_cross_or_unknown_depth_or_stale",
                )?;
                self.orders.remove(index);
                continue;
            }
            order.ahead = u128::from(self.view.as_ref().unwrap().quantity(order.buy, order.price))
                * u128::from(self.scenario.queue_multiplier_ppm)
                / 1_000_000;
            // Queue ahead is a cumulative position, including earlier own orders.
            // Shared public volume must not be consumed independently a second time.
            for earlier in self.orders.iter().filter(|o| {
                o.active
                    && !o.cancel_effective_logged
                    && o.buy == order.buy
                    && o.price == order.price
            }) {
                order.ahead = order
                    .ahead
                    .max(earlier.ahead + u128::from(earlier.remaining));
            }
            if self.execution_model != crate::portfolio::ExecutionModel::ExactPrice {
                order.entry_book = self.view.clone().map(std::sync::Arc::new);
            }
            self.initial_queue_ahead.insert(order.id, order.ahead);
            order.active = true;
            self.accepted += 1;
            self.log("accept", &order, 0, "visible_queue_joined_at_tail")?;
            self.orders[index] = order;
        }
        self.clock = time;
        self.reconcile()
    }
    fn invalidate(&mut self, reason: &str) -> Result<()> {
        *self
            .diagnostics
            .dependency_failures
            .entry(reason.into())
            .or_default() += 1;
        if self.last_mid.is_some() && self.view.is_some() {
            self.data_ok = false;
            self.interruptions += 1;
            self.epoch += 1;
        }
        let orders = std::mem::take(&mut self.orders);
        for o in orders {
            self.log("coverage_lost", &o, 0, reason)?;
        }
        self.view = None;
        self.last_book_ms = None;
        Ok(())
    }
    fn fill(
        &mut self,
        index: usize,
        qty: u64,
        execution_ms: u64,
        reference_mid: u64,
        trade: &mm_market_data::model::Trade,
        evidence_kind: &str,
    ) -> Result<()> {
        let mut o = self.orders.remove(index);
        let qty = qty.min(o.remaining);
        if qty == 0 {
            self.orders.insert(index, o);
            return Ok(());
        }
        let period = self
            .scenario
            .fees
            .at(execution_ms, &self.instrument.symbol)?;
        let charge = period.charge(o.price, qty, o.buy, true)?;
        let quote = if o.buy {
            ceil_div(u128::from(o.price) * u128::from(qty), S.into())?
        } else {
            notional(o.price, qty)?
        } as i128;
        let fee = charge.amount_atoms as i128;
        let asset = match charge.asset {
            FeeAsset::Quote => "USDT",
            FeeAsset::Base => self.instrument.base_asset.as_str(),
            FeeAsset::Bnb => "BNB",
        }
        .to_string();
        let rebate = if self.scenario.maker_rebate_microbps > 0 {
            u128::from(o.price) * u128::from(qty) * u128::from(self.scenario.maker_rebate_microbps)
                / (u128::from(S) * RATE_DENOM)
        } else {
            0
        } as i128;
        let cash_delta = if o.buy { -quote } else { quote }
            - if charge.asset == FeeAsset::Quote {
                fee
            } else {
                0
            }
            + rebate;
        let base_delta = if o.buy {
            i128::from(qty)
        } else {
            -i128::from(qty)
        } - if charge.asset == FeeAsset::Base {
            fee
        } else {
            0
        };
        let bnb_delta = if charge.asset == FeeAsset::Bnb {
            -fee
        } else {
            0
        };
        let new_remaining = o.remaining - qty;
        let remaining_reservations = if new_remaining > 0 {
            self.reserve(o.buy, o.price, new_remaining)?
        } else {
            (0, 0, 0)
        };
        let free_without_this_order = self.available();
        if free_without_this_order.0 + cash_delta < remaining_reservations.0 as i128
            || free_without_this_order.1 + base_delta < remaining_reservations.1 as i128
            || free_without_this_order.2 + bnb_delta < remaining_reservations.2 as i128
        {
            self.rejected += 1;
            self.data_ok = false;
            self.stopped = true;
            self.log("reject", &o, 0, "fee_balance_unavailable_invalid_model")?;
            return Ok(());
        }
        self.cost_basis_fill(
            o.buy,
            qty,
            quote,
            charge.asset,
            charge.amount_atoms,
            charge.quote_equivalent_atoms,
        )?;
        self.cash += cash_delta;
        self.base += base_delta;
        self.bnb += bnb_delta;
        for (a, d, reason) in [
            (
                "USDT".to_string(),
                if o.buy { -quote } else { quote },
                "trade",
            ),
            (
                self.instrument.base_asset.clone(),
                if o.buy {
                    i128::from(qty)
                } else {
                    -i128::from(qty)
                },
                "trade",
            ),
            (asset.clone(), -fee, "maker_fee"),
            ("USDT".to_string(), rebate, "hypothetical_rebate"),
        ] {
            if d != 0 {
                self.ledger.push(LedgerEntry {
                    time_ms: execution_ms,
                    asset: a,
                    delta: d,
                    reason: reason.into(),
                    order_id: Some(o.id),
                });
            }
        }
        self.fees += charge.quote_equivalent_atoms as i128;
        self.rebates += rebate;
        *self.fee_assets.entry(asset.clone()).or_default() += fee;
        let mid = reference_mid;
        let capture = (i128::from(mid) - i128::from(o.price)) * i128::from(qty) / i128::from(S)
            * if o.buy { 1 } else { -1 };
        self.gross += capture;
        self.turnover += notional(o.price, qty)?;
        self.filled_quantity += u128::from(qty);
        self.execution_evidence
            .entry(o.id)
            .or_default()
            .filled_quantity += u128::from(qty);
        if evidence_kind != "exact_price_after_queue_depletion" {
            self.execution_evidence
                .entry(o.id)
                .or_default()
                .through_filled_quantity += u128::from(qty);
        }
        self.fills.push(Fill {
            evidence_kind: evidence_kind.into(),
            aggregate_trade_id: trade.aggregate_id,
            observed_trade_price: trade.price,
            observed_trade_quantity: trade.quantity,
            observed_at_ms: self.clock,
            epoch: self.epoch,
            time_ms: execution_ms,
            buy: o.buy,
            price: o.price,
            quantity: qty,
            reference_mid: mid,
            order_id: o.id,
        });
        self.log("fill", &o, qty, evidence_kind)?;
        let log = self.logs.last_mut().unwrap();
        log.execution_time_ms = Some(execution_ms);
        log.fee_asset = Some(asset);
        log.fee_atoms = fee;
        log.quote_fee_atoms = charge.quote_equivalent_atoms as i128;
        o.remaining -= qty;
        if o.remaining > 0 {
            let (q, b, n) = self.reserve(o.buy, o.price, o.remaining)?;
            o.reserved_quote = q;
            o.reserved_base = b;
            o.reserved_bnb = n;
            self.orders.insert(index, o);
        }
        self.reconcile()
    }
    pub fn on_trade(&mut self, t: &mm_market_data::model::Trade) -> Result<()> {
        if t.symbol != self.instrument.symbol || t.price == 0 || t.quantity == 0 {
            self.invalidate("invalid_trade_dependency")?;
            return Ok(());
        }
        if self.last_trade_id.is_some_and(|id| t.aggregate_id <= id) {
            *self
                .diagnostics
                .dependency_failures
                .entry("duplicate_or_old_trade_ignored".into())
                .or_default() += 1;
            return Ok(());
        }
        self.last_trade_id = Some(t.aggregate_id);
        if self.view.is_none()
            || self
                .last_book_ms
                .is_none_or(|b| self.clock.saturating_sub(b) > self.scenario.strategy.stale_ms)
        {
            return Ok(());
        }
        if t.exchange_ms > self.clock
            || self.clock.saturating_sub(t.exchange_ms) > self.scenario.report_latency_ms
        {
            self.invalidate("trade_report_delay_exceeds_model_budget")?;
            return Ok(());
        }
        let Some(reference_mid) = self
            .reference_marks
            .iter()
            .rev()
            .find(|(time, _, epoch)| *time <= t.exchange_ms && *epoch == self.epoch)
            .map(|(_, mid, _)| *mid)
        else {
            return Ok(());
        };
        if t.exchange_ms < self.clock
            && self.orders.iter().any(|o| {
                o.cancel_effective_logged
                    && o.price == t.price
                    && o.buy == t.buyer_is_maker
                    && o.cancel_due.is_some_and(|c| t.exchange_ms <= c)
            })
            && self
                .orders
                .iter()
                .filter(|o| o.price == t.price && o.buy == t.buyer_is_maker)
                .count()
                > 1
        {
            self.invalidate("late_multi_order_cancel_priority_ambiguous")?;
            return Ok(());
        }
        let eligible =
            u128::from(t.quantity) * u128::from(self.scenario.trade_participation_ppm) / 1_000_000;
        let mut fill_budget = eligible;
        self.orders.sort_by_key(|o| (o.entry_due, o.id));
        let mut i = 0;
        while i < self.orders.len() {
            if !self.orders[i].active
                || t.exchange_ms <= self.orders[i].entry_due
                || self.orders[i]
                    .cancel_due
                    .is_some_and(|due| t.exchange_ms > due)
                || self.orders[i].buy != t.buyer_is_maker
            {
                i += 1;
                continue;
            }
            let through = self.orders[i].price != t.price;
            let mut event_budget = eligible;
            let mut kind = "exact_price_after_queue_depletion";
            if through {
                if !((self.orders[i].buy && t.price < self.orders[i].price)
                    || (!self.orders[i].buy && t.price > self.orders[i].price))
                {
                    i += 1;
                    continue;
                }
                self.execution_evidence
                    .entry(self.orders[i].id)
                    .or_default()
                    .trade_through_valid_window_events += 1;
                use crate::portfolio::ExecutionModel;
                match self.execution_model {
                    ExecutionModel::ExactPrice => {
                        i += 1;
                        continue;
                    }
                    ExecutionModel::ConservativeThrough => {
                        let book = self.orders[i]
                            .entry_book
                            .as_ref()
                            .context("entry depth unavailable")?;
                        let levels = if self.orders[i].buy {
                            &book.bids
                        } else {
                            &book.asks
                        };
                        let known = if self.orders[i].buy {
                            t.price >= levels.last().unwrap().0
                        } else {
                            t.price <= levels.last().unwrap().0
                        };
                        if !known {
                            self.execution_evidence
                                .entry(self.orders[i].id)
                                .or_default()
                                .unknown_through_depth_events += 1;
                            i += 1;
                            continue;
                        }
                        let barrier: u128 = levels
                            .iter()
                            .filter(|(p, _)| {
                                if self.orders[i].buy {
                                    *p < self.orders[i].price && *p > t.price
                                } else {
                                    *p > self.orders[i].price && *p < t.price
                                }
                            })
                            .map(|(_, q)| u128::from(*q))
                            .sum();
                        event_budget = event_budget.saturating_sub(barrier);
                        kind = "conservative_trade_through_after_depth_and_queue";
                    }
                    ExecutionModel::AlternativeThrough => {
                        self.orders[i].ahead = 0;
                        kind = "alternative_trade_through_assumes_priority_cleared";
                    }
                }
            }
            let evidence = self
                .execution_evidence
                .entry(self.orders[i].id)
                .or_default();
            if through {
                evidence.through_eligible_quantity += event_budget;
            } else {
                evidence.exact_price_eligible_trade_events += 1;
            }
            evidence.observed_eligible_quantity += event_budget;
            evidence.queue_blocked_trade_events += u64::from(self.orders[i].ahead >= event_budget);
            let consume = self.orders[i].ahead.min(event_budget);
            evidence.queue_consumed_quantity += consume;
            self.orders[i].ahead -= consume;
            let remaining = (event_budget - consume).min(fill_budget);
            let fill = remaining.min(u128::from(self.orders[i].remaining)) as u64
                / self.instrument.step_units
                * self.instrument.step_units;
            if fill == 0 && remaining > 0 {
                self.execution_evidence
                    .entry(self.orders[i].id)
                    .or_default()
                    .quantity_blocked_by_lot_rounding += remaining;
            }
            if fill > 0 {
                fill_budget -= u128::from(fill);
                let id = self.orders[i].id;
                self.fill(i, fill, t.exchange_ms, reference_mid, t, kind)?;
                if self.orders.get(i).is_some_and(|o| o.id == id) {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        Ok(())
    }
    fn quote(&mut self) -> Result<()> {
        if self.stopped
            || self.settling
            || self.clock.saturating_sub(self.last_refresh) < self.scenario.strategy.refresh_ms
        {
            return Ok(());
        }
        let Some(view) = self.view.clone() else {
            return Ok(());
        };
        self.diagnostics.quote_opportunities += 2;
        let mid = view.mid();
        let observed_spread = u128::from(view.ask() - view.bid()) * 10_000 / u128::from(mid);
        if observed_spread > self.scenario.strategy.max_spread_bps.into() {
            let ids: Vec<_> = self.orders.iter().map(|o| o.id).collect();
            for id in ids {
                self.cancel(id)?;
            }
            return Ok(());
        }
        let fee_bps = ceil_div(
            self.scenario
                .fees
                .at(self.clock, &self.instrument.symbol)?
                .roundtrip_microbps()?,
            1_000_000,
        )? as u64;
        let spread = self
            .scenario
            .strategy
            .spread_bps
            .max(fee_bps + self.scenario.strategy.required_edge_bps)
            .max(self.vol_bps * self.scenario.strategy.volatility_multiplier);
        if spread > self.scenario.strategy.spread_bps {
            self.diagnostics.fee_widened_opportunities += u64::from(
                fee_bps + self.scenario.strategy.required_edge_bps
                    > self.scenario.strategy.spread_bps,
            ) * 2;
        }
        if spread > self.scenario.strategy.max_quote_spread_bps {
            for buy in [true, false] {
                self.diagnostics.generated_quote_intents += 1;
                let fee_reject = fee_bps + self.scenario.strategy.required_edge_bps
                    > self.scenario.strategy.max_quote_spread_bps;
                if fee_reject {
                    self.diagnostics.rejected_fee_constraints += 1;
                } else {
                    self.diagnostics.rejected_risk_limits += 1;
                }
                self.diagnostic_decision(
                    buy,
                    mid,
                    0,
                    "rejected",
                    if fee_reject {
                        "fee_aware_width_exceeds_max_quote_spread"
                    } else {
                        "volatility_or_fixed_width_exceeds_quote_cap"
                    },
                )?;
            }
            let ids: Vec<_> = self.orders.iter().map(|o| o.id).collect();
            for id in ids {
                self.cancel(id)?;
            }
            self.last_refresh = self.clock;
            return Ok(());
        }
        let deviation = (self.base - i128::from(self.portfolio.initial_base_units))
            * i128::from(mid)
            / i128::from(S);
        let skew = deviation * i128::from(self.scenario.strategy.inventory_skew_bps)
            / (i128::from(self.scenario.strategy.inventory_limit_usdt) * i128::from(S));
        let center = (i128::from(mid) * (10_000 - skew.clamp(-9999, 9999)) / 10_000) as u128;
        let half = u128::from(mid) * u128::from(spread) / (2 * 10_000);
        let tick = u128::from(self.instrument.tick_units);
        let bid = (center.saturating_sub(half) / tick * tick) as u64;
        let ask = (ceil_div(center + half, tick)? * tick) as u64;
        let qty = (u128::from(self.scenario.strategy.clip_usdt) * u128::from(S) * u128::from(S)
            / u128::from(mid)
            / u128::from(self.instrument.step_units)
            * u128::from(self.instrument.step_units)) as u64;
        for (buy, p) in [(true, bid), (false, ask)] {
            if self.orders.iter().any(|o| o.buy == buy) {
                self.diagnostics.existing_quote_retained_or_canceling += 1;
                self.diagnostic_decision(
                    buy,
                    p,
                    qty,
                    "retained",
                    "existing_order_or_cancel_grace",
                )?;
                let ids: Vec<_> = self
                    .orders
                    .iter()
                    .filter(|o| {
                        o.buy == buy
                            && o.price.abs_diff(p)
                                >= self
                                    .instrument
                                    .tick_units
                                    .saturating_mul(self.scenario.strategy.refresh_ticks)
                    })
                    .map(|o| o.id)
                    .collect();
                for id in ids {
                    self.cancel(id)?;
                }
            } else {
                self.submit(buy, p, qty)?;
            }
        }
        self.last_refresh = self.clock;
        Ok(())
    }
    pub fn step(&mut self, tick: &Tick) -> Result<()> {
        ensure!(
            tick.time_ms >= self.clock,
            "nonchronological simulation input"
        );
        self.clock = tick.time_ms;
        if self
            .last_book_ms
            .is_some_and(|t| self.clock.saturating_sub(t) > self.scenario.strategy.stale_ms)
        {
            self.invalidate("stale_feed_unknown_fills_invalidates_experiment")?;
        }
        if self
            .scenario
            .outage_every_ms
            .is_some_and(|p| self.clock % p < self.scenario.outage_duration_ms)
        {
            self.invalidate("injected_data_outage")?;
            return Ok(());
        }
        self.due(tick.time_ms, false)?;
        match &tick.input {
            Input::Invalid { reason } => self.invalidate(reason)?,
            Input::Trade { trade } => self.on_trade(trade)?,
            Input::Book { view, exchange_ms } => {
                if *exchange_ms > tick.time_ms
                    || tick.time_ms.abs_diff(*exchange_ms) > self.scenario.strategy.stale_ms
                    || !view.valid()
                {
                    self.invalidate("invalid_or_lagged_book")?;
                    return Ok(());
                }
                if u128::from(view.ask() - view.bid()) * 10_000
                    > u128::from(view.mid()) * u128::from(self.scenario.strategy.max_spread_bps)
                {
                    self.invalidate("abnormal_spread_pause")?;
                    return Ok(());
                }
                if self.scenario.queue == QueueModel::ProportionalCancellation
                    && let Some(old) = &self.view
                {
                    for o in &mut self.orders {
                        let before = old.quantity(o.buy, o.price);
                        let after = view.quantity(o.buy, o.price);
                        if before > after && before > 0 {
                            o.ahead = o.ahead * u128::from(after) / u128::from(before);
                        }
                    }
                }
                // Public cancellation credit cannot move a later own order ahead of FIFO.
                self.orders.sort_by_key(|o| (o.entry_due, o.id));
                let mut tails: BTreeMap<(bool, u64), u128> = BTreeMap::new();
                for o in self
                    .orders
                    .iter_mut()
                    .filter(|o| o.active && !o.cancel_effective_logged)
                {
                    let tail = tails.entry((o.buy, o.price)).or_default();
                    o.ahead = o.ahead.max(*tail);
                    *tail = o.ahead + u128::from(o.remaining);
                }
                let mid = view.mid();
                if let Some(old) = self.last_mid {
                    let movement = u128::from(mid.abs_diff(old)) * 10_000 / u128::from(old);
                    self.vol_bps = ((u128::from(self.vol_bps) * 9 + movement) / 10) as u64;
                }
                ensure!(
                    self.reference_marks.len() < 100_000,
                    "reference trace cap exceeded"
                );
                self.reference_marks.push((*exchange_ms, mid, self.epoch));
                self.last_mid = Some(mid);
                self.view = Some(view.clone());
                self.last_book_ms = Some(tick.time_ms);
            }
        }
        self.due(tick.time_ms, true)?;
        if let Some(v) = &self.view {
            let mid = v.mid();
            let equity = self.marked_equity(mid)?;
            self.peak = self.peak.max(equity);
            self.max_drawdown = self.max_drawdown.max(self.peak - equity);
            self.max_inventory = self
                .max_inventory
                .max(notional(mid, u64::try_from(self.base)?)?);
            if self.peak - equity
                >= i128::from(self.scenario.strategy.drawdown_stop_usdt) * i128::from(S)
            {
                self.stopped = true;
                let ids: Vec<_> = self.orders.iter().map(|o| o.id).collect();
                for id in ids {
                    self.cancel(id)?;
                }
            }
            if self.equity.last().is_none_or(|e| {
                e.time_ms != tick.time_ms || e.equity_atoms != equity || e.mid != mid
            }) {
                ensure!(self.equity.len() < 100_000, "equity trace cap exceeded");
                self.equity.push(EquityPoint {
                    mid,
                    epoch: self.epoch,
                    time_ms: tick.time_ms,
                    equity_atoms: equity,
                    base_atoms: u64::try_from(self.base)?,
                });
            }
        }
        if self.automatic_quoting {
            self.quote()?;
        }
        self.reconcile()
    }
    fn exposure(&self, start: u64, end: u64) -> f64 {
        let mut area = 0.0;
        for (index, point) in self.equity.iter().enumerate() {
            let next = self
                .equity
                .get(index + 1)
                .map_or(end, |p| p.time_ms)
                .min(end);
            let duration = next.saturating_sub(point.time_ms.max(start));
            let fraction = notional(point.mid, point.base_atoms).unwrap_or(0) as f64
                / (self.scenario.strategy.inventory_limit_usdt as f64 * S as f64);
            area += fraction * duration as f64;
        }
        area / end.saturating_sub(start).max(1) as f64
    }
    pub fn capital_atoms(&self) -> i128 {
        self.capital
    }
    pub fn maximum_drawdown(&self) -> i128 {
        self.max_drawdown
    }
    pub fn fee_asset_totals(&self) -> &BTreeMap<String, i128> {
        &self.fee_assets
    }
    pub fn finish(&self, split: &str, start: u64, end: u64) -> Result<Summary> {
        self.reconcile()?;
        let final_quality = self.data_ok
            && self.view.is_some()
            && self
                .last_book_ms
                .is_some_and(|t| end.saturating_sub(t) <= self.scenario.strategy.stale_ms);
        let mid = self.last_mid.unwrap_or(self.instrument.price_units);
        let equity = self.marked_equity(mid)?;
        let pnl = equity - self.capital;
        let bid = self.view.as_ref().map_or(mid, View::bid);
        let stress_bid = (u128::from(bid)
            * u128::from(
                10_000
                    - self.scenario.adverse_terminal_shock_bps
                    - self.scenario.closeout_slippage_bps.min(1000),
            )
            / 10_000) as u64;
        let position = u64::try_from(self.base)?;
        let markdown =
            notional(mid, position)?.saturating_sub(notional(stress_bid, position)?) as i128;
        let taker = if position > 0 {
            self.scenario
                .fees
                .at(self.clock, &self.instrument.symbol)?
                .charge(stress_bid.max(1), position, false, false)?
                .quote_equivalent_atoms as i128
        } else {
            0
        };
        let closeout = markdown + taker;
        let inventory = pnl + self.fees - self.rebates - self.gross;
        let mut markouts = vec![];
        for seconds in [1u64, 5, 30, 60] {
            let mut sum_bps = 0.0;
            let mut adverse = 0i128;
            let mut completed = 0;
            for fill in &self.fills {
                let target = fill.time_ms + seconds * 1000;
                let future = self.reference_marks.iter().find(|(time, _, epoch)| {
                    *time >= target && *time <= target + 1500 && *epoch == fill.epoch
                });
                if let Some((_, future_mid, _)) = future {
                    let sign = if fill.buy { 1i128 } else { -1 };
                    sum_bps += (i128::from(*future_mid) - i128::from(fill.price)) as f64
                        * sign as f64
                        / fill.price as f64
                        * 10_000.;
                    adverse += (i128::from(fill.reference_mid) - i128::from(*future_mid))
                        * sign
                        * i128::from(fill.quantity)
                        / i128::from(S);
                    completed += 1;
                }
            }
            markouts.push(Markout {
                seconds,
                completed,
                mean_bps: if completed > 0 {
                    Some(sum_bps / completed as f64)
                } else {
                    None
                },
                adverse_quote_atoms: if completed > 0 { Some(adverse) } else { None },
            });
        }
        let mut daily = BTreeMap::new();
        let mut daily_dd: BTreeMap<String, i128> = BTreeMap::new();
        let mut previous = self.capital;
        let mut peaks: BTreeMap<String, i128> = BTreeMap::new();
        for point in &self.equity {
            let date = chrono::DateTime::from_timestamp_millis(point.time_ms as i64)
                .context("invalid timestamp")?
                .with_timezone(&chrono::FixedOffset::east_opt(3 * 3600).unwrap())
                .format("%Y-%m-%d")
                .to_string();
            *daily.entry(date.clone()).or_default() += point.equity_atoms - previous;
            let peak = peaks.entry(date.clone()).or_insert(previous);
            *peak = (*peak).max(point.equity_atoms);
            let dd = daily_dd.entry(date).or_default();
            *dd = (*dd).max(*peak - point.equity_atoms);
            previous = point.equity_atoms;
        }
        if let Some(day) = daily.keys().last().cloned() {
            *daily.get_mut(&day).unwrap() += equity - previous;
        }
        let filled_orders = self
            .fills
            .iter()
            .map(|f| f.order_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        Ok(Summary {symbol:self.instrument.symbol.clone(),scenario:self.scenario.name.clone(),split:split.into(),result_type:"simulated".into(),hypothetical:self.scenario.hypothetical,account_access_verified:self.scenario.account_access_verified,start_ms:start,end_ms:end,gross_spread_capture_atoms:self.gross,maker_fees_atoms:self.fees,rebate_atoms:self.rebates,closeout_spread_cost_atoms:notional(mid,position)?.saturating_sub(notional(bid,position)?) as i128,slippage_stress_cost_atoms:notional(bid,position)?.saturating_sub(notional(stress_bid,position)?) as i128,estimated_taker_fee_atoms:taker,inventory_pnl_atoms:inventory,net_pnl_atoms:pnl,estimated_closeout_cost_atoms:closeout,closeout_adjusted_pnl_atoms:pnl-closeout,pnl_per_million_usdt:if self.turnover>0{Some(pnl as f64/self.turnover as f64*1_000_000.)}else{None},executed_notional_atoms:self.turnover,turnover:self.turnover as f64/self.capital as f64,max_drawdown_atoms:self.max_drawdown,fill_count:self.fills.len(),submitted_orders:self.submitted,accepted_orders:self.accepted,fill_ratio:if self.accepted>0{Some(filled_orders as f64/self.accepted as f64)}else{None},filled_quantity_ratio:if self.submitted_quantity>0{Some(self.filled_quantity as f64/self.submitted_quantity as f64)}else{None},cancellation_ratio:if self.submitted>0{Some(self.cancelled as f64/self.submitted as f64)}else{None},rejected_orders:self.rejected,cancelled_orders:self.cancelled,open_orders_at_end:self.orders.len(),exposure_utilization:self.exposure(start,end),max_inventory_notional_atoms:self.max_inventory,markouts,data_quality_ok:final_quality,coverage_interruptions:self.interruptions,drawdown_stopped:self.stopped,net_positive_after_costs_and_without_favorable_inventory:final_quality && self.orders.is_empty() && pnl-closeout>0 && pnl-inventory.max(0)-closeout>0 && self.turnover>0,no_trade_pnl_atoms:0,no_trade_drawdown_atoms:0,daily_pnl_atoms:daily,daily_max_drawdown_atoms:daily_dd,fee_assets:self.fee_assets.clone(),warnings:vec!["Queue position and fills are model assumptions, never observed own executions. Exact-price trade evidence only; no fill from a touch or depth cancellation.".into(),"Unknown data intervals invalidate profitability conclusions; outstanding orders are not assumed reconciled at dataset end.".into(),"Markout costs are attribution diagnostics already reflected in inventory P&L; do not subtract them again.".into()]})
    }
}

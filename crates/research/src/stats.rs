use anyhow::{Result, ensure};
use mm_market_data::{book::Book, model::Trade, runtime::Observer};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Mutex};
const SCALE: f64 = 100_000_000.0;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sample {
    pub received_ms: u64,
    pub exchange_ms: u64,
    pub last_id: u64,
    pub bid: u64,
    pub ask: u64,
    pub spread_bps: f64,
    pub depth_usdt: [[f64; 2]; 4],
    pub imbalance: f64,
    pub concentration: f64,
}
pub fn bps(change: i128, reference: u128) -> Result<f64> {
    ensure!(reference > 0, "zero reference");
    Ok(change as f64 / reference as f64 * 10_000.0)
}
pub fn sample(book: &Book, received_ms: u64, exchange_ms: u64) -> Result<Sample> {
    ensure!(
        book.synced && received_ms.abs_diff(exchange_ms) <= 5000,
        "unsynchronized/stale book"
    );
    book.validate(10_000)?;
    let bid = *book.bids.last_key_value().unwrap().0;
    let ask = *book.asks.first_key_value().unwrap().0;
    let sum = u128::from(bid) + u128::from(ask);
    let mut depth = [[0.0; 2]; 4];
    let mut largest = 0.0_f64;
    for (j, band) in [5u128, 10, 25, 50].into_iter().enumerate() {
        for (side, map) in [&book.bids, &book.asks].into_iter().enumerate() {
            let mut atoms = 0u128;
            for (&p, &q) in map {
                let distance = (2 * u128::from(p)).abs_diff(sum);
                if distance * 10_000 <= sum * band {
                    let n = crate::fees::notional(p, q)?;
                    atoms = atoms
                        .checked_add(n)
                        .ok_or_else(|| anyhow::anyhow!("depth overflow"))?;
                    if j == 1 {
                        largest = largest.max(n as f64 / SCALE);
                    }
                }
            }
            depth[j][side] = atoms as f64 / SCALE;
        }
    }
    let total = depth[1][0] + depth[1][1];
    ensure!(total > 0.0, "no near-mid liquidity");
    Ok(Sample {
        received_ms,
        exchange_ms,
        last_id: book.last_id.unwrap(),
        bid,
        ask,
        spread_bps: (ask - bid) as f64 / (sum as f64 / 2.0) * 10_000.0,
        depth_usdt: depth,
        imbalance: (depth[1][0] - depth[1][1]) / total,
        concentration: largest / total,
    })
}
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TradeBin {
    pub second: u64,
    pub aggregate_messages: u64,
    pub underlying_trades: u64,
    pub quote_atoms: u128,
    pub buy_quantity: u128,
    pub sell_quantity: u128,
    pub buy_quote_atoms: u128,
    pub sell_quote_atoms: u128,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Series {
    pub start_ms: u64,
    pub end_ms: u64,
    pub samples: Vec<Sample>,
    pub trades: BTreeMap<u64, TradeBin>,
    pub last_aggregate_id: Option<u64>,
    pub duplicate_trades: u64,
    pub aggregate_gaps: u64,
    pub rejected_samples: u64,
    pub capacity_exceeded: bool,
}
impl Series {
    pub fn observe_book(&mut self, book: &Book, ms: u64, exchange_ms: u64) {
        if ms < self.start_ms || ms > self.end_ms {
            return;
        }
        if self
            .samples
            .last()
            .is_some_and(|s| s.received_ms / 1000 == ms / 1000)
        {
            return;
        }
        if self.samples.len() >= 7200 {
            self.capacity_exceeded = true;
            return;
        }
        match sample(book, ms, exchange_ms) {
            Ok(s) => self.samples.push(s),
            Err(_) => self.rejected_samples += 1,
        }
    }
    pub fn observe_trade(&mut self, t: &Trade, ms: u64) {
        if ms < self.start_ms || ms > self.end_ms || ms.abs_diff(t.exchange_ms) > 5000 {
            return;
        }
        if self
            .last_aggregate_id
            .is_some_and(|id| t.aggregate_id <= id)
        {
            self.duplicate_trades += 1;
            return;
        }
        if self
            .last_aggregate_id
            .is_some_and(|id| t.aggregate_id > id.saturating_add(1))
        {
            self.aggregate_gaps += 1;
        }
        self.last_aggregate_id = Some(t.aggregate_id);
        if self.trades.len() >= 7200 && !self.trades.contains_key(&(t.exchange_ms / 1000)) {
            self.capacity_exceeded = true;
            return;
        }
        let Some(count) = t
            .last_id
            .checked_sub(t.first_id)
            .and_then(|v| v.checked_add(1))
        else {
            self.capacity_exceeded = true;
            return;
        };
        let Ok(quote) = crate::fees::notional(t.price, t.quantity) else {
            self.capacity_exceeded = true;
            return;
        };
        let bin = self
            .trades
            .entry(t.exchange_ms / 1000)
            .or_insert_with(|| TradeBin {
                second: t.exchange_ms / 1000,
                ..TradeBin::default()
            });
        let add = |a: &mut u128, b: u128| -> bool {
            if let Some(v) = a.checked_add(b) {
                *a = v;
                true
            } else {
                false
            }
        };
        if !add(&mut bin.quote_atoms, quote) {
            self.capacity_exceeded = true;
            return;
        }
        let (quantity, notional) = if t.buyer_is_maker {
            (&mut bin.sell_quantity, &mut bin.sell_quote_atoms)
        } else {
            (&mut bin.buy_quantity, &mut bin.buy_quote_atoms)
        };
        if !add(quantity, t.quantity.into()) || !add(notional, quote) {
            self.capacity_exceeded = true;
            return;
        }
        bin.aggregate_messages += 1;
        bin.underlying_trades = bin.underlying_trades.saturating_add(count);
    }
}
#[derive(Default)]
pub struct ResearchObserver {
    pub series: Mutex<BTreeMap<String, Series>>,
}
impl Observer for ResearchObserver {
    fn book(&self, symbol: &str, book: &Book, ms: u64, exchange_ms: u64) {
        if let Some(s) = self.series.lock().unwrap().get_mut(symbol) {
            s.observe_book(book, ms, exchange_ms);
        }
    }
    fn trade(&self, trade: &Trade, ms: u64) {
        if let Some(s) = self.series.lock().unwrap().get_mut(&trade.symbol) {
            s.observe_trade(trade, ms);
        }
    }
}
pub fn quantile(values: &[f64], q: f64) -> Option<f64> {
    let mut sorted: Vec<_> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(f64::total_cmp);
    Some(sorted[((sorted.len() - 1) as f64 * q.clamp(0.0, 1.0)).round() as usize])
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Horizon {
    pub seconds: u64,
    pub observed_duration_seconds: f64,
    pub window_start_ms: u64,
    pub window_end_ms: u64,
    pub requested_window_complete: bool,
    pub spread_p10_bps: f64,
    pub spread_p50_bps: f64,
    pub spread_p90_bps: f64,
    pub samples: usize,
    pub depth_median_usdt: [[f64; 2]; 4],
    pub median_imbalance: f64,
    pub concentration_p90: f64,
    pub quote_stability_fraction: f64,
    pub realized_volatility_bps: Option<f64>,
    pub reference_move_bps: Option<f64>,
    pub trade_quote_volume_usdt: f64,
    pub underlying_trade_count: u64,
    pub trade_frequency_per_second: f64,
    pub aggregate_trade_messages: u64,
    pub usable_fraction: f64,
}
pub fn horizon(series: &Series, seconds: u64) -> Option<Horizon> {
    let start = series
        .end_ms
        .saturating_sub(seconds * 1000)
        .max(series.start_ms);
    let samples: Vec<_> = series
        .samples
        .iter()
        .filter(|s| s.received_ms >= start && s.received_ms <= series.end_ms)
        .collect();
    if samples.len() < 2 {
        return None;
    }
    let spreads: Vec<_> = samples.iter().map(|s| s.spread_bps).collect();
    let mut depth = [[0.0; 2]; 4];
    for (j, band) in depth.iter_mut().enumerate() {
        for (side, value) in band.iter_mut().enumerate() {
            *value = quantile(
                &samples
                    .iter()
                    .map(|s| s.depth_usdt[j][side])
                    .collect::<Vec<_>>(),
                0.5,
            )?;
        }
    }
    let mut returns = vec![];
    let mut unchanged = 0;
    let mut adjacent = 0;
    for pair in samples.windows(2) {
        let delta = pair[1].received_ms.saturating_sub(pair[0].received_ms);
        if (500..=1500).contains(&delta) {
            adjacent += 1;
            if pair[0].bid == pair[1].bid && pair[0].ask == pair[1].ask {
                unchanged += 1;
            }
            let before = u128::from(pair[0].bid) + u128::from(pair[0].ask);
            let after = u128::from(pair[1].bid) + u128::from(pair[1].ask);
            returns.push(bps(after as i128 - before as i128, before).ok()?);
        }
    }
    let volatility = if returns.len() >= 2 {
        let center = quantile(&returns, 0.5)?;
        let mad = quantile(
            &returns
                .iter()
                .map(|v| (v - center).abs())
                .collect::<Vec<_>>(),
            0.5,
        )?;
        // Winsorize only analytics; raw data are retained. A nonzero floor avoids suppressing rare moves when MAD=0.
        let bound = (mad * 6.0).max(1.0);
        let clipped: Vec<_> = returns
            .iter()
            .map(|v| v.clamp(center - bound, center + bound))
            .collect();
        Some((clipped.iter().map(|v| v * v).sum::<f64>() / clipped.len() as f64).sqrt())
    } else {
        None
    };
    let first = *samples.first()?;
    let last = *samples.last()?;
    let first_mid = u128::from(first.bid) + u128::from(first.ask);
    let last_mid = u128::from(last.bid) + u128::from(last.ask);
    let trades: Vec<_> = series
        .trades
        .values()
        .filter(|t| t.second * 1000 >= start && t.second * 1000 <= series.end_ms)
        .collect();
    let quote = trades.iter().map(|t| t.quote_atoms as f64 / SCALE).sum();
    let count = trades.iter().map(|t| t.underlying_trades).sum();
    let elapsed = (series.end_ms - start) as f64 / 1000.0;
    Some(Horizon {
        seconds,
        observed_duration_seconds: elapsed,
        window_start_ms: start,
        window_end_ms: series.end_ms,
        requested_window_complete: series.end_ms.saturating_sub(series.start_ms) >= seconds * 1000,
        samples: samples.len(),
        spread_p10_bps: quantile(&spreads, 0.1)?,
        spread_p50_bps: quantile(&spreads, 0.5)?,
        spread_p90_bps: quantile(&spreads, 0.9)?,
        depth_median_usdt: depth,
        median_imbalance: quantile(
            &samples.iter().map(|s| s.imbalance).collect::<Vec<_>>(),
            0.5,
        )?,
        concentration_p90: quantile(
            &samples.iter().map(|s| s.concentration).collect::<Vec<_>>(),
            0.9,
        )?,
        quote_stability_fraction: if adjacent == 0 {
            0.0
        } else {
            unchanged as f64 / adjacent as f64
        },
        realized_volatility_bps: volatility,
        reference_move_bps: bps(last_mid as i128 - first_mid as i128, first_mid).ok(),
        trade_quote_volume_usdt: quote,
        underlying_trade_count: count,
        trade_frequency_per_second: count as f64 / elapsed.max(1.0),
        aggregate_trade_messages: trades.iter().map(|t| t.aggregate_messages).sum(),
        usable_fraction: (samples.len() as f64 / elapsed.max(1.0)).min(1.0),
    })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Markout {
    pub seconds: u64,
    pub bins: usize,
    pub mean_passive_adverse_bps: Option<f64>,
    pub p90_positive_bps: Option<f64>,
}
pub fn markout(series: &Series, seconds: u64) -> Markout {
    let mut values = vec![];
    for t in series.trades.values() {
        let target = t.second * 1000 + 500 + seconds * 1000;
        let Some(future) = series
            .samples
            .iter()
            .find(|s| s.exchange_ms >= target && s.exchange_ms <= target + 1500)
        else {
            continue;
        };
        // Never bridge a missing quote-history interval for a markout.
        let coverage = series
            .samples
            .iter()
            .filter(|s| s.exchange_ms >= t.second * 1000 && s.exchange_ms <= future.exchange_ms)
            .count();
        if coverage < seconds as usize {
            continue;
        }
        let mid = (future.bid as f64 + future.ask as f64) / 2.0 / SCALE;
        let cost = (t.buy_quantity as f64 / SCALE * mid - t.buy_quote_atoms as f64 / SCALE)
            + (t.sell_quote_atoms as f64 / SCALE - t.sell_quantity as f64 / SCALE * mid);
        let quote = t.quote_atoms as f64 / SCALE;
        if quote > 0.0 {
            values.push(cost / quote * 10_000.0);
        }
    }
    Markout {
        seconds,
        bins: values.len(),
        mean_passive_adverse_bps: if values.is_empty() {
            None
        } else {
            Some(values.iter().sum::<f64>() / values.len() as f64)
        },
        p90_positive_bps: quantile(&values.iter().map(|v| v.max(0.0)).collect::<Vec<_>>(), 0.9),
    }
}

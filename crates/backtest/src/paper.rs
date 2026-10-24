//! Forward public-data simulation. No private transport or exchange submission.
use crate::{
    config::{Experiment, Scenario},
    data::{Input, Tick, View},
    engine::{Engine, Fill, LedgerEntry},
    portfolio::{ExecutionModel, Initialization},
};
use anyhow::{Context, Result, ensure};
use mm_market_data::{
    book::Book,
    model::Trade,
    runtime::{Config, Observer, SharedStatus, now_ms, run_symbol},
};
use mm_research::discovery::{Instrument, Policy, eligible, public_json};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::mpsc;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Research,
    Economic,
}
impl Profile {
    fn name(self) -> &'static str {
        match self {
            Self::Research => "research",
            Self::Economic => "economic",
        }
    }
}
pub(crate) fn scenario(profile: Profile, spread: u64) -> Result<Scenario> {
    ensure!(spread <= 100, "spread must be 0..100bps");
    let experiment: Experiment =
        serde_json::from_str(include_str!("../../../config/backtest.json"))?;
    let mut s = experiment.scenarios[0].clone();
    s.name = format!("paper_{}", profile.name());
    s.strategy.spread_bps = spread;
    // Economic guard includes 3bps adverse selection, 2bps inventory and 2bps safety.
    s.strategy.required_edge_bps = 7;
    s.strategy.max_quote_spread_bps = if profile == Profile::Research {
        100
    } else {
        spread.max(1)
    };
    s.strategy.volatility_multiplier = 1;
    s.strategy.refresh_ms = 500;
    s.strategy.refresh_ticks = 2;
    s.validate()?;
    Ok(s)
}

struct Bridge {
    tx: mpsc::Sender<Tick>,
    overflow: Arc<AtomicBool>,
}
impl Bridge {
    fn send(&self, input: Input, time_ms: u64) {
        if self
            .tx
            .try_send(Tick {
                time_ms,
                sequence: 0,
                input,
            })
            .is_err()
        {
            self.overflow.store(true, Ordering::SeqCst);
        }
    }
}
impl Observer for Bridge {
    fn book(&self, _: &str, b: &Book, time: u64, exchange_ms: u64) {
        self.send(
            Input::Book {
                view: View {
                    bids: b.bids.iter().rev().map(|(p, q)| (*p, *q)).collect(),
                    asks: b.asks.iter().map(|(p, q)| (*p, *q)).collect(),
                },
                exchange_ms,
            },
            time,
        );
    }
    fn trade(&self, t: &Trade, time: u64) {
        self.send(Input::Trade { trade: t.clone() }, time);
    }
    fn invalidated(&self, _: &str, reason: &str, time: u64) {
        self.send(
            Input::Invalid {
                reason: reason.into(),
            },
            time,
        );
    }
}
#[derive(Serialize, Deserialize)]
struct Wallet {
    version: u64,
    profile: Profile,
    spread_bps: u64,
    engine: Engine,
    sequence: u64,
    books: u64,
    trades: u64,
    fill_count: u64,
    recent: VecDeque<Fill>,
    mark: u64,
    #[serde(default)]
    mark_time_ms: u64,
    status: String,
    last_trade: Option<u64>,
}
impl Wallet {
    fn new(
        profile: Profile,
        spread: u64,
        instrument: Instrument,
        mid: u64,
        time: u64,
    ) -> Result<Self> {
        let mut engine = Engine::funded(
            scenario(profile, spread)?,
            instrument,
            Initialization {
                base_allocation_ppm: 500_000,
                valuation_price_units: mid,
                valuation_time_ms: time,
                fee_buffer_ppm: 100_000,
            },
            ExecutionModel::ConservativeThrough,
        )?;
        engine.research_quotes = profile == Profile::Research;
        Ok(Self {
            version: 1,
            profile,
            spread_bps: spread,
            engine,
            sequence: 0,
            books: 0,
            trades: 0,
            fill_count: 0,
            recent: VecDeque::new(),
            mark: mid,
            mark_time_ms: time,
            status: "starting".into(),
            last_trade: None,
        })
    }
    fn step(&mut self, mut tick: Tick) -> Result<Value> {
        self.sequence += 1;
        tick.sequence = self.sequence;
        tick.time_ms = tick.time_ms.max(self.engine.clock);
        match &tick.input {
            Input::Book { .. } => self.books += 1,
            Input::Trade { .. } => self.trades += 1,
            Input::Invalid { .. } => {}
        }
        if let Input::Trade { trade } = &tick.input {
            if self
                .last_trade
                .is_some_and(|id| trade.aggregate_id > id.saturating_add(1))
            {
                self.engine.step(&Tick {
                    time_ms: tick.time_ms,
                    sequence: self.sequence,
                    input: Input::Invalid {
                        reason: "aggregate_trade_id_gap_unknown_execution".into(),
                    },
                })?;
            }
            self.last_trade = Some(
                self.last_trade
                    .map_or(trade.aggregate_id, |id| id.max(trade.aggregate_id)),
            );
        }
        self.engine.step(&tick)?;
        if matches!(&tick.input, Input::Book { .. })
            && let Some(view) = &self.engine.view
        {
            self.mark = view.mid();
            self.mark_time_ms = tick.time_ms;
        }
        self.fill_count += self.engine.fills.len() as u64;
        for f in &self.engine.fills {
            self.recent.push_back(f.clone());
            if self.recent.len() > 12 {
                self.recent.pop_front();
            }
        }
        self.status = if self.engine.stopped {
            "drawdown_stopped"
        } else if self.engine.view.is_none() {
            "paused_missing_or_invalid_data"
        } else if self.profile == Profile::Economic
            && self.engine.diagnostics.rejected_fee_constraints > 0
            && self.engine.orders.is_empty()
        {
            "refusing_negative_estimated_edge"
        } else {
            "running"
        }
        .into();
        let row = json!({"sequence":self.sequence,"time_ms":tick.time_ms,"input_reference": match &tick.input { Input::Book{exchange_ms,..}=>json!({"exchange_ms":exchange_ms}), Input::Trade{trade}=>json!({"trade":trade}), Input::Invalid{reason}=>json!({"reason":reason}) },"input_sha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&tick)?)),"input_kind":match tick.input {Input::Book{..}=>"book",Input::Trade{..}=>"trade",Input::Invalid{..}=>"invalid"},"actions":self.engine.logs,"fills":self.engine.fills,"decisions":self.engine.decisions,"ledger":self.engine.ledger.iter().filter(|r|r.reason!="reconciliation_checkpoint_not_deposit").collect::<Vec<_>>()});
        self.compact()?;
        Ok(row)
    }
    fn compact(&mut self) -> Result<()> {
        let e = &mut self.engine;
        e.logs.clear();
        e.decisions.clear();
        e.fills.clear();
        e.equity.clear();
        e.execution_evidence.clear();
        while e.diagnostics.dependency_failures.len() > 128 {
            let key = e
                .diagnostics
                .dependency_failures
                .keys()
                .find(|k| k.as_str() != "other_retained_failure_counts")
                .unwrap()
                .clone();
            let count = e.diagnostics.dependency_failures.remove(&key).unwrap();
            *e.diagnostics
                .dependency_failures
                .entry("other_retained_failure_counts".into())
                .or_default() += count;
        }
        e.ledger = [
            ("USDT".to_string(), e.cash),
            (e.instrument.base_asset.clone(), e.base),
            ("BNB".into(), e.bnb),
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
        e.portfolio_pnl(self.mark)?;
        Ok(())
    }
    fn pause(&mut self, reason: &str) -> Result<Value> {
        self.step(Tick {
            time_ms: now_ms().max(self.engine.clock),
            sequence: 0,
            input: Input::Invalid {
                reason: reason.into(),
            },
        })
    }
    fn status(&self) -> Result<Value> {
        let e = &self.engine;
        let pnl = e.portfolio_pnl(self.mark)?;
        let net = i128::from(pnl["net_pnl_atoms"].as_i64().context("P&L overflow")?);
        Ok(json!({
            "mode":"paper", "live_trading_enabled":false,
            "profile":self.profile, "research_only":self.profile==Profile::Research,
            "profitability_validated":false, "symbol":e.instrument.symbol,
            "time_ms":now_ms(), "status":self.status,
            "market_price_units":e.view.as_ref().map(View::mid),
            "last_mark_units":self.mark, "mark_time_ms":self.mark_time_ms, "valuation_status":if e.view.is_some(){"fresh"}else{"last_known"}, "data_quality_ok":e.data_ok, "scale":100_000_000,
            "orders":e.orders.iter().map(|o|json!({
                "id":o.id,"side":if o.buy {"buy"} else {"sell"},
                "price_units":o.price,"remaining_units":o.remaining,"active":o.active,
                "cancel_pending":o.cancel_due.is_some()
            })).collect::<Vec<_>>(),
            "recent_simulated_fills":self.recent,"fill_count":self.fill_count,
            "balances":e.balances(),"pnl":pnl,
            "total_equity_atoms":e.portfolio.initial_equity_atoms+net,
            "max_drawdown_atoms":e.maximum_drawdown(),"fee_assets":e.fee_asset_totals(),
            "fee_assumptions":e.scenario.fees,"initial_allocation":e.portfolio,
            "books":self.books,"trades":self.trades,"diagnostics":e.diagnostics,
            "sequence":self.sequence,"coverage_interruptions":e.interruptions
        }))
    }
    fn display(&self) -> Result<()> {
        let scale = 100_000_000.0;
        let e = &self.engine;
        let b = e.balances();
        let pnl = e.portfolio_pnl(self.mark)?;
        let amount = |key: &str| -> f64 { pnl[key].as_i64().unwrap_or(0) as f64 / scale };
        println!(
            "PAPER {} | {} | market {} | fills {} | books {} trades {}",
            self.profile.name(),
            self.status,
            e.view.as_ref().map_or("unavailable".into(), |v| format!(
                "{:.8}",
                v.mid() as f64 / scale
            )),
            self.fill_count,
            self.books,
            self.trades
        );
        for o in &e.orders {
            println!(
                "  {} #{}, {:.8} x {:.8} [{}{}]",
                if o.buy { "BID" } else { "ASK" },
                o.id,
                o.price as f64 / scale,
                o.remaining as f64 / scale,
                if o.active { "active" } else { "pending" },
                if o.cancel_due.is_some() {
                    ", cancel pending"
                } else {
                    ""
                }
            );
        }
        println!(
            "  USDT total {:.8} available {:.8} reserved {:.8} | BTC total {:.8} available {:.8} reserved {:.8}",
            b.quote_total as f64 / scale,
            b.quote_available as f64 / scale,
            b.quote_reserved as f64 / scale,
            b.base_total as f64 / scale,
            b.base_available as f64 / scale,
            b.base_reserved as f64 / scale
        );
        println!(
            "  equity {:.8} | net P&L {:.8} | realized gross {:.8} unrealized gross {:.8} | fees {:.8} | max drawdown {:.8} USDT",
            5000.0 + amount("net_pnl_atoms"),
            amount("net_pnl_atoms"),
            amount("realized_gross_atoms"),
            amount("unrealized_gross_atoms"),
            e.fees as f64 / scale,
            e.maximum_drawdown() as f64 / scale
        );
        for f in self.recent.iter().rev().take(2) {
            println!(
                "  recent SIMULATED {} {:.8} x {:.8} | trade {} | {}",
                if f.buy { "BUY" } else { "SELL" },
                f.price as f64 / scale,
                f.quantity as f64 / scale,
                f.aggregate_trade_id,
                f.evidence_kind
            );
        }
        Ok(())
    }
}
fn atomic(path: &Path, value: &impl Serialize) -> Result<()> {
    let temp = path.with_extension("tmp");
    let mut f = File::create(&temp)?;
    serde_json::to_writer(&mut f, value)?;
    f.sync_all()?;
    std::fs::rename(temp, path)?;
    File::open(path.parent().context("parent missing")?)?.sync_all()?;
    Ok(())
}
struct Store {
    root: PathBuf,
    _lock: File,
}
impl Store {
    fn open(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(root.join("paper.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock).context("paper wallet already running")?;
        Ok(Self { root, _lock: lock })
    }
    fn save(&self, w: &Wallet, row: &Value) -> Result<()> {
        // At most eight 4MiB journals plus one bounded record; checkpoint is authoritative.
        let path = self.root.join("events.jsonl");
        if path.metadata().is_ok_and(|m| m.len() > 4 * 1024 * 1024) {
            let last = self.root.join("events.7.jsonl");
            if last.exists() {
                std::fs::remove_file(last)?;
            }
            for n in (1..7).rev() {
                let old = self.root.join(format!("events.{n}.jsonl"));
                if old.exists() {
                    std::fs::rename(old, self.root.join(format!("events.{}.jsonl", n + 1)))?;
                }
            }
            std::fs::rename(&path, self.root.join("events.1.jsonl"))?;
        }
        let bytes = serde_json::to_vec(row)?;
        ensure!(bytes.len() < 1024 * 1024, "paper journal row exceeds bound");
        let mut f = OpenOptions::new().create(true).append(true).open(path)?;
        f.write_all(&bytes)?;
        f.write_all(b"\n")?;
        f.sync_all()?;
        atomic(&self.root.join("wallet.json"), w)?;
        atomic(&self.root.join("status.json"), &w.status()?)?;
        Ok(())
    }
}

pub async fn run(args: &[String]) -> Result<()> {
    let profile = match args.first().map(String::as_str).unwrap_or("research") {
        "research" => Profile::Research,
        "economic" => Profile::Economic,
        _ => anyhow::bail!("profile must be research or economic"),
    };
    let mut spread = if profile == Profile::Research { 0 } else { 30 };
    let mut seconds = 0;
    let mut root = PathBuf::from(format!(".local/paper/{}", profile.name()));
    let mut i = 1;
    while i < args.len() {
        let value = args.get(i + 1).context("option needs a value")?;
        match args[i].as_str() {
            "--spread-bps" => spread = value.parse()?,
            "--seconds" => seconds = value.parse()?,
            "--state-dir" => root = value.into(),
            _ => anyhow::bail!("unknown option"),
        };
        i += 2;
    }
    scenario(profile, spread)?;
    let store = Store::open(root)?;
    ensure!(
        fs2::available_space(&store.root)? > 512 * 1024 * 1024,
        "need 512MiB free disk headroom"
    );
    let client = crate::collection::reqwest_client()?;
    let (_, info) = public_json(
        &client,
        "https://data-api.binance.vision/api/v3/exchangeInfo?symbol=BTCUSDT",
    )
    .await?;
    let (_, ticker) = public_json(
        &client,
        "https://data-api.binance.vision/api/v3/ticker/24hr?symbol=BTCUSDT",
    )
    .await?;
    let instrument = eligible(&info["symbols"][0], &ticker, &Policy::default())?;
    ensure!(instrument.symbol == "BTCUSDT", "unexpected instrument");
    let price_filter = info["symbols"][0]["filters"]
        .as_array()
        .context("filters missing")?
        .iter()
        .find(|f| f["filterType"] == "PRICE_FILTER")
        .context("price filter missing")?;
    let price_bounds = (
        mm_market_data::model::units(
            price_filter["minPrice"]
                .as_str()
                .context("min price missing")?,
        )?,
        mm_market_data::model::units(
            price_filter["maxPrice"]
                .as_str()
                .context("max price missing")?,
        )?,
    );
    ensure!(
        price_bounds.0 > 0 && price_bounds.1 >= price_bounds.0,
        "invalid price bounds"
    );
    atomic(
        &store.root.join("metadata.json"),
        &json!({"fetched_ms":now_ms(),"exchange_info":info,"ticker":ticker}),
    )?;
    let checkpoint = store.root.join("wallet.json");
    let mut wallet: Option<Wallet> = if checkpoint.exists() {
        ensure!(
            checkpoint.metadata()?.len() < 4 * 1024 * 1024,
            "checkpoint exceeds bound"
        );
        let mut w: Wallet = serde_json::from_slice(&std::fs::read(checkpoint)?)?;
        ensure!(
            w.version == 1 && w.profile == profile && w.spread_bps == spread,
            "existing wallet configuration differs; use another --state-dir for an isolated experiment"
        );
        ensure!(
            w.engine.instrument.symbol == instrument.symbol
                && w.engine.research_quotes == (profile == Profile::Research),
            "invalid checkpoint policy"
        );
        w.engine.scenario.validate()?;
        ensure!(
            serde_json::to_value(&w.engine.scenario)?
                == serde_json::to_value(scenario(profile, spread)?)?,
            "checkpoint scenario differs from current configuration; preserve wallet and use the matching binary/configuration"
        );
        ensure!(
            w.engine.capital_atoms() == 5000 * i128::from(crate::engine::S)
                && w.engine.portfolio.initial_equity_atoms == w.engine.capital_atoms(),
            "invalid starting equity"
        );
        w.engine.reconcile()?;
        w.engine.instrument = instrument.clone();
        w.engine.price_bounds = Some(price_bounds);
        w.engine.reference_marks.clear();
        w.engine.data_ok = false;
        w.engine.interruptions += 1;
        let row = w.pause("restart_gap_virtual_orders_voided_no_offline_fills")?;
        store.save(&w, &row)?;
        Some(w)
    } else {
        None
    };
    let (tx, mut rx) = mpsc::channel(64);
    let overflow = Arc::new(AtomicBool::new(false));
    let bridge = Arc::new(Bridge {
        tx,
        overflow: overflow.clone(),
    });
    let config = Config {
        include_trades: true,
        data_dir: store.root.join("market-data"),
        observer: Some(bridge),
        recording: mm_market_data::recording::Limits {
            max_segments: 128,
            max_total_bytes: Some(256 * 1024 * 1024),
            ..Default::default()
        },
        ..Config::default()
    };
    let shared: SharedStatus = Arc::new(Mutex::new(BTreeMap::new()));
    let mut task = tokio::spawn(run_symbol(config, "BTCUSDT", shared));
    println!(
        "PAPER {} | BTCUSDT | LIVE TRADING DISABLED | simulated $5,000 isolated wallet | manual maker fee 10bps/side | research fills never prove profitability",
        profile.name().to_uppercase()
    );
    let start = std::time::Instant::now();
    let mut timer = tokio::time::interval(Duration::from_secs(1));
    let mut collector_finished = false;
    let result: Result<()> = async {
        loop {
            tokio::select! {
                signal = tokio::signal::ctrl_c() => { signal?; break; }
                result = &mut task => {
                    collector_finished = true;
                    result??;
                    anyhow::bail!("public collector stopped");
                }
                _ = timer.tick() => {
                    if seconds > 0 && start.elapsed().as_secs() >= seconds { break; }
                    if let Some(w) = &mut wallet {
                        if w.engine.last_book_ms.is_some_and(|t|
                            now_ms().saturating_sub(t) > w.engine.scenario.strategy.stale_ms) {
                            let row = w.pause("stale_public_feed")?;
                            store.save(w, &row)?;
                        }
                        atomic(&store.root.join("status.json"), &w.status()?)?;
                        w.display()?;
                    } else {
                        println!("PAPER: waiting for synchronized public book; no portfolio or quotes yet");
                    }
                }
                Some(tick) = rx.recv() => {
                    if overflow.swap(false, Ordering::SeqCst)
                        || now_ms().saturating_sub(tick.time_ms) > 2000 {
                        while rx.try_recv().is_ok() {}
                        if let Some(w) = &mut wallet {
                            let row = w.pause("paper_queue_overflow_or_processing_lag_unknown_coverage")?;
                            store.save(w, &row)?;
                        }
                        continue;
                    }
                    if wallet.is_none()
                        && let Input::Book { view, exchange_ms } = &tick.input
                        && view.valid()
                        && tick.time_ms.saturating_sub(*exchange_ms) <= 2000 {
                        wallet = Some(Wallet::new(profile, spread, instrument.clone(), view.mid(), tick.time_ms)?);
                    }
                    if let Some(w) = &mut wallet {
                        w.engine.price_bounds=Some(price_bounds);
                        let row = w.step(tick)?;
                        store.save(w, &row)?;
                    }
                }
            }
        }
        ensure!(wallet.is_some(), "no synchronized book received; paper wallet was not initialized");
        Ok(())
    }.await;
    task.abort();
    if !collector_finished {
        let _ = task.await;
    }
    if let Some(w) = &mut wallet {
        let row = w.pause("paper_stopped_virtual_orders_voided")?;
        w.status = if result.is_ok() {
            "stopped"
        } else {
            "stopped_error"
        }
        .into();
        store.save(w, &row)?;
        println!("FINAL {}", serde_json::to_string(&w.status()?)?);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::S;
    fn instrument() -> Instrument {
        Instrument {
            symbol: "TESTUSDT".into(),
            base_asset: "TEST".into(),
            price_units: 100 * S,
            tick_units: S / 100,
            step_units: S / 1000,
            min_qty_units: S / 1000,
            max_qty_units: 1000 * S,
            min_notional_units: 5 * S,
            max_notional_units: None,
            clip_quantity_units: S,
            clip_notional_units: (100 * S).into(),
            volume_24h_usdt: 1e8,
            trades_24h: 100_000,
        }
    }
    fn book(time: u64) -> Tick {
        Tick {
            time_ms: time,
            sequence: 0,
            input: Input::Book {
                view: View {
                    bids: vec![(100 * S, 2 * S), (99 * S, 10 * S)],
                    asks: vec![(100 * S + S / 50, 2 * S), (101 * S, 10 * S)],
                },
                exchange_ms: time,
            },
        }
    }
    fn wallet(profile: Profile, spread: u64) -> Wallet {
        Wallet::new(profile, spread, instrument(), 100 * S + S / 100, 1000).unwrap()
    }
    fn trade(time: u64, id: u64, buy: bool, price: u64) -> Tick {
        Tick {
            time_ms: time,
            sequence: 0,
            input: Input::Trade {
                trade: Trade {
                    symbol: "TESTUSDT".into(),
                    aggregate_id: id,
                    first_id: id,
                    last_id: id,
                    exchange_ms: time,
                    price,
                    quantity: 4 * S,
                    buyer_is_maker: buy,
                },
            },
        }
    }
    #[test]
    fn research_two_sided_fills_charge_fees_and_reconcile() {
        let mut w = wallet(Profile::Research, 0);
        w.step(book(1000)).unwrap();
        assert_eq!(w.engine.orders.len(), 2);
        let bid = w.engine.orders.iter().find(|o| o.buy).unwrap().price;
        let ask = w.engine.orders.iter().find(|o| !o.buy).unwrap().price;
        assert!(bid < ask);
        w.step(book(1100)).unwrap();
        w.step(trade(1150, 1, true, bid)).unwrap();
        w.step(trade(1200, 2, false, ask)).unwrap();
        assert_eq!(w.fill_count, 2);
        assert!(w.engine.fees > 0);
        assert!(
            w.engine.balances().base_available >= 0 && w.engine.balances().quote_available >= 0
        );
        assert!(
            w.engine.portfolio_pnl(w.mark).unwrap()["net_pnl_atoms"]
                .as_i64()
                .unwrap()
                < 0
        );
        assert_eq!(w.status().unwrap()["profitability_validated"], false);
    }
    #[test]
    fn economic_negative_edge_refuses_and_positive_allows() {
        let mut w = wallet(Profile::Economic, 1);
        w.step(book(1000)).unwrap();
        assert!(w.engine.orders.is_empty());
        assert_eq!(w.engine.diagnostics.rejected_fee_constraints, 2);
        let mut positive = wallet(Profile::Economic, 30);
        positive.step(book(1000)).unwrap();
        assert_eq!(positive.engine.orders.len(), 2);
    }
    #[test]
    fn deterministic_forward_events_and_checkpoint_restart_preserve_wallet() {
        let run = || {
            let mut w = wallet(Profile::Research, 0);
            for t in [book(1000), book(1100), trade(1200, 1, true, 100 * S)] {
                w.step(t).unwrap();
            }
            w
        };
        let first = run();
        let second = run();
        assert_eq!(
            serde_json::to_value(&first).unwrap(),
            serde_json::to_value(&second).unwrap()
        );
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        store.save(&first, &json!({"test":"synthetic"})).unwrap();
        assert!(Store::open(dir.path().into()).is_err());
        let mut restored: Wallet =
            serde_json::from_slice(&std::fs::read(dir.path().join("wallet.json")).unwrap())
                .unwrap();
        let balances = (
            restored.engine.cash,
            restored.engine.base,
            restored.engine.fees,
            restored.fill_count,
        );
        restored.pause("restart_test").unwrap();
        assert!(restored.engine.orders.is_empty());
        assert_eq!(
            balances,
            (
                restored.engine.cash,
                restored.engine.base,
                restored.engine.fees,
                restored.fill_count
            )
        );
        restored.engine.reconcile().unwrap();
    }
    #[test]
    fn gaps_and_stale_books_cannot_fill_and_recover_only_on_book() {
        let mut w = wallet(Profile::Research, 0);
        w.step(book(1000)).unwrap();
        w.step(book(1100)).unwrap();
        w.step(trade(1150, 1, false, 110 * S)).unwrap();
        w.step(trade(1200, 3, true, 100 * S)).unwrap();
        assert_eq!(w.fill_count, 0);
        assert!(w.engine.orders.is_empty());
        assert!(w.engine.view.is_none());
        w.step(book(1300)).unwrap();
        w.step(book(1600)).unwrap();
        assert!(!w.engine.orders.is_empty());
        w.step(trade(4000, 4, true, 100 * S)).unwrap();
        assert_eq!(w.fill_count, 0);
        assert!(w.engine.orders.is_empty());
    }
    #[test]
    fn bridge_is_bounded_and_propagates_invalidation() {
        let (tx, mut rx) = mpsc::channel(1);
        let overflow = Arc::new(AtomicBool::new(false));
        let b = Bridge {
            tx,
            overflow: overflow.clone(),
        };
        b.invalidated("TESTUSDT", "test_gap", 1000);
        assert!(matches!(
            rx.try_recv().unwrap().input,
            Input::Invalid { .. }
        ));
        b.invalidated("TESTUSDT", "test_gap", 1000);
        b.invalidated("TESTUSDT", "test_gap", 1001);
        assert!(overflow.load(Ordering::SeqCst));
    }
    #[test]
    fn invalid_book_does_not_revalue_and_pre_activation_trade_cannot_fill() {
        let mut w = wallet(Profile::Research, 0);
        w.step(book(1000)).unwrap();
        w.step(trade(1001, 1, true, 100 * S)).unwrap();
        assert_eq!(w.fill_count, 0);
        let mark = w.mark;
        w.step(Tick {
            time_ms: 1100,
            sequence: 0,
            input: Input::Book {
                view: View {
                    bids: vec![],
                    asks: vec![],
                },
                exchange_ms: 1100,
            },
        })
        .unwrap();
        assert_eq!(w.mark, mark);
        assert!(w.engine.orders.is_empty());
    }
    #[test]
    fn economic_through_fills_keep_actual_fee_accounting() {
        let mut w = wallet(Profile::Economic, 30);
        w.step(book(1000)).unwrap();
        w.step(book(1100)).unwrap();
        let bid = w.engine.orders.iter().find(|o| o.buy).unwrap().price;
        w.step(trade(1200, 1, true, bid - S / 100)).unwrap();
        assert_eq!(w.fill_count, 1);
        assert!(w.recent[0].evidence_kind.contains("through"));
        assert!(w.engine.fees > 0);
        w.engine.reconcile().unwrap();
    }
    #[test]
    fn wide_research_quotes_refresh_and_drawdown_stop_survives_restore() {
        let mut narrow = wallet(Profile::Research, 0);
        narrow.step(book(1000)).unwrap();
        let mut wide = wallet(Profile::Research, 10);
        wide.step(book(1000)).unwrap();
        assert!(wide.engine.orders[0].price < narrow.engine.orders[0].price);
        narrow.step(book(1100)).unwrap();
        let mut moved = book(1700);
        if let Input::Book { view, .. } = &mut moved.input {
            for (p, _) in view.bids.iter_mut().chain(view.asks.iter_mut()) {
                *p += S / 100;
            }
        }
        narrow.engine.scenario.strategy.refresh_ticks = 1;
        narrow.step(moved).unwrap();
        assert!(narrow.engine.orders.iter().any(|o| o.cancel_due.is_some()));
        narrow.engine.stopped = true;
        let mut restored: Wallet =
            serde_json::from_value(serde_json::to_value(narrow).unwrap()).unwrap();
        restored.pause("restart").unwrap();
        assert!(restored.engine.stopped);
        assert!(restored.engine.orders.is_empty());
    }
    #[test]
    fn journal_rotation_and_storage_failure_are_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let w = wallet(Profile::Research, 0);
        let file = File::create(dir.path().join("events.jsonl")).unwrap();
        file.set_len(4 * 1024 * 1024 + 1).unwrap();
        store.save(&w, &json!({"synthetic":true})).unwrap();
        assert!(dir.path().join("events.1.jsonl").exists());
        std::fs::remove_file(dir.path().join("wallet.json")).unwrap();
        std::fs::create_dir(dir.path().join("wallet.json")).unwrap();
        assert!(store.save(&w, &json!({"synthetic":true})).is_err());
    }
}

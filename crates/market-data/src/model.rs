use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

pub const SCALE: u64 = 100_000_000;
pub const SYMBOLS: [&str; 2] = ["BTCUSDT", "ETHUSDT"];

/// Binance decimals are parsed exactly into 1e-8 units, never through f64.
pub fn units(value: &str) -> Result<u64> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    ensure!(
        !whole.is_empty() && whole.bytes().all(|c| c.is_ascii_digit()),
        "invalid decimal"
    );
    ensure!(
        fraction.len() <= 8 && fraction.bytes().all(|c| c.is_ascii_digit()),
        "unsupported decimal precision"
    );
    let whole: u64 = whole.parse()?;
    let fraction: u64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse()?
    };
    whole
        .checked_mul(SCALE)
        .and_then(|v| {
            v.checked_add(
                fraction * 10u64.pow(8 - value.split_once('.').map_or(0, |(_, f)| f.len()) as u32),
            )
        })
        .ok_or_else(|| anyhow::anyhow!("decimal overflow"))
}
pub fn decimal(value: u64) -> String {
    format!("{}.{:08}", value / SCALE, value % SCALE)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Level {
    pub price: u64,
    pub quantity: u64,
}
fn levels(values: Vec<[String; 2]>, allow_zero: bool) -> Result<Vec<Level>> {
    values
        .into_iter()
        .map(|[p, q]| {
            let level = Level {
                price: units(&p)?,
                quantity: units(&q)?,
            };
            ensure!(
                level.price > 0 && (allow_zero || level.quantity > 0),
                "invalid level"
            );
            Ok(level)
        })
        .collect()
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    pub last_id: u64,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}
#[derive(Debug, Deserialize)]
struct WireSnapshot {
    #[serde(rename = "lastUpdateId")]
    last_id: u64,
    bids: Vec<[String; 2]>,
    asks: Vec<[String; 2]>,
}
impl Snapshot {
    pub fn parse(raw: &str) -> Result<Self> {
        let wire: WireSnapshot = serde_json::from_str(raw)?;
        Ok(Self {
            last_id: wire.last_id,
            bids: levels(wire.bids, false)?,
            asks: levels(wire.asks, false)?,
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Depth {
    pub symbol: String,
    pub first_id: u64,
    pub last_id: u64,
    pub exchange_ms: u64,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}
#[derive(Debug, Deserialize)]
struct WireDepth {
    e: String,
    #[serde(rename = "E")]
    time: u64,
    s: String,
    #[serde(rename = "U")]
    first: u64,
    u: u64,
    b: Vec<[String; 2]>,
    a: Vec<[String; 2]>,
}
impl Depth {
    pub fn parse(raw: &str, expected: &str) -> Result<Self> {
        let wire: WireDepth = serde_json::from_str(raw)?;
        ensure!(
            wire.e == "depthUpdate" && wire.s == expected,
            "unexpected event/symbol"
        );
        if wire.first > wire.u {
            bail!("reversed update interval");
        }
        Ok(Self {
            symbol: wire.s,
            first_id: wire.first,
            last_id: wire.u,
            exchange_ms: wire.time,
            bids: levels(wire.b, true)?,
            asks: levels(wire.a, true)?,
        })
    }
}

/// Safe filesystem/stream identifier for a public Binance Spot symbol.
pub fn valid_symbol(symbol: &str) -> bool {
    symbol.len() >= 5
        && symbol.len() <= 30
        && symbol.ends_with("USDT")
        && symbol
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Trade {
    pub symbol: String,
    pub aggregate_id: u64,
    pub first_id: u64,
    pub last_id: u64,
    pub exchange_ms: u64,
    pub price: u64,
    pub quantity: u64,
    pub buyer_is_maker: bool,
}
impl Trade {
    pub fn parse(raw: &str, expected: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct Wire {
            s: String,
            a: u64,
            f: u64,
            l: u64,
            #[serde(rename = "T")]
            time: u64,
            p: String,
            q: String,
            m: bool,
        }
        let w: Wire = serde_json::from_str(raw)?;
        let trade = Self {
            symbol: w.s,
            aggregate_id: w.a,
            first_id: w.f,
            last_id: w.l,
            exchange_ms: w.time,
            price: units(&w.p)?,
            quantity: units(&w.q)?,
            buyer_is_maker: w.m,
        };
        ensure!(
            trade.symbol == expected
                && trade.first_id <= trade.last_id
                && trade.price > 0
                && trade.quantity > 0,
            "invalid aggregate trade"
        );
        Ok(trade)
    }
}

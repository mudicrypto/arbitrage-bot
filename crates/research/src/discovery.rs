use anyhow::{Context, Result, ensure};
use mm_market_data::model::{units, valid_symbol};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub capital_usdt: u64,
    pub clip_usdt: u64,
    pub min_quote_volume_24h: u64,
    pub min_trades_24h: u64,
    pub max_tick_bps: f64,
    pub max_min_order_usdt: u64,
    pub max_level_concentration: f64,
    pub min_depth_10bps_usdt: f64,
    pub min_uptime: f64,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            capital_usdt: 5000,
            clip_usdt: 100,
            min_quote_volume_24h: 5_000_000,
            min_trades_24h: 20_000,
            max_tick_bps: 10.0,
            max_min_order_usdt: 50,
            max_level_concentration: 0.85,
            min_depth_10bps_usdt: 1000.0,
            min_uptime: 0.85,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instrument {
    pub symbol: String,
    pub base_asset: String,
    pub price_units: u64,
    pub tick_units: u64,
    pub step_units: u64,
    pub min_qty_units: u64,
    pub max_qty_units: u64,
    pub min_notional_units: u64,
    pub max_notional_units: Option<u64>,
    pub clip_quantity_units: u64,
    pub clip_notional_units: u128,
    pub volume_24h_usdt: f64,
    pub trades_24h: u64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Exclusion {
    pub symbol: String,
    pub reason: String,
}
pub fn eligible(symbol: &Value, ticker: &Value, policy: &Policy) -> Result<Instrument> {
    let text = |k: &str| {
        symbol
            .get(k)
            .and_then(Value::as_str)
            .context("symbol metadata missing")
    };
    let name = text("symbol")?;
    ensure!(
        valid_symbol(name)
            && text("quoteAsset")? == "USDT"
            && text("status")? == "TRADING"
            && symbol["isSpotTradingAllowed"].as_bool() == Some(true),
        "inactive/non-USDT/non-spot symbol"
    );
    let filters = symbol["filters"].as_array().context("filters missing")?;
    let filter = |name: &str| {
        filters
            .iter()
            .find(|f| f["filterType"].as_str() == Some(name))
            .context("required filter missing")
    };
    let decimal = |f: &Value, key: &str| units(f[key].as_str().context("filter value missing")?);
    let price = units(ticker["lastPrice"].as_str().context("last price missing")?)?;
    let tick = decimal(filter("PRICE_FILTER")?, "tickSize")?;
    let lot = filter("LOT_SIZE")?;
    let step = decimal(lot, "stepSize")?;
    let min_qty = decimal(lot, "minQty")?;
    let max_qty = decimal(lot, "maxQty")?;
    let nf = filter("NOTIONAL").or_else(|_| filter("MIN_NOTIONAL"))?;
    let min_notional = decimal(nf, "minNotional")?;
    let max_notional = nf
        .get("maxNotional")
        .map(|v| units(v.as_str().context("invalid maxNotional")?))
        .transpose()?
        .filter(|v| *v > 0);
    ensure!(price > 0 && tick > 0 && step > 0, "zero price/tick/step");
    let volume = ticker["quoteVolume"]
        .as_str()
        .context("quote volume missing")?
        .parse::<f64>()?;
    let count = ticker["count"].as_u64().context("trade count missing")?;
    ensure!(
        volume.is_finite()
            && volume >= policy.min_quote_volume_24h as f64
            && count >= policy.min_trades_24h,
        "insufficient 24h trading activity"
    );
    ensure!(
        tick as f64 / price as f64 * 10_000.0 <= policy.max_tick_bps,
        "tick size unsuitable"
    );
    let clip_atoms = u128::from(policy.clip_usdt) * 100_000_000;
    let raw = clip_atoms
        .checked_mul(100_000_000)
        .context("clip overflow")?
        / u128::from(price);
    let quantity = u64::try_from(raw / u128::from(step) * u128::from(step))?;
    let cost = crate::fees::notional(price, quantity)?;
    ensure!(
        quantity >= min_qty
            && quantity <= max_qty
            && cost >= min_notional.into()
            && max_notional.is_none_or(|max| cost <= max.into()),
        "lot/notional constraints reject research clip"
    );
    let min_lot = crate::fees::notional(price, min_qty)?;
    ensure!(
        u128::from(min_notional).max(min_lot)
            <= u128::from(policy.max_min_order_usdt) * 100_000_000
            && policy.clip_usdt * 2 <= policy.capital_usdt,
        "minimum order impractical for research capital"
    );
    Ok(Instrument {
        symbol: name.into(),
        base_asset: text("baseAsset")?.into(),
        price_units: price,
        tick_units: tick,
        step_units: step,
        min_qty_units: min_qty,
        max_qty_units: max_qty,
        min_notional_units: min_notional,
        max_notional_units: max_notional,
        clip_quantity_units: quantity,
        clip_notional_units: cost,
        volume_24h_usdt: volume,
        trades_24h: count,
    })
}
pub async fn public_json(client: &reqwest::Client, url: &str) -> Result<(String, Value)> {
    let response = client.get(url).send().await?;
    ensure!(
        response.status().is_success(),
        "public discovery HTTP {} (stop; no retry loop)",
        response.status()
    );
    let mut response = response;
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            data.len() + chunk.len() <= 32 * 1024 * 1024,
            "public response too large"
        );
        data.extend_from_slice(&chunk);
    }
    let raw = String::from_utf8(data)?;
    let value = serde_json::from_str(&raw)?;
    Ok((raw, value))
}
pub fn discover(
    info: &Value,
    tickers: &Value,
    policy: &Policy,
    limit: usize,
) -> Result<(Vec<Instrument>, Vec<Exclusion>)> {
    ensure!(
        (1..=20).contains(&limit),
        "universe must contain at most 20 pairs"
    );
    let tickers = tickers.as_array().context("ticker list missing")?;
    let by_symbol: std::collections::BTreeMap<_, _> = tickers
        .iter()
        .filter_map(|t| Some((t["symbol"].as_str()?, t)))
        .collect();
    let mut accepted = vec![];
    let mut excluded = vec![];
    for s in info["symbols"].as_array().context("symbol list missing")? {
        let Some(name) = s["symbol"].as_str().filter(|_| s["quoteAsset"] == "USDT") else {
            continue;
        };
        match by_symbol
            .get(name)
            .context("ticker missing")
            .and_then(|t| eligible(s, t, policy))
        {
            Ok(i) => accepted.push(i),
            Err(e) => excluded.push(Exclusion {
                symbol: name.into(),
                reason: e.to_string(),
            }),
        }
    }
    accepted.sort_by(|a, b| {
        b.volume_24h_usdt
            .total_cmp(&a.volume_24h_usdt)
            .then(a.symbol.cmp(&b.symbol))
    });
    for i in accepted.iter().skip(limit) {
        excluded.push(Exclusion {symbol:i.symbol.clone(),reason:"eligible but outside bounded collection universe (24h activity priority, not profitability ranking)".into()});
    }
    accepted.truncate(limit);
    Ok((accepted, excluded))
}

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
pub const RATE_SCALE: u128 = 1_000_000;
pub const RATE_DENOM: u128 = 10_000 * RATE_SCALE;
pub fn ceil_div(n: u128, d: u128) -> Result<u128> {
    ensure!(d > 0, "zero denominator");
    Ok(n / d + u128::from(!n.is_multiple_of(d)))
}
pub fn notional(price: u64, quantity: u64) -> Result<u128> {
    Ok(u128::from(price)
        .checked_mul(quantity.into())
        .context("notional overflow")?
        / 100_000_000)
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FeeAsset {
    Quote,
    Base,
    Bnb,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeePeriod {
    pub from_ms: u64,
    pub until_ms: Option<u64>,
    pub symbol: Option<String>,
    pub standard_maker_microbps: u64,
    pub standard_taker_microbps: u64,
    #[serde(default)]
    pub standard_buy_microbps: u64,
    #[serde(default)]
    pub standard_sell_microbps: u64,
    #[serde(default)]
    pub buy_asset: Option<FeeAsset>,
    #[serde(default)]
    pub sell_asset: Option<FeeAsset>,
    pub buy_extra_microbps: u64,
    pub sell_extra_microbps: u64,
    pub bnb_discount_ppm: u32,
    pub asset: FeeAsset,
    pub bnb_quote_price_units: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeScenario {
    pub name: String,
    pub account_verified: bool,
    pub description: String,
    pub periods: Vec<FeePeriod>,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Charge {
    pub asset: FeeAsset,
    pub amount_atoms: u128,
    pub quote_equivalent_atoms: u128,
}
impl FeeScenario {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.account_verified,
            "account-specific fees cannot be verified without credentials in this milestone"
        );
        ensure!(
            !self.name.is_empty() && !self.periods.is_empty(),
            "empty fee scenario"
        );
        for p in &self.periods {
            ensure!(
                p.until_ms.is_none_or(|end| end > p.from_ms) && p.bnb_discount_ppm <= 1_000_000,
                "invalid fee period/discount"
            );
            ensure!(
                p.standard_maker_microbps <= 1_000_000_000
                    && p.standard_taker_microbps <= 1_000_000_000
                    && p.buy_extra_microbps <= 1_000_000_000
                    && p.sell_extra_microbps <= 1_000_000_000
                    && p.standard_buy_microbps <= 1_000_000_000
                    && p.standard_sell_microbps <= 1_000_000_000,
                "fee out of supported range"
            );
            ensure!(
                p.bnb_discount_ppm == 0
                    || p.asset_for(true) == FeeAsset::Bnb
                    || p.asset_for(false) == FeeAsset::Bnb,
                "BNB discount requires BNB fee asset"
            );
            ensure!(
                (p.asset_for(true) != FeeAsset::Bnb && p.asset_for(false) != FeeAsset::Bnb)
                    || p.bnb_quote_price_units.is_some_and(|p| p > 0),
                "BNB conversion assumption missing"
            );
        }
        for (i, a) in self.periods.iter().enumerate() {
            for b in &self.periods[i + 1..] {
                ensure!(
                    a.symbol != b.symbol
                        || a.until_ms.unwrap_or(u64::MAX) <= b.from_ms
                        || b.until_ms.unwrap_or(u64::MAX) <= a.from_ms,
                    "ambiguous overlapping fee periods"
                );
            }
        }
        Ok(())
    }
    pub fn at(&self, time: u64, symbol: &str) -> Result<&FeePeriod> {
        let active = |p: &&FeePeriod| p.from_ms <= time && p.until_ms.is_none_or(|end| time < end);
        self.periods
            .iter()
            .filter(active)
            .find(|p| p.symbol.as_deref() == Some(symbol))
            .or_else(|| {
                self.periods
                    .iter()
                    .filter(active)
                    .find(|p| p.symbol.is_none())
            })
            .context("missing historical fee assumption")
    }
}
impl FeePeriod {
    pub fn asset_for(&self, buy: bool) -> FeeAsset {
        if buy {
            self.buy_asset.unwrap_or(self.asset)
        } else {
            self.sell_asset.unwrap_or(self.asset)
        }
    }
    pub fn rate(&self, buy: bool, maker: bool) -> Result<u128> {
        let standard = if maker {
            self.standard_maker_microbps
        } else {
            self.standard_taker_microbps
        };
        let standard = standard
            + if buy {
                self.standard_buy_microbps
            } else {
                self.standard_sell_microbps
            };
        let discount = if self.asset_for(buy) == FeeAsset::Bnb {
            self.bnb_discount_ppm
        } else {
            0
        };
        let discounted = ceil_div(
            u128::from(standard) * (RATE_SCALE - u128::from(discount)),
            RATE_SCALE,
        )?;
        Ok(discounted
            + u128::from(if buy {
                self.buy_extra_microbps
            } else {
                self.sell_extra_microbps
            }))
    }
    pub fn charge(&self, price: u64, quantity: u64, buy: bool, maker: bool) -> Result<Charge> {
        ensure!(price > 0 && quantity > 0, "invalid fee basis");
        let rate = self.rate(buy, maker)?;
        let quote_numerator = u128::from(price)
            .checked_mul(quantity.into())
            .and_then(|v| v.checked_mul(rate))
            .context("fee overflow")?;
        let quote = ceil_div(quote_numerator, 100_000_000 * RATE_DENOM)?;
        let asset = self.asset_for(buy);
        let amount = match asset {
            FeeAsset::Quote => quote,
            FeeAsset::Base => ceil_div(
                u128::from(quantity)
                    .checked_mul(rate)
                    .context("base fee overflow")?,
                RATE_DENOM,
            )?,
            FeeAsset::Bnb => ceil_div(
                quote_numerator,
                RATE_DENOM
                    .checked_mul(
                        self.bnb_quote_price_units
                            .context("BNB price missing")?
                            .into(),
                    )
                    .context("BNB fee denominator overflow")?,
            )?,
        };
        Ok(Charge {
            asset,
            amount_atoms: amount,
            quote_equivalent_atoms: match asset {
                FeeAsset::Quote => quote,
                _ => ceil_div(
                    amount
                        .checked_mul(u128::from(if asset == FeeAsset::Base {
                            price
                        } else {
                            self.bnb_quote_price_units.context("BNB price missing")?
                        }))
                        .context("fee valuation overflow")?,
                    100_000_000,
                )?,
            },
        })
    }
    pub fn roundtrip_microbps(&self) -> Result<u128> {
        Ok(self.rate(true, true)? + self.rate(false, true)?)
    }
}

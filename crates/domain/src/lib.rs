//! Integer exchange units only. Instrument rules define tick and lot scaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingMode {
    Simulation,
    Paper,
}
impl TryFrom<&str> for TradingMode {
    type Error = &'static str;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "simulation" => Ok(Self::Simulation),
            "paper" => Ok(Self::Paper),
            _ => Err("only simulation and paper are supported"),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositiveUnits(u64);
impl PositiveUnits {
    pub fn new(value: u64) -> Result<Self, &'static str> {
        if value == 0 {
            Err("units must be positive")
        } else {
            Ok(Self(value))
        }
    }
    pub fn get(self) -> u64 {
        self.0
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceTicks(pub PositiveUnits);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuantityLots(pub PositiveUnits);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentId {
    pub exchange: String,
    pub symbol: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderIntent {
    pub client_order_id: String,
    pub instrument: InstrumentId,
    pub side: Side,
    pub price: PriceTicks,
    pub quantity: QuantityLots,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fee {
    pub asset: String,
    pub atomic_units: i128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventEnvelope<T> {
    pub schema_version: u16,
    pub sequence: u64,
    pub exchange_time_ns: u64,
    pub received_time_ns: u64,
    pub payload: T,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mode_is_fail_closed() {
        assert_eq!(
            TradingMode::try_from("simulation"),
            Ok(TradingMode::Simulation)
        );
        assert_eq!(TradingMode::try_from("paper"), Ok(TradingMode::Paper));
        for mode in ["live", "LIVE", "", "margin", "futures"] {
            assert!(TradingMode::try_from(mode).is_err());
        }
    }
    #[test]
    fn executable_units_are_positive_and_exact() {
        assert!(PositiveUnits::new(0).is_err());
        for value in [1, 100, u64::MAX] {
            assert_eq!(PositiveUnits::new(value).unwrap().get(), value);
        }
    }
}

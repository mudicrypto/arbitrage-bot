//! Public market data and simulated execution are separate capabilities.
use mm_domain::{EventEnvelope, InstrumentId, OrderIntent, OrderStatus, PriceTicks, QuantityLots};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookLevel {
    pub price: PriceTicks,
    pub quantity: QuantityLots,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookSnapshot {
    pub instrument: InstrumentId,
    pub last_update_id: u64,
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    Disconnected,
    StaleData,
    SequenceGap,
    InvalidInstrument,
}
pub trait MarketDataSource {
    fn snapshot(
        &mut self,
        instrument: &InstrumentId,
    ) -> Result<EventEnvelope<BookSnapshot>, AdapterError>;
}
/// Implementations MUST be local simulators; no live adapter is provided.
pub trait SimulatedExecution {
    fn submit(&mut self, intent: &OrderIntent) -> Result<OrderStatus, AdapterError>;
    fn cancel(&mut self, client_order_id: &str) -> Result<OrderStatus, AdapterError>;
    fn reconcile(&self, client_order_id: &str) -> Result<OrderStatus, AdapterError>;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_preserves_exchange_sequence() {
        let book = BookSnapshot {
            instrument: InstrumentId {
                exchange: "binance-spot".into(),
                symbol: "BTCUSDT".into(),
            },
            last_update_id: 42,
            bids: vec![],
            asks: vec![],
        };
        assert_eq!(book.clone().last_update_id, 42);
    }
}

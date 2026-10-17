use crate::model::{Depth, Snapshot};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Book {
    pub last_id: Option<u64>,
    pub synced: bool,
    #[serde(with = "level_map")]
    pub bids: BTreeMap<u64, u64>,
    #[serde(with = "level_map")]
    pub asks: BTreeMap<u64, u64>,
    // Boundaries of known snapshot coverage. Outside levels are partial knowledge.
    pub bid_floor: u64,
    pub ask_ceiling: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Applied {
    Updated,
    Duplicate,
}
impl Book {
    pub fn invalidate(&mut self) {
        *self = Self::default();
    }
    pub fn snapshot(&mut self, snapshot: &Snapshot, max_levels: usize) -> Result<()> {
        let mut next = Self {
            last_id: Some(snapshot.last_id),
            ..Self::default()
        };
        ensure!(
            snapshot.bids.len() <= max_levels && snapshot.asks.len() <= max_levels,
            "book level limit"
        );
        for l in &snapshot.bids {
            ensure!(
                l.price > 0 && l.quantity > 0 && next.bids.insert(l.price, l.quantity).is_none(),
                "invalid snapshot bid"
            );
        }
        for l in &snapshot.asks {
            ensure!(
                l.price > 0 && l.quantity > 0 && next.asks.insert(l.price, l.quantity).is_none(),
                "invalid snapshot ask"
            );
        }
        next.bid_floor = *next
            .bids
            .first_key_value()
            .ok_or_else(|| anyhow::anyhow!("empty bids"))?
            .0;
        next.ask_ceiling = *next
            .asks
            .last_key_value()
            .ok_or_else(|| anyhow::anyhow!("empty asks"))?
            .0;
        next.validate(max_levels)?;
        *self = next;
        Ok(())
    }
    pub fn apply(&mut self, event: &Depth, max_levels: usize) -> Result<Applied> {
        let last = self
            .last_id
            .ok_or_else(|| anyhow::anyhow!("snapshot missing"))?;
        ensure!(event.first_id <= event.last_id, "invalid sequence range");
        if event.last_id <= last {
            return Ok(Applied::Duplicate);
        }
        ensure!(
            event.first_id
                <= last
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("sequence overflow"))?,
            "sequence gap"
        );
        // Atomic validation: a failed update must never expose a partially modified book.
        let mut next = self.clone();
        for (side, updates) in [(&mut next.bids, &event.bids), (&mut next.asks, &event.asks)] {
            for level in updates {
                ensure!(level.price > 0, "zero price");
                if level.quantity == 0 {
                    side.remove(&level.price);
                } else {
                    side.insert(level.price, level.quantity);
                }
            }
        }
        next.last_id = Some(event.last_id);
        next.synced = true;
        next.validate(max_levels)?;
        *self = next;
        Ok(Applied::Updated)
    }
    pub fn validate(&self, max_levels: usize) -> Result<()> {
        ensure!(
            self.bids.len() <= max_levels && self.asks.len() <= max_levels,
            "book level limit"
        );
        let bid = *self
            .bids
            .last_key_value()
            .ok_or_else(|| anyhow::anyhow!("empty bids"))?
            .0;
        let ask = *self
            .asks
            .first_key_value()
            .ok_or_else(|| anyhow::anyhow!("empty asks"))?
            .0;
        ensure!(bid < ask, "crossed or locked book");
        ensure!(
            bid >= self.bid_floor && ask <= self.ask_ceiling,
            "snapshot coverage exhausted"
        );
        ensure!(
            self.bids
                .iter()
                .chain(self.asks.iter())
                .all(|(p, q)| *p > 0 && *q > 0),
            "invalid level"
        );
        Ok(())
    }
}

mod level_map {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;
    pub fn serialize<S: Serializer>(
        map: &BTreeMap<u64, u64>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        map.iter().collect::<Vec<_>>().serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<u64, u64>, D::Error> {
        let levels = Vec::<(u64, u64)>::deserialize(deserializer)?;
        let length = levels.len();
        let map: BTreeMap<_, _> = levels.into_iter().collect();
        if map.len() != length {
            return Err(serde::de::Error::custom("duplicate checkpoint level"));
        }
        Ok(map)
    }
}

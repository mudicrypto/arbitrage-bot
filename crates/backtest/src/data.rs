use anyhow::{Context, Result, ensure};
use mm_market_data::{
    model::Trade,
    recording::{Event, replay_visit, segment_paths},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct View {
    pub bids: Vec<(u64, u64)>,
    pub asks: Vec<(u64, u64)>,
}
impl View {
    pub fn valid(&self) -> bool {
        !self.bids.is_empty()
            && !self.asks.is_empty()
            && self.bids.len() <= 10_000
            && self.asks.len() <= 10_000
            && self
                .bids
                .iter()
                .chain(&self.asks)
                .all(|(p, q)| *p > 0 && *q > 0)
            && self.bids.windows(2).all(|p| p[0].0 > p[1].0)
            && self.asks.windows(2).all(|p| p[0].0 < p[1].0)
            && self.bid() < self.ask()
    }

    pub fn bid(&self) -> u64 {
        self.bids[0].0
    }
    pub fn ask(&self) -> u64 {
        self.asks[0].0
    }
    pub fn mid(&self) -> u64 {
        ((u128::from(self.bid()) + u128::from(self.ask())) / 2) as u64
    }
    pub fn quantity(&self, buy: bool, price: u64) -> u64 {
        if buy { &self.bids } else { &self.asks }
            .iter()
            .find(|(p, _)| *p == price)
            .map_or(0, |(_, q)| *q)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Input {
    Book { view: View, exchange_ms: u64 },
    Trade { trade: Trade },
    Invalid { reason: String },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tick {
    pub time_ms: u64,
    pub sequence: u64,
    pub input: Input,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAudit {
    pub symbol: String,
    pub segments: usize,
    pub records: usize,
    pub book_updates: usize,
    pub trades: usize,
    pub invalidations: usize,
    pub prefix_complete: bool,
    pub sequence_contiguous: bool,
    pub clock_clamps: usize,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub raw_messages: usize,
    pub trade_id_gaps: u64,
    pub local_top_of_book_supported: bool,
    pub depth_levels_retained_in_tape: usize,
    pub exact_queue_known: bool,
    pub limitations: Vec<String>,
}
pub fn stream_symbol(
    path: &Path,
    symbol: &str,
    mut visit: impl FnMut(Tick) -> Result<()>,
) -> Result<DataAudit> {
    let files = segment_paths(path)?;
    ensure!(!files.is_empty(), "recordings missing");
    let mut audit=DataAudit {symbol:symbol.into(),segments:0,records:0,book_updates:0,trades:0,invalidations:0,prefix_complete:true,sequence_contiguous:true,clock_clamps:0,start_ms:None,end_ms:None,raw_messages:0,trade_id_gaps:0,local_top_of_book_supported:false,depth_levels_retained_in_tape:10_000,exact_queue_known:false,limitations:vec!["Market-by-price snapshots/deltas do not reveal individual order IDs, hidden liquidity, cancellations ahead of us, or exact queue rank.".into(),"Aggregate trades reveal aggressor direction, price and total quantity, not the intra-aggregate ordering or matching against our hypothetical orders.".into(),"Tape retains the known reconstructed levels/side. Missing quote-price coverage rejects entries; raw multi-level books remain in Parquet.".into(),"Recorder application sequence determines causal availability; backwards receive times during snapshot buffering are clamped forward, never sorted to introduce future books.".into()]};
    let mut identity: Option<(String, u64)> = None;
    let mut clock = 0;
    let mut last_trade: Option<u64> = None;
    let mut last_session: Option<String> = None;
    for file in files {
        let mut first = true;
        let mut matches = false;
        replay_visit(&file, 10_000, 8 * 1024 * 1024, |record, book| {
            if record.symbol != symbol {
                return Ok(());
            }
            matches = true;
            audit.records += 1;
            if record.raw.is_some() {
                audit.raw_messages += 1;
            }
            if first {
                if let Some((id, seq)) = &identity {
                    if id != &record.recording_id || record.sequence != seq + 1 {
                        audit.sequence_contiguous = false;
                        visit(Tick {
                            time_ms: clock,
                            sequence: record.sequence,
                            input: Input::Invalid {
                                reason: "retained segment discontinuity".into(),
                            },
                        })?;
                    }
                } else {
                    audit.prefix_complete = record.sequence == 1;
                }
                first = false;
            }
            identity = Some((record.recording_id.clone(), record.sequence));
            let time = record.received_ms.max(clock);
            if record.received_ms < clock {
                audit.clock_clamps += 1;
            }
            clock = time;
            if last_session.as_deref() != Some(&record.connection_session) {
                if last_session.is_some() {
                    visit(Tick {
                        time_ms: time,
                        sequence: record.sequence,
                        input: Input::Invalid {
                            reason: "new connection session".into(),
                        },
                    })?;
                }
                last_session = Some(record.connection_session.clone());
            }
            let input = match &record.event {
                Event::AppliedDepth { depth } => {
                    audit.book_updates += 1;
                    audit.local_top_of_book_supported = true;
                    Some(Input::Book {
                        view: View {
                            bids: book
                                .bids
                                .iter()
                                .rev()
                                .take(10_000)
                                .map(|(p, q)| (*p, *q))
                                .collect(),
                            asks: book
                                .asks
                                .iter()
                                .take(10_000)
                                .map(|(p, q)| (*p, *q))
                                .collect(),
                        },
                        exchange_ms: depth.exchange_ms,
                    })
                }
                Event::Trade { trade } => {
                    if last_trade.is_some_and(|id| trade.aggregate_id <= id) {
                        return Ok(());
                    }
                    if last_trade.is_some_and(|id| trade.aggregate_id > id.saturating_add(1)) {
                        audit.trade_id_gaps += 1;
                        visit(Tick {
                            time_ms: time,
                            sequence: record.sequence,
                            input: Input::Invalid {
                                reason: "aggregate trade gap".into(),
                            },
                        })?;
                    }
                    last_trade = Some(trade.aggregate_id);
                    audit.trades += 1;
                    Some(Input::Trade {
                        trade: trade.clone(),
                    })
                }
                Event::Invalidated { reason } => {
                    audit.invalidations += 1;
                    Some(Input::Invalid {
                        reason: reason.clone(),
                    })
                }
                _ => None,
            };
            if let Some(input) = input {
                audit.start_ms.get_or_insert(time);
                audit.end_ms = Some(time);
                visit(Tick {
                    time_ms: time,
                    sequence: record.sequence,
                    input,
                })?;
            }
            Ok(())
        })?;
        if matches {
            audit.segments += 1;
        }
    }
    ensure!(identity.is_some(), "symbol recordings missing");
    Ok(audit)
}
pub fn load_symbol(path: &Path, symbol: &str) -> Result<(Vec<Tick>, DataAudit)> {
    let mut tape = vec![];
    let mut bytes = 0usize;
    let audit = stream_symbol(path, symbol, |tick| {
        bytes += match &tick.input {
            Input::Book { view, .. } => (view.bids.len() + view.asks.len()) * 16 + 128,
            _ => 512,
        };
        ensure!(
            bytes <= 512 * 1024 * 1024 && tape.len() < 1_000_000,
            "bounded tape limit exceeded; use streaming batch command"
        );
        tape.push(tick);
        Ok(())
    })?;
    Ok((tape, audit))
}
pub fn audit_all(root: &Path) -> Result<Vec<serde_json::Value>> {
    let paths = segment_paths(root)?;
    let mut groups: BTreeMap<String, Vec<std::path::PathBuf>> = BTreeMap::new();
    for p in paths {
        let parent = p
            .parent()
            .context("recording parent missing")?
            .to_string_lossy()
            .to_string();
        groups.entry(parent).or_default().push(p);
    }
    let mut results = vec![];
    for (path, files) in groups {
        let symbol = Path::new(&path)
            .file_name()
            .context("symbol directory missing")?
            .to_string_lossy();
        if !mm_market_data::model::valid_symbol(&symbol) {
            continue;
        }
        // Freeze filenames for a moving M2 retention window; fail clearly if a file vanishes.
        let mut records = 0;
        let mut trades = 0;
        let mut books = 0;
        let mut first_sequence = None;
        let mut raw = 0;
        let mut errors = vec![];
        for file in &files {
            match replay_visit(file, 10_000, 8 * 1024 * 1024, |r, _| {
                first_sequence.get_or_insert(r.sequence);
                records += 1;
                raw += usize::from(r.raw.is_some());
                trades += usize::from(matches!(r.event, Event::Trade { .. }));
                books += usize::from(matches!(r.event, Event::AppliedDepth { .. }));
                Ok(())
            }) {
                Ok(_) => (),
                Err(e) => errors.push(e.to_string()),
            }
        }
        results.push(serde_json::json!({"path":path,"symbol":symbol,"segments":files.len(),"records":records,"raw_messages":raw,"book_updates":books,"aggregate_trades":trades,"prefix_complete":first_sequence==Some(1),"checksum_or_read_errors":errors,"supports_aggressor_classification":trades>0,"supports_exact_queue":false,"supports_trade_evidence_partial_fill_model":trades>0 && books>0,"supports_long_horizon_statistics":false}));
    }
    Ok(results)
}

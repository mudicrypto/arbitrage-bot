//! Streaming integrity checks over a frozen finalized-file list; never evaluate a strategy.
use anyhow::{Context, Result, ensure};
use mm_market_data::recording::{Event, replay_visit, segment_paths};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrozenFile {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256_sidecar: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Coverage {
    pub records: u64,
    pub raw_messages: u64,
    pub book_updates: u64,
    pub aggregate_trades: u64,
    pub first_received_ms: Option<u64>,
    pub last_received_ms: Option<u64>,
    pub first_book_ms: Option<u64>,
    pub last_book_ms: Option<u64>,
    pub first_trade_ms: Option<u64>,
    pub last_trade_ms: Option<u64>,
    pub prefix_complete: bool,
    pub record_sequence_gaps: u64,
    pub trade_id_gaps: u64,
    pub raw_receive_time_reversals: u64,
    pub causal_clock_clamps: u64,
    pub book_exchange_time_reversals: u64,
    pub trade_exchange_time_reversals: u64,
    pub book_intervals_over_2s: Vec<(u64, u64)>,
    pub max_book_interval_ms: u64,
    pub session_changes: Vec<u64>,
    pub invalidations: BTreeMap<String, u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayAudit {
    pub symbols: BTreeMap<String, Coverage>,
    pub errors: Vec<String>,
    pub replay_digest: String,
}
pub fn inspect_frozen(files: &[FrozenFile]) -> Result<ReplayAudit> {
    ensure!(files.len() <= 40_000, "bounded audit file count exceeded");
    let mut symbols: BTreeMap<String, Coverage> = BTreeMap::new();
    let mut identities: BTreeMap<String, (String, u64)> = BTreeMap::new();
    let mut sessions: BTreeMap<String, String> = BTreeMap::new();
    let mut trade_ids: BTreeMap<String, u64> = BTreeMap::new();
    let mut receives: BTreeMap<String, u64> = BTreeMap::new();
    let mut clocks: BTreeMap<String, u64> = BTreeMap::new();
    let mut book_exchange: BTreeMap<String, u64> = BTreeMap::new();
    let mut trade_exchange: BTreeMap<String, u64> = BTreeMap::new();
    let mut errors = vec![];
    let mut digest = Sha256::new();
    for file in files {
        let observed = (|| -> Result<_> {
            ensure!(
                std::fs::metadata(&file.path)?.len() == file.bytes,
                "frozen size changed"
            );
            ensure!(
                std::fs::read_to_string(file.path.with_extension("sha256"))? == file.sha256_sidecar,
                "frozen checksum sidecar changed"
            );
            replay_visit(&file.path, 10_000, 8 * 1024 * 1024, |r, _| {
                digest.update(serde_json::to_vec(r)?);
                let c = symbols.entry(r.symbol.clone()).or_default();
                if let Some((identity, seq)) = identities.get(&r.symbol) {
                    if identity != &r.recording_id || r.sequence != seq + 1 {
                        c.record_sequence_gaps += 1;
                    }
                } else {
                    c.prefix_complete = r.sequence == 1;
                }
                identities.insert(r.symbol.clone(), (r.recording_id.clone(), r.sequence));
                if let Some(session) =
                    sessions.insert(r.symbol.clone(), r.connection_session.clone())
                    && session != r.connection_session
                {
                    ensure!(c.session_changes.len() < 100_000, "session audit bound");
                    c.session_changes.push(r.received_ms);
                }
                if let Some(t) = receives.insert(r.symbol.clone(), r.received_ms) {
                    c.raw_receive_time_reversals += u64::from(r.received_ms < t);
                }
                let clock = clocks.entry(r.symbol.clone()).or_default();
                c.causal_clock_clamps += u64::from(r.received_ms < *clock);
                *clock = (*clock).max(r.received_ms);
                c.records += 1;
                c.raw_messages += u64::from(r.raw.is_some());
                c.first_received_ms = Some(
                    c.first_received_ms
                        .map_or(r.received_ms, |t| t.min(r.received_ms)),
                );
                c.last_received_ms = Some(
                    c.last_received_ms
                        .map_or(r.received_ms, |t| t.max(r.received_ms)),
                );
                match &r.event {
                    Event::AppliedDepth { depth } => {
                        if let Some(last) = c.last_book_ms {
                            let gap = clock.saturating_sub(last);
                            c.max_book_interval_ms = c.max_book_interval_ms.max(gap);
                            if gap > 2000 {
                                ensure!(
                                    c.book_intervals_over_2s.len() < 100_000,
                                    "gap audit bound"
                                );
                                c.book_intervals_over_2s.push((last, *clock));
                            }
                        }
                        c.first_book_ms.get_or_insert(*clock);
                        c.last_book_ms = Some(*clock);
                        c.book_updates += 1;
                        if let Some(t) = book_exchange.insert(r.symbol.clone(), depth.exchange_ms) {
                            c.book_exchange_time_reversals += u64::from(depth.exchange_ms < t);
                        }
                    }
                    Event::Trade { trade } => {
                        if let Some(id) = trade_ids.insert(r.symbol.clone(), trade.aggregate_id) {
                            c.trade_id_gaps += u64::from(trade.aggregate_id > id.saturating_add(1));
                        }
                        if let Some(t) = trade_exchange.insert(r.symbol.clone(), trade.exchange_ms)
                        {
                            c.trade_exchange_time_reversals += u64::from(trade.exchange_ms < t);
                        }
                        c.first_trade_ms.get_or_insert(*clock);
                        c.last_trade_ms = Some(*clock);
                        c.aggregate_trades += 1;
                    }
                    Event::Invalidated { reason } => {
                        *c.invalidations.entry(reason.clone()).or_default() += 1;
                    }
                    _ => (),
                }
                Ok(())
            })
        })();
        match observed {
            Ok(replay) => {
                digest.update(serde_json::to_vec(&replay.book)?);
            }
            Err(e) => errors.push(format!("{}: {e:#}", file.path.display())),
        }
    }
    Ok(ReplayAudit {
        symbols,
        errors,
        replay_digest: format!("{:x}", digest.finalize()),
    })
}
pub fn capture(root: &Path, output: &Path) -> Result<()> {
    let config: crate::collection::CollectionConfig =
        serde_json::from_slice(&std::fs::read(root.join("collection-config.json"))?)?;
    config.validate()?;
    let paths = segment_paths(&root.join("recordings"))?;
    ensure!(paths.len() <= 40_000, "bounded file enumeration exceeded");
    let mut files = vec![];
    for p in paths {
        files.push(FrozenFile {
            bytes: std::fs::metadata(&p)?.len(),
            sha256_sidecar: std::fs::read_to_string(p.with_extension("sha256"))?,
            path: p,
        });
    }
    let status: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("status.json"))?)?;
    let captured_at = mm_market_data::runtime::now_ms();
    let first = inspect_frozen(&files)?;
    let second = inspect_frozen(&files)?;
    let completed = root.join("collection_finished.json").exists();
    let completion: Option<serde_json::Value> = if completed {
        Some(serde_json::from_slice(&std::fs::read(
            root.join("collection_finished.json"),
        )?)?)
    } else {
        None
    };
    let continuous = first.errors.is_empty()
        && first.symbols.len() == config.symbols.len()
        && first.symbols.values().all(|s| {
            s.prefix_complete
                && s.record_sequence_gaps == 0
                && s.trade_id_gaps == 0
                && s.book_intervals_over_2s.is_empty()
                && s.invalidations.is_empty()
                && s.book_updates > 0
                && s.aggregate_trades > 0
        });
    let target_met = completion
        .as_ref()
        .and_then(|c| c["observed_seconds"].as_u64())
        .is_some_and(|n| n >= config.duration_seconds);
    let recorded_tail_complete = completion
        .as_ref()
        .and_then(|c| c["end_ms"].as_u64())
        .is_some_and(|end| {
            first.symbols.values().all(|s| {
                s.last_book_ms
                    .is_some_and(|t| end.saturating_sub(t) <= 2000)
            })
        });
    let json = serde_json::json!({"schema_version":1,"result_type":"observed_recording_integrity_audit_not_strategy_performance","source":root,"captured_at_ms":captured_at,"target_seconds":config.duration_seconds,"completion_marker_present":completed,"target_completed":target_met,"completion":completion,"status_at_audit_start":status,"frozen_finalized_files":files,"finalized_payload_bytes":files.iter().map(|f|f.bytes).sum::<u64>(),"configured_payload_budget_bytes":config.bytes_per_symbol*config.symbols.len() as u64,"free_disk_bytes_at_audit":fs2::available_space(root)?,"first_replay":first,"second_replay_identical":first==second,"second_replay_digest":second.replay_digest,"recorded_tail_complete":recorded_tail_complete,"continuous_complete_target_ready":continuous && target_met && recorded_tail_complete && first==second,"strategy_evaluated":false,"final_oos_evaluated":false,"limitations":["Frozen finalized files only; new segments after enumeration and unflushed tails are excluded. Active retention can remove files and produces explicit read errors, never fabricated continuity.","Book intervals >2s are missing freshness coverage at the simulator threshold, not automatically proof of lost exchange messages. Session changes/invalidations and aggregate-ID gaps are separate indicators.","Raw buffered receive timestamps may reverse; causal replay clamps them forward in recorded application order. Source sequence and per-segment book checks remain authoritative.","Integrity replay counts source data only. It does not inspect strategy outcomes or authorize OOS evaluation. Exact queue rank and real own fills remain unknowable from MBP.","The legacy run command retains finite tape/trace caps. The batch command streams completed valid sources with explicit capital and output budgets."]});
    std::fs::create_dir_all(output.parent().context("output parent missing")?)?;
    std::fs::write(output, serde_json::to_vec_pretty(&json)?)?;
    Ok(())
}

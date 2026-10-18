//! Resumption creates a new immutable session; never reopens or edits a running capture.
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub root: PathBuf,
    pub collection: crate::collection::CollectionConfig,
    pub session_seconds: u64,
    pub max_sessions: usize,
    pub max_campaign_bytes: u64,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        self.collection.validate()?;
        ensure!(
            (3600..=21600).contains(&self.session_seconds)
                && (4..=72).contains(&self.max_sessions)
                && (1024 * 1024 * 1024..=16 * 1024 * 1024 * 1024)
                    .contains(&self.max_campaign_bytes),
            "invalid bounded campaign"
        );
        ensure!(
            self.session_seconds * self.max_sessions as u64 >= self.collection.duration_seconds,
            "session limit cannot reach target"
        );
        ensure!(
            self.max_campaign_bytes
                >= self.collection.bytes_per_symbol * self.collection.symbols.len() as u64,
            "campaign cannot fit one session budget"
        );
        Ok(())
    }
}
pub fn bytes_under(root: &Path) -> Result<u64> {
    let mut bytes = 0;
    let mut count = 0usize;
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        if !dir.exists() {
            continue;
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            count += 1;
            ensure!(count <= 200_000, "bounded campaign enumeration exceeded");
            let meta = entry.file_type()?;
            ensure!(!meta.is_symlink(), "campaign symlinks are unsupported");
            if meta.is_dir() {
                dirs.push(entry.path());
            } else if meta.is_file() {
                bytes += entry.metadata()?.len();
            }
        }
    }
    Ok(bytes)
}
pub fn inventory(root: &Path) -> Result<Vec<serde_json::Value>> {
    let mut result = vec![];
    if !root.exists() {
        return Ok(result);
    }
    let mut paths = fs::read_dir(root)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    for path in paths {
        ensure!(
            !fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "campaign session symlink is unsupported"
        );
        if !path.is_dir() {
            continue;
        }
        ensure!(result.len() < 72, "campaign session bound exceeded");
        let completed = path.join("collection_finished.json");
        if completed.exists() {
            let marker: serde_json::Value = serde_json::from_slice(&fs::read(completed)?)?;
            result.push(serde_json::json!({"path":path,"state":"completed_requires_replay_audit","completion":marker}));
        } else {
            result.push(serde_json::json!({"path":path,"state":"active_or_unclean_never_overwrite","observed_seconds_counted":0}));
        }
    }
    Ok(result)
}
pub async fn resume(config_path: &Path, smoke: Option<u64>) -> Result<()> {
    let bytes = fs::read(config_path)?;
    let config: Config = serde_json::from_slice(&bytes)?;
    config.validate()?;
    ensure!(
        smoke.is_none_or(|s| (10..=120).contains(&s)),
        "smoke must be 10..120 seconds"
    );
    ensure!(
        !config.root.join("collection-config.json").exists()
            && !config.root.join("recordings").exists(),
        "campaign root must not be an existing capture"
    );
    fs::create_dir_all(&config.root)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(config.root.join("campaign.lock"))?;
    lock.try_lock_exclusive()
        .context("campaign already running; do not restart its collector")?;
    let saved = config.root.join("campaign-config.json");
    if saved.exists() {
        ensure!(
            fs::read(&saved)? == bytes,
            "campaign configuration changed; use a new campaign root"
        );
    } else {
        fs::write(saved, &bytes)?;
    }
    let sessions = config.root.join("sessions");
    fs::create_dir_all(&sessions)?;
    loop {
        let old = inventory(&sessions)?;
        // Do not assume an incomplete old directory is dead, even if its heartbeat is old.
        ensure!(
            old.iter()
                .all(|s| s["state"] != "active_or_unclean_never_overwrite"),
            "unfinished session requires explicit external reconciliation; no automatic takeover"
        );
        let seconds: u64 = old
            .iter()
            .filter_map(|s| s["completion"]["observed_seconds"].as_u64())
            .sum();
        let used = bytes_under(&config.root)?;
        let state = serde_json::json!({"result_type":"observed_collection_campaign_progress_not_strategy_results","target_seconds":config.collection.duration_seconds,"elapsed_recorded_session_seconds":seconds,"continuous_coverage_not_assumed":true,"target_duration_reached":seconds>=config.collection.duration_seconds,"session_inventory":old,"bytes_used":used,"max_campaign_bytes":config.max_campaign_bytes});
        let tmp = config.root.join("campaign-state.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&state)?)?;
        fs::rename(tmp, config.root.join("campaign-state.json"))?;
        if seconds >= config.collection.duration_seconds {
            break;
        }
        ensure!(
            old.len() < config.max_sessions,
            "bounded session count reached; previous data preserved"
        );
        let reserve = config.collection.bytes_per_symbol * config.collection.symbols.len() as u64
            + 128 * 1024 * 1024;
        ensure!(
            used + reserve <= config.max_campaign_bytes,
            "campaign byte budget reached; previous sessions preserved"
        );
        let mut collection = config.collection.clone();
        collection.root = sessions.clone();
        let duration = smoke
            .unwrap_or(
                config
                    .session_seconds
                    .min(config.collection.duration_seconds - seconds),
            )
            .max(10);
        let path = crate::collection::collect_session(collection, duration).await?;
        let marker: serde_json::Value =
            serde_json::from_slice(&fs::read(path.join("collection_finished.json"))?)?;
        if smoke.is_some() || marker["reason"] != "duration_complete" {
            break;
        }
    }
    Ok(())
}

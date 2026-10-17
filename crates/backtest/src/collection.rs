//! Bounded long public-data collection; no research observations are invented.
use anyhow::{Result, ensure};
use mm_market_data::runtime::{Config, SharedStatus, now_ms, run_symbol, write_status};
use mm_research::{
    discovery::{eligible, public_json},
    run::{Manifest, ResearchConfig},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionConfig {
    pub duration_seconds: u64,
    pub symbols: Vec<String>,
    pub bytes_per_symbol: u64,
    pub flush_interval_ms: u64,
    pub root: PathBuf,
}
impl CollectionConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (86400..=259200).contains(&self.duration_seconds)
                && !self.symbols.is_empty()
                && self.symbols.len() <= 4,
            "long target must be 24–72 hours and 1..4 symbols"
        );
        ensure!(
            (16 * 1024 * 1024..=2 * 1024 * 1024 * 1024).contains(&self.bytes_per_symbol)
                && (2000..=30_000).contains(&self.flush_interval_ms),
            "invalid bounded recording limits"
        );
        let set: std::collections::BTreeSet<_> = self.symbols.iter().collect();
        ensure!(
            set.len() == self.symbols.len()
                && self
                    .symbols
                    .iter()
                    .all(|s| mm_market_data::model::valid_symbol(s)),
            "invalid or duplicate symbols"
        );
        Ok(())
    }
}
pub async fn collect(config: CollectionConfig, smoke_seconds: Option<u64>) -> Result<PathBuf> {
    config.validate()?;
    ensure!(
        smoke_seconds.is_none_or(|s| (10..=120).contains(&s)),
        "smoke duration must be 10..120 seconds"
    );
    std::fs::create_dir_all(&config.root)?;
    ensure!(
        fs2::available_space(&config.root)?
            > config.bytes_per_symbol * config.symbols.len() as u64 + 1024 * 1024 * 1024,
        "insufficient free storage for long-collection budget"
    );
    let client = reqwest_client()?;
    let (raw_info,info)=public_json(&client,"https://data-api.binance.vision/api/v3/exchangeInfo?permissions=SPOT&showPermissionSets=false").await?;
    let (raw_tickers, tickers) = public_json(
        &client,
        "https://data-api.binance.vision/api/v3/ticker/24hr",
    )
    .await?;
    let mut research: ResearchConfig =
        serde_json::from_slice(&std::fs::read("config/research.json")?)?;
    let mut instruments = vec![];
    for symbol in &config.symbols {
        let meta = info["symbols"]
            .as_array()
            .and_then(|a| a.iter().find(|s| s["symbol"] == symbol.as_str()))
            .ok_or_else(|| anyhow::anyhow!("public symbol metadata missing"))?;
        let ticker = tickers
            .as_array()
            .and_then(|a| a.iter().find(|s| s["symbol"] == symbol.as_str()))
            .ok_or_else(|| anyhow::anyhow!("public ticker missing"))?;
        instruments.push(eligible(meta, ticker, &research.policy)?);
    }
    let start = now_ms();
    let seconds = smoke_seconds.unwrap_or(config.duration_seconds);
    let end = start + seconds * 1000;
    let root = config.root.join(format!("session-{start}"));
    std::fs::create_dir_all(&root)?;
    std::fs::write(
        root.join("collection-config.json"),
        serde_json::to_vec_pretty(&config)?,
    )?;
    std::fs::write(root.join("exchange-info.json"), raw_info)?;
    std::fs::write(root.join("tickers-24h.json"), raw_tickers)?;
    research.duration_seconds = seconds;
    research.universe_limit = instruments.len();
    let mut manifest = Manifest {
        config: research,
        instruments,
        bounds: BTreeMap::new(),
    };
    let shared: SharedStatus = Arc::new(Mutex::new(BTreeMap::new()));
    let budget = Arc::new(tokio::sync::Mutex::new(tokio::time::Instant::now()));
    let mut tasks = vec![];
    for (index, symbol) in config.symbols.iter().enumerate() {
        if index > 0 {
            tokio::time::sleep(Duration::from_secs(4)).await;
        }
        manifest.bounds.insert(symbol.clone(), (now_ms(), end));
        let symbol = symbol.clone();
        let status = shared.clone();
        let mut md = Config {
            include_trades: true,
            snapshot_budget: Some(budget.clone()),
            data_dir: root.join("recordings"),
            flush_interval_ms: config.flush_interval_ms,
            ..Config::default()
        };
        md.recording.segment_records = 4096;
        md.recording.max_segments = 10_000;
        md.recording.max_total_bytes = Some(config.bytes_per_symbol);
        tasks.push(tokio::spawn(async move {
            run_symbol(md, &symbol, status).await
        }));
    }
    std::fs::write(
        root.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    println!(
        "Public-only collection: {} seconds (target {}), {}",
        seconds,
        config.duration_seconds,
        root.display()
    );
    let mut timer = tokio::time::interval(Duration::from_secs(1));
    let mut reason = "duration_complete";
    loop {
        tokio::select! {
         signal=tokio::signal::ctrl_c()=>{signal?;reason="interrupted_gracefully";break;}
         _=timer.tick()=>{
          write_status(&root.join("status.json"),&shared,5000)?;
          if now_ms()>=end {break;}
          if tasks.iter().all(|t|t.is_finished()) {reason="all_collectors_failed";break;}
         }
        }
    }
    for task in &tasks {
        task.abort();
    }
    for task in tasks {
        let _ = task.await;
    }
    let actual_end = now_ms().min(end);
    for bounds in manifest.bounds.values_mut() {
        bounds.1 = actual_end;
    }
    std::fs::write(
        root.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    std::fs::write(
        root.join("quality_metrics.json"),
        serde_json::to_vec_pretty(&*shared.lock().unwrap())?,
    )?;
    std::fs::write(
        root.join("collection_finished.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"start_ms":start,"end_ms":actual_end,"reason":reason,"smoke":smoke_seconds.is_some(),"target_seconds":config.duration_seconds,"observed_seconds":(actual_end-start)/1000,"retention_may_have_evicted_prefix":true,"requires_replay_audit":true}),
        )?,
    )?;
    Ok(root)
}
fn reqwest_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

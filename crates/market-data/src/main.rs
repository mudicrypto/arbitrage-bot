use anyhow::{Result, bail};
use mm_market_data::{
    recording::{Limits, replay, segment_paths},
    runtime::{Config, collect},
};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("collect") if args.len() == 1 => {
            let mut config = Config::default();
            if let Ok(path) = std::env::var("MARKET_DATA_DIR") {
                config.data_dir = PathBuf::from(path);
            }
            if let Ok(path) = std::env::var("MARKET_STATUS_FILE") {
                config.status_file = PathBuf::from(path);
            }
            // Endpoints are intentionally fixed to public, market-data-only Binance services.
            collect(config).await
        }
        Some("replay") if args.len() == 2 => {
            let paths = segment_paths(&PathBuf::from(&args[1]))?;
            if paths.is_empty() {
                bail!("no Parquet segments found");
            }
            for path in paths {
                let result = replay(&path, 10_000, Limits::default().max_file_bytes)?;
                println!("{}", serde_json::to_string(&result)?);
            }
            Ok(())
        }
        _ => bail!("usage: mm-market-data collect | replay <segment-or-directory>"),
    }
}

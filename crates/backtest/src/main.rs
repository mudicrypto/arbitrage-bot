use anyhow::{Result, bail};
use std::path::Path;
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("audit") if args.len() == 3 => {
            let rows = mm_backtest::data::audit_all(Path::new(&args[1]))?;
            if let Some(parent) = Path::new(&args[2]).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&args[2], serde_json::to_vec_pretty(&rows)?)?;
            Ok(())
        }
        Some("run") if args.len() == 4 || args.len() == 5 => {
            let include = args.get(4).is_some_and(|s| s == "--include-final-test");
            if args.len() == 5 && !include {
                bail!("unknown option");
            }
            let results = mm_backtest::experiment::run(
                Path::new(&args[1]),
                Path::new(&args[2]),
                Path::new(&args[3]),
                include,
            )?;
            println!(
                "{} simulated experiments; no live execution",
                results.summaries.len()
            );
            Ok(())
        }
        Some("collect") if args.len() == 2 || args.len() == 4 => {
            let config = serde_json::from_slice(&std::fs::read(&args[1])?)?;
            let smoke = if args.len() == 4 {
                if args[2] != "--smoke-seconds" {
                    bail!("unknown option");
                }
                Some(args[3].parse()?)
            } else {
                None
            };
            let path = mm_backtest::collection::collect(config, smoke).await?;
            println!("{}", path.display());
            Ok(())
        }
        _ => bail!(
            "usage: mm-backtest audit <data-root> <output.json> | run <run-dir> <experiment.json> <output-dir> [--include-final-test] | collect <collection.json> [--smoke-seconds 10..120]"
        ),
    }
}

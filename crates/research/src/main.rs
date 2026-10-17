use anyhow::{Result, bail};
use mm_research::run::{Manifest, ResearchConfig, execute, replay_series, summarize};
use std::path::Path;
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") if args.len() <= 3 => {
            let config: ResearchConfig = serde_json::from_slice(&std::fs::read(
                args.get(1).map_or("config/research.json", String::as_str),
            )?)?;
            execute(
                config,
                Path::new(args.get(2).map_or("reports/milestone3", String::as_str)),
            )
            .await?;
            Ok(())
        }
        Some("summarize") if args.len() == 3 => {
            summarize(Path::new(&args[1]), Path::new(&args[2]))?;
            Ok(())
        }
        Some("replay") if args.len() == 2 => {
            let root = Path::new(&args[1]);
            let manifest: Manifest =
                serde_json::from_slice(&std::fs::read(root.join("manifest.json"))?)?;
            let series = replay_series(&root.join("recordings"), &manifest.bounds)?;
            println!("{}", serde_json::to_string(&series)?);
            Ok(())
        }
        _ => bail!("usage: mm-research run [config.json] [output-dir] | replay <run-directory>"),
    }
}

# Crypto Market Maker

Research-first Binance Spot market-making foundation. Initial research capital assumption: USD 5,000; no funds are connected. Profitability is unproven. Milestones 1–2 provide domain/exchange contracts, Binance Spot public market data, deterministic replay, a read-only control API, and PostgreSQL infrastructure. Milestone 4 adds a local-only baseline strategy simulator; no exchange order transport exists.

## Development

Requires Rust 1.90.0 (rustfmt and clippy), Node.js 22+, npm, Docker Engine and Compose v2.

```sh
npm ci --prefix control-api
cargo build --workspace --locked
bash scripts/check.sh
docker compose up -d --wait postgres
docker compose exec -T postgres psql -U marketmaker -d marketmaker -v ON_ERROR_STOP=1 < migrations/001_foundation.sql
docker compose exec -T postgres psql -U marketmaker -d marketmaker -v ON_ERROR_STOP=1 < scripts/db-test.sql
TRADING_MODE=paper npm --prefix control-api start
```

`GET /health` returns the configured mode and explicitly reports execution unavailable. Default mode is simulation. Any other mode, including live, prevents startup. API binds to loopback, default port 3001 (`PORT` override). No exchange keys required; no order endpoints exist. API health does not assert database connectivity.

Database credentials in Compose are public local-development defaults. Database port binds only to loopback. Do not reuse them for deployment. `docker compose down` stops infrastructure; do not add `-v` unless intentionally deleting local data.

Cloud Rust activation: export `CARGO_HOME=/workspace/.tools/cargo`, `RUSTUP_HOME=/workspace/.tools/rustup`, and prepend `$CARGO_HOME/bin` to `PATH`.

See [architecture](ARCHITECTURE.md), [roadmap](ROADMAP.md), [risk policy](RISK_POLICY.md), and [decisions](docs/DECISIONS.md). Next.js, forward paper execution, durable OMS and live execution remain future separately approved work.


## Binance public market data (Milestone 2)

After the build, start the collector from the repository root in its own managed terminal:

```sh
cargo build --workspace --locked
./target/debug/mm-market-data collect
```

In a second terminal, from `control-api/`:

```sh
npm run build
TRADING_MODE=paper npm start
```

The collector connects only to public market-data endpoints (`data-stream.binance.vision` and `data-api.binance.vision`) for BTCUSDT/ETHUSDT. No keys are required. Set `MARKET_DATA_DIR` or `MARKET_STATUS_FILE` to override local paths; use the same absolute status path in both processes if overriding. Default paths relative to the root are `.local/market-data/` and `.local/market-status.json`; API defaults assume its working directory is `control-api/`. WSS honors `HTTPS_PROXY`/`https_proxy` when set (unauthenticated HTTP CONNECT proxy); REST uses the same proxy. Normal certificate verification stays enabled. Do not run two collectors against the same recording directories.

Read-only API endpoints:

- `GET /markets`: both markets, synchronization status, connection-session IDs, string update IDs, exact decimal quotes/spread, known depth counts, top 20 levels, feed age and counters.
- `GET /markets/BTCUSDT` or `/markets/ETHUSDT`: one market.
- `GET /metrics`: Prometheus text for synchronization, reconnects, sync failures, gaps, duplicates, stale events, recording failures, lag and feed age.
- `GET /health`: API process health; `executionAvailable` and `liveTradingEnabled` remain false.

Only synchronized and fresh books expose quotes. Feed and collector freshness limits are 5 seconds. Missing/malformed status returns 503. `depth_complete` is always false: the initial snapshot covers only 5000 levels/side, and only changed levels are learned beyond it. Known levels are not full-market liquidity. Unsupported precision, invalid/crossed/empty books, exhausted snapshot coverage, sequence gaps and stale data trigger a full reconnect/rebuild.

Replay one segment or all retained segments (JSON output can be large):

```sh
./target/debug/mm-market-data replay .local/market-data > /tmp/market-replay.jsonl
```

Each output line summarizes a verified segment and its final book. Sequence IDs and application order, not wall-clock sorting, drive state within a segment. Each segment is independently replayable. A directory replay enumerates segments deterministically by filename; independent recording runs are not merged into a global chronological feed. See [M2 results](docs/MILESTONE_2.md) for executed tests and a verified live sample; see [environment instructions](docs/ENVIRONMENT.md) for publication steps.

### Default bounds and failure policy

Each symbol has a 64-message transport queue and 64-event snapshot buffer, 1 MiB maximum input message/snapshot, 10,000 known levels per side and a 2 MiB maximum encoded record. Parquet buffers rotate at 64 rows (including checkpoint), approximately 2 MiB target, or 2 seconds. A checkpoint/large record may exceed the target, but individual finalized segment files are capped at 8 MiB. There are at most 32 finalized segments per symbol (256 MiB), plus one bounded temporary segment/checksum and tiny lock/checksum files. Rotation deletes the oldest segments; this is a cache, not an archive. Hard input limits bound memory; an oversized record stops that symbol rather than silently losing recording coverage.

Every finalized segment is flushed, fsynced, renamed atomically and checked with SHA-256 on replay. A process crash can lose the unflushed tail (normally up to 2 seconds; slow storage can increase this). Shutdown attempts to flush buffered data. Disk-full/write errors clear the affected book and halt its collector; free space/fix permissions and restart to recover. Overflow means coverage is lost: pending frames may be discarded and a new session begins. The invalidation reason is recorded when storage is writable. The API never treats the last quote as usable after a failure. Fixed-size status output is atomically replaced every 250 ms; metrics are process-local and reset on restart.

M2 itself implements public data only. M3/M4 additions below remain research; order submission and live trading are unavailable.

## Market scanner and profitability research (Milestone 3)

Rust `mm-research` discovers active USDT Spot pairs from public `exchangeInfo` and 24-hour tickers, applies activity/tick/lot/notional constraints, and reuses the M2 collector, synchronized books and Parquet replay. The default universe is six pairs; `config/research.json` allows up to 20 after memory/storage checks. Symbols start four seconds apart, with a shared scanner snapshot budget of at most one 5000-level request per three seconds. No funds are assigned: the $5,000 capital and $100 clip are research assumptions.

From the repository root:

```sh
cargo build --workspace --locked
./target/debug/mm-research run config/research.json reports/milestone3
```

Default collection lasts 180 seconds; results are always preliminary. Do not infer executable edge from quoted spreads or the ranking. Fees are explicit manual assumptions, never account-verified. All requested reports, selected metadata and exclusions are under `reports/milestone3/`. Capture hashes/counts from the milestone run are in `capture_audit.json`; the raw bounded journal remains under `.local/research/run-<timestamp>/` and is not pushed.

Replay captured research observations without contacting Binance:

```sh
./target/debug/mm-research replay .local/research/run-<timestamp> > /tmp/replayed-research.json
./target/debug/mm-research summarize .local/research/run-<timestamp> reports/milestone3
```

`summarize` verifies persisted online observations against checksum-verified replay before recomputing analytics with the saved manifest's assumptions. It requires the original report generation metadata and local capture; it does not invent missing history. Truncated windows expose their actual durations and `requested_window_complete=false`. Comparisons between report generations must use identical raw inputs and explicit fee assumptions.

Additional read-only TypeScript routes: `/research/rankings` (JSON), `/research/rankings.csv`, `/research/summary` (Markdown), `/research/fees` (Markdown). They serve historical reports with timestamps, not live recommendations. `RESEARCH_REPORT_DIR` overrides the report directory; default is `../reports/milestone3` relative to `control-api/`. Restart an older API process to load new routes, or start a separate instance with `PORT=3003` if port 3001 is occupied. No research route accepts writes or orders.

Research memory is capped at 7,200 sampled book observations and one-second trade bins per symbol; inputs retain M2 frame/book bounds. Research recording uses the same recorder with up to 128 segments/symbol, at most 8 MiB each (1 GiB/symbol), a 2 MiB target, 4,096-row target, and normal two-second flush. Free storage must exceed the worst-case per-symbol reservation plus 1 GiB; memory must exceed 256 MiB per symbol plus reserve. Long/high-activity runs may evict early segments; a missing prefix, segment gap or replay mismatch disqualifies candidates. Completed runs are retained for audit rather than silently deleted; explicitly archive/remove old captures if the next resource check refuses to start. Public request rate limits remain per process, so avoid running many scanners at once on a shared IP.

See [M3 findings and limitations](docs/MILESTONE_3.md). M3 provides preliminary screening only; M4 offline simulation is described below. No live orders, capital allocation or private API credentials are available.

## Execution backtesting (Milestone 4)

`mm-backtest` reuses checksum-verified M2/M3 raw books and aggregate trades. All orders, fills, fees and P&L are local simulations; public price touch alone never fills an order. See [M4 assumptions and findings](docs/MILESTONE_4.md) and [preliminary results](reports/milestone4/summary.md). The short sample produced no fills or demonstrated edge. No account fees or promotional eligibility have been verified.

```sh
cargo build --workspace --locked
./target/debug/mm-backtest audit .local reports/milestone4/all_recordings_audit.json
./target/debug/mm-backtest run .local/research/run-1792248889904 config/backtest.json reports/milestone4
# Public-only 24-hour target, configurable to 72 hours, two markets by default:
./target/debug/mm-backtest collect config/long_collection.json
# Optional connectivity smoke test (not a completed long-horizon study):
./target/debug/mm-backtest collect config/long_collection.json --smoke-seconds 15
```

Raw captures are local/ignored and required for historical reproduction. Long capture needs a continuously running environment and disk headroom. Default retention is 1 GiB per symbol plus bounded temporary/checksum files, so active markets can evict the beginning before 24 hours. Check status/quality and archive deliberately. Existing complete-prefix research replay rejects evicted history. Backtest tape/trace caps also reject overly large inputs; multi-day evaluation requires bounded chronological blocks with explicit boundary treatment or a future streaming report runner. Never present retained fragments as continuous multi-day validation.

Final test execution is disabled by default; `--include-final-test` is a deliberate evaluation action only after strategy/configuration freeze and sufficient untouched observations. Current embargoed validation/test intervals are empty and are not validation. Stop after M4; M5 needs explicit approval.

## Execution diagnostics and data readiness (Milestone 4.1)

The original 54 preliminary simulations are preserved. Diagnostics explain each quote gate and no-fill outcome, without selecting parameters or claiming that zero fills prove absence of an edge. See [M4.1 results](docs/MILESTONE_4_1.md) and [machine-readable diagnostics](reports/milestone4_1/execution_diagnostics.json).

```sh
./target/debug/mm-backtest diagnose .local/research/run-1792248889904 config/backtest.json reports/milestone4/backtest_results.json reports/milestone4_1
./target/debug/mm-backtest readiness .local/long-captures/session-1792251264110 reports/milestone4_1/data_readiness.json
```

`diagnose` requires the original source and frozen configuration, evaluates only previously evaluated partitions, and writes a separate output directory. `readiness` freezes a list of finalized files and streams two checksum/sequence/book replays; it neither runs a strategy nor waits for a capture to finish. New files and the unflushed tail are outside that audit. Both commands have bounded file/event/report resources and fail explicitly on missing data. `max_quote_spread_bps` is an optional strategy width cap (default 1000bps); fees plus the required edge exceeding it reject quotes. This adds an explicit fee-feasibility guard without changing the frozen M4 experiment. Queue position remains an estimate and exact-price fills still omit trade-through activity, now reported explicitly. Stop after M4.1; do not start M5 without approval.

## Funded portfolios and extended offline research (Milestone 4.2)

See [M4.2 implementation and verified findings](docs/MILESTONE_4_2.md). `config/backtest_funded.json` compares base allocations of 0/20/50/80% and exact, conservative-through and alternative-through execution. Each reported market is explicitly an isolated $5,000 alternative; never sum their capital or P&L. Optional partitioned mode validates market budgets summing to one $5,000 portfolio. Initial base uses the first causal valid mid with lot rounding and quote residue; no executed conversion is claimed. Reservations include fee assets and a fee buffer, and inventory limits/skew apply around the declared initial target.

```sh
cargo build --workspace --locked
# Fetch and freeze current public filters; no private/account commission endpoint:
./target/debug/mm-backtest metadata .local/research/run-1792248889904/manifest.json /tmp/current-exchange-metadata.json
# Use the committed frozen metadata/config to reproduce historical comparisons:
./target/debug/mm-backtest batch config/backtest_funded.json /tmp/funded-research
# Reproduce the recorded ZEC order regression; this is not a strategy-return estimate:
./target/debug/mm-backtest case-audit .local/research/run-1792248889904 reports/milestone4_1/execution_diagnostics.json /tmp/zec-execution-case
# Optional future campaign only; first inspect existing running collectors:
./target/debug/mm-backtest collect-campaign config/collection_campaign.json
```

The batch runner streams source events and external traces instead of retaining a full-depth tape. It rejects incomplete captures, corrupt/fragmented/gapped session-symbols and missing tails; the whole affected session-symbol is excluded. It never fills across unknown intervals. Output must be new/empty, has a trace byte budget, and remains `INCOMPLETE` until `batch_complete.json` exists. Research remains chronological training-only, with no final OOS evaluation or optimization. Legacy `run`/`diagnose` retain their finite tape caps.

Campaign resume creates new immutable session directories, bounded by session count, per-symbol bytes, campaign total bytes and free-disk checks. It does not reopen old sessions or automatically take over an unfinished directory. Reconcile unclean sessions explicitly; do not point campaign configuration at an existing capture. The current 24-hour-target collector has been preserved, not restarted; its current observed status and audit windows are in the report. No complete 24-hour dataset or profitable strategy is claimed. Stop after M4.2; M5 needs explicit approval.

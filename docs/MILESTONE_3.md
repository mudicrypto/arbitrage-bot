# Milestone 3 — Binance Market Scanner & Profitability Research

Completed research implementation and preliminary public-data validation on 2026-10-17. This milestone follows the user's scanner/research scope, superseding the earlier proposed simulation milestone. M4 is not approved or started.

## Architecture and implemented features

- Added Rust `mm-research`: public discovery, metadata eligibility, exact fee arithmetic, bounded observation statistics, conservative economic screening, deterministic rankings and reports.
- Reused M2 `run_symbol`, book synchronization/recovery, recorder/checksums and replay state application. Added optional combined depth/aggregate-trade subscription, typed `Trade`, observer callbacks and `replay_visit`. The default M2 BTC/ETH collector, API freshness checks, PostgreSQL schema and simulation/paper restrictions remain intact.
- Discovery uses public `exchangeInfo` and 24-hour tickers. Require active/tradable USDT Spot, at least $5M quote volume and 20,000 trades/day, supported integer precision, tick <=10bps, a feasible ~$100 lot-aligned clip and minimum order <=$50 for the $5,000 capital assumption. Metadata, filters, selected symbols and discovery exclusions are saved.
- Post-observation screens reject poor coverage, trade sequence gaps, missing markout history, insufficient two-sided known 10bps depth and abnormal single-level concentration. Default thresholds: >=85% occupied observation seconds, >=$1,000 known depth on each side within 10bps, p90 concentration <=85% of combined near-mid depth. These are configurable research screens, not trading permissions.
- Default six symbols; hard cap 20, duration 30–3600s with enough time after staggered startup. Current startup is four seconds apart with a shared snapshot budget of one request/three seconds, in addition to M2 per-symbol throttling and 418/429 cooldown. The primary run used the initial three-second stagger; exact per-symbol bounds are recorded. A later 40-second two-pair smoke run validated the final shared-budget path.
- Statistics: spread p10/p50/p90 in bps, known depth within 5/10/25/50bps, near-mid imbalance and concentration, robust one-second volatility, quote stability, reference-price movement, actual public trade quote volume/frequency, 1/5/15s passive-counterparty markouts and approximate synchronization coverage. Requested 30/60/180/600s windows expose actual observed durations and an explicit truncation flag; absent history is never presented as observed longer history.
- Samples: first valid applied book per receive-time second; trade bins use exchange-time seconds and retain aggregate IDs/underlying trade counts. At most 7,200 samples and trade bins/symbol. Research uses the existing bounded recorder with 128 segments/symbol, <=8MiB each, 2MiB/4096-row targets and normal two-second flushing. Free-resource reservations are checked before collection. Old completed runs are not silently deleted; future runs refuse to start when the reservation cannot be met.
- Replay enforces checksums, record order, complete prefix and segment continuity. The same observer reconstructs research inputs. Persisted observations must equal replayed observations; missing/evicted history disqualifies candidates. Exact JSON float-roundtrip support fixes a detected one-ULP deserialization discrepancy without altering market observations.
- TypeScript exposes historical reports read-only: `/research/rankings`, `/research/rankings.csv`, `/research/summary`, `/research/fees`. Files are bounded; malformed/missing data return 503. `/health`, M2 routes and order rejection remain unchanged. Latest API/report routes were verified locally on port 3003 because an existing process occupied 3001.

## Fee and economic assumptions

No private credentials were requested and `/api/v3/account/commission` was not called. Reviewed the official [commission FAQ](https://github.com/binance/binance-spot-api-docs/blob/master/faqs/commission_faq.md), [public market endpoints](https://github.com/binance/binance-spot-api-docs/blob/master/rest-api.md), and [aggregate trade streams](https://github.com/binance/binance-spot-api-docs/blob/master/web-socket-streams.md). Manual assumptions are explicitly unverified:

| Scenario | Maker buy/sell | Taker | Qualification |
|---|---:|---:|---|
| `manual_standard` | 10bps each (20bps roundtrip) | 10bps | Conservative manual hypothesis, not verified account rates |
| `hypothetical_bnb_discount` | 7.5bps each (15bps roundtrip) | 7.5bps | Assumed 25% discount on standard components only; entitlement unverified |
| `hypothetical_zero_maker` | 0bps each | 10bps | Hypothetical promotional scenario; no eligibility asserted |

Fee schedules support symbol overrides, historical effective intervals, separate maker/taker and standard buyer/seller components, non-discounted extra tax/special components, and different buy/sell fee assets. Charge arithmetic uses checked u128 products and ceiling division before any lossy notional truncation. Prices, quantities and asset charges use 1e-8 units; these are research accounting assumptions, not verified live settlement precision. BNB conversion assumes 1,000 USDT/BNB solely to test fee-asset valuation; this is **not an observed BNB price**. No rebate is modeled.

Screening formula: median quoted spread − worst applicable manual maker buy/sell fees − max(nonnegative observed passive-markout mean across horizons, adverse floor) − 2bps inventory allowance − 10% × (taker fee + 1bp rebalance slippage) − 2bps safety margin. Adverse floors are 0/1/3/10bps; baseline uses the 3bps floor. Baseline cost is therefore at least 28.1bps before gross spread. Cross-window p10 spread gives the conservative stability floor. Ranking score is this floor minus one-second volatility, data-quality penalty and unavailable quoted-clip penalty, with deterministic symbol tie-breaking. A positive score/rank is never an execution authorization.

## Real observation sample sizes

Primary run: **180 seconds**, 2026-10-17 **17:54:49.904–17:57:49.904 Europe/Moscow** (Unix 1792248889904–1792249069904ms). Staggered startup gives approximately 165–180 seconds per symbol. Actual inputs, bounds, checksums and counts are in [universe.json](../reports/milestone3/universe.json) and [capture_audit.json](../reports/milestone3/capture_audit.json).

| Pair | Valid book samples | Aggregate messages | Underlying trades | Approx. observation coverage |
|---|---:|---:|---:|---:|
| USDCUSDT | 178 | 548 | 568 | 98.9% |
| BTCUSDT | 176 | 743 | 1,877 | 99.4% |
| ETHUSDT | 173 | 204 | 1,057 | 99.4% |
| USD1USDT | 146 | 154 | 223 | 85.4% |
| SOLUSDT | 167 | 151 | 307 | 99.4% |
| ZECUSDT | 164 | 403 | 480 | 99.4% |

Total: **1,004** synchronized book samples; **2,203** aggregate messages representing **4,512** underlying trades; **454** checksum-verified Parquet segments totaling **29,063,084 bytes**. No aggregate-trade sequence gaps were observed. Online inputs and reconstructed replay inputs matched exactly. Raw captures remain local under `.local/research/run-1792248889904/`; their hashes are committed, but raw data are not remotely archived by Git.

The final shared-budget smoke run observed two real public pairs for 40 seconds, producing 74 synchronized samples with exact online/replay consistency. These supplemental observations do not replace or extend the primary report's 180-second sample.

## Actual preliminary findings

**No pair has a plausible positive edge after baseline fees and conservative costs. No winning candidate is manufactured.** All hypothetical fee/adverse-floor combinations in this sample also remain negative, including the zero-maker-fee scenario. This finding applies to these assumptions and this short sample; it is neither a proof of universal unprofitability nor a validated trading result.

| Research priority | Pair | Median quoted spread, bps | Baseline theoretical net, bps |
|---:|---|---:|---:|
| 1 | SOLUSDT | 0.9580 | -27.1420 |
| 2 | ETHUSDT | 0.0412 | -28.0588 |
| 3 | BTCUSDT | 0.0012 | -28.0988 |
| 4 | USDCUSDT | 0.0999 | -28.0001 |
| 5 | ZECUSDT | 0.0859 | -28.0141 |
| 6 | USD1USDT | 0.1000 | -28.0000 |

SOL's wider median spread still does not remotely offset standard fees. The score also penalizes volatile/unstable spreads and poor coverage; hence the order is not simply spread or volume order. USD1 is last because coverage is materially worse. Each row's exact score, liquidity, assumptions, exclusion list and explanation appear in the machine-readable report. None is marked `plausible_positive_edge` or `executable_edge_validated`.

Outputs:

- [market_rankings.json](../reports/milestone3/market_rankings.json)
- [market_rankings.csv](../reports/milestone3/market_rankings.csv)
- [research_summary.md](../reports/milestone3/research_summary.md)
- [fee_sensitivity.md](../reports/milestone3/fee_sensitivity.md)

## Executed checks

- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo test --workspace --locked`: **34 passed, 0 failed/ignored**, including all 20 M2 tests and 14 research tests; no doc tests.
- `cargo build --workspace --locked`: passed.
- TypeScript Prettier check, typecheck and build: passed. `npm --prefix control-api test`: **7 passed, 0 failed/skipped**.
- PostgreSQL Compose readiness, idempotent migration and `scripts/db-test.sql` with `ON_ERROR_STOP=1`: passed; live mode, duplicate order IDs and zero prices remain rejected.
- Live public discovery/collection, real report API responses, Parquet checksums, online/replay consistency, JSON persistence and report regeneration: verified.

Research tests cover bps/spread/depth/imbalance, fixed-point arithmetic properties, fee assets/side components/discount rounding, historical/symbol fee overrides, eligibility, invalid/stale/missing history, deterministic ranks, wide-spread outliers/concentration, truncated windows/trade gaps, Parquet observer consistency, exact persisted statistics, and zero/negative edge. Existing M2 tests retain book/reconnect/buffering/recording fault coverage. Remote GitHub Actions execution is not independently claimed.

## Run and reproduce

```sh
# From repository root, after the README cloud Rust activation:
cargo build --workspace --locked
./target/debug/mm-research run config/research.json reports/milestone3
./target/debug/mm-research replay .local/research/run-1792248889904 > /tmp/research-replay.json
./target/debug/mm-research summarize .local/research/run-1792248889904 reports/milestone3
```

Replay/summarize require the original local capture; hashes alone cannot recreate historical observations. The default config is explicit and reproducible, but a new live run observes a different market and replaces output reports. Run API from `control-api/` with `TRADING_MODE=paper npm start`, using `PORT` if needed. No API keys or order endpoints are involved.

## Limitations, risks and improvements

- Three minutes in one market session is insufficient for statistical significance, regime coverage or profitability validation. 180/600s horizons can be truncated; sample/markout tails are incomplete and explicitly counted. No multiple-testing-adjusted significance claim is made.
- One-second first-update sampling and second-binned aggregate trades omit queue changes, cancellations, subsecond ordering and exact own-fill conditioning. Passive-counterparty markouts are signed, equally weighted bin proxies. No touch-based fills, simulated executions, turnover forecasts or daily P&L are produced.
- Volatility is one-second RMS return with 6-MAD winsorization and a 1bp minimum clipping band. It resists isolated outliers but can understate extreme risk; raw data remain unchanged. Coverage is occupied valid seconds, not continuous synchronization uptime. There is no uptime confidence interval or long soak claim.
- Depth sums cover known reconstructed levels only; finite snapshot boundaries can understate wide bands. Displayed liquidity is cancellable and is not guaranteed executable liquidity. Concentration and stability are basic screens, not manipulation detection. No claim of full-market depth is made.
- Fee entitlements and all inventory/rebalance/safety assumptions are uncertain. BNB conversion is manual. No live commission accounting precision, tax treatment, actual hedge path, exchange filter refresh or strategy risk system is validated.
- Journal limits can evict history during long or unusually active runs; missing prefix/segment history fails consistency and disqualifies candidates. Resource reservations are conservative, not latency guarantees. Snapshot budget is shared within the scanner only; other processes sharing an IP still consume venue limits. Captures can accumulate across explicit runs until resource checks refuse another run; archive/remove them deliberately.
- Raw captures are local/ignored. Committed reports and checksums do not provide remote historical-data recovery. Cloud publication and fresh-task restoration remain manual/unverified as documented in ENVIRONMENT.md. Historical API reports must not be treated as live signals. API remains loopback-only and unauthenticated.

## Milestone 4 readiness

The software foundation supports broader, longer research and replay-backed experiments. It is **not ready for live trading or a profitability claim**. Proposed M4 scope: agree on longer multi-session collection, out-of-sample evaluation, fee verification workflow without exposing credentials, stronger depth/quality checks, and queue/latency-aware simulation design with independent risk and accounting requirements. The current findings provide no reason to allocate capital. Stop here until explicit M4 approval.

# Milestone 4 — Realistic execution backtesting and trading economics

Implemented on 2026-10-17 after explicit approval, starting from clean pushed M3 commit `525579710499096af105e5a9cef52a2149609b67`. This is a **preliminary simulation milestone**, not evidence of profitable execution. No trading, private API endpoints, credentials, borrowing or capital allocation were used. Stop before M5 until explicitly approved.

## Functionality and architecture

Added Rust `mm-backtest` with `audit`, `run` and `collect` commands. It reuses M2 book reconstruction, transport, checksum-verified Parquet `replay_visit`, and M3 instrument filters, checked fixed-point arithmetic and time-versioned fee engine. TypeScript endpoints and PostgreSQL schema are unchanged. No exchange write transport exists.

The deterministic event loop models entry latency, jitter with a fixed seed, post-only rejection on arrival, queue joining, exact-price aggressive-trade depletion, partial fills, cancellation deadlines and delayed pre-cancel reports. Trade events win cancellation ties conservatively; a trade at the entry timestamp is not attributed to the new order. Reservations survive the configured post-cancel report grace. Late reports beyond the budget invalidate coverage rather than invent fills. Source application order breaks timestamp ties. Buffered receive times are clamped forward to causal availability; books are never sorted backward into an earlier decision.

The baseline supports fixed or volatility-adjusted spreads, inventory skew, tick-based refresh thresholds, position limits, fee-aware minimum width, stale/invalid/disconnect/spread pauses and drawdown stops. Volatility adjustment is a transparent integer EWMA of book-to-book absolute bps changes, not calibrated volatility forecasting. No machine learning or outcome-driven optimization was introduced.

Cash, base and BNB balances and outstanding reservations reconcile to the per-asset ledger after every transition. Prices/quantities use 1e-8 units and instrument tick/step/min-max quantity/notional constraints. Buy quote fees, sell base fees and BNB fees reserve actual assets; fees use trade execution time, not receipt time. No shorting or negative available balances is allowed. Unsupported BNB-as-base/USDT-as-base wallet aliases reject rather than silently double count.

## Audit of existing data

[All-recording audit](../reports/milestone4/all_recordings_audit.json) inspected the retained M2 journals and both M3 research runs. [Selected-data audit](../reports/milestone4/selected_data_audit.json) checks the primary source in detail.

| Dataset | Retained segments | Applied book updates | Aggregate trades | Coverage assessment |
|---|---:|---:|---:|---|
| M2 BTCUSDT rolling cache | 32 | 719 | 0 | Initial prefix already evicted; no trade-evidence fills |
| M2 ETHUSDT rolling cache | 32 enumerated | 679 read | 0 | Prefix evicted; one file disappeared during active retention |
| Primary M3, six markets | 454 | 6,950 | 2,203 | Complete prefix, checked segments, no aggregate-ID gaps |
| M3 two-market smoke | 32 | 583 | 294 | Complete but only 40 seconds |

The M2 missing file is a moving-cache read limitation, not proof of corrupted bytes. Each finalized surviving segment has its own replay checkpoint, allowing local book reconstruction; neither M2 retained directory supplies continuous original history. M3's primary raw payload totals 29,063,084 bytes and its 2,203 aggregate messages represent 4,512 underlying trades. The older one-second M3 statistics contain only 1,004 sampled books; M4 uses all 6,950 applied updates from the underlying recorder instead.

| Capability | What is supported | What remains unknown |
|---|---|---|
| Top of book | Local reconstructed quotes from snapshots and checked deltas | Exchange-to-client latency and omitted/crash intervals |
| Multi-level depth | Known levels, up to 10,000/side, finite snapshot coverage | Full depth outside snapshot boundaries |
| Aggressor direction | M3 aggregate `buyer_is_maker` classification | Individual matches and intra-aggregate timing |
| Queue approximation | Visible size ahead at modeled entry, subsequent aggressive trades | Order IDs, exact priority, hidden liquidity, cancel location |
| Partial fills | Evidence-conditioned modeled depletion | Actual hypothetical order execution |
| Adverse selection | Signed observed future reference prices after modeled fills | Missing horizons and unobserved regimes |

Primary history spans 180 seconds (Unix 1792248889904–1792249069904ms), with per-symbol staggered starts. BTC/ETH/SOL/ZEC/USDC respectively have 9/7/2/7/1 causally clamped buffered records; USD1 has none. No primary recording invalidations or aggregate-ID gaps were found. Simulation can still detect stale intervals: USD1's sparse updates and deliberately interrupted scenarios explain the 14/54 results with failed modeled data quality. Short history cannot establish synchronization reliability over a day.

## Longer public collection

`config/long_collection.json` targets **86,400 seconds** for BTCUSDT and ETHUSDT. Targets of 24–72 hours and 1–4 eligible USDT symbols are supported. Existing combined depth/aggregate-trade collection retains raw messages, source/connection IDs, sequences, manifest filters, public metadata, current quality counters and completion metrics. Snapshot requests share the existing budget and symbols start four seconds apart.

A real 24-hour-target public session was started at Unix **1792251264110ms**, directory `.local/long-captures/session-1792251264110`. The committed [status evidence](../reports/milestone4/long_collection_status.json) records **564 elapsed seconds**, BTC **5,615** and ETH **5,461** applied updates, both synchronized, zero reconnects/data gaps/recording failures at that checkpoint. This is an **ongoing capture**, not a completed day or validated long-horizon study. No new strategy outcomes were evaluated on it. Processes may stop with the cloud environment; continuation is not guaranteed.

Default budget is 1 GiB finalized payload per symbol, max 10,000 segments, <=8 MiB individual file, bounded temporary/checksum overhead and a 30-second flush interval. Preflight reserves both symbol budgets plus 1 GiB free space. Rotation can evict a prefix before the target duration; a rolling journal does not guarantee a complete 24-hour archive. Recording failures halt the affected symbol. Completion/interrupt writes actual bounds and quality metrics; aborted tasks/crashes can lose the buffered tail (up to the flush interval or more under slow storage). Old session directories are not automatically removed, and repeated captures can exhaust disk until preflight refuses another run. Archive deliberately.

## Simulation assumptions and experiment design

[Configuration](../config/backtest.json) is copied into reports with SHA-256 `38887de25287f3556e2e422f0d7f95cfb817f48c77e1fe7fdc7049bb8eb9c21b`. Each market independently starts with **$5,000 cash and no base inventory**; results are not a simultaneously funded portfolio. Clip is $100, position cap $500, drawdown stop $50, refresh 2 seconds/2 ticks, stale threshold 2 seconds, maximum observed spread 50bps and inventory skew 10bps. Initial strategy width 4bps is widened to at least two-sided maker fees plus 2bps; quote rounding is outward to venue ticks. No invented opening base stock supplies asks.

Conservative queue model: join behind visible quantity at the quote price, reduce queue only with correctly directed, exact-price public trades after eligible entry, then fill the remaining modeled order. Touch, crossed marks, depth cancellation and trades at other prices never fill it. Ignoring trade-through evidence can understate fills. Alternative proportional-cancellation advancement reduces modeled queue ahead when public size falls; it is optimistic because depth reductions cannot identify cancellations ahead and may overlap trade effects. It never directly creates a fill. Neither model is a guaranteed lower/upper bound on actual execution.

Nine predeclared scenarios cover standard fees, cancellation advancement, hypothetical discount, hypothetical zero-maker, hypothetical rebate, worse-cost/latency/fill stress, five-second feed interruptions every 30 seconds, volatility-adjusted spread and 30bps width/$200 inventory cap. Standard entry/cancel/report budgets are 50/100/500ms with <=10ms seeded jitter. Stress doubles initial queue, halves participation, uses 500ms entry latency, 10bps estimated closeout slippage and 50bps adverse terminal inventory markdown. Shock is a hypothetical valuation stress, not a fabricated market observation or terminal fill. Synthetic quiet/trend/shock fixtures test accounting and risk behavior only, and never enter research findings.

Manual standard fees are **10bps maker each side and 10bps taker**. Discount assumes 25% off eligible standard fees, paid in BNB at a manual 1,000 USDT/BNB conversion, with $25 of the initial cash reclassified into BNB for fee funding. That is neither an observed BNB price nor free extra capital. Zero-maker and 1bp maker-rebate cases are hypothetical; no $5,000 account entitlement or available tier is asserted. A documented verified-promotion mode requires a verification reference; none is configured. Existing historical/symbol fee schedules and explicit assets remain supported. All current account-access verification flags are false.

Chronological splits are 50% train, 25% validation and 25% test with a 60-second embargo between partitions. [Split manifest](../reports/milestone4/split_manifest.json) records exact boundaries. Here train is only approximately 82–90 seconds; both later intervals are **empty after embargo**. Final-test evaluation is disabled and was not invoked. The entire older M3 run was already examined in M3, so its tail cannot retroactively be called pristine OOS. Future observations must remain untouched during development, followed by configuration freeze and one explicit final evaluation. No parameter winner was selected from this sample.

## Trading economics and actual findings

All executions/P&L below are **simulated on observed public data**, with hypothetical cases identified separately. Generated CSV rows and JSONL execution/ledger/equity rows identify simulation; audit/status files describe observed capture provenance.

**No positive expectancy or executable edge was demonstrated.** Across six markets × nine scenarios, **54 train simulations produced zero fills, zero turnover, zero fees/rebates, zero inventory exposure, zero P&L and zero drawdown**. No-trade also returns zero. P&L per $1M is null because executed notional is zero; 1/5/30/60-second fill markouts have zero completed observations and null values, not manufactured zero adverse selection.

There were 305 modeled submissions, 304 acceptances, 237 effective cancellations and 45 orders still awaiting modeled deadlines/report grace at the dataset boundaries, summed over independent scenarios. Boundary orders are explicitly unresolved and cannot qualify a positive result. Forty summaries retained modeled data quality and fourteen did not. Data-quality success alone does not imply enough data, fills or validation.

The ordinary baseline accepted 1/2/3/6/2/19 quotes respectively for USDC/BTC/ETH/USD1/SOL/ZEC, without a fill. Widths needed to cover two-sided standard fees lie far outside the very narrow quoted market in this sample; neither visible queue depletion nor exact-price trade evidence justified execution at our selected prices. Hypothetical zero/discount/rebate cases also did not produce fills. The short window, zero starting base, sparse eligible trade activity, exact-price rule, queue uncertainty and risk pauses limit inference. This does not prove all future market-making is unprofitable, nor support weakening fill assumptions to manufacture a winner.

Accounting identities: net MTM P&L = reference-relative gross capture + inventory P&L − maker fees + rebates. Inventory is the reconciled residual, so favorable reference movements are explicit. Markout adverse cost is a signed attribution diagnostic already reflected in inventory P&L, not an additional subtraction. Estimated terminal closeout separately charges bid-vs-mid, configured stress slippage/markdown and taker fee, without inventing an actual liquidation. A positive flag requires turnover, valid coverage, no unresolved orders, positive closeout-adjusted P&L and positivity after removing favorable inventory P&L. Missing/invalid inputs fail that flag.

Reports:

- [Backtest results](../reports/milestone4/backtest_results.json), [CSV](../reports/milestone4/results.csv), [summary](../reports/milestone4/summary.md).
- [Daily P&L/drawdown](../reports/milestone4/daily_pnl.csv), [fee/slippage attribution](../reports/milestone4/attribution.csv), [no-trade comparison](../reports/milestone4/strategy_vs_no_trade.csv), [queue sensitivity](../reports/milestone4/queue_sensitivity.md).
- `reports/milestone4/executions/`, `ledgers/`, `equity/`: complete bounded modeled action/asset/equity logs for every evaluated experiment. No fill log entry exists in the actual short-data run. Fixture executions are tests, not observed executions.
- [Frozen experiment](../reports/milestone4/experiment.json), audits, split manifest and observed long-capture status.

Daily results use Europe/Moscow and explicitly label partial days. No daily extrapolation, annualization, statistical significance or accessible fee-tier claim is made.

## Exact verification

After cloud Rust PATH activation (docs/ENVIRONMENT.md):

- `bash scripts/check.sh`: passed; includes `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, TypeScript Prettier, typecheck and tests.
- Rust: **53 passed, 0 failed/ignored**: 34 existing + 19 new (17 execution/property/fault tests and 2 collection/byte-retention tests). No doc-test failures.
- TypeScript: **7 passed, 0 failed/skipped**. API continues rejecting unsupported trading modes/order submission.
- `cargo build --workspace --locked` and `npm --prefix control-api run build`: passed.
- PostgreSQL: `docker compose up -d --wait postgres` healthy; idempotent `migrations/001_foundation.sql` and `scripts/db-test.sql` via `psql -v ON_ERROR_STOP=1` passed. Existing live-mode, order-ID and price constraints remain enforced.
- After adding explicit simulation labels and a terminal-freshness guard, the complete checks and builds passed again; primary report generation completed successfully with the final binary.
- Repeated report generation to `/tmp/m4-reproduction`: full result JSON and all execution/ledger/equity directories matched byte-for-byte (`cmp` / `diff -qr`), including the final terminal-freshness guard.
- Real public long-session WebSocket/snapshot synchronization and recording verified through status evidence. No 24-hour soak result claimed. Remote CI execution and cloud restoration were not independently verified.

New automated coverage includes event ordering; repeated nonempty simulations with identical full results, logs and ledger; queue/partial-fill evidence; wrong-price/touch/deletion non-fills; post-only/entry and cancellation/report races; execution-time historical fees and causal reference prices; quote/base/BNB assets; conservation/reconciliation; precision/rounding and arbitrary-size property tests; cash/position reservations; stale/disconnect/late-report invalidation and stale terminal-reference rejection; drawdown stop; markout horizons/missing history/coverage epochs; favorable-inventory-only rejection; synthetic regime faults; and retained-byte bounds across recorder restarts. Existing M2 REST/WebSocket interruption/recovery tests and M3 replay/fee/filter/property tests remain passing.

## Reproduce and operate

```sh
cargo build --workspace --locked
./target/debug/mm-backtest audit .local /tmp/all-data-audit.json
./target/debug/mm-backtest run .local/research/run-1792248889904 config/backtest.json /tmp/m4-reproduction
./target/debug/mm-backtest collect config/long_collection.json
# Optional 10–120-second public connectivity check, not a long study:
./target/debug/mm-backtest collect config/long_collection.json --smoke-seconds 15
```

Check an existing collection before launching another. Keep the process/environment running for the requested duration. `status.json` is atomic current status; `collection_finished.json` records actual completion/interruption, and `quality_metrics.json` is written on orderly finish. Backtesting refuses an active/unclean long run. Keep raw recordings/manifests together; Git reports/checksums alone cannot reproduce missing observations. Startup configuration was saved as a draft without new domains/secrets; review/save and Publish in environment settings remain manual and unverified.

## Remaining uncertainty and economic assessment

The collector can run longer, but the current offline report runner stores a **bounded full-depth tape (512 MiB / 1 million events)**, at most 100,000 equity/reference points and bounded action logs. It fails explicitly on oversized inputs. It does **not** yet implement automatic streaming multi-day experiments or carry state across automatically split blocks. Long-capture storage is rolling rather than guaranteed archival; a retained prefix failure invalidates whole-run conclusions. Multi-day validation needs archival policy plus a bounded streaming analysis runner or deliberate chronological blocks with explicit capital/queue/markout boundary treatment. Do not simply restart $5,000 in each block and present the aggregate as one portfolio.

Public aggregate timing, hidden priority, cancel placement, own market impact, unobserved in-flight fills, venue fee precision and real account eligibility remain uncertain. Finite snapshots omit distant liquidity; zero visible quantity is not proof of no hidden queue. The fee-guard spread is not a complete adverse-selection model. Terminal liquidation is an estimate; in-process ledger and simulated stops are not durable production risk/reconciliation. Final MTM can use the last known reference, but terminal staleness fails the data-quality/profitability flag. Missing coverage and unresolved orders cannot be treated as executable liquidation. No production OMS, private adapter or paper service was introduced.

Further **measurement and simulator validation** are justified to reduce these uncertainties. Expensive strategy optimization, machine learning, capital deployment or live access are **not economically justified by current evidence**. First secure longer complete multi-session history, implement scalable bounded evaluation, freeze hypotheses, evaluate genuinely untouched periods, and obtain a documented fee-eligibility workflow without sharing credentials. Assess stability across regimes with fill counts and uncertainty intervals, not just net-sign selection. M5 is not started and requires explicit approval.

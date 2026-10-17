# Milestone 4.1 — Execution diagnostics, fill-model validation and data readiness

Completed against accepted M4 commit `b39a0dca436d7d07546f5637da4ae622a795338a`, with the $5,000 cash-only capital assumption. This milestone explains and validates engineering behavior; **profitability validation remains incomplete**. No credentials, private endpoints, real orders, leverage or capital allocation were introduced. M5 has not started.

## What changed

- Added `mm-backtest diagnose` to reproduce only the 54 previously evaluated partitions using their frozen M4 configuration/source. It writes separate reports, verifies the original configuration SHA-256, and refuses newly evaluated final-test partitions. Original M4 reports are preserved.
- Added bounded quote/intent decisions, rejection reasons, signed distance from current bid/ask, fee/spread feasibility, order lifetime/censoring, queue estimates, eligible trade volume, fill fractions and per-order no-fill explanations. Counters distinguish fee-driven widening from fee rejection. Diagnostics never create fills.
- Corrected multi-order FIFO: queue ahead is a cumulative position including earlier own quantities. Each observed trade advances positions once, and one shared fill budget caps total own fills at eligible observed volume. The earlier implementation could consume the same public queue twice for two same-price own orders. Proportional cancellation credit now preserves own-order FIFO; effective cancellations release that own quantity from following positions. Ambiguous delayed pre-cancel reports across multiple own orders invalidate the experiment instead of inventing priority.
- Reject duplicate/old aggregate activity for execution, as well as invalid symbol/zero trade dependencies. Added optional `max_quote_spread_bps` (default 1000bps) so fees plus required edge exceeding a configured width cap explicitly prevent placement. The original experiment/configuration is unchanged; this guard is inactive in all 54 historical runs.
- Added streaming `mm-backtest readiness` over a frozen finalized-file list. It reuses the existing checksum/sequence/book replay twice, reports ordering/coverage/gaps/disk usage and compares digests. It never runs a strategy or waits for the capture target.

Rust retains connectivity, books, recorder, deterministic replay, fees and simulation. TypeScript endpoints, database schema and trading-mode restrictions are unchanged. Additional diagnostic state has explicit order/decision bounds; missing source files or oversized experiments fail rather than fabricate observations.

## Definitions and counting

A quote opportunity is **one side** at a refresh-eligible valid-book strategy evaluation. Existing quotes still open/canceling are counted as retained; a new submit attempt is a generated intent. Each intent has one gate outcome. For this historical diagnosis:

`opportunities = retained/canceling + generated intents`

`intents = fee rejections + risk rejections + filter rejections + data rejections + submissions`

Fee widening, execution arrival rejection, trade reach, latency, cancellation and data-coverage flags are distinct from this funnel. They can overlap and must not be summed as disjoint causes. Feed pauses before intent generation are recorded as dependency failures, not invented rejected intents. Quotes start with zero base inventory; unavailable asks are an explicit inventory-risk rejection, not a fee or simulator failure.

Each generated/retained decision records the then-known bid/ask, proposed price/quantity, signed same-side distance in microbps, observed quoted spread, configured roundtrip fees and reason. Positive distance means passive beyond the same-side best; negative means improvement. One microbp is 1e-6 bps. Fee/spread summaries sample these refresh decisions, not every raw book update. Price/quantity and fee amounts retain existing fixed-point scale 1e8.

Order explanations include entry/cancel deadlines, acceptance/end state, estimated initial queue ahead, active lifetime, right-censoring, exact-price eligible volume, queue-position advancement and filled quantity. Position advancement and eligible quantities are per-order diagnostics; overlapping orders can reference the same public trade, so those amounts must not be summed as unique exchange volume. Lifetime quantiles use index `floor((n−1)p)` and remain descriptive for tiny/censored samples. Empirical filled-order fraction is reported; **no calibrated fill probability is estimated** from this short MBP sample.

An exact-price directional match after cancellation is a **retrospective counterfactual**, not proof the order would have filled. Live trade-through activity is counted separately because the current fill model deliberately accepts only exact-price evidence. Its omission must not be described as “market never reached the quote.”

## Actual diagnosis of all 54 preliminary simulations

[Machine-readable JSON](../reports/milestone4_1/execution_diagnostics.json), [CSV](../reports/milestone4_1/execution_diagnostics.csv), [summary](../reports/milestone4_1/execution_diagnostics.md) and `reports/milestone4_1/decisions/` contain every experiment and quote decision. The original 180-second source remains `.local/research/run-1792248889904`. Only the original approximately 82–90-second training partitions were diagnosed. Empty embargoed validation/test intervals are not validation; no new OOS outcome was evaluated.

**All 54 resulting financial summaries are exactly equal to their M4 originals**, including zero historical simulated fills and zero P&L. FIFO corrections do not change these outcomes because the original strategy does not stack same-side own orders.

| Funnel stage | Count across independent simulations |
|---|---:|
| Refresh-side quote opportunities | 4,172 |
| Existing quote retained or canceling | 1,781 |
| New generated intents | 2,391 |
| Fee rejections | 0 |
| Risk rejections | 2,086 |
| Exchange tick/lot/notional filter rejections | 0 |
| Missing/stale-data rejections of an already generated intent | 0 |
| Submitted to simulated execution | 305 |
| Accepted at simulated arrival | 304 |
| Historical simulated fills | 0 |

All **2,086 risk rejections were insufficient available base inventory**. Starting with USDT only means no funded sell quote until an evidence-based buy acquires base; no such fill occurred. Zero fee rejections do not mean fees were irrelevant: **2,762 side opportunities were widened** by the fee-aware minimum. Standard configured maker fees require at least 22bps width including the 2bps safety edge, while refresh-sampled quoted spreads are much narrower. No account entitlement or promotional tier is verified. The explicit maximum-width fee rejection is exercised by controlled test F, not manufactured in the frozen historical experiment.

Primary order explanations are mutually exclusive and reconcile to **305 submitted orders**:

| Primary no-fill explanation | Orders |
|---|---:|
| Never matched by an exact-price directional trade; no live trade-through omission | 227 |
| Order assumptions invalidated by market-data coverage loss | 60 |
| Canceled before a later exact-price directional match | 16 |
| Live trade-through evidence omitted by the exact-price fill model | 2 |

Additional overlapping diagnostics:

- **289** quotes had no exact-price directional match anywhere after submission within the evaluated partition. This includes invalidated quotes and the two live trade-through cases; it is not a claim that all 289 prices were never reached economically.
- **303** quotes had neither exact-price nor trade-through activity in their valid execution window. **Zero** had eligible exact-price trades blocked by queue depletion, and **zero** had an exact-price match before entry latency elapsed. Consequently queue depletion and entry latency do not explain this sample's zero fills.
- **16** quotes had a later post-cancel exact-price directional match. These are counterfactual reach observations, not known fills. **45** orders ended with unresolved entry/cancel/report timing; they do not qualify as reconciled execution outcomes.
- Two live trade-through events concerned ZECUSDT quote 4 at 1163.60 USDT in `zero_maker_hypothetical` and `maker_rebate_hypothetical`. The scenarios reuse the same public observation; these are not two distinct exchange executions. This exposes a potentially underfilling model assumption, **not a missing profitable fill that can simply be credited**. There are 423 broader since-submission trade-through/order comparisons, mostly outside the valid lifetime; those comparisons are not eligible volume.
- **14/54** summaries fail modeled data quality (sparse USD1 updates and injected interruption scenarios). Dependencies and reasons appear per result. All selected primary source prefixes/record sequences were intact and had no aggregate-ID gaps; simulated stale intervals and injected outages remain separate from recorded corruption.

There is **insufficient executable evidence**, rather than a demonstrated positive edge or a universal proof of no edge. No price/fill assumptions were weakened to produce profitable results. Eligible exact-price traded volume during live valid orders was zero in this historical diagnosis; hypothetical fees did not create execution evidence.

## Controlled simulator validation

Reviewed official [Binance aggregate-trade semantics](https://github.com/binance/binance-spot-api-docs/blob/master/web-socket-streams.md#aggregate-trade-streams) on 2026-10-17. The stream aggregates a single taker order; `m` asks “Is the buyer the market maker?” Thus `m=true` means sell aggressor, eligible for a passive buy; `m=false` means buy aggressor, eligible for a passive sell. Both directions are tested through the actual wire parser. Retrieved document SHA-256: `9d2ba84d1b8a006a832a06de8786374666c0f5eb9ec82326ade8fc180b2ce8c1`.

| Synthetic scenario | Verified expected outcome |
|---|---|
| A: buy maker | Wrong aggressor cannot fill; eligible sell activity depletes queue and fully fills |
| B: sell maker | After modeled inventory acquisition, wrong aggressor cannot fill; buy activity fully fills sell |
| C: partial trades | 0.25 + 0.35 + 0.40 base fills conserve a one-unit order |
| D: queue not depleted | No fill; machine-readable queue-block reason and initial estimate |
| E: cancellation race | Fill while cancellation pending; no further fill after effective cancel |
| F: fee infeasibility | Required fee-aware width exceeds explicit cap; no orders, exact fee-rejection reasons |
| G: inventory limit | Pending buy commitments consume position budget and reject excess intent |
| H: stale market | Clear execution assumptions and suspend orders |
| I: invalid sequence | Gap invalidates execution; actual invalid recorded depth cannot produce a tape |
| J: replay | Nonempty execution, complete results, decisions and explanations repeat identically |

Additional tests cover own-order FIFO with shared volume, FIFO under proportional cancellation credit, arbitrary-volume property tests, duplicate suppression, live trade-through omissions, sell-side post-only rejection, wire aggressor semantics and readiness faults. Existing tests retain buy-side post-only, latency and delayed pre-cancel reports, fees in quote/base/BNB, historical fee changes, precision/filter constraints, conservation, no negative available balances, ledger reconciliation and drawdown stops. Controlled positive fills validate code paths only; **they are synthetic, not historical performance or actual exchange executions**.

## Ongoing capture readiness

[Data readiness JSON](../reports/milestone4_1/data_readiness.json) audited finalized files from `.local/long-captures/session-1792251264110` at **2026-10-17 19:17:54.710 Europe/Moscow**. The session began at 18:34:24.110 and targets 86,400 seconds. It was **still incomplete**, approximately **43 minutes 30 seconds** elapsed at this checkpoint. New files and unflushed tails after enumeration are excluded; this is a fixed audit checkpoint, not a claim about the latest wall-clock coverage.

| Metric | BTCUSDT | ETHUSDT |
|---|---:|---:|
| Verified record envelopes | 66,656 | 65,037 |
| Applied book updates | 25,887 | 25,166 |
| Aggregate trades | 14,736 | 14,567 |
| Raw messages | 40,663 | 39,768 |
| Retained prefix complete | Yes | Yes |
| Recorded-sequence discontinuities | 0 | 0 |
| Aggregate-ID gap ranges | 3 | 3 |
| Recorded overflow invalidations / session recoveries | 3 / 3 | 3 / 3 |
| Book intervals exceeding 2-second freshness | 3 | 3 |
| Longest book interval | 6.249s | 5.622s |
| Book/trade exchange-time reversals | 0 / 0 | 0 / 0 |
| Causal buffered-time clamps | 32 | 25 |

Both retained prefixes are intact, but **feed continuity is not**. Six overflow/recovery interruptions and six aggregate-ID gaps prevent presenting the source as uninterrupted observations. Missing book intervals are listed explicitly in JSON. Four raw receive-time reversals per symbol are attributable to buffered application ordering; causal replay clamps backward timestamps, while recorded sequence remains authoritative. Source checksums, within-segment book reconstruction and cross-file record identity/sequence were verified. Exact queue rank is still not observable.

Frozen finalized payload: **50,128,443 bytes across 187 segments**, against the configured **2,147,483,648-byte** combined payload budget. Free disk at audit completion: **18,991,435,776 bytes**. Tiny sidecars/locks and a bounded temporary segment are additional overhead. The one-GiB-per-symbol rolling policy can evict the prefix later; no 24-hour archive is guaranteed.

Two streaming replays produced identical coverage and reconstructed-data digest `adba208e338ad52a7b1f5906335d76281f2eb0a76f4e25bba4b9d4474aea081b`, with **zero checksum/read errors**. Deterministic replay success does not repair missing public intervals or prove trading suitability. `continuous_complete_target_ready=false`; the capture was not stopped or awaited merely to finish this task.

## Conditional research decision

No sufficiently complete continuous 24-hour dataset exists at this checkpoint. Therefore **no new long-horizon profitability research was run**. The 54 reruns above are solely diagnostics of already evaluated history. No strategy was evaluated on the ongoing capture and final OOS remains untouched. Actual exchange executions remain zero/unavailable.

Required future research: retain complete immutable public inputs with explicit gap boundaries; diagnose and reduce overflow/write-latency interruptions through separately tested bounded collection changes; preserve an untouched chronological final test; freeze fee/strategy hypotheses; and build a bounded streaming strategy/report runner or explicit block-state transfer. The existing offline tape/trace caps remain and can reject a multi-day source even if collection completes. Do not aggregate independently restarted $5,000 blocks as one continuous portfolio.

Resolve and independently validate trade-through handling before drawing economic conclusions. MBP cannot identify exact order IDs, hidden liquidity, cancellations ahead, or precise priority; conservative and cancellation-credit models are approximations. A quote through which the market trades deserves diagnosis, but automatic fill credit without timing, source coverage, quantity and queue rules would overstate evidence. Fees are configured manual hypotheses, not verified live account commissions; current conversion/settlement precision assumptions remain explicit.

## Verification and reproducibility

- `bash scripts/check.sh`: passed (Rust formatting, all-target Clippy with warnings denied, all Rust tests; TypeScript Prettier, typecheck and tests).
- **73 Rust tests passed, 0 failed/ignored**: 53 previous + 20 new. New tests comprise 17 execution/diagnostic/property tests and 3 recording-readiness fault tests. All A–J assertions passed before historical diagnosis. No doc-test failures.
- **7 TypeScript tests passed, 0 failed/skipped**; existing mode/order restrictions remain enforced.
- `cargo build --workspace --locked` and `npm --prefix control-api run build`: passed.
- PostgreSQL: Compose healthy, idempotent foundation migration and `scripts/db-test.sql` via `psql -v ON_ERROR_STOP=1` passed.
- Historical diagnosis produced all 54 reports with identical original financial summaries. Repeated diagnostics to `/tmp/m4-1-reproduction` matched JSON and all decision logs byte-for-byte (`cmp`, `diff -qr`).
- Frozen capture integrity replay matched twice as described above. No completed-day, live-profitability, remote CI or fresh-cloud-restoration claim is made.

```sh
# Activate Rust as documented in docs/ENVIRONMENT.md, then from repository root:
cargo build --workspace --locked
./target/debug/mm-backtest diagnose .local/research/run-1792248889904 config/backtest.json reports/milestone4/backtest_results.json /tmp/m4-1-diagnosis
./target/debug/mm-backtest readiness .local/long-captures/session-1792251264110 /tmp/m4-1-readiness.json
bash scripts/check.sh
```

Raw recordings are local/ignored, not backed up by committed reports or checksum lists. The ongoing collector is an existing public-data process, not a durable service; environment termination may stop it. No new environment publication or startup capability is required. Further measurement and model validation are justified; strategy optimization, deployment or capital commitment are not supported by the present evidence. Stop after M4.1; M5 requires explicit approval.

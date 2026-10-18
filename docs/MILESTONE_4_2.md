# Milestone 4.2 — Inventory initialization, execution fidelity and extended research readiness

Implemented after approval of `9d5589c9f77e1eff0c6c37432fe7df98719d1599`. Work spans 2026-10-17–18, Europe/Moscow. Trading remains disabled; no credentials, private endpoints, real orders, leverage, borrowing or M5 work were introduced. Engineering fidelity improved; **profitability remains unvalidated**.

## Initial inventory and accounting

`portfolio::Initialization` declares base allocation, initial price/time and fee-reservation buffer. The comparison configuration uses **0%, 20%, 50% and 80% base**, corresponding to the requested 100/0, 80/20, 50/50 and 20/80 quote/base portfolios. Each isolated trial starts with **exactly $5,000 equity**. Its first causal, fresh, reconstructed midpoint is the valuation price. Base quantity rounds down to the symbol lot increment; residual value stays in USDT. The recorded initialization includes the exact price, timestamp, amounts and moving-average cost basis.

Initial holdings are a priced accounting reclassification of the original capital, **not a simulated or actual acquisition fill**. No conversion execution or associated spread/fee is claimed. Studies of the cost of acquiring initial inventory must add that separately. No extra deposit funds the base leg. A configured BNB fee wallet is likewise funded from existing quote capital at the explicit manual conversion assumption, never added on top.

Balances expose total/reserved/available quote, base and BNB. Buy principal reserves quote; sells reserve base. Reservations cover applicable configured fee assets, a 10% fee-only buffer in this experiment, and conservatively known future fee-schedule changes. Base fees on a buy reduce acquired receipts; base fees on a sell require extra reserved base. Quote fees on a sell are paid from proceeds; BNB fees reserve BNB. Partial fills recalculate remaining reservations, while effective cancellation retains its report grace before releasing them. A fill checks that both remaining own reservations and other orders remain funded **before** posting balances/ledger changes. Unexpected fee insufficiency invalidates the model rather than spending another order's reserve.

Position limits and quote skew center on the declared initial base target. The configured $500 bound is a deviation allowance around that target, with pending buys/sells included; it is not a reason to reject an otherwise permitted $4,000 initial holding. Both sides can quote when funded. No short position or negative available balance is permitted.

Gross realized P&L uses moving-average base/BNB cost basis, including recognized fee-asset disposals. Gross unrealized P&L marks remaining holdings. The checked identity is:

`net P&L = realized gross + unrealized gross − fees + rebates = current equity − initial equity`

Fee expense is deducted once. Quote/base/BNB tests, partial fills, sales and source replays reconcile exactly in integer atoms. Financial values use 1e8 units; prices/quantities additionally obey venue increments. Exact notional-product comparisons avoid accepting even a sub-atomic excess over a maximum-notional boundary.

## Capital across markets

The committed [configuration](../config/backtest_funded.json) explicitly declares `isolated_alternatives_5000_each_do_not_sum`. Every row is a different $5,000 hypothesis; **never sum their capital, holdings or P&L across markets, inventory scenarios, execution models or source sessions**. Capital group IDs make that scope machine-readable.

Optional `partitioned_single_5000_budget` accepts explicit `shared_market_capital_usdt` partitions summing to **one $5,000 budget**, for example two markets at $2,500 each. Tests initialize two distinct assets from those partitions, verify combined initial equity/reservations, and reject two $5,000 allocations under the same budget. Batch rejects duplicate source instruments and unbudgeted symbols. Partitions are fixed subaccounts, not a dynamically transferable shared cash pool; rejected/unavailable market partitions remain unallocated cash. No joint portfolio P&L or global kill-switch validation is claimed. Scenario/model/source groups remain separate experiments in either mode.

## Execution models and the previously observed cases

All models retain post-only acceptance at simulated arrival, aggressor direction, activation time, receipt/report latency, source validity, remaining quantity, tick/lot constraints and cancellation races. Fills can occur while cancellation is pending, including eligible delayed pre-cancel reports; effective cancellation prevents later executions. Invalid/stale/gapped inputs invalidate assumptions. Duplicate/old aggregate activity cannot execute twice.

- **Exact price:** original aggressive-trade/queue-depletion model.
- **Conservative through:** aggressive sell trades below a resting buy, or buy trades above a resting sell, can supply execution evidence. The trade must lie inside known entry-depth coverage. Its participation volume first pays an allowance for entry-visible levels strictly between the own limit and observed trade, then estimated queue ahead; only residual quantity can fill the remaining order. Intervening depth is charged per event, which can deliberately underfill after depth has changed.
- **Alternative through:** through activity is assumed to have cleared public priority, omitting that depth allowance and clearing modeled queue ahead. This is a **less-conservative sensitivity assumption**, not verified FIFO or profitability.

Through fills occur at the **own resting limit**, not at the public deeper trade price, and never exceed observed participation volume. Evidence logs retain aggregate ID, observed price/quantity, own price/quantity and evidence type. Through modes support one own order per side, avoiding unsupported multi-price counterfactual volume attribution; exact-price mode retains tested multi-order FIFO. Market-by-price data cannot reveal hidden priority, cancellations ahead, individual order IDs or actual hypothetical own fills. Neither model guarantees an actual exchange outcome or bounds all queue uncertainty.

The two prior ZEC scenario entries reused **one public observation**, aggregate ID **240303953**. The order was a buy for **0.085 ZEC at 1163.60**, active at 1792248921730ms and canceled effectively at 1792248923774ms. A sell-aggressor trade at **1163.58** carried **0.006 ZEC** during its valid lifetime. Its estimated visible queue at entry was zero.

[Recorded fixture](../reports/milestone4_2/recorded_trade_through_case.json) preserves the source order and causal event window; the depth projection retains relevant original levels and observed best/farthest anchors. [Regression results](../reports/milestone4_2/trade_through_case_results.json): exact-price gives no fill; conservative and alternative modes each produce **one simulated 0.006 ZEC partial fill at 1163.60**, without exceeding the public quantity. Both repeat identically and preserve accounting. Tests verify the actual aggregate ID, timing, own price, observed price and quantity.

This is a **fixed recorded-order execution regression**, with automatic strategy decisions disabled and standard manual 10bps maker fees (0.00698160 USDT). The original order came from hypothetical zero-maker/rebate scenarios; their fee entitlement is not presumed. This controlled replay does **not** show that standard-fee strategy logic would place that quote or make money. Actual exchange executions remain zero/unavailable.

## Current public filters and fee assumptions

Fetched Binance Spot public `exchangeInfo`, last refreshed **2026-10-18 10:32:29.761 Europe/Moscow**. [Frozen metadata](../reports/milestone4_2/exchange_metadata.json) includes receipt time, endpoint and response SHA-256. The earlier snapshot is retained separately. Required filters are unchanged between snapshots and match the saved historical filters for evaluated markets.

| Symbol | Tick | Lot step | Minimum notional |
|---|---:|---:|---:|
| BTCUSDT | 0.01 | 0.00001 | 5 USDT |
| ETHUSDT | 0.01 | 0.0001 | 5 USDT |
| USDCUSDT | 0.00001 | 1 | 5 USDT |
| USD1USDT | 0.00001 | 1 | 5 USDT |
| SOLUSDT | 0.01 | 0.001 | 5 USDT |
| ZECUSDT | 0.01 | 0.001 | 5 USDT |

Batch checks active Spot eligibility, quote/base identities, price bounds/tick, quantity bounds/step, and min/max notional. Strategy prices round outward, quantities round down. Referenced official [filters](https://github.com/binance/binance-spot-api-docs/blob/master/filters.md) and [aggregate stream semantics](https://github.com/binance/binance-spot-api-docs/blob/master/web-socket-streams.md#aggregate-trade-streams) retain `m=true` as buyer maker/sell aggressor. Current filters are a frozen current-rule validation, not proof of every historical venue-rule version. Dynamic percent-price/weighted-average bands, actual matching priority and private account conditions are not fully reconstructed.

The comparison uses the existing **manual unverified standard** assumption: 10bps maker each side, 10bps taker, quote-asset charges, no rebate or discount/promotion. Fee assets are configurable and tested in base/quote/BNB, but actual account settlement asset/rates cannot be verified from public metadata. BNB conversion in fee-asset tests remains an explicit manual assumption, not an observed market price. No authenticated commission endpoint was called. No favorable fee tier was introduced to obtain a result.

## Funded historical comparisons

[JSON](../reports/milestone4_2/batch/comparisons.json), [CSV](../reports/milestone4_2/batch/comparisons.csv), full bounded `traces/`, and [frozen configuration](../config/backtest_funded.json) provide every requested diagnostic. Each row includes intents, funding/risk reasons, peak active orders, exact/through opportunities, partial/complete fills, gross/net P&L, fees, inventory exposure, maximum drawdown and initial/end balances.

The source is the original **180-second M3 recording**, using only its existing approximately 82–90-second training partitions. Final OOS remains untouched. USD1USDT is conservatively rejected at session-symbol scope because its book freshness intervals are invalid; this excludes twelve potential combinations. **Sixty accepted rows = five markets × four allocations × three execution models.** These counts are not directly comparable to M4.1's six-market/nine-scenario denominator.

The following counts combine five independent market alternatives **within one execution model**; all three models give the same standard-fee result:

| Quote/base allocation | Generated intents | Funding/risk rejections | Peak active orders per market | Partial/complete fills |
|---|---:|---:|---:|---:|
| 100/0 | 227 | 200 insufficient base | 1 | 0 / 0 |
| 80/20 | 54 | 0 | 2 | 0 / 0 |
| 50/50 | 54 | 0 | 2 | 0 / 0 |
| 20/80 | 54 | 0 | 2 | 0 / 0 |

Funded asks now remain active rather than repeatedly failing funding gates, so fewer new intents are generated. This resolves the inventory initialization defect as an explanation for missing sell quotes. It does **not** create an execution edge: all sixty rows have zero eligible exact-price/through opportunities at their selected standard-fee quotes, zero fills, zero gross spread capture and zero fee expense. The fee-aware minimum is about **22bps total width** including the safety edge, outside the narrow observed market in this brief sample. Queue rules cannot generate volume that was never observed at/through those quotes.

Net MTM differences below are entirely initial inventory movements, identical to holding the initialized portfolio without trading. They are **not spread profits**. Values are USDT; one row per market/initial allocation, identical across the three models:

| Market | 20% base net | 50% base net | 80% base net |
|---|---:|---:|---:|
| USDCUSDT | 0 | 0 | 0 |
| BTCUSDT | 0.01727250 | 0.04320240 | 0.06913230 |
| ETHUSDT | -0.20173300 | -0.50435700 | -0.80703000 |
| SOLUSDT | -0.19158000 | -0.47894000 | -0.76632000 |
| ZECUSDT | 0.59271000 | 1.48212000 | 2.37153000 |

Cash-only net is zero. Realized trading P&L, excess return over initial holdings and fees are zero. For example ZEC's 80%-base portfolio has 4.77743 USDT maximum drawdown despite positive final inventory MTM. Each row exposes that drawdown and exposure, rather than attributing favorable inventory movement to market-making. No winning strategy or profitable fee tier is inferred.

## Existing collector audit and preservation

The original process **PID 83334** under `.local/long-captures/session-1792251264110` was running when work began and remained running across the overnight pause. It was **not stopped, restarted, overwritten or reconfigured**. No new live collector was started during this milestone.

Before implementation, a full two-pass [preflight replay](../reports/milestone4_2/preflight_readiness.json), checkpoint **2026-10-17 19:52:28.927 Europe/Moscow**, verified about **78 minutes** of retained history: BTC 46,259 applied books/35,995 aggregate trades and ETH 45,399/33,312; both replays matched. It already contained nine BTC and five ETH overflow invalidations/trade-ID gap ranges, so it was not continuous clean history.

The later [current inventory/checksum audit](../reports/milestone4_2/current_capture_inventory.json), checkpoint **2026-10-18 10:31:44.117**, records **57,440 seconds = 15h 57m 20s elapsed**, with **no completion marker and no verified 24-hour completion**:

| Frozen retained state | BTCUSDT | ETHUSDT |
|---|---:|---:|
| Finalized segments | 2,091 | 2,022 |
| Payload bytes | 500,281,097 | 484,133,804 |
| Reconnects at checkpoint | 124 | 39 |
| Buffer overflows | 117 | 37 |
| Synchronization failures | 8 | 3 |
| Recording failures | 0 | 0 |

All **4,113 finalized files / 984,414,901 bytes** matched their SHA-256 sidecars. Segment numbers remained contiguous from one; the previously fully replayed prefix files were unchanged. Both payloads were below the one-GiB-per-symbol rolling budget. This verifies frozen file integrity, **not record-by-record replay or clean exchange coverage over all sixteen hours**.

A copied read-only latest-12-segment window per market was [replayed twice](../reports/milestone4_2/current_suffix_replay.json), with identical results and no checksum/read/record-sequence errors: BTC covered **250.506s**, 2,308 books/1,781 aggregate trades with two aggregate-ID gaps; ETH covered **362.851s**, 3,505 books/1,272 trades with no aggregate-ID gaps in that window. The copy lives under a separate `.local/audit-snapshots/` root and is an audit artifact, not a new completed collector session. No source payload was edited. Prefix and suffix windows must not be summed as one clean interval.

Elapsed process time, retained timestamp span, checksum verification and fully reconstructed coverage are explicitly different measures. Full middle-period reconstruction was not claimed. Status `data_gaps=0` alone is not proof of no lost trades; overflow/recovery and aggregate-ID gaps show otherwise. The still-active source was not used for strategy outcomes or future OOS evaluation. The task was not held open waiting for the 24-hour target.

## Bounded campaigns and offline batch

`collect-campaign` adds resumable sequential sessions around the existing collector. The default separate root is `.local/collection-campaigns/research-v1`, with a 24-hour target, <=6-hour sessions, <=12 sessions, 256 MiB per symbol/session and an 8-GiB campaign bound. It reserves storage before starting, preserves old data rather than silently deleting sessions, and uses a campaign lock/frozen configuration. Resume reconstructs progress from completed markers and creates a new session. It refuses an unfinished existing session or an existing capture as campaign root, preventing accidental takeover of a still-running process. Unclean/crashed sessions require explicit reconciliation; automated crash adoption is not implemented. These paths are tested locally; no new campaign or multi-day soak was executed.

`batch` streams the existing checksum-verified replay/normalization visitor, so it does not retain a full multi-day depth tape. Within each trial balances, orders, cost basis, risk state and counters persist. External JSONL traces retain ledger/actions/equity/fills; internal history is compacted to balance checkpoints and a causal reference window. Checkpoints are explicitly **not emitted as deposits**. Tests sum the full external ledger back to final balances and verify one initial funding entry.

Only completed sources are accepted (legacy completed M3 observations are explicitly declared). Checksums, prefixes, identities, record/trade gaps, stale intervals, exchange ordering and missing end tails are gates. An invalid interval rejects the **entire affected session-symbol**, rather than fabricating fills or resetting $5,000 inside a gap. Declared completion without a sufficiently fresh recorded tail is rejected. Completed sessions are independent experiment groups; no state transfer across separate sessions or combined continuous P&L is claimed.

The default trace budget is 256 MiB, with bounded source/session/scenario/model counts and separately bounded provenance metadata. A new/empty output directory is required. Aborted output retains `INCOMPLETE`; only `batch_complete.json` plus its result checksum completes the artifact. Repeated runs using frozen input/config/metadata produce identical files. Batch does not yet calculate fill markouts from compacted history; the original bounded runner still provides its original markout reports. Global pooled-wallet execution, durable OMS/restart recovery and automatic contaminated-interval salvage are not implemented. BNB/USDT base-wallet aliases remain unsupported; compatible configured Spot markets are required.

## Tests and reproduction

Executed after final financial guards:

- `bash scripts/check.sh`: Rust formatting, all-target Clippy with warnings denied, full Rust tests; TypeScript formatting, typecheck and tests passed.
- **93 Rust tests passed, 0 failed/ignored**: 73 previous + 20 new. New coverage includes all initial allocations, both funded quoting sides, shared partitions/over-allocation rejection, fee assets/buffers, reservation release, pending-sell limits, unexpected fee-change isolation, exact realized/unrealized equity, conservative/alternative through and activation/cancel/gap guards, allocation/volume properties, the actual ZEC regression, exact maximum-notional fractions, batch ledger/replay/determinism, incomplete/corrupt/missing-tail rejection and campaign preservation/resume.
- **7 TypeScript tests passed**, no failures/skips; live-mode/order restrictions remain enforced.
- `cargo build --workspace --locked`, `npm --prefix control-api run build`: passed.
- PostgreSQL Compose healthy; idempotent foundation migration and `scripts/db-test.sql` passed with `ON_ERROR_STOP=1`.
- Sixty completed historical comparison rows, all component accounting identities reconciled; repeated batch output directories match byte-for-byte. Recorded regression results repeat identically. No remote CI or cloud restoration claim.

```sh
# Rust activation: see docs/ENVIRONMENT.md; commands run from repository root.
cargo build --workspace --locked
./target/debug/mm-backtest batch config/backtest_funded.json /tmp/m4-2-research
./target/debug/mm-backtest case-audit .local/research/run-1792248889904 reports/milestone4_1/execution_diagnostics.json /tmp/m4-2-case
# Optional future operation, after inspecting existing processes; do not restart the current capture:
./target/debug/mm-backtest collect-campaign config/collection_campaign.json
```

Raw capture/manifest availability is required for research reproduction; reports and checksum lists are not a remote archive. The committed recorded-case projection supports its execution regression without the full raw source. Current-rule verification is frozen, not account-fee verification or full historical-rule reconstruction.

## Readiness assessment

The simulator now supports funded two-sided portfolios, exact accounting, evidence-conditioned partial through fills and reproducible streaming trials. It is suitable for **further bounded research on completed, accepted data**, subject to the stated queue/fee/market-impact assumptions. It is **not ready to establish profitable expectancy from the present capture**: that source is still incomplete and has substantial feed interruptions; strict gates would reject affected session-symbols. Collection campaigns do not themselves fix the transport/recording-overflow cause. Improving collection reliability and validating a principled gap/interval policy are prerequisites to a credible longer study.

No strategy optimization or synthetic profitability was introduced. Funded quotes correct one root cause; standard fees and observed execution opportunities remain a separate economic obstacle. Longer complete multi-regime data, genuine held-out evaluation, fill-conditioned markouts and verified fee assumptions are still needed. Stop after M4.2; M5 requires explicit approval.

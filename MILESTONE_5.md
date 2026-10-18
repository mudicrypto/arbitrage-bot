# Milestone 5 — Functional Binance Spot Paper Bot

Implemented on top of `295db6c`; public data and simulated funds only. No keys, signed requests, real orders, leverage or borrowing. Stop after M5. Profitability remains unvalidated.

## Launch

From `/workspace/arbitrage-bot`:

```sh
bash scripts/paper.sh                         # RESEARCH, 0bps requested width
bash scripts/paper.sh economic                # ECONOMIC, 30bps requested width
```

Run one profile at a time unless deliberately comparing **isolated** alternative wallets. Each has its own $5,000; their funds/results must never be summed. Terminal output shows market price, simulated bid/ask, pending/active/canceling orders, recent simulated fills, total/available/reserved USDT and BTC, fees, realized/unrealized P&L, equity, drawdown and strategy status. No API/database/frontend startup is required. Rust 1.90 and the existing public Binance connectivity are required; the script activates the installed cloud toolchain automatically.

**Ctrl-C stops safely. Repeat the same command to resume.** Wallets live in `.local/paper/{research,economic}/`; `wallet.json` is authoritative, `status.json` is machine-readable. Configuration mismatch refuses startup. Preserve these local files; Git does not back them up. Another width needs a deliberately separate simulated experiment:

```sh
bash scripts/paper.sh research --spread-bps 2 --state-dir .local/paper/research-wide
bash scripts/paper.sh economic --spread-bps 1 --state-dir .local/paper/economic-narrow
# Finite run, otherwise runs until Ctrl-C:
bash scripts/paper.sh research --seconds 60
```

## Reused engine and controls

The existing Binance snapshot/delta/aggregate-trade collector, Parquet recorder, fixed-point funded Engine, fee ledger, conservative trade-through model and deterministic replay remain in use. The only feed contract addition is an observer invalidation callback before reconnection. A bounded 64-event bridge feeds the same engine; stale/invalid/gapped data, processing lag, queue overflow and recording failure pause/stop execution. A fresh valid book permits recovery, with coverage interruptions retained. Market-by-price queue position remains an estimate, never observable FIFO truth. `m=true` means sell aggressor eligible against buys; `m=false` means buy aggressor eligible against sells.

Initial holdings use the first causal synchronized mid: 50% base with lot rounding, remaining value in USDT, exactly $5,000 total. This declares simulated holdings, not a conversion execution. For the main research sample the valuation was **81956.235 USDT/BTC**, at **2026-10-18 13:03:08.796 UTC**: **0.03050000 BTC + 2500.33483250 USDT**. Reservations, partial fills, fee assets, moving-average realized/unrealized P&L and MTM equity reuse M4.2 accounting. Quotes use mid fair value, the existing integer-bps EWMA volatility and inventory skew around initial holdings; $100 clips, $500 inventory-deviation limit and $50 drawdown stop remain. Quotes refresh every 500ms, with modeled 50ms entry/100ms cancellation latency plus seeded jitter. Tick, lot, quantity/notional and price bounds use current public metadata, saved with the run.

Both profiles **charge** manual, unverified **10bps maker fees per side in USDT**, no BNB discount, promotion or rebate. These accrued simulated expenses are separate from estimated future costs. RESEARCH can quote below fee costs, clamped to public best bid/ask to avoid intentional crossing, and is always research-only. ECONOMIC requires round-trip fees plus **7bps** (3 adverse selection, 2 inventory, 2 safety); its requested width is also its cap, so a negative estimated edge refuses placement. This allowance is an assumption, not proof of executable edge. All funding, post-only activation, latency, cancellation-race, volume and risk checks remain in both profiles.

## Actual observed public-data runs

Evidence is in [reports/milestone5](reports/milestone5/). Durations below are requested process timers; actual book-to-stop coverage is shorter during connection/synchronization. Counts are processed books/trades, not synthetic observations.

| Isolated experiment | Requested run | Books / trades | Simulated fills | Charged fees USDT | MTM net P&L USDT |
|---|---:|---:|---:|---:|---:|
| Research 0bps, first run | 60s | 581 / 228 | 1 | 0.09998632 | +0.00719678 |
| Same wallet after 30s restart run, cumulative | 60s + 30s, with downtime | 813 / 268 | 2 | 0.19997705 | +0.60993215 |
| Economic 30bps | 40s | 379 / 131 | 0 | 0 | -0.37271000 |
| Economic 1bps | 30s | 223 / 81 | 0 | 0 | +0.70272000 |
| Research 2bps | 30s | 228 / 40 | 0 | 0 | +0.70272000 |

The narrow economic run refused quotes with **86 fee-constraint rejections**. Zero-fill P&L is entirely initial-inventory MTM, not trading returns. After restart, research gross realized P&L was **0.00412853**, unrealized **0.80578067**, and fees **0.19997705**: the positive net figure is not evidence of profitable spread capture. No actual exchange executions occurred. Synthetic positive-fill tests are separate from these observations.

A checksum-verified replay independently matched the second research fill to actual public aggregate trade **4092413100**, `m=false`, price **81959.61**, public quantity **1.92233000 BTC**; the simulated sell was only **0.00122000 BTC**. Source path, SHA-256 and raw public message are in `fill_source_verification.txt`. The earlier run used 32-segment retention and evicted its prefix: its audit reports `prefix_complete=false`, so full-session historical reproduction is unavailable. No completeness claim is made.

A final build smoke run and Ctrl-C restart retained its complete raw prefix: **17 segments, 315 book updates, 71 aggregate trades, zero checksum/read errors**. The wallet processed 315 books/70 trades (one source trade preceded wallet initialization). Ctrl-C exited **0**, orders were empty, initial valuation stayed unchanged, and balances remained exactly **2500.60862385 USDT / 0.03049000 BTC**, with no deposits or offline fills. Sequence advanced **355 → 388**; see `restart_check.json`. Its later -1.04641680 USDT MTM was inventory movement, with zero fills/fees. These are brief functionality checks, not continuous long-horizon validation.

## Validation executed

- `bash scripts/check.sh`: **103 Rust tests + 7 TypeScript tests passed**, including all prior suites; Rust fmt, Clippy workspace/all-targets with `-D warnings`, TypeScript format and typecheck passed.
- `cargo build --workspace --locked` and `npm --prefix control-api run build`: passed.
- `docker compose up -d --wait postgres`; foundation migration and `scripts/db-test.sql` through `psql -v ON_ERROR_STOP=1`: passed; live-mode restriction preserved.
- Nine new paper tests cover two-sided fee-paying fills, economic refusal/acceptance, conservative through fills, quote refresh, stop persistence, deterministic event results, checkpoint restoration, exclusive wallet lock, rotation/write failure, stale/gapped/invalid data, bounded bridge and activation latency. One new local WebSocket fault test verifies observer invalidation on interruption. Synthetic fills in these tests are explicitly test data.
- Real public launch commands above were exercised with `--seconds 60/40/30` and separate `.local/paper/m5-*` directories; the same wallet was restarted and Ctrl-C tested. Existing checksum-verified audit/replay commands passed on retained source segments. Earlier implementation/compile errors were corrected before these final passing checks.

## Persistence, limits and remaining uncertainty

Per-event journals are sequenced and contain normalized input hashes/source references, actions, fills and ledger deltas. After journal fsync, wallet/status are atomically replaced and directory-fsynced; the wallet sequence defines committed state. A journal tail beyond it is uncommitted after a crash. Restart preserves balances, fees, RNG, counters and drawdown stop, voids virtual orders and requires fresh books. No attempt is made to simulate downtime executions or reconcile real exchange orders. Book/queue snapshots intentionally are not restored.

Current bounds: 64-event bridge, 12 recent fills, compacted traces/reference history, at most eight journal files (4MiB rotation target, up to 1MiB row overshoot each), and 128 raw segments subject to a **256MiB total raw budget**, plus bounded temporary/checksum files. At least 512MiB free headroom is required initially. Retention, crash tails, gaps and external watchdog/restart events limit full forward replay; input hashes alone are not a replacement for retained raw data. This is a local paper checkpoint, not an archival production OMS. `status.json` must be treated as stale if its timestamp is old; paused/stopped values use the explicitly labeled last known mark.

Queue uncertainty, aggregate-trade granularity, proxy lag, reconnect/overflow interruptions, coarse integer volatility, unverified fee-asset assumptions, initial inventory exposure and unmodeled liquidation/operating costs remain. No strategy optimization, sustained profitability validation or live execution is authorized. The pre-existing long collector (PID 83334) was preserved and observed running; M5 neither claims a verified 24-hour capture nor uses that capture for strategy evaluation.

The cloud setup skill's **start instructions** were saved as a draft for this workflow; installation/network/secret settings were unchanged. Environment review/save/publish remains manual and fresh-task restoration was not tested. Source publication to Git is separate from environment publication.

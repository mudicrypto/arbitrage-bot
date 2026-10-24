# Milestone 6 — Profitability Discovery

**Result: no strategy has a defensible positive simulated edge in this study.** No candidate was nominated by training. Economic quotes received no simulated fills; fee-paying research touch quotes lost trading value despite positive inventory-driven MTM. This is preliminary evidence against the tested configurations, not proof that every Binance strategy is impossible. Operation remains paper-only; no keys, orders, leverage, borrowing or actual capital allocation were introduced.

## Economic-mode audit

The actual M5 scenario builder and Engine were probed against the first valid observed book for each market. Standard manual maker fees are 10bps per side, hence **20bps round trip**, plus **7bps** adverse-selection/inventory/safety allowance. Requested widths 0/1/26bps each produced two fee rejections and no submissions. **27/30bps passed and submitted both sides** on all four markets. The default 30bps economic mode does not universally refuse quotes; it proposes wide quotes that were not reached and filled in this sample. Independent volatility caps, stale/invalid books, funding, inventory, exchange filters and post-only activation may also reject orders. See `reports/milestone6/study/economic_gate_audit.json` for exact funnels. Quote feasibility is not executable edge.

## Frozen experiment and real observations

The protocol was frozen at **12:46:11 MSK on 24 October 2026**, before the first holdout began. A separate existing-collector capture ran **12:44:37–12:59:37 MSK**, with four-second staggered startup, for BTCUSDT, ETHUSDT, SOLUSDT and ZECUSDT. Selection covered liquid majors and an existing higher-volatility market; it did not use holdout outcomes. All four passed the existing public exchange filters/activity policy.

- Reused existing collector, checksum-verified Parquet/replay, funded Engine, conservative trade-through execution and actual paper fee configuration. No new service, trading transport, ML or infrastructure rebuild.
- Pre-registered strategies: fixed economic 30bps; inventory-aware adaptive 27–60bps using trailing one-second RMS moves; adaptive plus causal momentum/10bps-depth imbalance/liquidity filters; explicitly uneconomic research touch control. Existing $100 clips, $500 inventory-deviation limit and $50 drawdown stop remain.
- Chronological **40/30/30** train/validation/test with **60-second embargoes**; independent declared $5,000 50/50 portfolios per alternative/fold, no pooled capital/P&L or state leakage. Last 65 seconds settle virtual orders and complete 60-second markouts. No holdout-based tuning; reruns verify deterministic replay only.
- Three execution assumptions: conservative entry-visible depth/queue; doubled queue and 250ms entry/cancel latency; alternative-through optimistic diagnostic only. Both received-asset fees and M5 quote-asset fees use the same manual rate. No discount, promotion or rebate qualifies.
- **288 rows**, all source/freshness-valid. Raw source: BTC **8,937 books / 4,179 aggregate trades**; ETH **8,721 / 3,152**; SOL **6,486 / 978**; ZEC **7,783 / 3,060**. No sequence gaps, trade-ID gaps, invalidations or >2s book intervals were found in this new capture.

Actual account/symbol/tax/special commission rates remain unverified. The official Binance FAQ confirms received-asset semantics but explicitly labels its example rates fictional. Public fee schedule requests were blocked by CONNECT 403. Thus 10bps is a clearly identified manual standard-tier baseline, not a verified account entitlement.

## Findings

| Market | Median quoted spread bps | One-second realized volatility bps | Underlying trades/s | Final-test research fills | Research capture less fees bps | Research max drawdown USDT |
|---|---:|---:|---:|---:|---:|---:|
| BTCUSDT | 0.001241 | 0.2077 | 19.39 | 3 | -9.9994 | 0.45988132 |
| ETHUSDT | 0.042269 | 0.3150 | 10.99 | 2 | -9.4720 | 1.55520000 |
| SOLUSDT | 0.986826 | 0.3680 | 4.05 | 2 | -10.0000 | 1.81584000 |
| ZECUSDT | 0.086978 | 0.8334 | 4.64 | 10 | -9.4885 | 4.99590857 |

Table fills/edge/drawdown use conservative final-test execution and received-asset fees. They are research-only, simulated executions. Bps use **total one-way executed notional**, not starting equity or a full two-sided quoted spread. Full p10/p50/p90 spreads, displayed depth, imbalance, stability, volatility, trade frequency, reference moves, per-market/per-strategy drawdowns, P&L and all sensitivity cells are in [observed results](reports/milestone6/observed_results.md) and the full JSON/CSV.

All three economic strategies had **zero fills across all markets, folds and sensitivities**. Their trading edge is **N/A, not zero**. For example, final-test SOL MTM was +2.22030000 USDT without any trading; the touch control ended +1.95133892 but underperformed the same initial holdings by **0.26896108 USDT**. Initial inventory appreciation does not establish an edge. Queue/latency stress reduced touch-control fills; optimistic through assumptions increased them but remained fee-negative and cannot qualify.

PnL is split into gross reference-mid spread capture, actual modeled fee expense, initial-holdings appreciation and incremental inventory effects. A separate conservative screen takes the lower of excess over initial holdings and capture less fees/unfavorable 60s markouts, then deducts full remaining-inventory liquidation stress and the risk allowance. This diagnostic is not charged again to actual P&L; low turnover amplifies its per-notional bps cost. All rows failed that screen and the predeclared duration floor. Qualification also requires at least **one hour and 100 fills per holdout cell**, conservative/stressed agreement and both fee assets; these are necessary floors, not proof of significance. A 15-minute source cannot validate long-run expectancy.

No viable configuration was identified at the manual standard Binance tier. Alternative spot venues are compared [at a high level](reports/milestone6/alternative_venues.md), with no unsupported current fee quotes or implied profitable fallback. Lower fees alone do not establish sufficient executable spread after adverse selection and risk costs.

## Reproduction and verification

```sh
export CARGO_HOME=/workspace/.tools/cargo RUSTUP_HOME=/workspace/.tools/rustup
export PATH="$CARGO_HOME/bin:$PATH"
# Same financial code, optimized only to accelerate full-depth replay:
cargo --config 'profile.dev.package.mm-market-data.opt-level=3' \
  --config 'profile.dev.package.mm-backtest.opt-level=2' build -p mm-backtest --locked
./target/debug/mm-backtest profitability \
  .local/milestone6/session-1792835077714 config/milestone6.json /tmp/m6-reproduce
```

Output must be new/empty. `INCOMPLETE` cannot be interpreted; `complete.json` is required. New public captures use `mm-backtest collect-session config/milestone6_collection.json 900`; declare/freeze a new experiment before its holdout and preserve its configuration. The committed protocol cannot be applied retrospectively to older data to claim an untouched holdout.

**113 Rust tests and 7 TypeScript tests passed**, including all prior suites and ten new protocol, fee, causal-signal, qualification, deterministic-pipeline and corrupted/incomplete-source tests. `bash scripts/check.sh` passed Rust formatting, Clippy workspace/all-targets with `-D warnings`, Prettier and TypeScript typecheck/tests. Workspace build, TypeScript build, PostgreSQL readiness, foundation migration and database checks passed. The real completed study was replayed twice: results, CSV, 35,461,275-byte execution trace, market comparison and quote-gate audit were byte-identical. See `validation.json` and `replay_consistency.json`. Synthetic test fills remain separate from research observations; actual exchange executions are **zero**.

Raw recordings and full traces remain bounded local/ignored artifacts, not Git backups. The committed trace manifest includes its local location, size and SHA-256. Source files/configuration/code hashes are preserved; missing/corrupt history fails closed. No existing collector or paper wallet was modified. The earlier slow debug audit was interrupted before completing and retained only an uninterpretable INCOMPLETE output; the optimized runs above produced the verified results.

The old 24-hour collector finished its scheduled target, but its completion marker alone does not validate uninterrupted execution coverage. First/last segment checks and the collector metrics are documented in `reports/milestone6/old_capture_inventory.json`: both prefixes were retained, but the status log reports 180 BTC / 65 ETH disconnections. The optional full-history read-only replay was stopped after roughly ten CPU minutes without a final result; it is explicitly unverified, and no strategy was run on that old capture. Longer clean out-of-sample observations, enough conservative fills, independent fee verification and regime testing remain necessary. **Stop after Milestone 6; no live pilot is enabled.**

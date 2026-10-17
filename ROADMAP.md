# Phased roadmap

1. **Foundation (completed):** Rust workspace and domain/exchange contracts; simulation/paper configuration; TypeScript health API; PostgreSQL schema; Compose; CI and unit/integration checks. No execution. Acceptance: build/typecheck/tests pass, migrations apply, unsupported modes fail closed.
2. **Public market data (implemented and validated):** Binance Spot public adapter, exact normalized decimals, snapshot/delta reconstruction, gap recovery, stale detection, immutable recording and Parquet. Acceptance: sequence-gap fixtures and reconnect replay pass without private credentials.
3. **Deterministic simulation:** venue filter metadata, durable OMS, independent risk, balances and ledger, historical replay, latency and queue-sensitive partial fills. Acceptance: reproducible results, no touch-only fills, restart/idempotency and accounting invariants tested.
4. **Quantitative research:** strategy plugin, inventory-aware reservation price, dynamic spreads, microprice/imbalance/volatility/trade-flow signals, adverse-selection gates, fee-aware quotes and attribution. Acceptance: held-out evaluation with costs, stress and sensitivity reports; do not assume profitability.
5. **Forward paper trading:** real public feeds and simulated execution, monitoring/Next.js dashboard, reconciliation and kill switches. Acceptance: sustained forward evidence, bounded inventory and operational recovery exercises.
6. **Separately approved live pilot:** only after proven risk controls and credible net-expectancy evidence; fresh official API/filter review, LIMIT_MAKER behavior, scoped permissions and a strict small-capital budget. No leverage, borrowing or futures. Scale only after pilot evidence.

M1 and M2 are approved. Stop after M2; M3 requires explicit approval. Each milestone ends with functionality, executed checks, limitations, safety review, documentation, commit and proposed next step. Multi-exchange coordination/hedging stays an extension point until justified.

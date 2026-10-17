# Phased roadmap

1. **Foundation (completed):** Rust workspace and domain/exchange contracts; simulation/paper configuration; TypeScript health API; PostgreSQL schema; Compose; CI and unit/integration checks. No execution. Acceptance: build/typecheck/tests pass, migrations apply, unsupported modes fail closed.
2. **Public market data (implemented and validated):** Binance Spot public adapter, exact normalized decimals, snapshot/delta reconstruction, gap recovery, stale detection, immutable recording and Parquet. Acceptance: sequence-gap fixtures and reconnect replay pass without private credentials.
3. **Market scanner and profitability research (completed):** public symbol discovery, bounded multi-pair collection, horizon statistics, manual fee scenarios, conservative economic screening, deterministic research replay, reports and read-only API. Preliminary evidence only; no executable edge assumed.
4. **Proposed next milestone, not approved:** longer multi-session research, held-out validation and queue-aware simulation design. Independent risk, durable OMS, balanced ledger and realistic latency/partial-fill modeling must precede any execution. Scope needs explicit user approval.
5. **Forward paper trading:** real public feeds and simulated execution, monitoring/Next.js dashboard, reconciliation and kill switches. Acceptance: sustained forward evidence, bounded inventory and operational recovery exercises.
6. **Separately approved live pilot:** only after proven risk controls and credible net-expectancy evidence; fresh official API/filter review, LIMIT_MAKER behavior, scoped permissions and a strict small-capital budget. No leverage, borrowing or futures. Scale only after pilot evidence.

M1–M3 are approved. Stop after M3; M4 requires explicit approval. Each milestone ends with functionality, executed checks, limitations, safety review, documentation, commit and proposed next step. Multi-exchange coordination/hedging stays an extension point until justified.

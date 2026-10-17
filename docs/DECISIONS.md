# Architecture decisions

- ADR-001: Modular Rust monolith with domain and exchange boundary crates. Additional modules follow validated workflows; avoid premature services.
- ADR-002: Rust 1.90.0, edition 2024; Node 22+ and pinned TypeScript dependencies. Commit lockfiles and use locked installs in CI.
- ADR-003: Simulation and paper are the only representable modes. No live adapter or write API in M1.
- ADR-004: Integer ticks/lots for executable values; venue scale/filter validation follows in M2. Fees stay in actual asset units.
- ADR-005: PostgreSQL for durable state; Parquet for future event datasets. Session-scoped idempotency keys and replay sequence are explicit. Ledger schema does not yet implement accounting.
- ADR-006: Node built-in HTTP server minimizes foundation dependencies. Read-only loopback API; dashboard deferred.
- ADR-007: Verified Binance official REST documentation on 2026-10-17: LIMIT_MAKER is post-only and rejected if it immediately takes liquidity. No exchange implementation is included. Revalidate current symbol filters, authentication, timeout/unknown outcomes and stream sequencing before implementing the relevant adapters.
  Source: https://github.com/binance/binance-spot-api-docs/blob/master/rest-api.md (New order / LIMIT_MAKER). This review does not establish an implemented or tested Binance integration.
- ADR-008: Starting capital is a research assumption. No live pilot until realistic net-cost validation and separate approval.

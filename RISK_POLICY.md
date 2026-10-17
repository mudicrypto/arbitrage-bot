# Risk policy

M1–M2 support simulation and paper configuration only. Both are scaffolding: there is no simulator or paper engine yet. Live mode is absent from Rust types, rejected by the API and prohibited by a database constraint. No private keys, signed exchange requests, order endpoints or live transports exist. Do not request real API credentials or submit real orders.

No leverage, margin borrowing or futures. USD 5,000 is a research assumption, not an authorized deposit or trading budget. Public feeds, simulated execution and any future live transport require separate capabilities and permissions.

Before any execution implementation: independent pre-trade checks, per-asset/total position and notional limits, cash reservations, price/quantity filter checks, stale/gapped feed rejection, kill switch, rate limits, durable intent before submission, unknown-outcome reconciliation, cancellation-race handling and restart recovery must be implemented and tested. Never resubmit unknown outcomes blindly.

Never infer a fill from a touched price. Model queue uncertainty, latency, partial fills, maker/taker fees and actual fee assets. Preserve deterministic replay. Performance reports must include adverse selection, slippage, inventory risk, fee-asset conversion and operating costs. No profitability claim can be made from M1.

Local API has no authentication and must remain loopback-only. Compose password is a documented local default. Production security, backup, retention, auth and operational runbooks remain future work.

M2 permits live public market-data collection only. It provides no private authentication, order submission or execution capability. Invalid, stale or unrecordable books are unavailable through the API. Snapshot depth is partial; never treat it as full-market liquidity. Retention eviction and crash tails limit historical coverage; use finalized, checksum-verified segments and state these limits in research.

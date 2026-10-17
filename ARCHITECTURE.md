# Architecture and contracts

A modular monolith owns trading state on a deterministic event loop. The TypeScript control API is a separate read-only process. PostgreSQL stores durable lifecycle and accounting records; immutable Parquet partitions will store historical data. Next.js will consume the control API in a later milestone. Python is optional for offline research only.

## Boundaries

| Component | Contract and responsibility | Stage |
|---|---|---|
| Domain | Instrument IDs, positive integer ticks/lots, modes, event envelope, order intent/status, fees by asset | Types implemented |
| Exchange adapters | Public MarketDataSource separated from SimulatedExecution; exchange-specific errors and sequence IDs | Interfaces implemented |
| Book reconstruction | Snapshot plus buffered deltas, gap invalidation, resynchronization | M2 |
| Recorder | Versioned raw events, checksummed Parquet partitions, reproducible ordering | M2 |
| Fair value | Microprice, imbalance, volatility and trade flow from validated books | M4 |
| Strategy plugins | State + event + inventory -> proposed quote intents; no direct exchange access | M4 |
| Capital/inventory | Asset balances, reservation pricing, exposure allocation | M3/M4 |
| Independent risk | Intent + balances + feed age + limits -> approved intent or rejection | M3 |
| OMS | Idempotent intent IDs, durable transitions, unknown-outcome reconciliation | M3 |
| Simulator/backtester | Ordered replay, queue-sensitive fills, latency, partial fills, cancellations | M3 |
| Ledger/analytics | Asset-denominated fees, inventory valuation, spread capture, adverse selection and costs | M3/M4 |
| API/dashboard | Read models and later authenticated control with audit trail | API scaffold M1; dashboard M5 |
| Multi-exchange coordination | Future venue capabilities, capital allocation, hedging interfaces | Deferred |

## Data contracts

Rust EventEnvelope carries schema_version, deterministic sequence, exchange timestamp and receive timestamp (nanoseconds). Replay must order by recorded sequence; timestamps alone are insufficient. BookSnapshot preserves the venue update ID. Future deltas need first/final update IDs and explicit deletion semantics; snapshots use strictly positive quantities. No serialized wire format is promised yet.

Executable PriceTicks and QuantityLots contain positive u64 units. Future instrument metadata supplies exact decimal scale, tick/lot sizes and current venue filters. These wrappers prevent zero but do not yet validate symbol filters. Fee records retain signed atomic amounts and actual asset. No floating point is used for executable quantities.

Order identity is (session_id, client_order_id); events additionally have session-wide sequence uniqueness. Unknown submission states must remain unresolved until reconciliation. Database schema is a foundation: atomic event-and-state transitions, fill deduplication, balanced ledger postings and crash recovery are not implemented. Traits are synchronous boundary sketches, not working exchange adapters.

Target lifecycle: pending -> open/partially_filled -> filled/cancelled, with rejection or unknown states as appropriate. Real fill evidence, never a price touch, drives transitions. Risk gates precede every order and cancellation races require reconciliation. All live transport and credential handling are absent in M1.

Future profitability gates require out-of-sample historical tests, realistic queue and latency assumptions, forward paper results and stress scenarios. Report net expectancy after maker/taker fees in their actual assets, adverse selection, slippage, inventory mark-to-market and operating costs. No profitability threshold is asserted yet.


## Milestone 2 implementation

`mm-market-data` contains a Binance protocol decoder/transport, interval-sequenced local book, bounded recorder, and deterministic replay. Two independent symbol tasks connect to public streams first and buffer while REST snapshots are in flight. Normalized data uses exact 1e-8 integers and explicit deletion quantities. Books are synchronized only after a bridging update. Any invalidation clears public quotes and rebuilds from a new connection/session. The transport queue and synchronization buffer are separately bounded; event-loop overload is a gap, never silent dropping while keeping a valid book.

The collector writes schema-versioned status via atomic rename every 250 ms. The TypeScript API validates this document and independently recomputes both heartbeat and feed freshness. Books, top 20 levels, known depth counts, spread, lag and lifecycle counters are exposed read-only. This filesystem contract replaces the need for another internal network server; it requires the two processes to share a local filesystem. The existing M1 API/DB/domain capabilities are preserved.

Each symbol owns one recorder lock and independent bounded Snappy Parquet segments with checkpoint, raw/normalized records, connection IDs and SHA-256 sidecars. Receive records are distinct from successfully applied updates. Replay applies only snapshot/apply/invalidation events in recorded order. Default bounds, retention and failure behavior are documented in README.md. No database migration was needed for market-data storage; PostgreSQL remains the M1 durable trading foundation. M2 contains no order transport or strategy.

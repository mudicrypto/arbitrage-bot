# Milestone 2 — Binance Market Data Integration

Validation date: 2026-10-17. M1 and M2 are approved; M3 is not started.

## Persistence and environment preflight

- Verified M1 commit `bd20c58779d5ce4554bdeeadaae86d23d44ba5cf` and an initially clean working tree.
- Origin is the requested `mudicrypto/arbitrage-bot` repository. It initially had no branches. An ordinary push created `origin/main`; the exact remote M1 SHA was verified. Local `work` tracks `origin/main`. No force push or remote overwrite was used.
- Confirmed Rust 1.90.0, Node v24.19.0, npm 11.9.0, Docker and PostgreSQL 17.6. All six M1 Rust/API tests and the idempotent migration/database checks passed before implementation.
- Read the saved environment draft. Added only the two public Binance market-data domains while preserving package/GitHub presets. The initial REST CONNECT proxy 403 was resolved in the running machine; both live feeds subsequently synchronized. No secret or API key was requested.
- Updated reusable install/start instructions for the collector and API. Publication remains a manual UI step; see [environment instructions](ENVIRONMENT.md). A draft save is not proof of publication or fresh-task restoration.

## Implemented

- Rust `mm-market-data` crate and CLI, with Binance-only public transport and protocol decoding separated from normalized book, recorder and replay modules. Existing M1 domain/exchange interfaces are preserved.
- Independent BTCUSDT and ETHUSDT depth streams (`@depth@100ms`) and REST snapshots, using official market-data-only endpoints.
- Open WebSocket first; buffer events during snapshot retrieval; retry old snapshots; discard obsolete/duplicate updates; require a bridging sequence interval and contiguous subsequent coverage. Apply absolute quantities, including zero-quantity deletion, using exact integers.
- Detect sequence gaps, stale/future exchange timestamps, silent feeds, disconnections, malformed messages, locked/crossed/empty books, exhausted known snapshot coverage and capacity overflow. Invalidate the book, reconnect with bounded backoff, and rebuild. Echo ping payloads; rotate sessions after 23 hours. Snapshot throttling and REST 418/429 Retry-After cooldown prevent rapid retry loops.
- Versioned raw UTF-8 and normalized records with receive/exchange timestamps, sequence ranges, record sequence, recording ID and connection-session UUID. Received and successfully applied events are distinct.
- Bounded Snappy Parquet segments, pre-event replay checkpoints, fsync/atomic publication, SHA-256 verification, retention across restarts and exclusive writer locks. Unrelated local files are preserved. Recording failure stops the affected symbol and clears quotes.
- Deterministic segment replay validates checksums, schema and record sequence, then reconstructs book state without network or wall-clock dependencies.
- TypeScript read-only `/markets`, `/markets/BTCUSDT`, `/markets/ETHUSDT`, and Prometheus `/metrics` endpoints. Quotes/spread and update IDs are strings; known depth and top 20 levels are exposed. Both feed and collector freshness are checked, including when the Rust process freezes.
- Process-local monitoring: connection attempts/reconnections, synchronization failures, gaps, duplicates, invalid messages, stale events, disconnections, overflows, recording failures, updates, lag and age.

Official sources reviewed: [local-book synchronization, pings and connection lifetime](https://github.com/binance/binance-spot-api-docs/blob/master/web-socket-streams.md), [public-only endpoints](https://github.com/binance/binance-spot-api-docs/blob/master/faqs/market_data_only.md), and [REST order-book/rate-limit documentation](https://github.com/binance/binance-spot-api-docs/blob/master/rest-api.md). No private endpoint or order request was used.

## Exact validation

Final local checks:

| Command/check | Result |
|---|---|
| `cargo build --workspace --locked` | Passed, including collector/replay binary |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 20 passed, 0 failed, 0 ignored; zero doc tests |
| `npm --prefix control-api run format:check` | Passed |
| `npm --prefix control-api run typecheck` | Passed |
| `npm --prefix control-api test` | Build passed; 6 tests passed, 0 failed/skipped |
| `npm ci --prefix control-api --ignore-scripts` | Passed from committed lockfile |
| `bash scripts/install-cloud.sh` | Passed: tool activation, frozen installs, build/checks and image pull |
| PostgreSQL migration, initial and repeated application | Passed |
| `scripts/db-test.sql` through `psql -v ON_ERROR_STOP=1` | Passed: live mode, duplicate order ID and zero price rejected; fee asset retained |
| `git diff --check` | Passed |

Rust test breakdown: existing domain 2 and exchange 1; book/decimal/property tests 6; recording/replay tests 4; local REST/WebSocket runtime/fault tests 7. Runtime recovery test exercises six separate faults: missing ranges, interrupted socket, crossed book, silent feed, stale timestamp and malformed message. Other tests verify snapshot buffering, old-snapshot retry, duplicate/out-of-order old events, zero deletion, limits, recording failure, both symbols and stale status output. Tests use real local HTTP/WebSocket transport; remote Binance is not needed for deterministic CI. Remote GitHub Actions execution has not been independently verified.

Live smoke checks:

1. Both actual Binance Spot streams synchronized; REST snapshots contained 5000 levels per side. The API served fresh, uncrossed books with zero gaps/sync/recording failures in the captured run.
2. Stopped the collector with SIGINT. Final status showed both markets stopped with quotes cleared; buffered recordings were flushed.
3. Replayed 42 finalized live Parquet segments (21 per symbol), containing 1,798 records, twice. Outputs were byte-for-byte identical (`cmp`). Final reconstructed update IDs and best bid/ask exactly matched the saved live API sample for both symbols. Recorded Parquet sizes at that checkpoint were 2,085,468 bytes for BTCUSDT and 2,084,339 bytes for ETHUSDT.
4. Restarted the collector and API, then paused the collector for 6.5 seconds. The API marked both markets stale and removed quotes. After resume, both streams automatically reconnected and rebuilt; each reported one reconnection and no recording failures.

## Run

From `/workspace/arbitrage-bot`, activate cloud Rust as documented in README.md, then:

```sh
npm ci --prefix control-api --ignore-scripts
cargo build --workspace --locked
./target/debug/mm-market-data collect
```

In another managed terminal, from `/workspace/arbitrage-bot/control-api`:

```sh
npm run build
TRADING_MODE=paper npm start
```

Inspect the read-only API locally on port 3001. `GET /health` verifies the API process; `GET /markets` must report both markets synchronized and fresh to establish data readiness. Default local recording/status paths and overrides are documented in README.md. Stop only collector/API processes you own; do not launch duplicate writers.

For replay of finalized recordings:

```sh
./target/debug/mm-market-data replay .local/market-data > /tmp/market-replay.jsonl
```

Pause/stop collection while comparing two directory replay runs so retention cannot change the input set. Each output line is one segment's reconstructed final state.

## Verified live order-book sample

The excerpt below was fetched from the TypeScript API and matched exactly by the final live-data replay. It is a historical sample, not a current quotation. Full excerpt including counts/age is in [market-data-sample.json](market-data-sample.json).

| Symbol | Update ID | Best bid | Best ask | Spread | Known bid/ask levels | Age at request |
|---|---|---|---|---|---|---|
| BTCUSDT | 101564479612 | 81600.57000000 | 81600.58000000 | 0.01000000 | 5009 / 4976 | 89 ms |
| ETHUSDT | 82170147659 | 2427.62000000 | 2427.63000000 | 0.01000000 | 4957 / 5049 | 83 ms |

## Bounds, known limitations and outstanding risks

- Per symbol: transport queue 64 messages, synchronization buffer 64 events, input/snapshot at most 1 MiB, known book at most 10,000 levels/side, encoded record at most 2 MiB. Frame and queue bounds also apply during REST synchronization. Overflow invalidates coverage and reconnects; it never silently keeps a valid book.
- Parquet target: 64 rows or approximately 2 MiB, flushing at least every 2 seconds during normal operation. Large checkpoint/record pairs may exceed the target; final files are capped at 8 MiB. Retain at most 32 owned segments/symbol (256 MiB), plus one bounded temporary file and tiny checksum/lock files. This policy deletes old recordings; it is not an archival system. Unrelated files in a user-selected directory are outside the recorder's quota and are not deleted.
- A crash can lose the unflushed tail, normally up to 2 seconds. Slow/blocking storage can increase this; fsync/encoding are synchronous and can cause event-loop lag and a safe resynchronization. No production latency guarantee, sustained soak, process-kill-at-every-write-boundary test, or physical-disk-full test is claimed. Recorder failures are fault-injected with file/size limits.
- The snapshot covers only 5000 levels/side. Unchanged levels outside that range remain unknown; `depth_complete=false` always. Depth counts are known levels, not full liquidity. Exhausting the original best-quote coverage forces a rebuild; no silent pruning is used to satisfy level limits.
- Only BTCUSDT/ETHUSDT and exact 1e-8 normalization are supported. These are data units, not validated executable ticks/lots. Symbol status and dynamic exchange filter metadata must be implemented before executable orders. No private keys, order submission, fills or trading are present.
- Replay is deterministic per retained segment with checkpoints. Eviction intentionally removes history. Directory replay enumerates filenames deterministically but does not merge independent recording sessions into a global chronological feed or validate continuity across missing segments. Raw arrivals rejected because of a transport overflow or crash tail may be unavailable; invalidation records preserve the coverage break when storage is writable.
- Counters reset on process restart. Feed age/lag use local clocks; keep clock synchronization working. Timestamp anomalies fail closed. Retry limits are per process, not coordinated across applications sharing an IP. Extremely long/malformed Retry-After values use the documented bounded cooldown.
- Status-file IPC requires a shared local filesystem. API is unauthenticated and loopback-only; it must not be exposed publicly. Health is not database or market readiness. HTTP CONNECT proxy support is unauthenticated; other proxy protocols/auth schemes are unsupported.
- Environment draft persistence is confirmed; environment publication/fresh-task restoration remain unverified manual steps. Source Git persistence is checked separately from environment snapshots. Local recordings are ignored and not pushed.

## Safety review and next milestone

Simulation/paper configuration remains available; unsupported/live modes still fail closed. `/orders` is absent, `executionAvailable=false`, `liveTradingEnabled=false`. No API keys were requested and no real orders, leverage, margin or futures were enabled. Public live data is permitted; live trading is not.

Suggested M3: deterministic simulated execution/backtesting with explicit queue/latency assumptions, independent risk, durable idempotent OMS, venue metadata, balanced asset ledger, actual fee-asset accounting and crash/reconciliation tests. A touched price must never imply a fill. Proceed only after explicit M3 approval.

# Crypto Market Maker

Research-first Binance Spot market-making foundation. Initial research capital assumption: USD 5,000; no funds are connected. Profitability is unproven. Milestone 1 contains types, interfaces, a read-only control API, and database infrastructure, not a functioning trading strategy.

## Development

Requires Rust 1.90.0 (rustfmt and clippy), Node.js 22+, npm, Docker Engine and Compose v2.

```sh
npm ci --prefix control-api
cargo build --workspace --locked
bash scripts/check.sh
docker compose up -d --wait postgres
docker compose exec -T postgres psql -U marketmaker -d marketmaker -v ON_ERROR_STOP=1 < migrations/001_foundation.sql
docker compose exec -T postgres psql -U marketmaker -d marketmaker -v ON_ERROR_STOP=1 < scripts/db-test.sql
TRADING_MODE=paper npm --prefix control-api start
```

`GET /health` returns the configured mode and explicitly reports execution unavailable. Default mode is simulation. Any other mode, including live, prevents startup. API binds to loopback, default port 3001 (`PORT` override). No exchange keys required; no order endpoints exist. API health does not assert database connectivity.

Database credentials in Compose are public local-development defaults. Database port binds only to loopback. Do not reuse them for deployment. `docker compose down` stops infrastructure; do not add `-v` unless intentionally deleting local data.

Cloud Rust activation: export `CARGO_HOME=/workspace/.tools/cargo`, `RUSTUP_HOME=/workspace/.tools/rustup`, and prepend `$CARGO_HOME/bin` to `PATH`.

See [architecture](ARCHITECTURE.md), [roadmap](ROADMAP.md), [risk policy](RISK_POLICY.md), and [decisions](docs/DECISIONS.md). Next.js dashboard, Parquet recording, strategy execution, realistic fills, and accounting logic are later milestones.

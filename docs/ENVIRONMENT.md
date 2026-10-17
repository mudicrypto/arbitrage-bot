# Cloud environment and persistence

Milestone 1 preflight on 2026-10-17 verified `bd20c58779d5ce4554bdeeadaae86d23d44ba5cf` with a clean working tree. The configured origin is `https://github.com/mudicrypto/arbitrage-bot.git`. The remote had no branches; an ordinary push created `main`, and `git ls-remote origin refs/heads/main` confirmed the exact M1 commit. Local branch `work` tracks `origin/main`; do not assume the local branch name is `main`. Never force push.

Rust 1.90.0 is installed under `/workspace/.tools`, Node.js v24.19.0/npm 11.9.0 are supplied by this machine, and PostgreSQL 17.6 runs in Docker Compose. CI uses Node 22. Frozen npm installs use `/workspace/.tools/npm-cache` because the default home cache is not writable here. The baseline M1 checks and database checks passed before M2 work.

## Configuration and publication

The environment configuration contains `install_script` and `start_skill`. M2 startup instructions supersede the earlier M1-only restriction. Required custom network destinations are `data-api.binance.vision` (REST) and `data-stream.binance.vision` (WebSocket); package-manager and GitHub presets remain intact. No API keys or new secrets are required.

The initial REST probe returned CONNECT proxy 403. After the domain draft update, the running collector successfully established both public WebSocket feeds and REST snapshots with normal TLS verification. This proves connectivity in the current machine, not publication or fresh-task restoration.

Manual steps in the environment UI:

1. Review and save the pending configuration, including both Binance destinations and the complete installation/start instructions.
2. Publish the environment to create its reusable snapshot/configuration version.
3. In a fresh task, follow the saved startup instructions and recheck both markets. This restoration/publication step has not been performed by the agent.

Saving a draft does not itself execute its scripts or establish publication. Processes, WebSocket sessions and Docker-volume contents must not be assumed to survive restoration. Start PostgreSQL and the collector/API again; reapply the idempotent foundation migration. Application source is committed and pushed independently of the environment snapshot. Local recordings are bounded, ignored, disposable research cache, not a durable archive or backup.

## Commands

Use the existing checkout, not a new worktree. From `/workspace/arbitrage-bot`:

```sh
export CARGO_HOME=/workspace/.tools/cargo
export RUSTUP_HOME=/workspace/.tools/rustup
export PATH="$CARGO_HOME/bin:$PATH"
export npm_config_cache=/workspace/.tools/npm-cache
bash scripts/install-cloud.sh
docker compose up -d --wait postgres
docker compose exec -T postgres psql -U marketmaker -d marketmaker -v ON_ERROR_STOP=1 < migrations/001_foundation.sql
docker compose exec -T postgres psql -U marketmaker -d marketmaker -v ON_ERROR_STOP=1 < scripts/db-test.sql
```

Start the processes using the working directories in README.md. Use managed terminals; reuse a healthy process rather than launching duplicates. The recorder rejects concurrent writers in each symbol directory. Verify `/health`, `/markets`, and `/metrics` with local HTTP requests. Do not expose localhost previews. Do not enable trading. Stop after M3; M4 requires approval.

## Milestone 3 update

M1–M3 are approved; stop before M4. The same installation script builds `mm-research` through the Rust workspace, without additional services, domains or credentials. Startup instructions now include read-only historical research routes and optional explicitly requested public collection. The API can serve committed preliminary reports without raw captures; replay requires the original local `.local/research/run-*` inputs. Raw data/checksums in the capture audit are not a remote backup. Use an alternate `PORT` or restart an owned API process to load new routes if an old instance occupies port 3001. The current research API was verified on port 3003. Review/save and publish the updated draft for future cloud tasks; publication/restoration have not been performed by the agent.

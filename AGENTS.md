# Development instructions

Use the existing isolated checkout; do not create worktrees unless the user requests one. Read README.md, ARCHITECTURE.md, ROADMAP.md and RISK_POLICY.md first. Implement only the approved milestone. M1 and M2 are approved. Stop after M2; M3 requires user approval.

Never request private exchange keys or submit real orders. Keep live execution unavailable. Use integer executable prices/quantities, deterministic event ordering and explicit fee assets. Unknown submission outcomes require reconciliation. Price touch is not evidence of execution.

Run `bash scripts/check.sh` and the README database checks after relevant changes. Keep lockfiles reproducible. Update decisions, safety limitations and roadmap; report exact test results and commit each milestone. Do not claim unimplemented scaffolding is working trading functionality.

For market-data changes, run deterministic local WebSocket/REST fault tests in `crates/market-data/tests`; preserve bounded queues/storage, checksum verification and the fail-closed API freshness policy. Live public data collection is allowed; live trading is not. See docs/ENVIRONMENT.md for tool activation and cloud publication.

# Development instructions

Use the existing isolated checkout; do not create worktrees unless the user requests one. Read README.md, ARCHITECTURE.md, ROADMAP.md and RISK_POLICY.md first. Implement only the approved milestone. M1–M5 are approved. M5 connects public live data to funded simulated execution with research/economic profiles. Stop after M5; M6/live execution requires fresh explicit approval.

Never request private exchange keys or submit real orders. Keep live execution unavailable. Use integer executable prices/quantities, deterministic event ordering and explicit fee assets. Unknown submission outcomes require reconciliation. Price touch is not evidence of execution.

Run `bash scripts/check.sh` and the README database checks after relevant changes. Keep lockfiles reproducible. Update decisions, safety limitations and roadmap; report exact test results and commit each milestone. Do not claim unimplemented scaffolding is working trading functionality.

For market-data changes, run deterministic local WebSocket/REST fault tests in `crates/market-data/tests`; preserve bounded queues/storage, checksum verification and the fail-closed API freshness policy. Live public data collection is allowed; live trading is not. See docs/ENVIRONMENT.md for tool activation and cloud publication.

Research changes must retain real-observation provenance, exact fee arithmetic, manual/unverified fee labels, deterministic persisted/replay consistency, bounded collection, and preliminary conclusions. Never manufacture a winning candidate. See config/research.json, docs/MILESTONE_3.md and reports/milestone3/.

Backtesting must reuse M2/M3 recording and replay, require aggressive-trade execution evidence, conserve each asset, retain latency/cancel races and report queue uncertainty. See docs/MILESTONE_4.md. Future final out-of-sample observations must stay untouched during development. Long capture processes are public data only and may not survive cloud restoration.

M4.1 diagnostic counts must reconcile, distinguish exact-price from trade-through evidence, and keep synthetic tests, historical simulated fills and actual executions separate. Preserve original M4 reports and future OOS data; audit live capture integrity without running strategies on it. See docs/MILESTONE_4_1.md.

M4.2 must preserve the existing running collector. Use explicit isolated capital alternatives or validated partitions summing to one $5000 budget. Initial holdings are priced accounting allocations, never claimed executions. Only completed valid session-symbols enter batch research; do not sum independent experiments or silently reset equity over missing intervals. See docs/MILESTONE_4_2.md.

M5 paper wallets must preserve capital/fee/drawdown state on restart; void virtual orders and require fresh books after outages. Research may bypass fee-width guards only with explicit research labeling; it still pays configured fees and retains all funding, post-only, position and drawdown controls. Never call paper fills actual exchange executions. See MILESTONE_5.md.

# Milestone 4.2 artifacts

- `batch/comparisons.json`, CSV and `traces/`: historical **simulations** on the old completed M3 source, training partitions only. Capital scope is explicit; independent market/scenario/model/session alternatives must not be summed. `batch_complete.json` is required to interpret the output.
- `recorded_trade_through_case.json` and `trade_through_case_results.json`: a fixed recorded-order execution regression. Two original fee scenarios reused one aggregate trade. Simulated fill behavior is not a strategy return or an actual exchange execution.
- `exchange_metadata.json`: frozen current official public filters; `exchange_metadata_initial.json` preserves the earlier snapshot. Manual fees are unverified account assumptions.
- `preflight_readiness.json`: fully replayed prefix checkpoint before implementation. `current_capture_inventory.json`: later elapsed-time, retained-file checksum and disk inventory. `current_suffix_replay.json`: double reconstruction of a copied limited suffix. These are distinct audit scopes, not a fully validated continuous sixteen-hour dataset or a completed day.
- `verification.json`: engineering/synthetic test results, never market-profitability evidence.

See [M4.2 report](../../docs/MILESTONE_4_2.md) for results, limits and reproduction. Live trading remains disabled. No M5 work is authorized.

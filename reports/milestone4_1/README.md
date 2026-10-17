# Milestone 4.1 report interpretation

`execution_diagnostics.json`, CSV/Markdown and `decisions/*.jsonl` describe **historical simulated decisions**, rerunning only the original 54 evaluated partitions. They contain no synthetic test fills or actual exchange executions. The frozen experiment is copied as `experiment.json`. Original M4 financial summaries are preserved and matched exactly.

`data_readiness.json` is an **observed recording integrity audit**, with its own frozen finalized-file list, timestamp, checksums, coverage gaps, byte usage and two-pass replay digest. It is not a strategy run or a completed-day assertion. Subsequent live files and unflushed tails are outside that checkpoint.

`simulator_validation.json` describes **synthetic test assertions**, never market observations or profitability. Execution and ledger tests are local models. No live order transport or private credentials exist.

Counters use different populations: quote opportunities, newly generated intents, submitted orders and overlapping reach flags. Exact-price non-reach is separate from live trade-through omissions. Post-cancel matches are counterfactual diagnostics, not fills; queue estimates are not directly observed ranks. Zero empirical fills do not provide a calibrated fill probability or prove universal absence of an edge.

See [M4.1 methods and findings](../../docs/MILESTONE_4_1.md). Raw local capture is required for reproduction; these reports and hashes do not archive it. No final OOS outcomes were evaluated, and no Milestone 5 work is authorized.

# Milestone 4 report interpretation

Backtest CSV/JSON/JSONL files are **simulated results on observed short public data**. Hypothetical fee and stress cases are identified by scenario and the copied `experiment.json`; none represents verified account access. The audit and long-collection status files describe **observed recording provenance**, not own execution. `split_manifest.json` and `experiment.json` are experiment declarations, not market observations.

No fills or profitable edge were demonstrated. Empty validation/test intervals, missing markouts and zero turnover must not be interpreted as successful validation. The original M3 sample was already inspected before M4. Final OOS evaluation was not invoked; future data must remain untouched until configuration freeze.

See ../../docs/MILESTONE_4.md for exact methods, tests, source availability, resource caps, ongoing capture and known limitations. Action logs include submissions, modeled accept/cancel transitions and coverage loss, never real order submissions. Raw source recordings are ignored local files and are not archived by these reports. CSV daily P&L contains partial-day values only; no annualized claim is made.

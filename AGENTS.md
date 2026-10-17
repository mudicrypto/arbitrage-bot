# Development instructions

Use the existing isolated checkout; do not create worktrees unless the user requests one. Read README.md, ARCHITECTURE.md, ROADMAP.md and RISK_POLICY.md first. Implement only the approved milestone. M2 requires user approval.

Never request private exchange keys or submit real orders. Keep live execution unavailable. Use integer executable prices/quantities, deterministic event ordering and explicit fee assets. Unknown submission outcomes require reconciliation. Price touch is not evidence of execution.

Run `bash scripts/check.sh` and the README database checks after relevant changes. Keep lockfiles reproducible. Update decisions, safety limitations and roadmap; report exact test results and commit each milestone. Do not claim unimplemented scaffolding is working trading functionality.

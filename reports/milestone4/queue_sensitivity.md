# Queue-model sensitivity — simulated, preliminary

Compare standard_conservative (trade-only depletion) with standard_cancel_credit (proportional cancellation advancement; optimistic modeling alternative). Cancellations never directly create fills. All orders require exact-price aggressive-trade evidence, entry eligibility and depleted queue. Other scenario differences are not isolated queue effects.

| Market | Scenario | Split | Fills | P&L USDT |
|---|---|---|---:|---:|
| USDCUSDT | standard_conservative | train | 0 | 0.00000000 |
| USDCUSDT | standard_cancel_credit | train | 0 | 0.00000000 |
| BTCUSDT | standard_conservative | train | 0 | 0.00000000 |
| BTCUSDT | standard_cancel_credit | train | 0 | 0.00000000 |
| ETHUSDT | standard_conservative | train | 0 | 0.00000000 |
| ETHUSDT | standard_cancel_credit | train | 0 | 0.00000000 |
| USD1USDT | standard_conservative | train | 0 | 0.00000000 |
| USD1USDT | standard_cancel_credit | train | 0 | 0.00000000 |
| SOLUSDT | standard_conservative | train | 0 | 0.00000000 |
| SOLUSDT | standard_cancel_credit | train | 0 | 0.00000000 |
| ZECUSDT | standard_conservative | train | 0 | 0.00000000 |
| ZECUSDT | standard_cancel_credit | train | 0 | 0.00000000 |

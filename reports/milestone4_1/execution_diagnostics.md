# M4.1 execution diagnostics — historical simulations, preliminary

These diagnostics rerun only the previously evaluated partitions with the frozen M4 configuration. No synthetic fills or actual exchange executions are included. Retrospective post-cancel matches are not fills. Counts across independent scenarios are not unique market events.

| Market | Scenario | Opportunities | Intents | Risk rejections | Submitted | Never reached | Queue blocked | No-fill explanation |
|---|---|---:|---:|---:|---:|---:|---:|---|
| USDCUSDT | standard_conservative | 82 | 42 | 41 | 1 | 1 | 0 | never_reached_by_exact_price_directional_trade |
| USDCUSDT | standard_cancel_credit | 82 | 42 | 41 | 1 | 1 | 0 | never_reached_by_exact_price_directional_trade |
| USDCUSDT | discounted_hypothetical | 82 | 42 | 41 | 1 | 1 | 0 | never_reached_by_exact_price_directional_trade |
| USDCUSDT | zero_maker_hypothetical | 82 | 42 | 41 | 1 | 1 | 0 | never_reached_by_exact_price_directional_trade |
| USDCUSDT | maker_rebate_hypothetical | 82 | 42 | 41 | 1 | 1 | 0 | never_reached_by_exact_price_directional_trade |
| USDCUSDT | stress_cost_latency | 82 | 42 | 41 | 1 | 1 | 0 | never_reached_by_exact_price_directional_trade |
| USDCUSDT | interrupted_feed | 70 | 39 | 35 | 4 | 4 | 0 | market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| USDCUSDT | volatility_adjusted | 82 | 42 | 41 | 1 | 1 | 0 | never_reached_by_exact_price_directional_trade |
| USDCUSDT | wider_lower_inventory | 82 | 42 | 41 | 1 | 1 | 0 | never_reached_by_exact_price_directional_trade |
| BTCUSDT | standard_conservative | 84 | 44 | 42 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| BTCUSDT | standard_cancel_credit | 84 | 44 | 42 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| BTCUSDT | discounted_hypothetical | 84 | 44 | 42 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| BTCUSDT | zero_maker_hypothetical | 84 | 44 | 42 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| BTCUSDT | maker_rebate_hypothetical | 84 | 44 | 42 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| BTCUSDT | stress_cost_latency | 84 | 44 | 42 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| BTCUSDT | interrupted_feed | 74 | 41 | 37 | 4 | 4 | 0 | market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| BTCUSDT | volatility_adjusted | 84 | 44 | 42 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| BTCUSDT | wider_lower_inventory | 84 | 44 | 42 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| ETHUSDT | standard_conservative | 84 | 45 | 42 | 3 | 3 | 0 | never_reached_by_exact_price_directional_trade |
| ETHUSDT | standard_cancel_credit | 84 | 45 | 42 | 3 | 3 | 0 | never_reached_by_exact_price_directional_trade |
| ETHUSDT | discounted_hypothetical | 84 | 45 | 42 | 3 | 3 | 0 | never_reached_by_exact_price_directional_trade |
| ETHUSDT | zero_maker_hypothetical | 84 | 45 | 42 | 3 | 2 | 0 | canceled_before_later_directional_trade_counterfactual; never_reached_by_exact_price_directional_trade |
| ETHUSDT | maker_rebate_hypothetical | 84 | 45 | 42 | 3 | 2 | 0 | canceled_before_later_directional_trade_counterfactual; never_reached_by_exact_price_directional_trade |
| ETHUSDT | stress_cost_latency | 84 | 45 | 42 | 3 | 3 | 0 | never_reached_by_exact_price_directional_trade |
| ETHUSDT | interrupted_feed | 72 | 41 | 36 | 5 | 5 | 0 | market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| ETHUSDT | volatility_adjusted | 84 | 45 | 42 | 3 | 3 | 0 | never_reached_by_exact_price_directional_trade |
| ETHUSDT | wider_lower_inventory | 84 | 45 | 42 | 3 | 3 | 0 | never_reached_by_exact_price_directional_trade |
| USD1USDT | standard_conservative | 64 | 38 | 32 | 6 | 6 | 0 | market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| USD1USDT | standard_cancel_credit | 64 | 38 | 32 | 6 | 6 | 0 | market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| USD1USDT | discounted_hypothetical | 64 | 38 | 32 | 6 | 6 | 0 | market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| USD1USDT | zero_maker_hypothetical | 64 | 38 | 32 | 6 | 6 | 0 | market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| USD1USDT | maker_rebate_hypothetical | 64 | 38 | 32 | 6 | 6 | 0 | market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| USD1USDT | stress_cost_latency | 64 | 38 | 32 | 6 | 6 | 0 | market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| USD1USDT | interrupted_feed | 54 | 35 | 27 | 8 | 8 | 0 | market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| USD1USDT | volatility_adjusted | 64 | 38 | 32 | 6 | 6 | 0 | market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| USD1USDT | wider_lower_inventory | 64 | 38 | 32 | 6 | 6 | 0 | market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| SOLUSDT | standard_conservative | 78 | 41 | 39 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| SOLUSDT | standard_cancel_credit | 78 | 41 | 39 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| SOLUSDT | discounted_hypothetical | 78 | 41 | 39 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| SOLUSDT | zero_maker_hypothetical | 78 | 41 | 39 | 2 | 1 | 0 | canceled_before_later_directional_trade_counterfactual; never_reached_by_exact_price_directional_trade |
| SOLUSDT | maker_rebate_hypothetical | 78 | 41 | 39 | 2 | 1 | 0 | canceled_before_later_directional_trade_counterfactual; never_reached_by_exact_price_directional_trade |
| SOLUSDT | stress_cost_latency | 78 | 41 | 39 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| SOLUSDT | interrupted_feed | 70 | 39 | 35 | 4 | 4 | 0 | market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| SOLUSDT | volatility_adjusted | 78 | 41 | 39 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| SOLUSDT | wider_lower_inventory | 78 | 41 | 39 | 2 | 2 | 0 | never_reached_by_exact_price_directional_trade |
| ZECUSDT | standard_conservative | 78 | 58 | 39 | 19 | 18 | 0 | canceled_before_later_directional_trade_counterfactual; never_reached_by_exact_price_directional_trade |
| ZECUSDT | standard_cancel_credit | 78 | 58 | 39 | 19 | 18 | 0 | canceled_before_later_directional_trade_counterfactual; never_reached_by_exact_price_directional_trade |
| ZECUSDT | discounted_hypothetical | 78 | 58 | 39 | 19 | 18 | 0 | canceled_before_later_directional_trade_counterfactual; never_reached_by_exact_price_directional_trade |
| ZECUSDT | zero_maker_hypothetical | 78 | 58 | 39 | 19 | 16 | 0 | canceled_before_later_directional_trade_counterfactual; never_reached_by_exact_price_directional_trade; trade_through_evidence_omitted_by_exact_price_fill_model |
| ZECUSDT | maker_rebate_hypothetical | 78 | 58 | 39 | 19 | 16 | 0 | canceled_before_later_directional_trade_counterfactual; never_reached_by_exact_price_directional_trade; trade_through_evidence_omitted_by_exact_price_fill_model |
| ZECUSDT | stress_cost_latency | 78 | 58 | 39 | 19 | 18 | 0 | canceled_before_later_directional_trade_counterfactual; never_reached_by_exact_price_directional_trade |
| ZECUSDT | interrupted_feed | 72 | 52 | 36 | 16 | 15 | 0 | canceled_before_later_directional_trade_counterfactual; market_data_coverage_lost; never_reached_by_exact_price_directional_trade |
| ZECUSDT | volatility_adjusted | 78 | 58 | 39 | 19 | 18 | 0 | canceled_before_later_directional_trade_counterfactual; never_reached_by_exact_price_directional_trade |
| ZECUSDT | wider_lower_inventory | 78 | 58 | 39 | 19 | 19 | 0 | never_reached_by_exact_price_directional_trade |

Zero fills provide insufficient evidence of executable edge; they are not evidence that a market never has an edge. Report funnel gates, queue assumptions and missing history separately. A zero empirical fill fraction is not a calibrated probability estimate. Fee guard widening is counted separately from explicit fee-cap rejection. See docs/MILESTONE_4_1.md for methods, tests and data readiness.

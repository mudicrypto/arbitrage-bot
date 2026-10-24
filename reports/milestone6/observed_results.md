# M6 observed comparisons

All rows describe isolated $5,000 simulated alternatives; never sum their P&L or fills. Dates in JSON are UTC milliseconds. No actual exchange executions occurred. Standard maker fees are manual/unverified 10bps per side, no discount, rebate or promotion.

## Markets: actual 15-minute public sample

Volatility is the existing robust one-second RMS return statistic in bps, not annualized volatility. Depth is known displayed depth per side within 10bps, cancellable and not guaranteed execution. Trade frequency counts underlying exchange trades; aggregate message counts are separate.

| Market | Spread p10 / p50 / p90 bps | 10bps depth bid / ask USDT | Volatility bps | Trades/s | Book updates | Aggregate trade messages |
|---|---:|---:|---:|---:|---:|---:|
| BTCUSDT | 0.001240 / 0.001241 / 0.001242 | 6,882,830 / 6,870,534 | 0.2077 | 19.39 | 8937 | 4179 |
| ETHUSDT | 0.042234 / 0.042269 / 0.042366 | 2,330,224 / 2,368,632 | 0.3150 | 10.99 | 8721 | 3152 |
| SOLUSDT | 0.985756 / 0.986826 / 0.988191 | 1,356,927 / 838,069 | 0.3680 | 4.05 | 6486 | 978 |
| ZECUSDT | 0.086619 / 0.086978 / 0.087324 | 140,665 / 247,178 | 0.8334 | 4.64 | 7783 | 3060 |

SOL had the widest quoted spread of these four markets, still below 1bps at the median, versus 20bps assumed round-trip fees and a 7bps allowance. BTC had the deepest displayed liquidity and greatest trade activity but the narrowest spread. ZEC had less depth and higher observed volatility. None was an economic winner. Complete 60/300/900-second statistics, 5/10/25/50bps depth, quote stability, imbalance and reference moves are in `study/market_comparison.json`.

## Final test: conservative model, received-asset fees

The three non-research strategies each had zero fills for every market, fee assumption and execution sensitivity in every fold. Their final-test MTM exactly equaled initial holdings appreciation: BTC +0.83695990, ETH +0.83455600, SOL +2.22030000, ZEC +2.40870000 USDT. Trading edge is **N/A**, not zero, because there was no executed notional. Final-test maximum drawdown for each of these strategies was respectively 0.39971890, 1.69024000, 1.97360000 and 5.42500000 USDT. All three retained the existing risk limits.

The research-only touch control produced the following **simulated** results:

| Market | Fills | Gross capture USDT | Maker fees USDT | Net MTM USDT | Initial-hold P&L USDT | Excess over hold USDT | Capture less fees bps | Conservative cost-stressed bps | Max drawdown USDT |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| BTCUSDT | 3 | 0.00001860 | 0.29991922 | 0.50069801 | 0.83695990 | -0.33626189 | -9.9994 | -111.0669 | 0.45988132 |
| ETHUSDT | 2 | 0.01055000 | 0.19979506 | 0.63855894 | 0.83455600 | -0.19599706 | -9.4720 | -155.2135 | 1.55520000 |
| SOLUSDT | 2 | 0.00000000 | 0.19994108 | 1.95133892 | 2.22030000 | -0.26896108 | -10.0000 | -160.7964 | 1.81584000 |
| ZECUSDT | 10 | 0.03151000 | 0.61603581 | 1.85917855 | 2.40870000 | -0.54952145 | -9.4885 | -64.2486 | 4.99590857 |

Every bps result uses total one-way executed notional (sum of buy and sell notionals), not starting capital. Do not compare it directly with a full two-leg quoted spread without accounting for the denominator. The cost-stressed metric includes full funded-inventory liquidation stress; low turnover makes it much more negative. It is a conservative screening envelope, not an unbiased estimate of mean fill expectancy or an actual fee charged to the ledger.

## Queue and latency sensitivity: research-only control, final test, received fees

| Market | Assumption | Simulated fills | Capture less fees bps | Cost-stressed bps |
|---|---|---:|---:|---:|
| BTCUSDT | conservative | 3 | -9.9994 | -111.0669 |
| BTCUSDT | queue2_latency250 | 1 | -9.9994 | -304.6740 |
| BTCUSDT | optimistic_diagnostic_only | 11 | -10.0393 | -72.6906 |
| ETHUSDT | conservative | 2 | -9.4720 | -155.2135 |
| ETHUSDT | queue2_latency250 | 1 | -10.9078 | -303.1725 |
| ETHUSDT | optimistic_diagnostic_only | 44 | -9.7672 | -39.1968 |
| SOLUSDT | conservative | 2 | -10.0000 | -160.7964 |
| SOLUSDT | queue2_latency250 | 1 | -10.4931 | -317.1380 |
| SOLUSDT | optimistic_diagnostic_only | 14 | -10.1011 | -53.7251 |
| ZECUSDT | conservative | 10 | -9.4885 | -64.2486 |
| ZECUSDT | queue2_latency250 | 5 | -9.1371 | -142.5536 |
| ZECUSDT | optimistic_diagnostic_only | 45 | -10.1702 | -34.8568 |

Doubling estimated queue ahead and increasing entry/cancel latency to 250ms reduced conservative final-test fills. The alternative-through model produced more fills but remained fee-negative and is explicitly excluded from qualification. Quote-asset variants are in the full JSON/CSV; neither asset assumption produced a qualifying strategy. Partial fills count as separate execution events, not independent statistical samples.

## Attribution and qualification

Let N be exact marked equity minus initial capital, H the no-trade initial-holdings P&L, G gross reference-mid spread capture, F actual modeled maker fees, A the sum of unfavorable 60-second fill markouts, C stressed liquidation cost of the entire remaining base inventory, and R a 7bps round-trip allowance (3.5bps per one-way executed notional).

- Actual simulated MTM is N, with realized/unrealized components and actual fee expenses reconciled in each row.
- Initial inventory appreciation is H; incremental inventory P&L is N + F − G − H.
- Trading excess over initial holdings is N − H; it can still contain incremental inventory effects.
- Conservative trading screen is min(N − H, G − F − A) − C − R. Do not subtract this diagnostic again from actual P&L. No favorable markout or inventory gain rescues a negative spread/fee result.

All 288 rows passed source/freshness checks. All failed the predeclared observation-duration floor and nonpositive conservative trading-cost screen; 286 failed the fill-count floor. All research rows and optimistic models are excluded regardless of P&L. No non-research candidate was nominated by training, so none advanced as a claimed winner. Validation and final-test results are pre-registered diagnostic comparisons, never a tuning objective. The one-hour and 100-fill minimum per holdout cell is only a necessary evidence floor, not statistical proof. The 15-minute source cannot meet that duration requirement.

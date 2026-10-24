# Alternative-venue economics — high-level comparison only

No alternative venue was connected, funded or backtested. No order, transfer, API key, leverage or allocation is authorized. **None is recommended as a demonstrated profitable alternative.**

For a completed buy/sell cycle, a necessary theoretical condition is:

`gross capture > maker_buy + maker_sell + adverse selection + inventory/rebalance costs + safety margin`.

Venue comparisons must use executed two-sided economics, not volume, a fee headline or quoted spread alone. A $5,000 balance does not determine a volume-based fee tier: actual trailing executed volume, account/jurisdiction, asset pair and fee-payment method matter. Fragmenting that balance or repeatedly moving inventory introduces additional costs and constraints.

| Spot venue | What would need to improve | Why a cheaper fee headline is insufficient |
|---|---|---|
| Kraken Pro | Verify applicable maker/taker tier and sufficient usable spread/depth for $100 clips | Pair-specific liquidity, queue competition, adverse selection, tick/lot rules and fee assets need new observations. Public schedule/account tier was not verified here. |
| Coinbase Advanced / Exchange | Verify the actual product/account schedule and compare net capture at the applicable volume tier | Advanced/Exchange product schedules must not be treated as interchangeable. Fees, quoting constraints, connectivity and regional availability can change the result. No current rates or eligibility are asserted. |
| Binance.US, where available | Independently verify spot-market eligibility, pair-level commissions and liquidity | This is a separate venue; Binance global books, fee assumptions and depth cannot be transferred to it. No zero-fee entitlement or equivalent execution quality is assumed. |

Current public fee pages could not be retrieved through this environment (CONNECT 403); URLs, errors and attempted retrieval timestamps are in `sources/fee_source_checks.json`. The accessible official Binance commission FAQ was used only for commission semantics. It explicitly says its numeric examples are fictional; those examples are **not** evidence of an applicable fee tier.

The existing **manual assumption** remains 10bps maker and taker per side with no discount/rebate/promotion, tax/special extras assumed zero but unverified. M6 compares received-asset commission (buy in base, sell in USDT) with M5's quote-asset simplification. No actual account-specific commission can be verified without prohibited private access; no credentials were requested.

For context only, dividing the observed median quoted spread by two gives a zero-other-cost upper bound on a symmetric maker fee of approximately 0.000620bps BTC, 0.021135bps ETH, 0.493413bps SOL and 0.043489bps ZEC. This assumes capturing the entire displayed spread on both sides, which the bot has not demonstrated. The fee ceiling falls further after adverse selection and risk costs; with the current 7bps allowance, all four median quoted spreads fail even before positive maker fees. This arithmetic neither assumes hypothetical free trading nor proves an alternative venue can supply an executable edge.

No viable standard-fee strategy was identified in this study. Lower verified fees **and** better observed execution economics would be required to justify another controlled research comparison. The current evidence does not justify silently relaxing risk limits or switching venues with real funds.

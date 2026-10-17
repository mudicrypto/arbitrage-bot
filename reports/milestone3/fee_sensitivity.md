# Manual fee and adverse-selection sensitivity

Unverified assumptions, not account commission rates. No rebate or promotion entitlement is assumed. Hypothetical zero fees do not establish eligibility. BNB conversion price is a manual scenario assumption, not an observed market price. Discount applies only to standard commission components. Worst active fee rate across observed timestamps is used for screening.

- **manual_standard**: Unverified conservative manual assumption: 10bps maker and 10bps taker each side; no rebate or promotion.
- **hypothetical_bnb_discount**: Unverified 25% standard-commission discount, fees in BNB; conversion assumes 1000 USDT/BNB solely for fee-asset arithmetic, not a market observation.
- **hypothetical_zero_maker**: Hypothetical promotional maker-zero scenario only; eligibility is NOT asserted; taker remains 10bps.

Net bps = median quoted spread − maker buy/sell fees − max(observed passive markout proxy, adverse floor) − inventory allowance − expected rebalance fraction × (taker fee + slippage) − safety margin. Proxy markouts are not conditional on our fills. No daily P&L or trade count is projected.

| Pair | Manual fee scenario | Adverse floor | Used adverse | Maker roundtrip | Rebalance | Net theoretical bps |
|---|---|---:|---:|---:|---:|---:|
| SOLUSDT | manual_standard | 0.000 | 0.000 | 20.000 | 1.100 | -24.142 |
| SOLUSDT | manual_standard | 1.000 | 1.000 | 20.000 | 1.100 | -25.142 |
| SOLUSDT | manual_standard | 3.000 | 3.000 | 20.000 | 1.100 | -27.142 |
| SOLUSDT | manual_standard | 10.000 | 10.000 | 20.000 | 1.100 | -34.142 |
| SOLUSDT | hypothetical_bnb_discount | 0.000 | 0.000 | 15.000 | 0.850 | -18.892 |
| SOLUSDT | hypothetical_bnb_discount | 1.000 | 1.000 | 15.000 | 0.850 | -19.892 |
| SOLUSDT | hypothetical_bnb_discount | 3.000 | 3.000 | 15.000 | 0.850 | -21.892 |
| SOLUSDT | hypothetical_bnb_discount | 10.000 | 10.000 | 15.000 | 0.850 | -28.892 |
| SOLUSDT | hypothetical_zero_maker | 0.000 | 0.000 | 0.000 | 1.100 | -4.142 |
| SOLUSDT | hypothetical_zero_maker | 1.000 | 1.000 | 0.000 | 1.100 | -5.142 |
| SOLUSDT | hypothetical_zero_maker | 3.000 | 3.000 | 0.000 | 1.100 | -7.142 |
| SOLUSDT | hypothetical_zero_maker | 10.000 | 10.000 | 0.000 | 1.100 | -14.142 |
| ETHUSDT | manual_standard | 0.000 | 0.045 | 20.000 | 1.100 | -25.103 |
| ETHUSDT | manual_standard | 1.000 | 1.000 | 20.000 | 1.100 | -26.059 |
| ETHUSDT | manual_standard | 3.000 | 3.000 | 20.000 | 1.100 | -28.059 |
| ETHUSDT | manual_standard | 10.000 | 10.000 | 20.000 | 1.100 | -35.059 |
| ETHUSDT | hypothetical_bnb_discount | 0.000 | 0.045 | 15.000 | 0.850 | -19.853 |
| ETHUSDT | hypothetical_bnb_discount | 1.000 | 1.000 | 15.000 | 0.850 | -20.809 |
| ETHUSDT | hypothetical_bnb_discount | 3.000 | 3.000 | 15.000 | 0.850 | -22.809 |
| ETHUSDT | hypothetical_bnb_discount | 10.000 | 10.000 | 15.000 | 0.850 | -29.809 |
| ETHUSDT | hypothetical_zero_maker | 0.000 | 0.045 | 0.000 | 1.100 | -5.103 |
| ETHUSDT | hypothetical_zero_maker | 1.000 | 1.000 | 0.000 | 1.100 | -6.059 |
| ETHUSDT | hypothetical_zero_maker | 3.000 | 3.000 | 0.000 | 1.100 | -8.059 |
| ETHUSDT | hypothetical_zero_maker | 10.000 | 10.000 | 0.000 | 1.100 | -15.059 |
| BTCUSDT | manual_standard | 0.000 | 0.088 | 20.000 | 1.100 | -25.187 |
| BTCUSDT | manual_standard | 1.000 | 1.000 | 20.000 | 1.100 | -26.099 |
| BTCUSDT | manual_standard | 3.000 | 3.000 | 20.000 | 1.100 | -28.099 |
| BTCUSDT | manual_standard | 10.000 | 10.000 | 20.000 | 1.100 | -35.099 |
| BTCUSDT | hypothetical_bnb_discount | 0.000 | 0.088 | 15.000 | 0.850 | -19.937 |
| BTCUSDT | hypothetical_bnb_discount | 1.000 | 1.000 | 15.000 | 0.850 | -20.849 |
| BTCUSDT | hypothetical_bnb_discount | 3.000 | 3.000 | 15.000 | 0.850 | -22.849 |
| BTCUSDT | hypothetical_bnb_discount | 10.000 | 10.000 | 15.000 | 0.850 | -29.849 |
| BTCUSDT | hypothetical_zero_maker | 0.000 | 0.088 | 0.000 | 1.100 | -5.187 |
| BTCUSDT | hypothetical_zero_maker | 1.000 | 1.000 | 0.000 | 1.100 | -6.099 |
| BTCUSDT | hypothetical_zero_maker | 3.000 | 3.000 | 0.000 | 1.100 | -8.099 |
| BTCUSDT | hypothetical_zero_maker | 10.000 | 10.000 | 0.000 | 1.100 | -15.099 |
| USDCUSDT | manual_standard | 0.000 | 0.000 | 20.000 | 1.100 | -25.000 |
| USDCUSDT | manual_standard | 1.000 | 1.000 | 20.000 | 1.100 | -26.000 |
| USDCUSDT | manual_standard | 3.000 | 3.000 | 20.000 | 1.100 | -28.000 |
| USDCUSDT | manual_standard | 10.000 | 10.000 | 20.000 | 1.100 | -35.000 |
| USDCUSDT | hypothetical_bnb_discount | 0.000 | 0.000 | 15.000 | 0.850 | -19.750 |
| USDCUSDT | hypothetical_bnb_discount | 1.000 | 1.000 | 15.000 | 0.850 | -20.750 |
| USDCUSDT | hypothetical_bnb_discount | 3.000 | 3.000 | 15.000 | 0.850 | -22.750 |
| USDCUSDT | hypothetical_bnb_discount | 10.000 | 10.000 | 15.000 | 0.850 | -29.750 |
| USDCUSDT | hypothetical_zero_maker | 0.000 | 0.000 | 0.000 | 1.100 | -5.000 |
| USDCUSDT | hypothetical_zero_maker | 1.000 | 1.000 | 0.000 | 1.100 | -6.000 |
| USDCUSDT | hypothetical_zero_maker | 3.000 | 3.000 | 0.000 | 1.100 | -8.000 |
| USDCUSDT | hypothetical_zero_maker | 10.000 | 10.000 | 0.000 | 1.100 | -15.000 |
| ZECUSDT | manual_standard | 0.000 | 1.636 | 20.000 | 1.100 | -26.650 |
| ZECUSDT | manual_standard | 1.000 | 1.636 | 20.000 | 1.100 | -26.650 |
| ZECUSDT | manual_standard | 3.000 | 3.000 | 20.000 | 1.100 | -28.014 |
| ZECUSDT | manual_standard | 10.000 | 10.000 | 20.000 | 1.100 | -35.014 |
| ZECUSDT | hypothetical_bnb_discount | 0.000 | 1.636 | 15.000 | 0.850 | -21.400 |
| ZECUSDT | hypothetical_bnb_discount | 1.000 | 1.636 | 15.000 | 0.850 | -21.400 |
| ZECUSDT | hypothetical_bnb_discount | 3.000 | 3.000 | 15.000 | 0.850 | -22.764 |
| ZECUSDT | hypothetical_bnb_discount | 10.000 | 10.000 | 15.000 | 0.850 | -29.764 |
| ZECUSDT | hypothetical_zero_maker | 0.000 | 1.636 | 0.000 | 1.100 | -6.650 |
| ZECUSDT | hypothetical_zero_maker | 1.000 | 1.636 | 0.000 | 1.100 | -6.650 |
| ZECUSDT | hypothetical_zero_maker | 3.000 | 3.000 | 0.000 | 1.100 | -8.014 |
| ZECUSDT | hypothetical_zero_maker | 10.000 | 10.000 | 0.000 | 1.100 | -15.014 |
| USD1USDT | manual_standard | 0.000 | 0.000 | 20.000 | 1.100 | -25.000 |
| USD1USDT | manual_standard | 1.000 | 1.000 | 20.000 | 1.100 | -26.000 |
| USD1USDT | manual_standard | 3.000 | 3.000 | 20.000 | 1.100 | -28.000 |
| USD1USDT | manual_standard | 10.000 | 10.000 | 20.000 | 1.100 | -35.000 |
| USD1USDT | hypothetical_bnb_discount | 0.000 | 0.000 | 15.000 | 0.850 | -19.750 |
| USD1USDT | hypothetical_bnb_discount | 1.000 | 1.000 | 15.000 | 0.850 | -20.750 |
| USD1USDT | hypothetical_bnb_discount | 3.000 | 3.000 | 15.000 | 0.850 | -22.750 |
| USD1USDT | hypothetical_bnb_discount | 10.000 | 10.000 | 15.000 | 0.850 | -29.750 |
| USD1USDT | hypothetical_zero_maker | 0.000 | 0.000 | 0.000 | 1.100 | -5.000 |
| USD1USDT | hypothetical_zero_maker | 1.000 | 1.000 | 0.000 | 1.100 | -6.000 |
| USD1USDT | hypothetical_zero_maker | 3.000 | 3.000 | 0.000 | 1.100 | -8.000 |
| USD1USDT | hypothetical_zero_maker | 10.000 | 10.000 | 0.000 | 1.100 | -15.000 |

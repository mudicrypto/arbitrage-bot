# Preliminary Binance Spot profitability screening

No pair has a plausible positive edge under the baseline manual fees and conservative risk assumptions. No winning candidate is manufactured.

Actual public-data run: 1792248889904–1792249069904 (Unix milliseconds), 180.0 seconds, 6 collected pairs, 1004 synchronized samples. Capital assumption: $5000; no allocation or trading. All fee scenarios are manual and unverified. Replay consistency: true.

Rankings use conservative net edge, quoted clip liquidity, capital fit, volatility, data quality and cross-window stability. A rank is a research priority, not a trading recommendation.

1. **SOLUSDT** — 167 synchronized samples; quality 99.4%; median spread 0.9580bps; baseline net -27.1420bps; conservative window floor -27.1422bps; two-sided 10bps quoted depth $518503.17; clip $99.99; one-second volatility 0.2029bps. Passed bounded research data/liquidity screens. Quoted liquidity is not a fill guarantee; queue position, latency and fill-conditioned selection are unmodeled.

2. **ETHUSDT** — 173 synchronized samples; quality 99.4%; median spread 0.0412bps; baseline net -28.0588bps; conservative window floor -28.0588bps; two-sided 10bps quoted depth $2414237.25; clip $99.80; one-second volatility 0.1475bps. Passed bounded research data/liquidity screens. Quoted liquidity is not a fill guarantee; queue position, latency and fill-conditioned selection are unmodeled.

3. **BTCUSDT** — 176 synchronized samples; quality 99.4%; median spread 0.0012bps; baseline net -28.0988bps; conservative window floor -28.0988bps; two-sided 10bps quoted depth $6104575.68; clip $99.52; one-second volatility 0.1613bps. Passed bounded research data/liquidity screens. Quoted liquidity is not a fill guarantee; queue position, latency and fill-conditioned selection are unmodeled.

4. **USDCUSDT** — 178 synchronized samples; quality 98.9%; median spread 0.0999bps; baseline net -28.0001bps; conservative window floor -28.0001bps; two-sided 10bps quoted depth $9776468.60; clip $99.11; one-second volatility 0.0000bps. Passed bounded research data/liquidity screens. Quoted liquidity is not a fill guarantee; queue position, latency and fill-conditioned selection are unmodeled.

5. **ZECUSDT** — 164 synchronized samples; quality 99.4%; median spread 0.0859bps; baseline net -28.0141bps; conservative window floor -28.0141bps; two-sided 10bps quoted depth $94899.31; clip $98.88; one-second volatility 0.9656bps. Passed bounded research data/liquidity screens. Quoted liquidity is not a fill guarantee; queue position, latency and fill-conditioned selection are unmodeled.

6. **USD1USDT** — 146 synchronized samples; quality 85.4%; median spread 0.1000bps; baseline net -28.0000bps; conservative window floor -28.0000bps; two-sided 10bps quoted depth $5779559.77; clip $99.02; one-second volatility 0.0122bps. Passed bounded research data/liquidity screens. Quoted liquidity is not a fill guarantee; queue position, latency and fill-conditioned selection are unmodeled.

## Limitations

- PRELIMINARY: a short, single-session sample cannot establish profitability, regime robustness or statistical significance.
- No fills or queue position modeled; displayed/quoted depth is cancellable and not a guarantee of executable liquidity. No capital was allocated.
- Manual fee assumptions are unverified; account-specific commissions, promotions, rebates and tax/special components require independent verification before any later execution.
- Passive-counterparty aggregate-trade markouts are proxies, not adverse selection conditional on this strategy's executions. Aggregate trades and one-second samples omit microstructure detail.
- Volatility is robust one-second RMS return in bps (6-MAD winsorization with 1bps floor); rare extreme returns remain in raw recordings and can be understated by this statistic.
- Observation coverage approximates synchronization uptime from occupied one-second bins, not continuous-time uptime. The universe is prioritized by 24h activity for collection, not economic ranking.
- Auditable raw inputs, manifest and online observations: .local/research/run-1792248889904. Fixed bounded recorder retention can invalidate full-run replay for long/high-activity runs; such reports exclude every candidate.
- Requested 30/60/180/600-second statistics include actual observed duration and an explicit requested_window_complete flag. Truncated horizons do not imply longer historical coverage.
- Depth statistics sum known reconstructed levels only. Bands extending beyond the finite snapshot coverage can be understated; displayed depth is a quoted lower bound, not guaranteed executable liquidity.

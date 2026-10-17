import { open } from "node:fs/promises";
export const symbols = ["BTCUSDT", "ETHUSDT"] as const;
export type SymbolName = (typeof symbols)[number];
const metricNames = [
  "connection_attempts",
  "reconnections",
  "synchronization_failures",
  "data_gaps",
  "duplicates",
  "invalid_messages",
  "stale_events",
  "disconnections",
  "buffer_overflows",
  "recording_failures",
  "updates",
  "max_lag_ms",
] as const;
const states = [
  "starting",
  "connecting",
  "synchronizing",
  "synchronized",
  "reconnecting",
  "stale",
  "stopped",
  "recording_failed",
];
interface Market {
  symbol: SymbolName;
  state: string;
  session: string | null;
  last_update_id: string | null;
  last_received_ms: number | null;
  last_exchange_ms: number | null;
  age_ms: number | null;
  best_bid: string | null;
  best_ask: string | null;
  spread: string | null;
  bid_levels: number;
  ask_levels: number;
  bids: string[][];
  asks: string[][];
  depth_complete: false;
  reason: string | null;
  metrics: Record<(typeof metricNames)[number], number> & {
    last_lag_ms: number | null;
  };
}
export interface MarketDocument {
  schema_version: 1;
  generated_at_ms: number;
  stale_after_ms: number;
  markets: Record<SymbolName, Market>;
}
const object = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);
const integer = (v: unknown): v is number =>
  typeof v === "number" && Number.isSafeInteger(v) && v >= 0;
const optionalInteger = (v: unknown) => v === null || integer(v);
const optionalString = (v: unknown) => v === null || typeof v === "string";
const price = (v: unknown): v is string =>
  typeof v === "string" && /^\d{1,12}\.\d{8}$/.test(v);
const levels = (v: unknown) =>
  Array.isArray(v) &&
  v.length <= 20 &&
  v.every((l) => Array.isArray(l) && l.length === 2 && l.every(price));
export function validateStatus(
  value: unknown,
): asserts value is MarketDocument {
  if (
    !object(value) ||
    value.schema_version !== 1 ||
    !integer(value.generated_at_ms) ||
    !integer(value.stale_after_ms) ||
    value.stale_after_ms < 1 ||
    value.stale_after_ms > 60_000 ||
    !object(value.markets)
  )
    throw new Error("Invalid market status document");
  for (const symbol of symbols) {
    const m = value.markets[symbol];
    if (
      !object(m) ||
      m.symbol !== symbol ||
      typeof m.state !== "string" ||
      !states.includes(m.state) ||
      !optionalString(m.session) ||
      !optionalString(m.reason) ||
      !(
        m.last_update_id === null ||
        (typeof m.last_update_id === "string" && /^\d+$/.test(m.last_update_id))
      ) ||
      !optionalInteger(m.last_received_ms) ||
      !optionalInteger(m.last_exchange_ms) ||
      !optionalInteger(m.age_ms) ||
      !integer(m.bid_levels) ||
      !integer(m.ask_levels) ||
      m.bid_levels > 20_000 ||
      m.ask_levels > 20_000 ||
      m.depth_complete !== false ||
      !levels(m.bids) ||
      !levels(m.asks) ||
      ![m.best_bid, m.best_ask, m.spread].every(
        (v) => v === null || price(v),
      ) ||
      !object(m.metrics) ||
      !optionalInteger(m.metrics.last_lag_ms)
    )
      throw new Error(`Invalid ${symbol} status`);
    const metrics = m.metrics;
    if (!metricNames.every((k) => integer(metrics[k])))
      throw new Error("Invalid metrics");
    if (m.state === "synchronized") {
      if (
        !price(m.best_bid) ||
        !price(m.best_ask) ||
        !price(m.spread) ||
        m.last_received_ms === null ||
        m.last_exchange_ms === null ||
        m.last_update_id === null ||
        (m.bids as unknown[]).length === 0 ||
        (m.asks as unknown[]).length === 0
      )
        throw new Error("Synchronized book missing data");
      const atomic = (v: string) => BigInt(v.replace(".", ""));
      if (
        atomic(m.best_bid) >= atomic(m.best_ask) ||
        atomic(m.best_ask) - atomic(m.best_bid) !== atomic(m.spread)
      )
        throw new Error("Invalid spread");
    }
  }
}
export async function readStatus(
  path: string,
  now: number,
): Promise<MarketDocument> {
  const file = await open(path, "r");
  let value: unknown;
  try {
    const buffer = Buffer.alloc(256 * 1024 + 1);
    const { bytesRead } = await file.read(buffer, 0, buffer.length, 0);
    if (bytesRead > 256 * 1024) throw new Error("Status file too large");
    value = JSON.parse(buffer.subarray(0, bytesRead).toString("utf8"));
  } finally {
    await file.close();
  }
  validateStatus(value);
  if (value.generated_at_ms > now + 1000)
    throw new Error("Future collector timestamp");
  for (const m of Object.values(value.markets)) {
    m.age_ms =
      m.last_received_ms === null
        ? null
        : Math.max(0, now - m.last_received_ms);
    const collectorStale = now - value.generated_at_ms > value.stale_after_ms;
    const feedStale =
      m.age_ms === null ||
      m.age_ms > value.stale_after_ms ||
      (m.last_received_ms ?? 0) > now + 1000;
    if (collectorStale || (m.state === "synchronized" && feedStale)) {
      m.state = "stale";
      m.reason = collectorStale ? "collector heartbeat stale" : "feed stale";
    }
    if (m.state !== "synchronized") {
      m.best_bid = null;
      m.best_ask = null;
      m.spread = null;
      m.bids = [];
      m.asks = [];
      m.bid_levels = 0;
      m.ask_levels = 0;
      m.last_update_id = null;
    }
  }
  return value;
}
export function prometheus(document: MarketDocument): string {
  const lines: string[] = [];
  for (const m of Object.values(document.markets)) {
    const label = `{symbol="${m.symbol}"}`;
    lines.push(
      `market_data_synchronized${label} ${m.state === "synchronized" ? 1 : 0}`,
    );
    lines.push(`market_data_age_ms${label} ${m.age_ms ?? -1}`);
    lines.push(
      `market_data_last_lag_ms${label} ${m.metrics.last_lag_ms ?? -1}`,
    );
    for (const name of metricNames)
      lines.push(`market_data_${name}${label} ${m.metrics[name]}`);
  }
  return lines.join("\n") + "\n";
}

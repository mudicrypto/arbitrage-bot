import test from "node:test";
import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createApp } from "../dist/app.js";

function document(now = 100_000) {
  const market = (symbol) => ({
    symbol,
    state: "synchronized",
    session: "fixture-session",
    last_update_id: "9007199254740993",
    last_received_ms: now - 20,
    last_exchange_ms: now - 25,
    age_ms: 20,
    best_bid: "100.00000000",
    best_ask: "102.00000000",
    spread: "2.00000000",
    bid_levels: 2,
    ask_levels: 2,
    bids: [["100.00000000", "2.00000000"]],
    asks: [["102.00000000", "3.00000000"]],
    depth_complete: false,
    reason: null,
    metrics: {
      connection_attempts: 2,
      reconnections: 1,
      synchronization_failures: 0,
      data_gaps: 1,
      duplicates: 2,
      invalid_messages: 0,
      stale_events: 0,
      disconnections: 1,
      buffer_overflows: 0,
      recording_failures: 0,
      updates: 10,
      last_lag_ms: 5,
      max_lag_ms: 12,
    },
  });
  return {
    schema_version: 1,
    generated_at_ms: now,
    stale_after_ms: 5000,
    markets: { BTCUSDT: market("BTCUSDT"), ETHUSDT: market("ETHUSDT") },
  };
}
async function withApi(callback) {
  const dir = await mkdtemp(join(tmpdir(), "mm-api-"));
  const path = join(dir, "status.json");
  const server = createApp("paper", {
    statusFile: path,
    now: () => 100_000,
  }).listen(0, "127.0.0.1");
  await once(server, "listening");
  try {
    await callback({ path, url: `http://127.0.0.1:${server.address().port}` });
  } finally {
    await new Promise((resolve) => server.close(resolve));
    await rm(dir, { recursive: true });
  }
}
test("market API serves exact decimal quotes, sequence IDs, depth, freshness and metrics", async () =>
  withApi(async ({ path, url }) => {
    await writeFile(path, JSON.stringify(document()));
    const response = await fetch(`${url}/markets`);
    assert.equal(response.status, 200);
    assert.equal(response.headers.get("cache-control"), "no-store");
    const body = await response.json();
    assert.equal(body.markets.BTCUSDT.last_update_id, "9007199254740993");
    assert.equal(body.markets.ETHUSDT.spread, "2.00000000");
    assert.equal(body.markets.BTCUSDT.age_ms, 20);
    assert.equal(
      (await (await fetch(`${url}/markets/BTCUSDT`)).json()).best_bid,
      "100.00000000",
    );
    const metrics = await (await fetch(`${url}/metrics`)).text();
    assert.match(metrics, /market_data_reconnections\{symbol="BTCUSDT"\} 1/);
    assert.match(metrics, /market_data_synchronized\{symbol="ETHUSDT"\} 1/);
    assert.equal((await fetch(`${url}/markets/INVALID`)).status, 404);
    assert.equal(
      (await fetch(`${url}/orders`, { method: "POST" })).status,
      404,
    );
  }));
test("stale collector and stale feed never expose executable quotes", async () =>
  withApi(async ({ path, url }) => {
    for (const type of ["collector", "feed", "invalid"]) {
      const value = document();
      if (type === "collector") value.generated_at_ms = 90_000;
      else if (type === "feed") value.markets.BTCUSDT.last_received_ms = 90_000;
      else value.markets.BTCUSDT.state = "reconnecting";
      await writeFile(path, JSON.stringify(value));
      const body = await (await fetch(`${url}/markets/BTCUSDT`)).json();
      assert.notEqual(body.state, "synchronized");
      assert.equal(body.best_bid, null);
      assert.deepEqual(body.bids, []);
    }
  }));
test("missing, malformed, oversized and invalid status return unavailable; health remains intact", async () =>
  withApi(async ({ path, url }) => {
    assert.equal((await fetch(`${url}/markets`)).status, 503);
    for (const data of [
      "{",
      "x".repeat(300_000),
      JSON.stringify({ schema_version: 2 }),
      (() => {
        const d = document();
        d.markets.BTCUSDT.best_bid = "103.00000000";
        return JSON.stringify(d);
      })(),
      JSON.stringify(document(110_000)),
    ]) {
      await writeFile(path, data);
      assert.equal((await fetch(`${url}/markets`)).status, 503);
      assert.equal((await fetch(`${url}/metrics`)).status, 503);
    }
    const health = await (await fetch(`${url}/health`)).json();
    assert.equal(health.liveTradingEnabled, false);
    assert.equal(health.executionAvailable, false);
  }));

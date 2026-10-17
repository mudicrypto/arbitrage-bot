import test from "node:test";
import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createApp } from "../dist/app.js";
test("research routes expose bounded historical reports read-only and retain safety", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mm-research-api-"));
  const server = createApp("simulation", { researchDir: dir }).listen(
    0,
    "127.0.0.1",
  );
  await once(server, "listening");
  const url = `http://127.0.0.1:${server.address().port}`;
  try {
    assert.equal((await fetch(`${url}/research/rankings`)).status, 503);
    const report = {
      schema_version: 1,
      preliminary: true,
      observations_are_real: true,
      generated_at_ms: 1000,
      rankings: [{ symbol: "BTCUSDT", executable_edge_validated: false }],
    };
    await writeFile(join(dir, "market_rankings.json"), JSON.stringify(report));
    assert.deepEqual(
      await (await fetch(`${url}/research/rankings`)).json(),
      report,
    );
    for (const [route, file, body] of [
      ["rankings.csv", "market_rankings.csv", "symbol,net\nBTCUSDT,-20\n"],
      ["summary", "research_summary.md", "# Preliminary"],
      ["fees", "fee_sensitivity.md", "# Manual unverified"],
    ]) {
      await writeFile(join(dir, file), body);
      assert.equal(
        await (await fetch(`${url}/research/${route}`)).text(),
        body,
      );
    }
    assert.equal(
      (await fetch(`${url}/research/rankings`, { method: "POST" })).status,
      404,
    );
    assert.equal(
      (await fetch(`${url}/orders`, { method: "POST" })).status,
      404,
    );
    report.rankings[0].executable_edge_validated = true;
    await writeFile(join(dir, "market_rankings.json"), JSON.stringify(report));
    assert.equal((await fetch(`${url}/research/rankings`)).status, 503);
    await writeFile(
      join(dir, "market_rankings.json"),
      "x".repeat(8 * 1024 * 1024 + 1),
    );
    assert.equal((await fetch(`${url}/research/rankings`)).status, 503);
    assert.equal(
      (await (await fetch(`${url}/health`)).json()).liveTradingEnabled,
      false,
    );
  } finally {
    await new Promise((resolve) => server.close(resolve));
    await rm(dir, { recursive: true });
  }
});

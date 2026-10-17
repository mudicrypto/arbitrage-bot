import test from "node:test";
import assert from "node:assert/strict";
import { once } from "node:events";
import { createApp, parseMode } from "../dist/app.js";
test("configuration rejects every unsupported trading mode", () => {
  assert.equal(parseMode(), "simulation");
  assert.equal(parseMode("paper"), "paper");
  for (const value of ["live", "LIVE", "", "margin", "futures"])
    assert.throws(() => parseMode(value));
  assert.throws(() => createApp("live"));
});
for (const mode of ["simulation", "paper"]) {
  test(`HTTP health and forbidden order submission in ${mode}`, async () => {
    const server = createApp(mode).listen(0, "127.0.0.1");
    await once(server, "listening");
    try {
      const url = `http://127.0.0.1:${server.address().port}`;
      const response = await fetch(`${url}/health`);
      assert.equal(response.status, 200);
      assert.deepEqual(await response.json(), {
        status: "ok",
        mode,
        liveTradingEnabled: false,
        executionAvailable: false,
      });
      assert.equal(
        (await fetch(`${url}/orders`, { method: "POST" })).status,
        404,
      );
    } finally {
      await new Promise((resolve) => server.close(resolve));
    }
  });
}

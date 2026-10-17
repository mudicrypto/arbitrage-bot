import { createServer } from "node:http";
import {
  readStatus,
  prometheus,
  symbols,
  type SymbolName,
} from "./market-status.js";
export type TradingMode = "simulation" | "paper";
export function parseMode(value: string = "simulation"): TradingMode {
  if (value !== "simulation" && value !== "paper")
    throw new Error("Live trading unavailable: expected simulation or paper");
  return value;
}
export function createApp(
  mode: TradingMode,
  options: { statusFile?: string; now?: () => number } = {},
) {
  // Validate at runtime as callers may be JavaScript.
  parseMode(mode);
  return createServer(async (req, res) => {
    res.setHeader("Content-Type", "application/json");
    if (req.method === "GET" && req.url === "/health") {
      res.end(
        JSON.stringify({
          status: "ok",
          mode,
          liveTradingEnabled: false,
          executionAvailable: false,
        }),
      );
    } else if (
      req.method === "GET" &&
      (req.url === "/markets" ||
        req.url === "/metrics" ||
        symbols.some((s) => req.url === `/markets/${s}`))
    ) {
      res.setHeader("Cache-Control", "no-store");
      try {
        const document = await readStatus(
          options.statusFile ??
            process.env.MARKET_STATUS_FILE ??
            "../.local/market-status.json",
          (options.now ?? Date.now)(),
        );
        if (req.url === "/metrics") {
          res.setHeader("Content-Type", "text/plain; version=0.0.4");
          res.end(prometheus(document));
        } else {
          res.end(
            JSON.stringify(
              req.url === "/markets"
                ? document
                : document.markets[req.url!.split("/")[2] as SymbolName],
            ),
          );
        }
      } catch {
        res.statusCode = 503;
        res.end(JSON.stringify({ error: "market_data_unavailable" }));
      }
    } else {
      res.statusCode = 404;
      res.end(JSON.stringify({ error: "not_found" }));
    }
  });
}

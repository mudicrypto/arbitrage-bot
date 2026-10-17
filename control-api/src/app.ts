import { createServer } from 'node:http';
export type TradingMode = 'simulation' | 'paper';
export function parseMode(value: string = 'simulation'): TradingMode {
  if (value !== 'simulation' && value !== 'paper') throw new Error('Live trading unavailable: expected simulation or paper');
  return value;
}
export function createApp(mode: TradingMode) {
  // Validate at runtime as callers may be JavaScript.
  parseMode(mode);
  return createServer((req, res) => {
    res.setHeader('Content-Type', 'application/json');
    if (req.method === 'GET' && req.url === '/health') {
      res.end(JSON.stringify({ status: 'ok', mode, liveTradingEnabled: false, executionAvailable: false }));
    } else {
      res.statusCode = 404;
      res.end(JSON.stringify({ error: 'not_found' }));
    }
  });
}

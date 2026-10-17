import { createApp, parseMode } from './app.js';
const mode = parseMode(process.env.TRADING_MODE);
const port = Number(process.env.PORT ?? 3001);
if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error('Invalid PORT');
createApp(mode).listen(port, '127.0.0.1', () => console.log(`Control API listening on port ${port}, mode=${mode}`));

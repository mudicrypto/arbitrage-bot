BEGIN;
CREATE TABLE IF NOT EXISTS schema_migrations (version integer PRIMARY KEY, applied_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE IF NOT EXISTS trading_sessions (
    id uuid PRIMARY KEY,
    mode text NOT NULL CHECK (mode IN ('simulation', 'paper')),
    config jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS orders (
    session_id uuid NOT NULL REFERENCES trading_sessions(id),
    client_order_id text NOT NULL,
    exchange text NOT NULL,
    symbol text NOT NULL,
    side text NOT NULL CHECK (side IN ('buy', 'sell')),
    price_ticks numeric(20,0) NOT NULL CHECK (price_ticks > 0),
    quantity_lots numeric(20,0) NOT NULL CHECK (quantity_lots > 0),
    status text NOT NULL CHECK (status IN ('pending','open','partially_filled','filled','cancelled','rejected','unknown')),
    PRIMARY KEY (session_id, client_order_id)
);
CREATE TABLE IF NOT EXISTS order_events (
    session_id uuid NOT NULL,
    client_order_id text NOT NULL,
    sequence bigint NOT NULL CHECK (sequence >= 0),
    payload jsonb NOT NULL,
    PRIMARY KEY (session_id, sequence),
    FOREIGN KEY (session_id, client_order_id) REFERENCES orders(session_id, client_order_id)
);
CREATE TABLE IF NOT EXISTS ledger_entries (
    session_id uuid NOT NULL REFERENCES trading_sessions(id),
    entry_id text NOT NULL,
    asset text NOT NULL,
    amount_atomic numeric(39,0) NOT NULL,
    kind text NOT NULL CHECK (kind IN ('deposit','trade','fee','adjustment')),
    PRIMARY KEY (session_id, entry_id, asset)
);
INSERT INTO schema_migrations(version) VALUES (1) ON CONFLICT DO NOTHING;
COMMIT;

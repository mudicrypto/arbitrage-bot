BEGIN;
INSERT INTO trading_sessions VALUES ('00000000-0000-0000-0000-000000000001', 'paper', '{}', now());
DO $$
BEGIN
  BEGIN
    INSERT INTO trading_sessions VALUES ('00000000-0000-0000-0000-000000000002', 'live', '{}', now());
    RAISE EXCEPTION 'live mode accepted';
  EXCEPTION WHEN check_violation THEN NULL;
  END;
END $$;
INSERT INTO orders VALUES ('00000000-0000-0000-0000-000000000001','test','binance-spot','BTCUSDT','buy',100,1,'unknown');
DO $$
BEGIN
  BEGIN
    INSERT INTO orders VALUES ('00000000-0000-0000-0000-000000000001','test','binance-spot','BTCUSDT','buy',100,1,'open');
    RAISE EXCEPTION 'duplicate order accepted';
  EXCEPTION WHEN unique_violation THEN NULL;
  END;
  BEGIN
    INSERT INTO orders VALUES ('00000000-0000-0000-0000-000000000001','zero','binance-spot','BTCUSDT','buy',0,1,'open');
    RAISE EXCEPTION 'zero price accepted';
  EXCEPTION WHEN check_violation THEN NULL;
  END;
END $$;
INSERT INTO ledger_entries VALUES ('00000000-0000-0000-0000-000000000001','commission','BNB',-12,'fee');
ROLLBACK;

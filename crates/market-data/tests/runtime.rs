use futures_util::{SinkExt, StreamExt};
use mm_market_data::{
    recording::{Limits, replay, segment_paths},
    runtime::{Config, MarketStatus, SharedStatus, now_ms, run_symbol, write_status},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, Copy)]
enum Fault {
    None,
    Gap,
    Interrupt,
    Stale,
    Timestamp,
    Crossed,
    Overflow,
    OldSnapshot,
    Invalid,
}
struct Harness {
    config: Config,
    shared: SharedStatus,
    connections: Arc<AtomicUsize>,
    requests: Arc<AtomicUsize>,
    tasks: Vec<JoinHandle<()>>,
    _dir: tempfile::TempDir,
}
impl Drop for Harness {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
fn depth(symbol: &str, first: u64, last: u64, stale: bool, crossed: bool) -> String {
    let time = if stale {
        now_ms().saturating_sub(10_000)
    } else {
        now_ms()
    };
    let price = if crossed { "104" } else { "100" };
    format!(
        r#"{{"e":"depthUpdate","E":{time},"s":"{symbol}","U":{first},"u":{last},"b":[["{price}","2"]],"a":[]}}"#
    )
}
async fn harness(fault: Fault, recording_failure: bool) -> Harness {
    harness_symbol(fault, recording_failure, "BTCUSDT").await
}
async fn harness_symbol(fault: Fault, recording_failure: bool, symbol: &'static str) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let http = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let connections = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(AtomicUsize::new(0));
    let config = Config {
        rest_base: format!("http://{}", http.local_addr().unwrap()),
        ws_base: format!("ws://{}", ws.local_addr().unwrap()),
        data_dir: dir.path().join("recordings"),
        status_file: dir.path().join("status.json"),
        proxy: None,
        stale_ms: 700,
        max_lag_ms: 1000,
        reconnect_ms: 20,
        max_reconnect_ms: 100,
        snapshot_interval_ms: 10,
        buffer_events: if matches!(fault, Fault::Overflow) {
            2
        } else {
            64
        },
        recording: if recording_failure {
            Limits {
                max_file_bytes: 1,
                segment_records: 2,
                ..Limits::default()
            }
        } else {
            Limits {
                segment_records: 16,
                ..Limits::default()
            }
        },
        ..Config::default()
    };
    let reqs = requests.clone();
    let http_task = tokio::spawn(async move {
        loop {
            let (mut stream, _) = http.accept().await.unwrap();
            let request = reqs.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let mut buffer = [0; 4096];
                let _ = stream.read(&mut buffer).await;
                tokio::time::sleep(Duration::from_millis(if matches!(fault, Fault::Overflow) {
                    250
                } else {
                    100
                }))
                .await;
                let id = if matches!(fault, Fault::OldSnapshot) && request == 0 {
                    50
                } else {
                    100
                };
                let body = format!(
                    r#"{{"lastUpdateId":{id},"bids":[["100","1"],["99","2"]],"asks":[["102","3"],["103","4"]]}}"#
                );
                let _ = stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    )
                    .await;
            });
        }
    });
    let count = connections.clone();
    let ws_task = tokio::spawn(async move {
        loop {
            let (stream, _) = ws.accept().await.unwrap();
            let connection = count.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                    return;
                };
                if matches!(fault, Fault::Invalid) && connection == 0 {
                    let _ = socket.send(Message::text("not JSON")).await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    return;
                }
                if socket
                    .send(Message::text(depth(
                        symbol,
                        100,
                        101,
                        matches!(fault, Fault::Timestamp) && connection == 0,
                        false,
                    )))
                    .await
                    .is_err()
                {
                    return;
                }
                if matches!(fault, Fault::Overflow) && connection == 0 {
                    for id in 102..250 {
                        if socket
                            .send(Message::text(depth(symbol, id, id, false, false)))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                // Duplicate and out-of-order old updates arrive while REST is pending.
                for (first, last) in if matches!(fault, Fault::Overflow) {
                    vec![]
                } else {
                    vec![(102, 102), (100, 101), (102, 102)]
                } {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    if socket
                        .send(Message::text(depth(symbol, first, last, false, false)))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                tokio::time::sleep(Duration::from_millis(if matches!(fault, Fault::Overflow) {
                    350
                } else {
                    180
                }))
                .await;
                if connection == 0 {
                    match fault {
                        Fault::Gap => {
                            let _ = socket
                                .send(Message::text(depth(symbol, 105, 105, false, false)))
                                .await;
                        }
                        Fault::Crossed => {
                            let _ = socket
                                .send(Message::text(depth(symbol, 103, 103, false, true)))
                                .await;
                        }
                        Fault::Interrupt => {
                            let _ = socket.close(None).await;
                            return;
                        }
                        Fault::Stale => {
                            tokio::time::sleep(Duration::from_secs(3)).await;
                            return;
                        }
                        _ => (),
                    }
                }
                let mut id = if matches!(fault, Fault::Overflow) {
                    102
                } else {
                    103
                };
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(30)) => {
                            if socket.send(Message::text(depth(symbol,id,id,false,false))).await.is_err() { return; } id += 1;
                        }
                        message = socket.next() => { if message.is_none() { return; } }
                    }
                }
            });
        }
    });
    let shared = Arc::new(Mutex::new(BTreeMap::new()));
    Harness {
        config,
        shared,
        connections,
        requests,
        tasks: vec![http_task, ws_task],
        _dir: dir,
    }
}
async fn wait_for(shared: &SharedStatus, check: impl Fn(&MarketStatus) -> bool) -> MarketStatus {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if let Some(status) = shared
                .lock()
                .unwrap()
                .get("BTCUSDT")
                .filter(|s| check(s))
                .cloned()
            {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out; status: {:?}", shared.lock().unwrap()))
}
#[tokio::test]
async fn snapshot_buffers_updates_and_reports_exact_quotes() {
    let mut h = harness(Fault::None, false).await;
    let config = h.config.clone();
    let shared = h.shared.clone();
    h.tasks.push(tokio::spawn(async move {
        let _ = run_symbol(config, "BTCUSDT", shared).await;
    }));
    let status = wait_for(&h.shared, |s| {
        s.state == "synchronized" && s.metrics.duplicates >= 2
    })
    .await;
    assert_eq!(status.best_bid.as_deref(), Some("100.00000000"));
    assert_eq!(status.best_ask.as_deref(), Some("102.00000000"));
    assert_eq!(status.spread.as_deref(), Some("2.00000000"));
    assert_eq!(status.bid_levels, 2);
    assert_eq!(status.ask_levels, 2);
    assert_eq!(status.bids[0][1], "2.00000000");
    assert!(!status.depth_complete);
    write_status(&h.config.status_file, &h.shared, h.config.stale_ms).unwrap();
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&h.config.status_file).unwrap()).unwrap();
    assert_eq!(doc["markets"]["BTCUSDT"]["best_bid"], "100.00000000");
    // Snapshot + buffered deltas have been persisted before later segment rotation.
    let files = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let files = segment_paths(&h.config.data_dir).unwrap();
            if !files.is_empty() {
                break files;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    for path in files {
        let a = replay(&path, 10_000, 8 * 1024 * 1024).unwrap();
        let b = replay(&path, 10_000, 8 * 1024 * 1024).unwrap();
        assert_eq!(a, b);
    }
}
#[tokio::test]
async fn gap_interrupt_crossed_and_stale_streams_automatically_recover() {
    for fault in [
        Fault::Gap,
        Fault::Interrupt,
        Fault::Crossed,
        Fault::Stale,
        Fault::Timestamp,
        Fault::Invalid,
    ] {
        let mut h = harness(fault, false).await;
        let config = h.config.clone();
        let shared = h.shared.clone();
        h.tasks.push(tokio::spawn(async move {
            let _ = run_symbol(config, "BTCUSDT", shared).await;
        }));
        let status = wait_for(&h.shared, |s| {
            s.state == "synchronized" && s.metrics.reconnections >= 1
        })
        .await;
        assert!(h.connections.load(Ordering::SeqCst) >= 2);
        assert!(status.metrics.disconnections >= 1);
        match fault {
            Fault::Gap => assert!(status.metrics.data_gaps >= 1),
            Fault::Stale | Fault::Timestamp => assert!(status.metrics.stale_events >= 1),
            Fault::Invalid => assert!(status.metrics.invalid_messages >= 1),
            _ => (),
        }
        assert_eq!(status.best_bid.as_deref(), Some("100.00000000"));
    }
}
#[tokio::test]
async fn bounded_sync_buffer_overflow_recovers() {
    let mut h = harness(Fault::Overflow, false).await;
    let config = h.config.clone();
    let shared = h.shared.clone();
    h.tasks.push(tokio::spawn(async move {
        let _ = run_symbol(config, "BTCUSDT", shared).await;
    }));
    let status = wait_for(&h.shared, |s| {
        s.state == "synchronized" && s.metrics.buffer_overflows >= 1
    })
    .await;
    assert!(status.metrics.reconnections >= 1);
}
#[tokio::test]
async fn old_snapshot_is_retried_without_dropping_buffer() {
    let mut h = harness(Fault::OldSnapshot, false).await;
    let config = h.config.clone();
    let shared = h.shared.clone();
    h.tasks.push(tokio::spawn(async move {
        let _ = run_symbol(config, "BTCUSDT", shared).await;
    }));
    let status = wait_for(&h.shared, |s| s.state == "synchronized").await;
    assert!(h.requests.load(Ordering::SeqCst) >= 2);
    assert_eq!(status.metrics.reconnections, 0);
}
#[tokio::test]
async fn recording_failure_stops_collection_and_clears_quotes() {
    let h = harness(Fault::None, true).await;
    let config = h.config.clone();
    let shared = h.shared.clone();
    let task = tokio::spawn(async move { run_symbol(config, "BTCUSDT", shared).await });
    let status = wait_for(&h.shared, |s| s.state == "recording_failed").await;
    assert!(status.metrics.recording_failures > 0);
    assert!(status.best_bid.is_none());
    assert!(
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
}
#[test]
fn stale_status_writer_hides_old_quotes() {
    let dir = tempfile::tempdir().unwrap();
    let mut status = MarketStatus::new("BTCUSDT");
    status.state = "synchronized".into();
    status.last_received_ms = Some(now_ms().saturating_sub(10_000));
    status.best_bid = Some("100.00000000".into());
    let shared = Arc::new(Mutex::new(BTreeMap::from([("BTCUSDT".into(), status)])));
    let path = dir.path().join("status.json");
    write_status(&path, &shared, 1000).unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(doc["markets"]["BTCUSDT"]["state"], "stale");
    assert!(doc["markets"]["BTCUSDT"]["best_bid"].is_null());
}

#[tokio::test]
async fn both_required_symbols_produce_independent_books() {
    let mut btc = harness_symbol(Fault::None, false, "BTCUSDT").await;
    let mut eth = harness_symbol(Fault::None, false, "ETHUSDT").await;
    eth.shared = btc.shared.clone();
    for (h, symbol) in [(&mut btc, "BTCUSDT"), (&mut eth, "ETHUSDT")] {
        let config = h.config.clone();
        let shared = h.shared.clone();
        h.tasks.push(tokio::spawn(async move {
            let _ = run_symbol(config, symbol, shared).await;
        }));
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if btc
                .shared
                .lock()
                .unwrap()
                .values()
                .filter(|s| s.state == "synchronized")
                .count()
                == 2
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let states = btc.shared.lock().unwrap();
    assert_ne!(states["BTCUSDT"].session, states["ETHUSDT"].session);
    assert_eq!(states["ETHUSDT"].best_bid.as_deref(), Some("100.00000000"));
}

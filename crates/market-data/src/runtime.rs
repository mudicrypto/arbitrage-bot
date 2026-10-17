use crate::{
    book::{Applied, Book},
    model::{Depth, SYMBOLS, Snapshot, Trade, decimal},
    recording::{Event, Limits, Recorder},
};
use anyhow::{Context, Result, bail, ensure};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::mpsc,
    task::JoinHandle,
    time::{Instant, timeout},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, protocol::WebSocketConfig},
};

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
/// Protocol decoding is separate from exchange-neutral books, recording and replay.
pub trait DepthProtocol {
    fn snapshot(raw: &str) -> Result<Snapshot>;
    fn depth(raw: &str, symbol: &str) -> Result<Depth>;
}
pub struct BinanceSpot;
impl DepthProtocol for BinanceSpot {
    fn snapshot(raw: &str) -> Result<Snapshot> {
        Snapshot::parse(raw)
    }
    fn depth(raw: &str, symbol: &str) -> Result<Depth> {
        Depth::parse(raw, symbol)
    }
}
pub trait Observer: Send + Sync {
    fn book(&self, symbol: &str, book: &Book, received_ms: u64, exchange_ms: u64);
    fn trade(&self, trade: &Trade, received_ms: u64);
}
#[derive(Clone)]
pub struct Config {
    pub rest_base: String,
    pub ws_base: String,
    pub data_dir: PathBuf,
    pub status_file: PathBuf,
    pub stale_ms: u64,
    pub max_lag_ms: u64,
    pub buffer_events: usize,
    pub max_levels: usize,
    pub max_message_bytes: usize,
    pub reconnect_ms: u64,
    pub max_reconnect_ms: u64,
    pub snapshot_interval_ms: u64,
    pub proxy: Option<String>,
    pub recording: Limits,
    pub include_trades: bool,
    pub observer: Option<Arc<dyn Observer>>,
    pub snapshot_budget: Option<Arc<tokio::sync::Mutex<Instant>>>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            rest_base: "https://data-api.binance.vision".into(),
            ws_base: "wss://data-stream.binance.vision".into(),
            data_dir: PathBuf::from(".local/market-data"),
            status_file: PathBuf::from(".local/market-status.json"),
            stale_ms: 5000,
            max_lag_ms: 5000,
            buffer_events: 64,
            max_levels: 10_000,
            max_message_bytes: 1024 * 1024,
            reconnect_ms: 3000,
            max_reconnect_ms: 60_000,
            snapshot_interval_ms: 15_000,
            proxy: std::env::var("HTTPS_PROXY")
                .or_else(|_| std::env::var("https_proxy"))
                .ok(),
            recording: Limits::default(),
            include_trades: false,
            observer: None,
            snapshot_budget: None,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.stale_ms > 0
                && self.max_lag_ms > 0
                && self.buffer_events > 0
                && self.buffer_events <= 4096,
            "invalid buffering/time limits"
        );
        ensure!(
            self.max_levels >= 5000
                && self.max_levels <= 20_000
                && self.max_message_bytes <= 1024 * 1024
                && self.max_message_bytes > 0,
            "invalid book/message limits"
        );
        ensure!(
            self.reconnect_ms > 0 && self.max_reconnect_ms >= self.reconnect_ms,
            "invalid reconnect limits"
        );
        for (base, secure, local) in [
            (&self.rest_base, "https", "http"),
            (&self.ws_base, "wss", "ws"),
        ] {
            let url = reqwest::Url::parse(base)?;
            ensure!(
                url.username().is_empty() && url.password().is_none() && url.query().is_none(),
                "credentials/query prohibited in endpoint"
            );
            ensure!(
                url.scheme() == secure
                    || (url.scheme() == local
                        && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))),
                "insecure nonlocal endpoint"
            );
        }
        let rest = reqwest::Url::parse(&self.rest_base)?;
        if !matches!(rest.host_str(), Some("127.0.0.1" | "localhost")) {
            ensure!(
                self.reconnect_ms >= 3000 && self.snapshot_interval_ms >= 15_000,
                "public endpoint retry budget too aggressive"
            );
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metrics {
    pub connection_attempts: u64,
    pub reconnections: u64,
    pub synchronization_failures: u64,
    pub data_gaps: u64,
    pub duplicates: u64,
    pub invalid_messages: u64,
    pub stale_events: u64,
    pub disconnections: u64,
    pub buffer_overflows: u64,
    pub recording_failures: u64,
    pub updates: u64,
    pub last_lag_ms: Option<u64>,
    pub max_lag_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketStatus {
    pub symbol: String,
    pub state: String,
    pub session: Option<String>,
    pub last_update_id: Option<String>,
    pub last_received_ms: Option<u64>,
    pub last_exchange_ms: Option<u64>,
    pub age_ms: Option<u64>,
    pub best_bid: Option<String>,
    pub best_ask: Option<String>,
    pub spread: Option<String>,
    pub bid_levels: usize,
    pub ask_levels: usize,
    pub bids: Vec<[String; 2]>,
    pub asks: Vec<[String; 2]>,
    pub depth_complete: bool,
    pub reason: Option<String>,
    pub metrics: Metrics,
}
impl MarketStatus {
    pub fn new(symbol: &str) -> Self {
        Self {
            symbol: symbol.into(),
            state: "starting".into(),
            session: None,
            last_update_id: None,
            last_received_ms: None,
            last_exchange_ms: None,
            age_ms: None,
            best_bid: None,
            best_ask: None,
            spread: None,
            bid_levels: 0,
            ask_levels: 0,
            bids: vec![],
            asks: vec![],
            depth_complete: false,
            reason: None,
            metrics: Metrics::default(),
        }
    }
    fn update(&mut self, book: &Book, ms: u64, exchange_ms: u64) {
        self.last_received_ms = Some(ms);
        self.last_exchange_ms = Some(exchange_ms);
        self.last_update_id = book.last_id.map(|id| id.to_string());
        self.state = "synchronized".into();
        self.reason = None;
        self.age_ms = Some(now_ms().saturating_sub(ms));
        let bid = *book.bids.last_key_value().unwrap().0;
        let ask = *book.asks.first_key_value().unwrap().0;
        self.best_bid = Some(decimal(bid));
        self.best_ask = Some(decimal(ask));
        self.spread = Some(decimal(ask - bid));
        self.bid_levels = book.bids.len();
        self.ask_levels = book.asks.len();
        self.bids = book
            .bids
            .iter()
            .rev()
            .take(20)
            .map(|(p, q)| [decimal(*p), decimal(*q)])
            .collect();
        self.asks = book
            .asks
            .iter()
            .take(20)
            .map(|(p, q)| [decimal(*p), decimal(*q)])
            .collect();
    }
    fn clear(&mut self, state: &str, reason: &str) {
        self.state = state.into();
        self.reason = Some(reason.into());
        self.best_bid = None;
        self.best_ask = None;
        self.spread = None;
        self.bid_levels = 0;
        self.ask_levels = 0;
        self.bids.clear();
        self.asks.clear();
        self.last_update_id = None;
    }
}
pub type SharedStatus = Arc<Mutex<BTreeMap<String, MarketStatus>>>;
fn publish(shared: &SharedStatus, status: &MarketStatus) {
    shared
        .lock()
        .expect("status mutex poisoned")
        .insert(status.symbol.clone(), status.clone());
}
#[derive(Serialize)]
struct StatusDocument {
    schema_version: u16,
    generated_at_ms: u64,
    stale_after_ms: u64,
    markets: BTreeMap<String, MarketStatus>,
}
pub fn write_status(path: &Path, statuses: &SharedStatus, stale_ms: u64) -> Result<()> {
    let now = now_ms();
    let mut markets = statuses.lock().expect("status mutex poisoned").clone();
    for market in markets.values_mut() {
        market.age_ms = market.last_received_ms.map(|t| now.saturating_sub(t));
        if market.state == "synchronized" && market.age_ms.is_none_or(|age| age > stale_ms) {
            market.clear("stale", "feed stale");
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec(&StatusDocument {
            schema_version: 1,
            generated_at_ms: now,
            stale_after_ms: stale_ms,
            markets,
        })?,
    )?;
    std::fs::rename(tmp, path)?;
    Ok(())
}
struct ReaderGuard(JoinHandle<Result<()>>);
impl Drop for ReaderGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}
struct Incoming {
    raw: String,
    received_ms: u64,
}
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
async fn connect(config: &Config, symbol: &str) -> Result<Socket> {
    let endpoint = format!(
        "{}/ws/{}@depth@100ms",
        config.ws_base.trim_end_matches('/'),
        symbol.to_lowercase()
    );
    let endpoint = if config.include_trades {
        let lower = symbol.to_lowercase();
        format!(
            "{}/stream?streams={lower}@depth@100ms/{lower}@aggTrade",
            config.ws_base.trim_end_matches('/')
        )
    } else {
        endpoint
    };
    let url = reqwest::Url::parse(&endpoint)?;
    let host = url.host_str().context("missing stream host")?;
    let port = url.port_or_known_default().context("missing stream port")?;
    let stream = if let Some(proxy) = &config.proxy {
        let proxy = reqwest::Url::parse(proxy)?;
        ensure!(
            proxy.scheme() == "http" && proxy.username().is_empty() && proxy.password().is_none(),
            "only unauthenticated HTTP CONNECT proxy supported"
        );
        let mut socket = TcpStream::connect((
            proxy.host_str().context("missing proxy host")?,
            proxy.port_or_known_default().context("proxy port")?,
        ))
        .await?;
        socket
            .write_all(
                format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n").as_bytes(),
            )
            .await?;
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            ensure!(header.len() < 8192, "proxy response headers too large");
            header.push(socket.read_u8().await?);
        }
        let header = std::str::from_utf8(&header)?;
        let status = header
            .lines()
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .nth(1)
            .unwrap_or_default();
        ensure!(status == "200", "WebSocket CONNECT proxy status {status}");
        socket
    } else {
        TcpStream::connect((host, port)).await?
    };
    stream.set_nodelay(true)?;
    let ws = WebSocketConfig::default()
        .max_message_size(Some(config.max_message_bytes))
        .max_frame_size(Some(config.max_message_bytes));
    let (socket, _) =
        tokio_tungstenite::client_async_tls_with_config(endpoint, stream, Some(ws), None).await?;
    // The socket must own its transport, not a borrowed TcpStream.
    Ok(socket)
}
async fn reader(mut socket: Socket, tx: mpsc::Sender<Incoming>) -> Result<()> {
    while let Some(message) = socket.next().await {
        match message? {
            Message::Text(text) => {
                let message = Incoming {
                    raw: text.to_string(),
                    received_ms: now_ms(),
                };
                tx.try_send(message)
                    .map_err(|_| anyhow::anyhow!("buffer overflow or receiver closed"))?;
            }
            Message::Ping(payload) => socket.send(Message::Pong(payload)).await?,
            Message::Close(_) => bail!("WebSocket closed"),
            Message::Binary(_) => bail!("unexpected binary frame"),
            _ => (),
        }
    }
    bail!("WebSocket disconnected")
}
#[derive(Debug)]
struct RateLimited(u64);
impl std::fmt::Display for RateLimited {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "REST rate limited; retry after {}ms", self.0)
    }
}
impl std::error::Error for RateLimited {}
async fn snapshot(
    client: &reqwest::Client,
    config: &Config,
    symbol: &str,
    gate: &tokio::sync::Mutex<Instant>,
) -> Result<String> {
    let mut next = gate.lock().await;
    tokio::time::sleep_until(*next).await;
    *next = Instant::now() + Duration::from_millis(config.snapshot_interval_ms);
    drop(next);
    if let Some(budget) = &config.snapshot_budget {
        let mut next = budget.lock().await;
        tokio::time::sleep_until(*next).await;
        *next = Instant::now() + Duration::from_secs(3);
    }
    let url = format!(
        "{}/api/v3/depth?symbol={symbol}&limit=5000",
        config.rest_base.trim_end_matches('/')
    );
    let response = client.get(url).send().await?;
    if matches!(response.status().as_u16(), 418 | 429) {
        let seconds = response
            .headers()
            .get("retry-after")
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60)
            .clamp(1, 7 * 86400);
        return Err(RateLimited(seconds * 1000).into());
    }
    let mut response = response.error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= config.max_message_bytes,
            "snapshot response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    let raw = String::from_utf8(bytes)?;
    Ok(raw)
}
struct Session<'a> {
    config: &'a Config,
    symbol: &'a str,
    id: &'a str,
    recorder: &'a mut Recorder,
    book: &'a mut Book,
    status: &'a mut MarketStatus,
    shared: &'a SharedStatus,
    snapshot_gate: &'a tokio::sync::Mutex<Instant>,
}
impl Session<'_> {
    fn record(&mut self, ms: u64, raw: Option<String>, event: Event) -> Result<()> {
        if let Err(error) = self.recorder.append(self.id, ms, raw, event, self.book) {
            self.status.metrics.recording_failures += 1;
            return Err(error.context("recording failure"));
        }
        Ok(())
    }
    fn decode(&mut self, incoming: Incoming) -> Result<Option<(Depth, u64)>> {
        let value: serde_json::Value =
            serde_json::from_str(&incoming.raw).unwrap_or(serde_json::Value::Null);
        let payload = value.get("data").unwrap_or(&value);
        let raw = serde_json::to_string(payload)?;
        if self.config.include_trades
            && payload.get("e").and_then(|v| v.as_str()) == Some("aggTrade")
        {
            let trade = Trade::parse(&raw, self.symbol)?;
            ensure!(
                incoming.received_ms.abs_diff(trade.exchange_ms) <= self.config.max_lag_ms,
                "stale trade timestamp"
            );
            self.record(
                incoming.received_ms,
                Some(incoming.raw),
                Event::Trade {
                    trade: trade.clone(),
                },
            )?;
            if let Some(observer) = &self.config.observer {
                observer.trade(&trade, incoming.received_ms);
            }
            return Ok(None);
        }
        let depth = match BinanceSpot::depth(&raw, self.symbol) {
            Ok(depth) => depth,
            Err(error) => {
                self.status.metrics.invalid_messages += 1;
                self.record(
                    incoming.received_ms,
                    Some(incoming.raw),
                    Event::Rejected {
                        reason: error.to_string(),
                    },
                )?;
                return Err(error.context("invalid stream message"));
            }
        };
        self.record(
            incoming.received_ms,
            Some(incoming.raw),
            Event::ReceivedDepth {
                depth: depth.clone(),
            },
        )?;
        let lag = incoming.received_ms.saturating_sub(depth.exchange_ms);
        self.status.metrics.last_lag_ms = Some(lag);
        self.status.metrics.max_lag_ms = self.status.metrics.max_lag_ms.max(lag);
        if lag > self.config.max_lag_ms
            || depth.exchange_ms > incoming.received_ms.saturating_add(self.config.max_lag_ms)
        {
            self.status.metrics.stale_events += 1;
            bail!("stale or future exchange timestamp");
        }
        ensure!(
            now_ms().saturating_sub(incoming.received_ms) <= self.config.stale_ms,
            "buffered event stale"
        );
        Ok(Some((depth, incoming.received_ms)))
    }
    fn apply(&mut self, depth: Depth, ms: u64) -> Result<bool> {
        let before = self.book.clone();
        match self.book.apply(&depth, self.config.max_levels) {
            Ok(Applied::Duplicate) => {
                self.status.metrics.duplicates += 1;
                Ok(false)
            }
            Ok(Applied::Updated) => {
                // Restore pre-state while the recorder captures a possible segment checkpoint.
                let after = std::mem::replace(self.book, before);
                self.record(
                    ms,
                    None,
                    Event::AppliedDepth {
                        depth: depth.clone(),
                    },
                )?;
                *self.book = after;
                self.status.metrics.updates += 1;
                self.status.update(self.book, ms, depth.exchange_ms);
                publish(self.shared, self.status);
                if let Some(observer) = &self.config.observer {
                    observer.book(self.symbol, self.book, ms, depth.exchange_ms);
                }
                Ok(true)
            }
            Err(error) => {
                if error.to_string().contains("sequence gap") {
                    self.status.metrics.data_gaps += 1;
                }
                Err(error)
            }
        }
    }
    async fn run(
        &mut self,
        client: &reqwest::Client,
        rx: &mut mpsc::Receiver<Incoming>,
        reader: &mut JoinHandle<Result<()>>,
    ) -> Result<()> {
        let mut buffered = VecDeque::new();
        let initial_deadline = Instant::now() + Duration::from_millis(self.config.stale_ms);
        loop {
            let first = tokio::select! {
                biased;
                result = &mut *reader => return reader_result(result),
                incoming = tokio::time::timeout_at(initial_deadline, rx.recv()) => incoming?.context("stream disconnected")?,
            };
            if let Some(depth) = self.decode(first)? {
                buffered.push_back(depth);
                break;
            }
        }
        let mut accepted = None;
        for _ in 0..3 {
            let future = snapshot(client, self.config, self.symbol, self.snapshot_gate);
            tokio::pin!(future);
            let deadline = tokio::time::sleep(Duration::from_millis(self.config.stale_ms));
            tokio::pin!(deadline);
            let raw = loop {
                tokio::select! {
                    biased;
                    result = &mut *reader => return reader_result(result),
                    _ = &mut deadline => bail!("snapshot synchronization timeout"),
                    result = &mut future => break result?,
                    incoming = rx.recv() => {
                        ensure!(buffered.len() < self.config.buffer_events, "synchronization buffer overflow");
                        if let Some(depth)=self.decode(incoming.context("stream disconnected")?)? { buffered.push_back(depth); }
                    }
                }
            };
            let candidate = BinanceSpot::snapshot(&raw).and_then(|snapshot| {
                let mut checked = Book::default();
                checked.snapshot(&snapshot, self.config.max_levels)?;
                Ok(snapshot)
            });
            let snapshot = match candidate {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    self.record(
                        now_ms(),
                        Some(raw),
                        Event::Rejected {
                            reason: error.to_string(),
                        },
                    )?;
                    return Err(error.context("invalid snapshot"));
                }
            };
            self.record(
                now_ms(),
                Some(raw),
                Event::ReceivedSnapshot {
                    snapshot: snapshot.clone(),
                },
            )?;
            if snapshot.last_id >= buffered.front().unwrap().0.first_id {
                accepted = Some(snapshot);
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        let snapshot = accepted.context("snapshot older than buffered stream")?;
        self.record(
            now_ms(),
            None,
            Event::Snapshot {
                snapshot: snapshot.clone(),
            },
        )?;
        self.book.snapshot(&snapshot, self.config.max_levels)?;
        let mut last_applied = Instant::now();
        while let Some((event, ms)) = buffered.pop_front() {
            ensure!(
                now_ms().saturating_sub(ms) <= self.config.stale_ms,
                "buffered event stale"
            );
            if self.apply(event, ms)? {
                last_applied = Instant::now();
            }
        }
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        let mut last_flush = Instant::now();
        let started = Instant::now();
        loop {
            tokio::select! {
                biased;
                result = &mut *reader => return reader_result(result),
                _ = tick.tick() => {
                    ensure!(last_applied.elapsed().as_millis() < u128::from(self.config.stale_ms), "feed stale");
                    ensure!(self.status.last_received_ms.is_none_or(|t| now_ms().saturating_sub(t) <= self.config.stale_ms), "feed stale");
                    // Rotate before Binance's 24-hour connection expiry.
                    ensure!(started.elapsed() < Duration::from_secs(23 * 3600), "scheduled connection rotation");
                    if last_flush.elapsed() >= Duration::from_secs(2) {
                        self.recorder.flush().inspect_err(|_| self.status.metrics.recording_failures += 1)?;
                        last_flush = Instant::now();
                    }
                }
                incoming = rx.recv() => {
                    if let Some((depth, ms))=self.decode(incoming.context("stream disconnected")?)? && self.apply(depth, ms)? { last_applied = Instant::now(); }
                }
            }
        }
    }
}
fn reader_result(result: Result<Result<()>, tokio::task::JoinError>) -> Result<()> {
    result??;
    bail!("stream reader stopped")
}

pub async fn run_symbol(config: Config, symbol: &str, shared: SharedStatus) -> Result<()> {
    config.validate()?;
    let mut status = MarketStatus::new(symbol);
    let mut recorder = match Recorder::open(&config.data_dir, symbol, config.recording.clone()) {
        Ok(recorder) => recorder,
        Err(error) => {
            status.metrics.recording_failures += 1;
            status.clear("recording_failed", "cannot open recorder");
            publish(&shared, &status);
            return Err(error);
        }
    };
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(proxy) = &config.proxy {
        builder = builder.proxy(reqwest::Proxy::all(proxy)?);
    } else {
        builder = builder.no_proxy();
    }
    let client = builder.build()?;
    let mut book = Book::default();
    let mut delay = config.reconnect_ms;
    let snapshot_gate = tokio::sync::Mutex::new(Instant::now());
    loop {
        status.metrics.connection_attempts += 1;
        if status.metrics.connection_attempts > 1 {
            status.metrics.reconnections += 1;
        }
        let session = uuid::Uuid::new_v4().to_string();
        status.session = Some(session.clone());
        status.clear("connecting", "connecting public market data");
        publish(&shared, &status);
        let socket = timeout(Duration::from_secs(10), connect(&config, symbol)).await;
        let result = match socket {
            Ok(Ok(socket)) => {
                status.clear("synchronizing", "buffering while retrieving snapshot");
                publish(&shared, &status);
                let (tx, mut rx) = mpsc::channel(config.buffer_events);
                let mut handle = ReaderGuard(tokio::spawn(reader(socket, tx)));
                let mut session_state = Session {
                    config: &config,
                    symbol,
                    id: &session,
                    recorder: &mut recorder,
                    book: &mut book,
                    status: &mut status,
                    shared: &shared,
                    snapshot_gate: &snapshot_gate,
                };
                let result = match session_state.record(now_ms(), None, Event::Connected) {
                    Ok(()) => session_state.run(&client, &mut rx, &mut handle.0).await,
                    Err(error) => Err(error),
                };
                handle.0.abort();
                // Do not await a JoinHandle twice if session.run already consumed its result.
                result
            }
            Ok(Err(error)) => Err(error),
            Err(error) => Err(error.into()),
        };
        let rate_delay = result
            .as_ref()
            .err()
            .and_then(|e| e.downcast_ref::<RateLimited>())
            .map_or(0, |e| e.0);
        let was_synced = book.synced;
        if !was_synced {
            status.metrics.synchronization_failures += 1;
        }
        let error = result
            .err()
            .map_or_else(|| "stream stopped".into(), |e| e.to_string());
        if error.contains("buffer overflow") {
            status.metrics.buffer_overflows += 1;
        }
        if error.contains("stale") {
            status.metrics.stale_events += 1;
        }
        status.metrics.disconnections += 1;
        let recording_result = recorder
            .append(
                &session,
                now_ms(),
                None,
                Event::Invalidated {
                    reason: error.clone(),
                },
                &book,
            )
            .and_then(|_| recorder.flush());
        book.invalidate();
        status.clear("reconnecting", &error);
        if let Err(error) = recording_result {
            status.metrics.recording_failures += 1;
            status.clear(
                "recording_failed",
                "recording unavailable; collection stopped",
            );
            publish(&shared, &status);
            return Err(error);
        }
        if status.metrics.recording_failures > 0 {
            status.clear(
                "recording_failed",
                "recording unavailable; collection stopped",
            );
            publish(&shared, &status);
            bail!("recording failure");
        }
        publish(&shared, &status);
        tokio::time::sleep(Duration::from_millis(delay.max(rate_delay))).await;
        delay = if was_synced {
            config.reconnect_ms
        } else {
            delay.saturating_mul(2).min(config.max_reconnect_ms)
        };
    }
}

pub async fn collect(config: Config) -> Result<()> {
    config.validate()?;
    let shared: SharedStatus = Arc::new(Mutex::new(
        SYMBOLS
            .into_iter()
            .map(|s| (s.into(), MarketStatus::new(s)))
            .collect(),
    ));
    let mut tasks = Vec::new();
    for symbol in SYMBOLS {
        tasks.push(tokio::spawn(run_symbol(
            config.clone(),
            symbol,
            shared.clone(),
        )));
    }
    let mut interval = tokio::time::interval(Duration::from_millis(250));
    let result = loop {
        tokio::select! {
            result = tokio::signal::ctrl_c() => { result?; break Ok(()); }
            _ = interval.tick() => {
                if let Err(error) = write_status(&config.status_file, &shared, config.stale_ms) { break Err(error); }
                if tasks.iter().all(JoinHandle::is_finished) { break Err(anyhow::anyhow!("all collectors stopped; inspect market status")); }
            }
        }
    };
    for task in &tasks {
        task.abort();
    }
    for task in tasks {
        let _ = task.await;
    }
    for status in shared.lock().unwrap().values_mut() {
        if status.state != "recording_failed" {
            status.clear("stopped", "collector stopped");
        }
    }
    write_status(&config.status_file, &shared, config.stale_ms)?;
    result
}

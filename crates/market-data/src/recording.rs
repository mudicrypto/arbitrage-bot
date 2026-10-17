//! Bounded, checksummed Parquet segments. Each starts with a replay checkpoint.
use crate::{
    book::Book,
    model::{Depth, Snapshot},
};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use parquet::{
    data_type::{ByteArray, ByteArrayType},
    file::{
        reader::{FileReader, SerializedFileReader},
        writer::SerializedFileWriter,
    },
    record::RowAccessor,
    schema::parser::parse_message_type,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

pub const MAX_RECORD_BYTES: usize = 2 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Checkpoint { book: Book },
    Connected,
    ReceivedDepth { depth: Depth },
    ReceivedSnapshot { snapshot: Snapshot },
    Snapshot { snapshot: Snapshot },
    AppliedDepth { depth: Depth },
    Invalidated { reason: String },
    Rejected { reason: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub schema_version: u16,
    pub recording_id: String,
    pub connection_session: String,
    pub symbol: String,
    pub sequence: u64,
    pub received_ms: u64,
    pub raw: Option<String>,
    pub event: Event,
}
#[derive(Debug, Clone)]
pub struct Limits {
    pub segment_bytes: usize,
    pub segment_records: usize,
    pub max_segments: usize,
    pub max_file_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            segment_bytes: 2 * 1024 * 1024,
            segment_records: 64,
            max_segments: 32,
            max_file_bytes: 8 * 1024 * 1024,
        }
    }
}
pub struct Recorder {
    dir: PathBuf,
    symbol: String,
    recording_id: String,
    sequence: u64,
    rows: Vec<String>,
    bytes: usize,
    limits: Limits,
    segment: u64,
    _lock: File,
}
impl Recorder {
    pub fn open(root: &Path, symbol: &str, limits: Limits) -> Result<Self> {
        ensure!(
            crate::model::SYMBOLS.contains(&symbol),
            "unsupported symbol"
        );
        ensure!(
            limits.segment_records >= 2
                && limits.segment_bytes > 0
                && limits.max_segments > 0
                && limits.max_file_bytes > 0,
            "invalid recording limits"
        );
        let dir = root.join(symbol);
        fs::create_dir_all(&dir)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join("writer.lock"))?;
        lock.try_lock_exclusive()
            .context("recording directory already has a writer")?;
        // Only this writer owns these temporary files. Clean interrupted flushes.
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if owned_segment(&path) && path.extension().is_some_and(|s| s == "tmp") {
                fs::remove_file(path)?;
            }
        }
        let mut recorder = Self {
            dir,
            symbol: symbol.into(),
            recording_id: uuid::Uuid::new_v4().to_string(),
            sequence: 0,
            rows: vec![],
            bytes: 0,
            limits,
            segment: 0,
            _lock: lock,
        };
        recorder.prune(0)?;
        Ok(recorder)
    }
    fn record(
        &mut self,
        session: &str,
        ms: u64,
        raw: Option<String>,
        event: Event,
    ) -> Result<String> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .context("record sequence overflow")?;
        let row = serde_json::to_string(&Record {
            schema_version: 1,
            recording_id: self.recording_id.clone(),
            connection_session: session.into(),
            symbol: self.symbol.clone(),
            sequence: self.sequence,
            received_ms: ms,
            raw,
            event,
        })?;
        ensure!(row.len() <= MAX_RECORD_BYTES, "record too large");
        Ok(row)
    }
    pub fn append(
        &mut self,
        session: &str,
        ms: u64,
        raw: Option<String>,
        event: Event,
        before: &Book,
    ) -> Result<()> {
        let size = serde_json::to_vec(&event)?.len() + raw.as_ref().map_or(0, String::len) + 1024;
        ensure!(size <= MAX_RECORD_BYTES, "record too large");
        if !self.rows.is_empty()
            && (self.bytes + size > self.limits.segment_bytes
                || self.rows.len() >= self.limits.segment_records)
        {
            self.flush()?;
        }
        if self.rows.is_empty() {
            let checkpoint = self.record(
                session,
                ms,
                None,
                Event::Checkpoint {
                    book: before.clone(),
                },
            )?;
            self.bytes += checkpoint.len();
            self.rows.push(checkpoint);
        }
        let row = self.record(session, ms, raw, event)?;
        self.bytes += row.len();
        self.rows.push(row);
        Ok(())
    }
    fn segments(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if owned_segment(&path) && path.extension().is_some_and(|s| s == "parquet") {
                files.push(path);
            }
        }
        files.sort();
        Ok(files)
    }
    fn prune(&mut self, reserve: usize) -> Result<()> {
        let mut paths = self.segments()?;
        while paths.len() + reserve > self.limits.max_segments {
            let oldest = paths.remove(0);
            fs::remove_file(oldest.with_extension("sha256"))?;
            fs::remove_file(oldest)?;
        }
        // Handle orphan checksums left by interrupted publication/deletion.
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if owned_segment(&path)
                && path.extension().is_some_and(|s| s == "sha256")
                && !path.with_extension("parquet").exists()
            {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
    pub fn flush(&mut self) -> Result<()> {
        let result = self.flush_inner();
        if result.is_err()
            && let Ok(entries) = fs::read_dir(&self.dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if owned_segment(&path) && path.extension().is_some_and(|s| s == "tmp") {
                    let _ = fs::remove_file(path);
                }
            }
        }
        result
    }
    fn flush_inner(&mut self) -> Result<()> {
        if self.rows.is_empty() {
            return Ok(());
        }
        self.prune(1)?;
        self.segment += 1;
        let name = format!(
            "{:020}-{}-{:020}",
            crate::runtime::now_ms(),
            self.recording_id,
            self.segment
        );
        let path = self.dir.join(format!("{name}.parquet"));
        let temp = path.with_extension("tmp");
        let schema = Arc::new(parse_message_type(
            "message market_events { REQUIRED BINARY record (UTF8); }",
        )?);
        let mut writer = SerializedFileWriter::new(
            File::create(&temp)?,
            schema,
            Arc::new(
                parquet::file::properties::WriterProperties::builder()
                    .set_compression(parquet::basic::Compression::SNAPPY)
                    .build(),
            ),
        )?;
        let mut group = writer.next_row_group()?;
        let mut column = group.next_column()?.context("missing parquet column")?;
        let rows: Vec<ByteArray> = self
            .rows
            .iter()
            .map(|s| ByteArray::from(s.as_str()))
            .collect();
        column
            .typed::<ByteArrayType>()
            .write_batch(&rows, None, None)?;
        column.close()?;
        group.close()?;
        writer.close()?;
        ensure!(
            fs::metadata(&temp)?.len() <= self.limits.max_file_bytes,
            "segment file limit exceeded"
        );
        File::open(&temp)?.sync_all()?;
        let digest = hash_file(&temp)?;
        let checksum_temp = self.dir.join(format!("{name}.sha256.tmp"));
        let mut checksum = File::create(&checksum_temp)?;
        checksum.write_all(digest.as_bytes())?;
        checksum.sync_all()?;
        fs::rename(checksum_temp, path.with_extension("sha256"))?;
        fs::rename(&temp, &path)?;
        File::open(&self.dir)?.sync_all()?;
        self.rows.clear();
        self.bytes = 0;
        Ok(())
    }
    pub fn buffered_bytes(&self) -> usize {
        self.bytes
    }
}
// Only mutate files generated by this recorder; preserve unrelated local files.
fn owned_segment(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    let base = name.split('.').next().unwrap_or_default();
    base.is_ascii()
        && base.len() == 78
        && base[..20].bytes().all(|c| c.is_ascii_digit())
        && base.as_bytes()[20] == b'-'
        && base.as_bytes()[57] == b'-'
        && uuid::Uuid::parse_str(&base[21..57]).is_ok()
        && base[58..].bytes().all(|c| c.is_ascii_digit())
}
fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}
pub fn segment_paths(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            files.extend(segment_paths(&path)?);
        } else if path.extension().is_some_and(|s| s == "parquet") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ReplayResult {
    pub symbol: String,
    pub recording_id: String,
    pub final_sequence: u64,
    pub records: usize,
    pub book: Book,
}
pub fn replay(path: &Path, max_levels: usize, max_file_bytes: u64) -> Result<ReplayResult> {
    ensure!(
        fs::metadata(path)?.len() <= max_file_bytes,
        "replay file too large"
    );
    ensure!(
        hash_file(path)? == fs::read_to_string(path.with_extension("sha256"))?,
        "recording checksum mismatch"
    );
    let reader = SerializedFileReader::new(File::open(path)?)?;
    let mut book = Book::default();
    let mut identity: Option<(String, String)> = None;
    let mut sequence = 0;
    let mut records = 0;
    for row in reader.get_row_iter(None)? {
        let row = row?;
        let data = row.get_string(0)?;
        ensure!(data.len() <= MAX_RECORD_BYTES, "replay record too large");
        let record: Record = serde_json::from_str(data)?;
        ensure!(record.schema_version == 1, "unknown record schema");
        if records == 0 {
            ensure!(
                matches!(record.event, Event::Checkpoint { .. }),
                "checkpoint missing"
            );
            identity = Some((record.symbol.clone(), record.recording_id.clone()));
        } else {
            ensure!(record.sequence == sequence + 1, "record sequence gap");
            ensure!(
                identity.as_ref() == Some(&(record.symbol.clone(), record.recording_id.clone())),
                "record identity changed"
            );
            ensure!(
                !matches!(record.event, Event::Checkpoint { .. }),
                "unexpected checkpoint"
            );
        }
        match record.event {
            Event::Checkpoint { book: saved } => {
                if saved.last_id.is_some() {
                    saved.validate(max_levels)?;
                }
                book = saved;
            }
            Event::Snapshot { snapshot } => book.snapshot(&snapshot, max_levels)?,
            Event::AppliedDepth { depth } => {
                book.apply(&depth, max_levels)?;
            }
            Event::Invalidated { .. } => book.invalidate(),
            _ => (),
        }
        sequence = record.sequence;
        records += 1;
    }
    let (symbol, recording_id) = identity.context("empty segment")?;
    Ok(ReplayResult {
        symbol,
        recording_id,
        final_sequence: sequence,
        records,
        book,
    })
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

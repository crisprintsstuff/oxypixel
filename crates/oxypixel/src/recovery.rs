//! Crash recovery copies. These live in the app data directory, next to `recent.txt`.
//!
//! A recovery file is a normal `.oxypix` plus a small text sidecar. It is never
//! the path the user saved. Writes go to a temp file in the same directory and
//! then rename into place. The window does that work on a background thread.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oxypixel_core::{save_oxypix_bytes, Document};
use sha2::{Digest, Sha256};

use crate::recent;

/// How long a dirty document waits before the next recovery copy.
pub const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(60);

/// How many document edits trigger a recovery copy before the interval.
pub const AUTOSAVE_EDIT_BATCH: u64 = 30;

const META_HEADER: &str = "oxypixel-recovery 1";
const MAX_META_BYTES: u64 = 64 * 1024;
const MAX_LISTED: usize = 32;

/// Whether a recovery copy should be scheduled.
///
/// `busy` is true while a stroke or a slider drag is still going, so the copy
/// is not taken on the frame that is painting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutosaveQuery {
    pub enabled: bool,
    pub dirty: bool,
    pub busy: bool,
    pub write_in_flight: bool,
    pub edits_since_recovery: u64,
    pub elapsed: Duration,
}

pub fn should_write(query: AutosaveQuery) -> bool {
    if !query.enabled || !query.dirty || query.busy || query.write_in_flight {
        return false;
    }
    if query.edits_since_recovery == 0 {
        return false;
    }
    query.edits_since_recovery >= AUTOSAVE_EDIT_BATCH || query.elapsed >= AUTOSAVE_INTERVAL
}

/// What to store beside the `.oxypix` bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryRecord {
    pub id: String,
    pub name: String,
    pub source: Option<PathBuf>,
    pub saved_unix: u64,
    /// Reference sidecar text, written as `{id}.oxypix.refs` beside the copy.
    /// Empty writes no sidecar and removes an older one.
    pub refs: Vec<u8>,
}

/// One recovery copy found on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryEntry {
    pub id: String,
    pub name: String,
    pub source: Option<PathBuf>,
    pub saved_unix: u64,
    pub file: PathBuf,
}

/// `data/recovery/`. Missing when the platform data directory cannot be resolved.
pub fn directory() -> Option<PathBuf> {
    recent::data_dir().map(|dir| dir.join("recovery"))
}

pub fn autosave_enabled(dir: &Path) -> bool {
    match std::fs::read_to_string(dir.join("autosave.txt")) {
        Ok(text) => !text.trim().eq_ignore_ascii_case("off"),
        Err(_) => true,
    }
}

pub fn set_autosave_enabled(dir: &Path, enabled: bool) -> io::Result<()> {
    let text = if enabled { "on\n" } else { "off\n" };
    write_atomic(dir, &dir.join("autosave.txt"), text.as_bytes())
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

pub fn format_unix_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// Stable id for a saved document. The same path always maps to the same file.
pub fn id_for_path(path: &Path) -> String {
    let digest = Sha256::digest(path_key(path).as_bytes());
    format!("doc-{}", hex_encode(&digest[..16]))
}

pub fn untitled_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!("untitled-{nanos:x}-{n:x}")
}

pub fn list(dir: &Path) -> Vec<RecoveryEntry> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut seen = 0usize;
    for entry in read.flatten() {
        seen += 1;
        if seen > 256 {
            break;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(id) = name.strip_suffix(".oxypix") else {
            continue;
        };
        if !safe_id(id) {
            continue;
        }
        let meta_path = dir.join(format!("{id}.meta"));
        let Ok(meta_bytes) = std::fs::read(&meta_path) else {
            continue;
        };
        if meta_bytes.len() as u64 > MAX_META_BYTES {
            continue;
        }
        let Ok(text) = std::str::from_utf8(&meta_bytes) else {
            continue;
        };
        let Some(parsed) = parse_meta(text) else {
            continue;
        };
        found.push(RecoveryEntry {
            id: id.to_string(),
            name: parsed.name,
            source: parsed.source,
            saved_unix: parsed.saved_unix,
            file: dir.join(name),
        });
    }
    found.sort_by(|a, b| {
        b.saved_unix
            .cmp(&a.saved_unix)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
    found.truncate(MAX_LISTED);
    found
}

pub fn load_document(path: &Path) -> Result<Document, oxypixel_core::Error> {
    oxypixel_core::load_oxypix(path)
}

pub fn store_document(dir: &Path, record: &RecoveryRecord, doc: &Document) -> io::Result<()> {
    let bytes = save_oxypix_bytes(doc).map_err(|err| io::Error::other(err.to_string()))?;
    write_recovery(dir, record, &bytes, &|| false)
}

/// Write `bytes` as `{id}.oxypix` and a sidecar. `cancel` skips the rename.
pub fn write_recovery(
    dir: &Path,
    record: &RecoveryRecord,
    bytes: &[u8],
    cancel: &dyn Fn() -> bool,
) -> io::Result<()> {
    let staged = stage_recovery(dir, record, bytes)?;
    if cancel() {
        drop(staged);
        return Ok(());
    }
    commit_staged(staged)
}

pub fn remove_id(dir: &Path, id: &str) -> io::Result<()> {
    if !safe_id(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "recovery id is not a single file name",
        ));
    }
    let _ = std::fs::remove_file(dir.join(format!("{id}.oxypix")));
    let _ = std::fs::remove_file(dir.join(format!("{id}.oxypix.refs")));
    let _ = std::fs::remove_file(dir.join(format!("{id}.meta")));
    Ok(())
}

/// Drop temp files left by a crash. Do not call this while a write is in progress.
pub fn clear_temps(dir: &Path) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') && name.ends_with(".tmp") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub fn oxypix_path(dir: &Path, id: &str) -> Option<PathBuf> {
    if !safe_id(id) {
        return None;
    }
    Some(dir.join(format!("{id}.oxypix")))
}

enum Command {
    Write(Box<WriteJob>),
    Shutdown,
}

pub struct WriteJob {
    pub epoch: u64,
    pub generation: u64,
    pub dir: PathBuf,
    pub record: RecoveryRecord,
    pub document: Document,
}

#[derive(Debug)]
pub enum RecoveryEvent {
    Wrote { epoch: u64, generation: u64 },
    Cancelled,
    Failed { message: String },
}

/// Background writer. The UI thread only clones the document and sends it.
pub struct RecoveryWriter {
    tx: Option<Sender<Command>>,
    events: Receiver<RecoveryEvent>,
    join: Option<std::thread::JoinHandle<()>>,
    epoch: Arc<AtomicU64>,
    publish: Arc<Mutex<()>>,
    dead: AtomicBool,
}

impl RecoveryWriter {
    pub fn spawn() -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let (event_tx, events) = mpsc::channel();
        let epoch = Arc::new(AtomicU64::new(1));
        let publish = Arc::new(Mutex::new(()));
        let epoch_thread = Arc::clone(&epoch);
        let publish_thread = Arc::clone(&publish);
        let join = std::thread::Builder::new()
            .name("oxypixel-recovery".to_string())
            .spawn(move || worker_loop(rx, event_tx, epoch_thread, publish_thread))?;
        Ok(Self {
            tx: Some(tx),
            events,
            join: Some(join),
            epoch,
            publish,
            dead: AtomicBool::new(false),
        })
    }

    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    pub fn submit(&self, job: WriteJob) -> Result<(), ()> {
        match &self.tx {
            Some(tx) => tx.send(Command::Write(Box::new(job))).map_err(|_| ()),
            None => Err(()),
        }
    }

    pub fn poll(&self) -> Option<RecoveryEvent> {
        if self.dead.load(Ordering::Relaxed) {
            return None;
        }
        match self.events.try_recv() {
            Ok(event) => Some(event),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.dead.store(true, Ordering::Relaxed);
                Some(RecoveryEvent::Failed {
                    message: "recovery writer stopped".to_string(),
                })
            }
        }
    }

    /// Drop an in-flight write. A job that has not renamed yet will not land.
    pub fn retire_inflight(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let _guard = lock_publish(&self.publish);
    }

    /// Bump the epoch and delete this id. Holds the publish lock across the delete.
    pub fn invalidate(&self, dir: &Path, id: &str) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let _guard = lock_publish(&self.publish);
        let _ = remove_id(dir, id);
    }

    pub fn shutdown(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Command::Shutdown);
        }
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for RecoveryWriter {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker_loop(
    rx: Receiver<Command>,
    tx: Sender<RecoveryEvent>,
    epoch: Arc<AtomicU64>,
    publish: Arc<Mutex<()>>,
) {
    while let Ok(command) = rx.recv() {
        match command {
            Command::Shutdown => break,
            Command::Write(job) => {
                let event = run_write(*job, &epoch, &publish);
                if tx.send(event).is_err() {
                    break;
                }
            }
        }
    }
}

fn run_write(job: WriteJob, epoch: &AtomicU64, publish: &Mutex<()>) -> RecoveryEvent {
    if epoch.load(Ordering::SeqCst) != job.epoch {
        return RecoveryEvent::Cancelled;
    }
    let bytes = match save_oxypix_bytes(&job.document) {
        Ok(bytes) => bytes,
        Err(err) => {
            return RecoveryEvent::Failed {
                message: err.to_string(),
            };
        }
    };
    if epoch.load(Ordering::SeqCst) != job.epoch {
        return RecoveryEvent::Cancelled;
    }
    let staged = match stage_recovery(&job.dir, &job.record, &bytes) {
        Ok(staged) => staged,
        Err(err) => {
            return RecoveryEvent::Failed {
                message: err.to_string(),
            };
        }
    };
    let _guard = lock_publish(publish);
    if epoch.load(Ordering::SeqCst) != job.epoch {
        drop(staged);
        return RecoveryEvent::Cancelled;
    }
    match commit_staged(staged) {
        Ok(()) => RecoveryEvent::Wrote {
            epoch: job.epoch,
            generation: job.generation,
        },
        Err(err) => RecoveryEvent::Failed {
            message: err.to_string(),
        },
    }
}

fn lock_publish(publish: &Mutex<()>) -> std::sync::MutexGuard<'_, ()> {
    publish.lock().unwrap_or_else(|poison| poison.into_inner())
}

struct StagedRecovery {
    ox_tmp: PathBuf,
    refs_tmp: PathBuf,
    meta_tmp: PathBuf,
    ox_dest: PathBuf,
    refs_dest: PathBuf,
    meta_dest: PathBuf,
}

impl Drop for StagedRecovery {
    fn drop(&mut self) {
        if !self.ox_tmp.as_os_str().is_empty() {
            let _ = std::fs::remove_file(&self.ox_tmp);
        }
        if !self.refs_tmp.as_os_str().is_empty() {
            let _ = std::fs::remove_file(&self.refs_tmp);
        }
        if !self.meta_tmp.as_os_str().is_empty() {
            let _ = std::fs::remove_file(&self.meta_tmp);
        }
    }
}

fn stage_recovery(dir: &Path, record: &RecoveryRecord, bytes: &[u8]) -> io::Result<StagedRecovery> {
    let (ox_dest, refs_dest, meta_dest) = dest_paths(dir, &record.id)?;
    std::fs::create_dir_all(dir)?;
    // Built first so an early return below drops whatever was already written.
    let mut staged = StagedRecovery {
        ox_tmp: temp_file(dir, "oxypix"),
        refs_tmp: PathBuf::new(),
        meta_tmp: PathBuf::new(),
        ox_dest,
        refs_dest,
        meta_dest,
    };
    write_synced(&staged.ox_tmp, bytes)?;
    if !record.refs.is_empty() {
        staged.refs_tmp = temp_file(dir, "refs");
        write_synced(&staged.refs_tmp, &record.refs)?;
    }
    staged.meta_tmp = temp_file(dir, "meta");
    write_synced(&staged.meta_tmp, encode_meta(record).as_bytes())?;
    Ok(staged)
}

fn commit_staged(mut staged: StagedRecovery) -> io::Result<()> {
    replace_file(&staged.ox_tmp, &staged.ox_dest)?;
    staged.ox_tmp = PathBuf::new();
    // The sidecar lands before the meta file, which is what marks a copy complete.
    if staged.refs_tmp.as_os_str().is_empty() {
        let _ = std::fs::remove_file(&staged.refs_dest);
    } else {
        replace_file(&staged.refs_tmp, &staged.refs_dest)?;
        staged.refs_tmp = PathBuf::new();
    }
    replace_file(&staged.meta_tmp, &staged.meta_dest)?;
    staged.meta_tmp = PathBuf::new();
    Ok(())
}

fn dest_paths(dir: &Path, id: &str) -> io::Result<(PathBuf, PathBuf, PathBuf)> {
    if !safe_id(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "recovery id is not a single file name",
        ));
    }
    Ok((
        dir.join(format!("{id}.oxypix")),
        dir.join(format!("{id}.oxypix.refs")),
        dir.join(format!("{id}.meta")),
    ))
}

fn encode_meta(record: &RecoveryRecord) -> String {
    let name = one_line(if record.name.trim().is_empty() {
        "Untitled"
    } else {
        record.name.trim()
    });
    let path = match &record.source {
        Some(path) => one_line(&path.to_string_lossy()),
        None => "-".to_string(),
    };
    format!(
        "{META_HEADER}\nsaved_unix {}\nname {name}\npath {path}\n",
        record.saved_unix
    )
}

fn one_line(text: &str) -> String {
    text.chars()
        .map(|ch| if ch == '\n' || ch == '\r' { ' ' } else { ch })
        .take(512)
        .collect()
}

struct ParsedMeta {
    name: String,
    source: Option<PathBuf>,
    saved_unix: u64,
}

fn parse_meta(text: &str) -> Option<ParsedMeta> {
    let mut lines = text.lines();
    if lines.next()?.trim() != META_HEADER {
        return None;
    }
    let mut saved_unix = None;
    let mut name = None;
    let mut source = None;
    let mut saw_path = false;
    for line in lines {
        if let Some(rest) = line.strip_prefix("saved_unix ") {
            saved_unix = rest.trim().parse::<u64>().ok();
        } else if let Some(rest) = line.strip_prefix("name ") {
            let trimmed = rest.trim();
            name = Some(if trimmed.is_empty() {
                "Untitled".to_string()
            } else {
                trimmed.to_string()
            });
        } else if let Some(rest) = line.strip_prefix("path ") {
            saw_path = true;
            let trimmed = rest.trim();
            source = if trimmed.is_empty() || trimmed == "-" {
                None
            } else {
                Some(PathBuf::from(trimmed))
            };
        }
    }
    if !saw_path {
        return None;
    }
    Some(ParsedMeta {
        name: name?,
        source,
        saved_unix: saved_unix?,
    })
}

fn safe_id(id: &str) -> bool {
    let len = id.len();
    (1..=80).contains(&len)
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn path_key(path: &Path) -> String {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path),
            Err(_) => path.to_path_buf(),
        }
    };
    let canonical = std::fs::canonicalize(&joined).unwrap_or(joined);
    let text = canonical.to_string_lossy();
    #[cfg(windows)]
    {
        text.to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        text.into_owned()
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn write_atomic(dir: &Path, dest: &Path, bytes: &[u8]) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = temp_file(dir, "autosave");
    write_synced(&tmp, bytes)?;
    if let Err(err) = replace_file(&tmp, dest) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(windows)]
    if to.exists() {
        std::fs::remove_file(to)?;
    }
    std::fs::rename(from, to)
}

fn temp_file(dir: &Path, label: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(".{label}.{}.{n:x}.tmp", std::process::id()))
}

/// Days since 1970-01-01 to a civil date. Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxypixel_core::{Editor, MouseButton, Rgba};
    use std::time::Instant;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!(
                "oxypixel-recovery-{label}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn painted() -> (Document, Vec<u8>) {
        let mut editor = Editor::new(4, 4).unwrap();
        editor.set_primary(Rgba::new(9, 8, 7, 255));
        editor.pointer_down(1, 2, MouseButton::Primary, false);
        editor.pointer_up(1, 2, false);
        let bytes = save_oxypix_bytes(editor.document()).unwrap();
        (editor.document().clone(), bytes)
    }

    fn record(id: &str, name: &str, source: Option<PathBuf>, saved_unix: u64) -> RecoveryRecord {
        RecoveryRecord {
            id: id.to_string(),
            name: name.to_string(),
            source,
            saved_unix,
            refs: Vec::new(),
        }
    }

    #[test]
    fn recovery_write_detect_restore_and_cleanup() {
        let dir = TempDir::new("life");
        let user_file = dir.path().join("drawings").join("cottage.oxypix");
        let (doc, bytes) = painted();
        assert_eq!(doc.composite_pixel(0, 1, 2), Some(Rgba::new(9, 8, 7, 255)));
        let id = id_for_path(&user_file);
        let saved = record(
            &id,
            "cottage.oxypix",
            Some(user_file.clone()),
            1_700_000_000,
        );
        write_recovery(dir.path(), &saved, &bytes, &|| false).unwrap();

        assert!(
            !user_file.exists(),
            "recovery must not create the user's file"
        );
        let listed = list(dir.path());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, id);
        assert_eq!(listed[0].name, "cottage.oxypix");
        assert_eq!(listed[0].saved_unix, 1_700_000_000);
        assert_eq!(listed[0].source.as_deref(), Some(user_file.as_path()));
        assert_eq!(
            format_unix_utc(listed[0].saved_unix),
            "2023-11-14 22:13 UTC"
        );

        let restored = load_document(&listed[0].file).unwrap();
        assert_eq!(
            restored.composite_pixel(0, 1, 2),
            Some(Rgba::new(9, 8, 7, 255))
        );
        assert_eq!(save_oxypix_bytes(&restored).unwrap(), bytes);

        let mut replaced = Editor::new(4, 4).unwrap();
        replaced.set_primary(Rgba::new(1, 2, 3, 255));
        replaced.pointer_down(0, 0, MouseButton::Primary, false);
        replaced.pointer_up(0, 0, false);
        let next = save_oxypix_bytes(replaced.document()).unwrap();
        let later = record(
            &id,
            "cottage.oxypix",
            Some(user_file.clone()),
            1_700_000_060,
        );
        write_recovery(dir.path(), &later, &next, &|| false).unwrap();
        let listed = list(dir.path());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].saved_unix, 1_700_000_060);
        let restored = load_document(&listed[0].file).unwrap();
        assert_eq!(
            restored.composite_pixel(0, 0, 0),
            Some(Rgba::new(1, 2, 3, 255))
        );
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.iter().all(|name| !name.ends_with(".tmp")),
            "temp files should be gone, found {names:?}"
        );

        remove_id(dir.path(), &id).unwrap();
        assert!(list(dir.path()).is_empty());
        assert!(!dir.path().join(format!("{id}.oxypix")).exists());
        assert!(!dir.path().join(format!("{id}.meta")).exists());
        assert!(!user_file.exists());
    }

    #[test]
    fn references_are_kept_beside_the_recovery_copy() {
        let dir = TempDir::new("refs");
        let (doc, bytes) = painted();
        let image = dir.path().join("trace.png");
        oxypixel_core::export_png(&doc, 0, &image).unwrap();
        let id = untitled_id();
        let copy = oxypix_path(dir.path(), &id).unwrap();
        let refs_file = oxypixel_core::sidecar_path(&copy);

        let mut refs = oxypixel_core::References::new();
        refs.add_file(&image).unwrap();
        refs.set_opacity(0, 77).unwrap();
        let mut saved = record(&id, "Untitled", None, 1_792_778_640);
        saved.refs = refs.sidecar_bytes(&copy).unwrap();
        write_recovery(dir.path(), &saved, &bytes, &|| false).unwrap();
        let listed = list(dir.path());
        assert_eq!(listed.len(), 1);
        let restored = oxypixel_core::References::read_sidecar(&listed[0].file).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored.opacity(0), 77);
        assert!(!restored.missing(0));

        // An empty list is stored too, so a restore does not fall back to older references.
        saved.refs = oxypixel_core::References::new()
            .sidecar_bytes(&copy)
            .unwrap();
        write_recovery(dir.path(), &saved, &bytes, &|| false).unwrap();
        assert!(refs_file.is_file());
        assert!(oxypixel_core::References::read_sidecar(&copy)
            .unwrap()
            .is_empty());

        // A cancelled write keeps the previous sidecar.
        saved.refs = refs.sidecar_bytes(&copy).unwrap();
        write_recovery(dir.path(), &saved, &bytes, &|| true).unwrap();
        assert!(oxypixel_core::References::read_sidecar(&copy)
            .unwrap()
            .is_empty());

        // No bytes at all removes an older sidecar, and discarding removes it.
        write_recovery(dir.path(), &saved, &bytes, &|| false).unwrap();
        assert!(refs_file.is_file());
        saved.refs.clear();
        write_recovery(dir.path(), &saved, &bytes, &|| false).unwrap();
        assert!(!refs_file.exists());
        saved.refs = refs.sidecar_bytes(&copy).unwrap();
        write_recovery(dir.path(), &saved, &bytes, &|| false).unwrap();
        remove_id(dir.path(), &id).unwrap();
        assert!(!refs_file.exists());
        assert!(list(dir.path()).is_empty());
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn untitled_recovery_roundtrip() {
        let dir = TempDir::new("untitled");
        let (_doc, bytes) = painted();
        let id = untitled_id();
        assert_ne!(id, untitled_id());
        let saved = record(&id, "Untitled", None, 1_792_778_640);
        write_recovery(dir.path(), &saved, &bytes, &|| false).unwrap();
        let listed = list(dir.path());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Untitled");
        assert!(listed[0].source.is_none());
        assert_eq!(
            format_unix_utc(listed[0].saved_unix),
            "2026-10-23 18:04 UTC"
        );
        let restored = load_document(&listed[0].file).unwrap();
        assert_eq!(save_oxypix_bytes(&restored).unwrap(), bytes);
        remove_id(dir.path(), &id).unwrap();
        assert!(list(dir.path()).is_empty());
    }

    #[test]
    fn list_skips_partial_and_unsafe_names() {
        let dir = TempDir::new("partial");
        let (_doc, bytes) = painted();
        let good = record("doc-good", "kept.oxypix", None, 50);
        write_recovery(dir.path(), &good, &bytes, &|| false).unwrap();
        std::fs::write(
            dir.path().join("doc-metaonly.meta"),
            b"oxypixel-recovery 1\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("doc-pixonly.oxypix"), &bytes).unwrap();
        std::fs::write(dir.path().join("BAD.oxypix"), &bytes).unwrap();
        std::fs::write(dir.path().join(".doc-good.oxypix.1.tmp"), b"partial").unwrap();
        std::fs::write(dir.path().join("doc-badheader.oxypix"), &bytes).unwrap();
        std::fs::write(
            dir.path().join("doc-badheader.meta"),
            b"not-a-recovery\nsaved_unix 1\nname X\npath -\n",
        )
        .unwrap();

        let listed = list(dir.path());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "doc-good");

        let outside = dir.path().join("keep.txt");
        std::fs::write(&outside, b"keep").unwrap();
        let err = remove_id(dir.path(), "../keep.txt").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
    }

    #[test]
    fn cancelled_write_leaves_the_previous_copy() {
        let dir = TempDir::new("cancel");
        let (_doc, bytes) = painted();
        let saved = record("doc-cancel", "sprite.oxypix", None, 10);
        write_recovery(dir.path(), &saved, &bytes, &|| false).unwrap();
        let mut other = Editor::new(2, 2).unwrap();
        other.set_primary(Rgba::new(4, 5, 6, 255));
        other.pointer_down(0, 0, MouseButton::Primary, false);
        other.pointer_up(0, 0, false);
        let next = save_oxypix_bytes(other.document()).unwrap();
        write_recovery(dir.path(), &saved, &next, &|| true).unwrap();
        let restored = load_document(&list(dir.path())[0].file).unwrap();
        assert_eq!(save_oxypix_bytes(&restored).unwrap(), bytes);
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|name| !name.ends_with(".tmp")));
    }

    #[test]
    fn newest_recovery_is_listed_first() {
        let dir = TempDir::new("order");
        let (_doc, bytes) = painted();
        write_recovery(
            dir.path(),
            &record("doc-old", "old.oxypix", None, 10),
            &bytes,
            &|| false,
        )
        .unwrap();
        write_recovery(
            dir.path(),
            &record("doc-new", "new.oxypix", None, 20),
            &bytes,
            &|| false,
        )
        .unwrap();
        let listed = list(dir.path());
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].name, "new.oxypix");
        assert_eq!(listed[1].name, "old.oxypix");
        remove_id(dir.path(), "doc-new").unwrap();
        let listed = list(dir.path());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "doc-old");
    }

    #[test]
    fn autosave_waits_for_the_interval_or_a_batch_of_edits() {
        let base = AutosaveQuery {
            enabled: true,
            dirty: true,
            busy: false,
            write_in_flight: false,
            edits_since_recovery: 1,
            elapsed: Duration::from_secs(0),
        };
        assert!(!should_write(base));
        assert!(!should_write(AutosaveQuery {
            elapsed: Duration::from_secs(59),
            ..base
        }));
        assert!(should_write(AutosaveQuery {
            elapsed: AUTOSAVE_INTERVAL,
            ..base
        }));
        assert!(!should_write(AutosaveQuery {
            edits_since_recovery: AUTOSAVE_EDIT_BATCH - 1,
            ..base
        }));
        assert!(should_write(AutosaveQuery {
            edits_since_recovery: AUTOSAVE_EDIT_BATCH,
            ..base
        }));
        assert!(!should_write(AutosaveQuery {
            enabled: false,
            elapsed: AUTOSAVE_INTERVAL,
            ..base
        }));
        assert!(!should_write(AutosaveQuery {
            dirty: false,
            elapsed: AUTOSAVE_INTERVAL,
            ..base
        }));
        assert!(!should_write(AutosaveQuery {
            busy: true,
            edits_since_recovery: AUTOSAVE_EDIT_BATCH,
            ..base
        }));
        assert!(!should_write(AutosaveQuery {
            write_in_flight: true,
            elapsed: AUTOSAVE_INTERVAL,
            ..base
        }));
        assert!(!should_write(AutosaveQuery {
            edits_since_recovery: 0,
            elapsed: Duration::from_secs(120),
            ..base
        }));
    }

    #[test]
    fn autosave_setting_defaults_on_and_can_be_turned_off() {
        let dir = TempDir::new("setting");
        assert!(autosave_enabled(dir.path()));
        set_autosave_enabled(dir.path(), false).unwrap();
        assert!(!autosave_enabled(dir.path()));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("autosave.txt")).unwrap(),
            "off\n"
        );
        set_autosave_enabled(dir.path(), true).unwrap();
        assert!(autosave_enabled(dir.path()));
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|name| !name.ends_with(".tmp")));
    }

    #[test]
    fn path_ids_are_stable_and_do_not_create_the_file() {
        let dir = TempDir::new("ids");
        let path = dir.path().join("missing.oxypix");
        let first = id_for_path(&path);
        let second = id_for_path(&path);
        assert_eq!(first, second);
        assert!(safe_id(&first));
        assert_ne!(first, id_for_path(&dir.path().join("other.oxypix")));
        assert!(!path.exists());
    }

    #[test]
    fn format_unix_utc_known_dates() {
        assert_eq!(format_unix_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_unix_utc(86_400), "1970-01-02 00:00 UTC");
        assert_eq!(format_unix_utc(1_709_164_800), "2024-02-29 00:00 UTC");
        assert_eq!(format_unix_utc(1_792_778_640), "2026-10-23 18:04 UTC");
    }

    #[test]
    fn background_write_publishes_and_a_stale_epoch_does_not_replace_it() {
        let dir = TempDir::new("thread");
        let mut writer = RecoveryWriter::spawn().unwrap();
        let (doc, bytes) = painted();
        let saved = record("doc-thread", "sprite.oxypix", None, 3);
        let epoch = writer.epoch();
        writer
            .submit(WriteJob {
                epoch,
                generation: 4,
                dir: dir.path().to_path_buf(),
                record: saved.clone(),
                document: doc,
            })
            .unwrap();
        let event = wait_event(&writer);
        match event {
            RecoveryEvent::Wrote {
                epoch: wrote,
                generation,
            } => {
                assert_eq!(wrote, epoch);
                assert_eq!(generation, 4);
            }
            other => panic!("expected a write, got {other:?}"),
        }
        assert_eq!(list(dir.path()).len(), 1);
        assert_eq!(
            std::fs::read(list(dir.path())[0].file.as_path()).unwrap(),
            bytes
        );

        writer.retire_inflight();
        let mut other = Editor::new(2, 2).unwrap();
        other.set_primary(Rgba::new(9, 9, 9, 255));
        other.pointer_down(0, 0, MouseButton::Primary, false);
        other.pointer_up(0, 0, false);
        writer
            .submit(WriteJob {
                epoch,
                generation: 9,
                dir: dir.path().to_path_buf(),
                record: saved,
                document: other.document().clone(),
            })
            .unwrap();
        let event = wait_event(&writer);
        assert!(matches!(event, RecoveryEvent::Cancelled));
        let restored = load_document(&list(dir.path())[0].file).unwrap();
        assert_eq!(save_oxypix_bytes(&restored).unwrap(), bytes);
        writer.shutdown();
    }

    fn wait_event(writer: &RecoveryWriter) -> RecoveryEvent {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(event) = writer.poll() {
                return event;
            }
            if Instant::now() >= deadline {
                panic!("recovery writer did not finish before the deadline");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

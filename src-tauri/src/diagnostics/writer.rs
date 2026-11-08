use super::Health;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const RETAIN_FILES: usize = 7;
// RollingFileAppender suppresses rollover failures and prints OS errors. Use
// tracing-appender's maintained nonblocking worker with this fallible writer.
pub(super) struct BoundedWriter {
    file: File,
    day: u64,
    directory: PathBuf,
    health: Arc<Health>,
}
fn today() -> io::Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs() / 86400)
        .map_err(|_| io::Error::other("diagnostic clock unavailable"))
}
fn files(directory: &Path) -> io::Result<Vec<(u64, PathBuf, u64)>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(day) = name
            .strip_prefix("jentrics.")
            .and_then(|name| name.strip_suffix(".jsonl"))
            .and_then(|day| day.parse::<u64>().ok())
        else {
            continue;
        };
        // DirEntry caches Windows enumeration metadata, including a length
        // which can lag writes. Query the path for the current file size.
        let metadata = fs::metadata(entry.path())?;
        if metadata.is_file() {
            files.push((day, entry.path(), metadata.len()));
        }
    }
    Ok(files)
}
impl BoundedWriter {
    pub fn open(directory: &Path, health: Arc<Health>) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        let day = today()?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join(format!("jentrics.{day}.jsonl")))?;
        let mut writer = Self {
            file,
            day,
            directory: directory.to_owned(),
            health,
        };
        writer.prune()?;
        Ok(writer)
    }
    fn prune(&mut self) -> io::Result<()> {
        let mut entries = files(&self.directory)?;
        entries.sort_by_key(|entry| entry.0);
        let remove = entries.len().saturating_sub(RETAIN_FILES);
        for (_, path, _) in entries.into_iter().take(remove) {
            fs::remove_file(path)?;
        }
        Ok(())
    }
    fn ensure_day(&mut self, day: u64) -> io::Result<()> {
        if self.day == day {
            return Ok(());
        }
        self.file.flush()?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.directory.join(format!("jentrics.{day}.jsonl")))?;
        self.file = file;
        self.day = day;
        self.prune()
    }
    fn write_bounded(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.ensure_day(today()?)?;
        let total = files(&self.directory)?
            .iter()
            .fold(0u64, |sum, entry| sum.saturating_add(entry.2));
        if total.saturating_add(bytes.len() as u64) > MAX_BYTES {
            self.health.failure.store(3, Ordering::Relaxed);
            return Err(io::Error::other("diagnostic capacity reached"));
        }
        self.file.write_all(bytes)?;
        self.health.written.fetch_add(1, Ordering::Relaxed);
        Ok(bytes.len())
    }
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = self.write_bounded(bytes);
        if result.is_err() && self.health.failure.load(Ordering::Relaxed) == 0 {
            self.health.failure.store(2, Ordering::Relaxed);
        }
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        // Upstream's final flush prints arbitrary OS errors. Record a fixed
        // status here and consume that error before it reaches its stderr.
        if self.file.flush().is_err() {
            self.health.failure.store(2, Ordering::Relaxed);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn daily_rollover_writes_separate_files_and_prunes_real_old_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut writer = BoundedWriter::open(dir.path(), Arc::new(Health::default())).unwrap();
        let day = writer.day;
        for offset in 1..=9 {
            writer.ensure_day(day + offset).unwrap();
            writer.file.write_all(b"{}\n").unwrap();
        }
        let entries = files(dir.path()).unwrap();
        assert_eq!(entries.len(), 7);
        assert!(entries.iter().all(|entry| entry.0 >= day + 3));
        assert!(entries.iter().all(|entry| entry.2 == 3), "{entries:?}");
    }
    #[test]
    fn missing_log_directory_is_observable_without_retaining_raw_error() {
        let dir = tempfile::tempdir().unwrap();
        let health = Arc::new(Health::default());
        let mut writer = BoundedWriter::open(dir.path(), health.clone()).unwrap();
        writer.directory = dir.path().join("private missing directory");
        assert!(writer.write(b"{}\n").is_err());
        assert_eq!(health.failure.load(Ordering::Relaxed), 2);
    }
}

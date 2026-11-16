//! dhat heap profiler control (feature `dhat-heap`, works on Windows and Linux).
//!
//! dhat writes its JSON report when the profiler is dropped. To get dumps while the
//! server keeps running, `snapshot()` drops the current profiler (writing its file)
//! and immediately starts a new one. Each file therefore covers one time window:
//! blocks still listed as live at the end of a window ("t-end") were allocated in that
//! window and not freed yet -> leak candidates if they keep showing up window after window.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const OUTPUT_DIR: &str = "dhat-heap";

/// Running profiler + the file it will write when dropped.
static PROFILER: Mutex<Option<(dhat::Profiler, String)>> = Mutex::new(None);
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

pub struct Snapshot {
    pub file: String,
    pub stats: dhat::HeapStats,
}

/// Start profiling (no-op if already running).
pub fn start() {
    let mut guard = PROFILER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.is_none() {
        *guard = Some(new_profiler());
    }
}

/// Stop profiling and write the final report.
pub fn stop() -> Option<String> {
    let taken = PROFILER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    taken.map(|(profiler, file)| {
        drop(profiler); // writes the report
        file
    })
}

/// Live heap counters of the current window, if profiling.
pub fn stats() -> Option<dhat::HeapStats> {
    let guard = PROFILER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.as_ref().map(|_| dhat::HeapStats::get())
}

/// Write the current window to disk and start a new window.
pub fn snapshot() -> Option<Snapshot> {
    let mut guard = PROFILER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (profiler, file) = guard.take()?;
    let stats = dhat::HeapStats::get();
    drop(profiler); // writes `file`
    *guard = Some(new_profiler());
    Some(Snapshot { file, stats })
}

fn new_profiler() -> (dhat::Profiler, String) {
    let _ = std::fs::create_dir_all(OUTPUT_DIR);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file = format!("{OUTPUT_DIR}/dhat-heap-{timestamp}-{sequence:03}.json");
    // dhat keeps only 10 frames per stack by default, which often cuts the stack off
    // inside library code (image, tokio, axum) before it reaches this app's functions.
    let profiler = dhat::Profiler::builder()
        .file_name(&file)
        .trim_backtraces(Some(40))
        .build();
    (profiler, file)
}

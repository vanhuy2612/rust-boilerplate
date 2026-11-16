//! Runtime memory inspection endpoints (registered only when DEBUG_ENDPOINTS=true).
//!
//! - `GET /debug/memory`: live heap counters (JSON). dhat when built with `dhat-heap`
//!   (Windows + Linux), otherwise jemalloc (Linux).
//! - `POST /debug/dhat/snapshot`: write a dhat report now and keep running (`dhat-heap`).
//! - `GET /debug/pprof/heap`: heap profile in pprof format (Linux, `heap-profiling`).
//! - `GET /debug/pprof/heap/flamegraph`: heap profile as an SVG flamegraph (Linux, `heap-profiling`).
//!
//! The heap profile shows *live* (not yet freed) bytes grouped by the call stack that
//! allocated them. Take two dumps some minutes apart under load and diff them
//! (`go tool pprof -diff_base old.pb.gz new.pb.gz`): stacks that keep growing are leaks.

use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

/// Allocator counters, read live from jemalloc.
pub async fn memory_stats() -> Response {
    Json(read_memory_stats()).into_response()
}

/// dhat owns the global allocator in `dhat-heap` builds (on every platform).
#[cfg(feature = "dhat-heap")]
fn read_memory_stats() -> Value {
    match crate::service::heap_profiler::stats() {
        Some(stats) => json!({
            "allocator": "dhat (heap profiling)",
            "window": "counters cover the current profiling window (since start or last snapshot)",
            // live objects right now
            "curr_bytes": stats.curr_bytes,
            "curr_blocks": stats.curr_blocks,
            // peak live bytes in this window
            "max_bytes": stats.max_bytes,
            "max_blocks": stats.max_blocks,
            // everything allocated in this window, freed or not
            "total_bytes": stats.total_bytes,
            "total_blocks": stats.total_blocks,
        }),
        None => json!({ "allocator": "dhat", "error": "profiler is not running" }),
    }
}

#[cfg(all(not(feature = "dhat-heap"), not(target_env = "msvc")))]
fn read_memory_stats() -> Value {
    use tikv_jemalloc_ctl::{epoch, stats};

    // jemalloc caches its statistics; advancing the epoch refreshes them.
    if let Err(error) = epoch::advance() {
        return json!({ "error": format!("jemalloc epoch advance failed: {error}") });
    }

    let read = |result: Result<usize, tikv_jemalloc_ctl::Error>| result.ok();
    json!({
        "allocator": "jemalloc",
        // bytes currently allocated by the application (live objects)
        "allocated_bytes": read(stats::allocated::read()),
        // bytes in active pages (allocated + internal fragmentation)
        "active_bytes": read(stats::active::read()),
        // bytes physically resident in RAM held by the allocator
        "resident_bytes": read(stats::resident::read()),
        // bytes mapped from the OS in active extents
        "mapped_bytes": read(stats::mapped::read()),
        // bytes freed but kept mapped by jemalloc for reuse (not counted in RSS once purged)
        "retained_bytes": read(stats::retained::read()),
        // jemalloc's own bookkeeping
        "metadata_bytes": read(stats::metadata::read()),
        "process_rss_bytes": process_rss_bytes(),
        "heap_profiling": heap_profiling_status(),
    })
}

#[cfg(all(not(feature = "dhat-heap"), target_env = "msvc"))]
fn read_memory_stats() -> Value {
    json!({
        "allocator": "system (Windows heap)",
        "note": "no live heap counters for the Windows system allocator: run with --features dhat-heap",
    })
}

/// Write the current dhat window to disk without stopping the server.
pub async fn dhat_snapshot() -> Response {
    dhat_snapshot_inner().await
}

#[cfg(feature = "dhat-heap")]
async fn dhat_snapshot_inner() -> Response {
    // Serializing the profile can take a while: keep it off the async workers.
    match tokio::task::spawn_blocking(crate::service::heap_profiler::snapshot).await {
        Ok(Some(snapshot)) => Json(json!({
            "file": snapshot.file,
            "viewer": "https://nnethercote.github.io/dh_view/dh_view.html",
            "curr_bytes": snapshot.stats.curr_bytes,
            "curr_blocks": snapshot.stats.curr_blocks,
            "max_bytes": snapshot.stats.max_bytes,
            "total_bytes": snapshot.stats.total_bytes,
        }))
        .into_response(),
        Ok(None) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "dhat profiler is not running",
        )
            .into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

#[cfg(not(feature = "dhat-heap"))]
async fn dhat_snapshot_inner() -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        "dhat snapshots need a build with --features dhat-heap",
    )
        .into_response()
}

/// Resident set size of the whole process, from /proc (Linux only).
#[cfg(all(not(feature = "dhat-heap"), not(target_env = "msvc")))]
fn process_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

#[cfg(all(
    feature = "heap-profiling",
    not(feature = "dhat-heap"),
    not(target_env = "msvc")
))]
fn heap_profiling_status() -> &'static str {
    match jemalloc_pprof::PROF_CTL.as_ref() {
        Some(_) => "enabled",
        None => "compiled in, but not enabled at startup (set _RJEM_MALLOC_CONF=prof:true,...)",
    }
}

#[cfg(all(
    not(feature = "heap-profiling"),
    not(feature = "dhat-heap"),
    not(target_env = "msvc")
))]
fn heap_profiling_status() -> &'static str {
    "not compiled in (build with --features heap-profiling)"
}

/// Heap profile (live allocations by call stack) in gzipped pprof format.
pub async fn heap_pprof() -> Response {
    match dump_heap(HeapFormat::Pprof).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "application/octet-stream"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"heap.pb.gz\"",
                ),
            ],
            bytes,
        )
            .into_response(),
        Err((status, message)) => (status, message).into_response(),
    }
}

/// Heap profile rendered as an SVG flamegraph (open it in a browser).
pub async fn heap_flamegraph() -> Response {
    match dump_heap(HeapFormat::Flamegraph).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "image/svg+xml")], bytes).into_response(),
        Err((status, message)) => (status, message).into_response(),
    }
}

#[cfg_attr(
    not(all(feature = "heap-profiling", not(target_env = "msvc"))),
    allow(dead_code)
)]
enum HeapFormat {
    Pprof,
    Flamegraph,
}

#[cfg(all(feature = "heap-profiling", not(target_env = "msvc")))]
async fn dump_heap(format: HeapFormat) -> Result<Vec<u8>, (StatusCode, String)> {
    let Some(prof_ctl) = jemalloc_pprof::PROF_CTL.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "heap profiling is not enabled: start the server with \
             _RJEM_MALLOC_CONF=prof:true,prof_active:true,lg_prof_sample:19"
                .to_string(),
        ));
    };
    let prof_ctl = prof_ctl.clone();

    // Dumping + symbolizing walks the whole profile: keep it off the async workers.
    tokio::task::spawn_blocking(move || {
        let mut prof_ctl = prof_ctl.blocking_lock();
        if !prof_ctl.activated() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "heap profiling is not active (prof_active:false)".to_string(),
            ));
        }
        let result = match format {
            HeapFormat::Pprof => prof_ctl.dump_pprof(),
            HeapFormat::Flamegraph => prof_ctl.dump_flamegraph(),
        };
        result.map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
}

#[cfg(not(all(feature = "heap-profiling", not(target_env = "msvc"))))]
async fn dump_heap(_format: HeapFormat) -> Result<Vec<u8>, (StatusCode, String)> {
    Err((
        StatusCode::NOT_IMPLEMENTED,
        "heap profiling needs a Linux build with --features heap-profiling \
         (on Windows use the dhat-heap feature)"
            .to_string(),
    ))
}

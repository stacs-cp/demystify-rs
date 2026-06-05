//! A persistent, multi-process cache for parsed puzzles.
//!
//! Running Conjure + Savile Row to compile a `.eprime`/`.essence` model down
//! to DIMACS, then correlating the literals back to named variables, is the
//! slow part of loading a puzzle (seconds to tens of seconds on larger
//! instances). When generating *many* instances of the *same* model — as
//! `mystify` does — that work is identical every time, so we cache the parsed
//! result keyed by everything that could change it.
//!
//! ## Key
//!
//! The cache key is a SHA-256 over, length-delimited:
//!   - the model file bytes (`.eprime`/`.essence`),
//!   - the param file bytes,
//!   - the model file extension (it selects the `is_essence` code path),
//!   - the Savile Row version line,
//!   - the Conjure version,
//!   - a build-time hash of demystify's own source (`DEMYSTIFY_SRC_HASH`),
//!     which also covers the exact Savile Row/Conjure flags we pass, since
//!     those live in [`crate::problem::parse`].
//!
//! Any change to any of these yields a different key, so a hit is always a
//! faithful reproduction of a fresh parse. We never fall back silently: a
//! stored value that fails to decompress or deserialize is a hard error
//! (it signals genuine corruption), not a cue to recompute.
//!
//! ## Value
//!
//! The value is the parsed puzzle as compact JSON, zstd-compressed (the JSON
//! is dominated by the CNF and compresses ~12–30×).
//!
//! ## Storage and location
//!
//! Backed by [`cute_sqlite_kv::BlobStore`] (SQLite, WAL, multi-process safe),
//! so several demystify processes can share one cache without coordination.
//! By default the database lives under the OS temp directory
//! (`std::env::temp_dir()/demystify-parse-cache/parse.sqlite`) — fine to lose
//! on reboot. Override the directory with `DEMYSTIFY_PARSE_CACHE=<dir>`, or
//! disable caching entirely with `DEMYSTIFY_PARSE_CACHE=off`.

use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use cute_sqlite_kv::BlobStore;
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::problem::parse::PuzzleParse;
use crate::problem::util::exec::ProgramRunner;

/// Bumped if the *cache layout itself* (key construction or value encoding)
/// changes in a way not captured by `DEMYSTIFY_SRC_HASH`. In practice
/// `DEMYSTIFY_SRC_HASH` already covers source changes, but a leading version
/// tag makes the scheme explicit and lets us hand-invalidate if needed.
const CACHE_VERSION: &str = "parse-v1";

/// zstd compression level. The parse JSON is written once per unique puzzle
/// and read many times; level 6 is a good balance (fast decompress, ~20×
/// ratio) without the long compress times of level 19.
const ZSTD_LEVEL: i32 = 6;

/// Resolve the cache database file, or `None` if caching is disabled.
///
/// `DEMYSTIFY_PARSE_CACHE` unset → default temp location.
/// `DEMYSTIFY_PARSE_CACHE=off` (or `0`/`false`/empty) → disabled.
/// `DEMYSTIFY_PARSE_CACHE=<dir>` → `<dir>/parse.sqlite`.
fn cache_db_path() -> Option<PathBuf> {
    let dir = match std::env::var("DEMYSTIFY_PARSE_CACHE") {
        Ok(v) => {
            let t = v.trim();
            if t.is_empty() || t.eq_ignore_ascii_case("off") || t == "0" || t == "false" {
                return None;
            }
            PathBuf::from(t)
        }
        Err(_) => std::env::temp_dir().join("demystify-parse-cache"),
    };
    Some(dir.join("parse.sqlite"))
}

/// External-tool versions, probed once per process.
fn tool_versions() -> &'static (String, String) {
    static VERSIONS: OnceLock<(String, String)> = OnceLock::new();
    VERSIONS.get_or_init(|| {
        let sr = ProgramRunner::get_savilerow_version()
            .expect("parse cache: failed to read Savile Row version");
        let conjure = ProgramRunner::get_conjure_version()
            .expect("parse cache: failed to read Conjure version");
        (sr, conjure)
    })
}

/// Compute the cache key for a model/param pair.
pub fn cache_key(model_bytes: &[u8], param_bytes: &[u8], model_ext: &str) -> String {
    let (sr_ver, conjure_ver) = tool_versions();
    let src_hash = env!("DEMYSTIFY_SRC_HASH");

    let mut h = Sha256::new();
    let mut field = |label: &str, bytes: &[u8]| {
        h.update(label.as_bytes());
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    };
    field("src", src_hash.as_bytes());
    field("savilerow", sr_ver.as_bytes());
    field("conjure", conjure_ver.as_bytes());
    field("ext", model_ext.as_bytes());
    field("model", model_bytes);
    field("param", param_bytes);

    format!("{CACHE_VERSION}-{:x}", h.finalize())
}

/// Open the cache store, creating its directory if necessary.
fn open_store() -> Result<Option<BlobStore>> {
    let Some(path) = cache_db_path() else {
        return Ok(None);
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parse cache directory {parent:?}"))?;
    }
    let store = BlobStore::new_from_file(&path)
        .with_context(|| format!("opening parse cache at {path:?}"))?;
    Ok(Some(store))
}

/// Look up a parsed puzzle in the cache.
///
/// Returns `Ok(None)` on a miss or when caching is disabled. A stored value
/// that fails to decompress/deserialize is a hard error (corruption), never a
/// silent recompute.
pub fn try_load(key: &str) -> Result<Option<PuzzleParse>> {
    let Some(store) = open_store()? else {
        return Ok(None);
    };
    let Some(compressed) = store.get(key) else {
        return Ok(None);
    };
    let json = zstd::decode_all(compressed.as_slice())
        .context("parse cache: decompressing cached parse (cache may be corrupt)")?;
    let puzzle = PuzzleParse::from_json_bytes(&json)
        .context("parse cache: deserializing cached parse (cache may be corrupt)")?;
    Ok(Some(puzzle))
}

/// Store a freshly parsed puzzle under `key`.
///
/// A racing writer that stored the same key first is harmless: the value is a
/// deterministic function of the key, so the overwrite is idempotent.
pub fn store(key: &str, puzzle: &PuzzleParse) -> Result<()> {
    let Some(store) = open_store()? else {
        return Ok(());
    };
    let json = puzzle.to_json_bytes()?;
    let compressed =
        zstd::encode_all(json.as_slice(), ZSTD_LEVEL).context("parse cache: compressing parse")?;
    store.insert(key, &compressed);
    info!(target: "progress", "stored parse in cache ({} bytes compressed)", compressed.len());
    Ok(())
}

/// Whether caching is currently enabled (for logging/diagnostics).
pub fn is_enabled() -> bool {
    cache_db_path().is_some()
}

/// Emit a one-line note about cache configuration the first time it matters.
pub fn log_status() {
    match cache_db_path() {
        Some(p) => info!(target: "progress", "parse cache enabled at {p:?}"),
        None => warn!(target: "progress", "parse cache disabled (DEMYSTIFY_PARSE_CACHE=off)"),
    }
}

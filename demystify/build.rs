//! Bakes a hash of demystify's own source (plus its resolved dependency
//! versions) into the binary as `DEMYSTIFY_SRC_HASH`.
//!
//! The parse cache (see `problem::parse_cache`) keys cached parses partly on
//! this hash, so that any change to demystify's parsing code — or to a
//! dependency version that could change the generated CNF — invalidates the
//! cache rather than serving a stale parse. We deliberately hash the *whole*
//! crate source: over-invalidating (recomputing a parse that would not have
//! changed) is cheap and safe, whereas under-invalidating would silently
//! serve a wrong result.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn hash_file(hasher: &mut Sha256, label: &str, path: &Path) {
    let bytes = fs::read(path).unwrap_or_default();
    // Length-delimit so distinct (label, content) pairs can't alias.
    hasher.update(label.as_bytes());
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(&bytes);
}

fn main() {
    let mut hasher = Sha256::new();

    // All source files, in a stable order.
    let mut files = Vec::new();
    collect_files(Path::new("src"), &mut files);
    files.sort();
    for path in &files {
        hash_file(&mut hasher, &path.to_string_lossy(), path);
    }

    // This crate's manifest, and the workspace lockfile (captures dependency
    // version changes from `cargo update` that touch nothing else).
    hash_file(&mut hasher, "Cargo.toml", Path::new("Cargo.toml"));
    hash_file(&mut hasher, "Cargo.lock", Path::new("../Cargo.lock"));

    let digest = hasher.finalize();
    println!("cargo:rustc-env=DEMYSTIFY_SRC_HASH={digest:x}");

    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=../Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");
}

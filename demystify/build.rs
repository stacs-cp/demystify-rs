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
    let entries =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("build.rs: cannot read dir {dir:?}: {e}"));
    for entry in entries {
        let path = entry.expect("build.rs: dir entry").path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Hash one file's contents under `label`. `required` files must exist (a
/// missing one is a build bug); optional files (the workspace lockfile, absent
/// from a published crate package) hash as empty.
fn hash_file(hasher: &mut Sha256, label: &str, path: &Path, required: bool) {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(_) if !required => Vec::new(),
        Err(e) => panic!("build.rs: cannot read {path:?}: {e}"),
    };
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
        hash_file(&mut hasher, &path.to_string_lossy(), path, true);
    }

    // This crate's manifest, and the workspace lockfile (captures dependency
    // version changes from `cargo update` that touch nothing else). The
    // lockfile is absent from a published crate package, so treat it as
    // optional — the hash is still deterministic per published version.
    hash_file(&mut hasher, "Cargo.toml", Path::new("Cargo.toml"), true);
    hash_file(&mut hasher, "Cargo.lock", Path::new("../Cargo.lock"), false);

    let digest = hasher.finalize();
    println!("cargo:rustc-env=DEMYSTIFY_SRC_HASH={digest:x}");

    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=../Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");
}

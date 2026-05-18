#!/usr/bin/env bash
# Test runner for the new Builder bindings on Lua and WASM.
#
# Run from the demystify-rs workspace root.  Writes a log to
# build_test_log.txt so a follow-up Claude can read it.
#
# Lua side: requires luajit on PATH.
# WASM side: requires wasm-pack + the wasm32 target; skipped if missing.

set -uo pipefail

LOG=build_test_log.txt
: > "$LOG"

log() {
    echo "$@" | tee -a "$LOG"
}

run() {
    local label="$1"; shift
    log
    log "=== $label ==="
    log "\$ $*"
    "$@" >>"$LOG" 2>&1
    local status=$?
    if [[ $status -eq 0 ]]; then
        log "[ok] $label"
    else
        log "[FAIL: status=$status] $label"
    fi
    return $status
}

cd "$(dirname "${BASH_SOURCE[0]}")"

# 1. cargo fmt / clippy / build for builder crate + downstream crates
overall=0
run "cargo fmt --check"           cargo fmt --check                              || overall=1
run "cargo clippy demystify-builder" cargo clippy -p demystify-builder -- -D warnings || overall=1
run "cargo clippy demystify-lua"     cargo clippy -p demystify-lua -- -D warnings    || overall=1
run "cargo clippy demystify-wasm"    cargo clippy -p demystify-wasm -- -D warnings   || overall=1

# 2. unit tests for the builder crate
run "cargo test demystify-builder"  cargo test -p demystify-builder                || overall=1

# 3. Lua end-to-end (requires luajit)
if command -v luajit >/dev/null 2>&1; then
    run "lua builder bindings" bash demystify-lua/test/run_tests.sh || overall=1
else
    log
    log "=== lua builder bindings ==="
    log "[skip] luajit not on PATH"
fi

# 4. WASM end-to-end (requires wasm-pack + wasm32 target)
if command -v wasm-pack >/dev/null 2>&1; then
    if rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown; then
        run "wasm-pack test demystify-wasm" wasm-pack test --node demystify-wasm || overall=1
    else
        log
        log "=== wasm-pack test demystify-wasm ==="
        log "[skip] wasm32-unknown-unknown target not installed (rustup target add wasm32-unknown-unknown)"
    fi
else
    log
    log "=== wasm-pack test demystify-wasm ==="
    log "[skip] wasm-pack not on PATH"
fi

log
if [[ $overall -eq 0 ]]; then
    log "ALL CHECKED STEPS PASSED"
else
    log "ONE OR MORE STEPS FAILED — see $LOG"
fi
exit $overall

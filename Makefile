.PHONY: test wasm wasm-test wasm-mt wasm-mt-test

# Toolchain for the multi-threaded wasm build.  `-Z build-std` is unstable by
# construction, so if a nightly bump breaks the build, override this with a
# known-good dated nightly rather than working around the breakage:
#   make wasm-mt WASM_MT_TOOLCHAIN=nightly-2026-03-18
WASM_MT_TOOLCHAIN ?= nightly

# Flags required to emit a shared, imported memory plus the TLS exports the
# worker bootstrap needs.  `+atomics,+bulk-memory` alone is NOT enough on a
# modern nightly: the build still succeeds but produces a private, unshared
# memory that no worker can attach to.  See wasm-bindgen-rayon PR #34.
WASM_MT_RUSTFLAGS = -C target-feature=+atomics,+bulk-memory \
  -C link-arg=--shared-memory \
  -C link-arg=--max-memory=1073741824 \
  -C link-arg=--import-memory \
  -C link-arg=--export=__wasm_init_tls \
  -C link-arg=--export=__tls_size \
  -C link-arg=--export=__tls_align \
  -C link-arg=--export=__tls_base \
  -C link-arg=--export=__heap_base

# `build-std` is passed through the *environment* rather than as `-Z` on the
# command line, because `wasm-pack test` runs a preliminary
# `cargo build --tests` that forwards none of our arguments.  RUSTFLAGS is
# inherited by that step but `-Z build-std` would not be, so it would link the
# prebuilt sysroot std -- which has no atomics -- and rust-lld then rejects
# `--shared-memory`.  The env var reaches every nested cargo invocation.
WASM_MT_ENV = RUSTFLAGS="$(WASM_MT_RUSTFLAGS)" \
  CARGO_UNSTABLE_BUILD_STD=panic_abort,std

test:
	cargo test --workspace
	bash demystify-lua/test/run_tests.sh

# Single-threaded, portable artifact -> demystify-wasm/pkg.
# Runs anywhere; no COOP/COEP requirement.
wasm:
	wasm-pack build --target web --release demystify-wasm

wasm-test:
	wasm-pack test --node demystify-wasm

# Multi-threaded artifact -> demystify-wasm/pkg-mt.
#
# Requires: rustup toolchain install $(WASM_MT_TOOLCHAIN)
#           rustup component add rust-src --toolchain $(WASM_MT_TOOLCHAIN)
#           rustup target add wasm32-unknown-unknown --toolchain $(WASM_MT_TOOLCHAIN)
#
# The output can only be instantiated in a cross-origin-isolated page
# (COOP: same-origin + COEP: require-corp), because it imports a shared
# memory backed by a SharedArrayBuffer.  Ship it alongside `pkg` and pick
# between them with feature detection -- one binary cannot do both.
wasm-mt:
	$(WASM_MT_ENV) \
	  rustup run $(WASM_MT_TOOLCHAIN) \
	  wasm-pack build --target web --release demystify-wasm --out-dir pkg-mt \
	    -- --features parallel

# Threads need real Web Workers, so this cannot run under --node.  The
# wasm-bindgen test server already sends COOP/COEP, so SharedArrayBuffer is
# available without extra setup.
#
# Only the three `threads_mt*` targets are selected.  The rest of the suite
# constructs a WasmPlanner without initialising a pool, which the parallel
# build correctly rejects -- those tests belong to `wasm-test` (the
# single-threaded build), which is where they still run.
wasm-mt-test:
	$(WASM_MT_ENV) \
	  rustup run $(WASM_MT_TOOLCHAIN) \
	  wasm-pack test --headless --firefox demystify-wasm \
	    --features parallel \
	    --test threads_mt --test threads_mt_guard --test threads_mt_mainthread

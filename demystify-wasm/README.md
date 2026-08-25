# demystify-wasm

WebAssembly bindings for [`demystify`](../demystify), a constraint-solving tool
that produces step-by-step explanations of puzzle deductions. Mirrors the
[`demystify-lua`](../demystify-lua) API.

Two entry points:

1. **Load a pre-parsed puzzle** (`load_puzzle` + `WasmPlanner`) — when Conjure
   has already compiled a `.eprime` / `.param` pair to JSON on the native side.
2. **Build a puzzle in JS** ([`WasmBuilder`](#building-puzzles-in-js)) — no
   Conjure needed; for embedders that construct puzzles at runtime (minesweeper,
   star battle, sudoku, …).

The wasm build uses [`rustsat-batsat`](https://crates.io/crates/rustsat-batsat)
(pure-Rust SAT) instead of Glucose/CaDiCaL — they're C/C++ FFI and don't reach
the browser. Solving is slower than native, but works.

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
make wasm          # or: wasm-pack build --target web demystify-wasm
```

`pkg/` then contains the `.wasm`, JS shim, and TypeScript declarations.

## Multi-threaded build (optional)

MUS search is the expensive part of solving and is parallelised with `rayon`.
On wasm that parallelism is **silently lost by default**: `rayon` detects that
`std::thread::spawn` is unsupported and installs a single-threaded fallback
pool, so `par_iter` runs serially with no warning. Hard puzzles that take a few
seconds natively can take 30s or more in the browser.

The `parallel` feature restores real threads via
[`wasm-bindgen-rayon`](https://github.com/RReverser/wasm-bindgen-rayon) (Web
Workers over a `SharedArrayBuffer`):

```sh
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
rustup target add wasm32-unknown-unknown --toolchain nightly
make wasm-mt       # -> demystify-wasm/pkg-mt
make wasm-mt-test  # headless browser; cannot run under --node
```

Use the Makefile targets rather than calling `wasm-pack` directly — the build
needs a nightly toolchain, `-Z build-std`, and an exact set of `RUSTFLAGS`
(shared/imported memory plus `__tls_*` and `__heap_base` exports). Getting
those wrong does **not** fail the build; it produces a module with a private,
unshared memory that no worker can attach to.

### Choosing a build (for consumers)

**Nothing changes if you do nothing.** `pkg/` is the same single-threaded build
as before — same 14 exports, unshared memory, no COOP/COEP requirement, callable
from anywhere including the main thread. Existing code keeps working untouched.

**The threaded build must run inside a Web Worker.** This is not a preference.
The parallel MUS search blocks on a `std::sync::Mutex`, and the browser main
thread is a "cannot-block" agent, so a solve started there hangs forever — no
exception, nothing in the console. `WasmPlanner::new` detects this and returns
an error rather than hanging, but the constraint is real: **cross-origin
isolation alone is not enough**, you also need to be off the main thread.

Consequently threads are **opt-in, never feature-detected**. Auto-enabling would
hang every isolated page that solves synchronously from the UI.

`pkg-mt/` is otherwise a strict **superset** of `pkg/`: identical API plus
`initThreadPool`, about 5% larger (1.09 MB vs 1.04 MB). No changes to your
solving code — only to where it runs and how it is initialised.

```js
// inside worker.js
import { loadDemystify } from './loader.js';

const { pkg, threads } = await loadDemystify({ threads: true, numThreads: 4 });
const puzzle = pkg.load_puzzle(json);
const planner = new pkg.WasmPlanner(puzzle);
// ... postMessage results back to the page
```

`loadDemystify()` with no arguments gives you the single-threaded build, which
is the right default for main-thread callers. Pass `{ threads: true }` to demand
threads — it throws (naming the reason: main thread, or missing isolation)
rather than silently falling back and running N times slower. `numThreads`
defaults to `min(hardwareConcurrency, 8)`; each worker holds its own SAT solver
instance, so more is not automatically better.

`threadsAvailable()` reports whether this context could use threads at all —
isolated, `SharedArrayBuffer` present, and not the main thread.

**Bundlers:** both packages are built with wasm-pack's `--target web`, and the
loader's dynamic `import()` uses a computed specifier that webpack and Vite
cannot follow statically. If you bundle, import the package you want directly.

### Browser requirements

Threads need three things, and the binding constraint is nested dedicated Workers
(our Worker spawns the rayon pool's Workers). Versions from MDN's
browser-compat-data, checked 2026-08-25:

| | Safari / iOS | Chrome | Firefox |
|---|---|---|---|
| Nested dedicated Workers | **16.4** | 69 | 34 |
| `SharedArrayBuffer` | 15.2 | 68 | 79 |
| `navigator.hardwareConcurrency` | 15.4 | 37 | 48 |

So the floor is **Safari/iOS 16.4** (March 2023). MDN's "partial implementation"
note against nested Workers refers to *Shared* Workers; dedicated Workers, which
is what we use, are fine. iOS is not excluded by this.

Safari clamps `hardwareConcurrency` to 4 or 8 to resist fingerprinting, so it
self-limits to roughly the cap the loader applies anyway.

Anything below those versions should simply not be offered the threaded build —
which is why the loader defaults to off rather than feature-detecting.

### Serving: keep isolation scoped to opt-in users

The headers, not the wasm, are the change with real blast radius. COOP/COEP apply
to the **page**, so they affect every visitor — including those who never enable
threads — and `require-corp` makes every cross-origin subresource fail unless it
opts in via CORP or CORS.

Prefer to send the isolation headers **only** when threads are actually being
requested: a separate route, or headers conditional on whatever flag your app uses
to opt in. Then non-participating users are unaffected and there is nothing to
regress. `COEP: credentialless` is a lighter-weight alternative if `require-corp`
proves awkward.

Treat the threaded build as a beta you switch on deliberately, not as a default
that degrades. The single-threaded build remains the supported path for everyone
else, and is unchanged.

### Two artifacts, not one adaptive binary

A threaded module *imports* a shared memory, so instantiating it requires
`SharedArrayBuffer`, which requires the page to be cross-origin isolated. There
is no way to build one binary that degrades to single-threaded when the headers
are absent — instantiation simply fails. So ship both and choose at load time
(see [Choosing a build](#choosing-a-build-for-consumers) — the choice must be an
explicit opt-in from inside a Worker, not a feature-detect).

Serving `pkg-mt` requires two response headers on the *page*:

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

Note that `require-corp` also forces every cross-origin subresource on that page
to opt in via CORP/CORS, or it will fail to load. `credentialless` is a lighter
alternative. This is why `pkg` remains the default artifact.

### Initialisation order matters

**`initThreadPool` must be awaited before constructing a `WasmPlanner`.**
`rayon` installs its fallback pool on the first call to *any* of its APIs, and
`WasmPlanner::new` reaches one (via `mark_trivial_lits_as_deduced` →
`get_provable_varlits` → `par_iter`). Once the fallback is in place the real
pool can never be installed and `initThreadPool` fails with
`GlobalPoolAlreadyInitialized`.

In the `parallel` build `WasmPlanner::new` therefore checks the pool up front
and returns a descriptive error rather than silently running serially — if you
have paid the COOP/COEP cost, quietly taking 30s instead of 4s is not a helpful
fallback. Recovering requires reloading the page. The default (single-threaded)
build has no such check and no such requirement.

## Running the test suite

```sh
make wasm-test      # single-threaded suite, under node
make wasm-mt-test   # threaded suite, headless Firefox
```

`wasm-test` drives the planner + builder through node's V8. Tests live in
`demystify-wasm/tests/`.

`wasm-mt-test` selects only the three `threads_mt*` targets, because the rest of
the suite constructs a `WasmPlanner` without a worker pool — which the parallel
build correctly rejects. Those tests belong to the single-threaded run. It also
needs a browser: node has no Web `Worker`, so threads cannot be tested there.

The threaded tests use `run_in_dedicated_worker`, not `run_in_browser` —
`threads_mt` solves a puzzle, and a threaded solve on the main thread would hang
rather than fail. The exception is `threads_mt_mainthread`, which deliberately
runs on the main thread to assert that construction is *refused* there; it never
gets far enough to block.

## Loading a pre-parsed puzzle

```js
import init, { load_puzzle, WasmPlanner } from './pkg/demystify_wasm.js';

await init();

const json = await fetch('sudoku.json').then(r => r.text());
const puzzle = load_puzzle(json);
const planner = new WasmPlanner(puzzle);

while (!planner.isSolved()) {
    const step = planner.bestStep();
    if (!step) break;
    console.log(step.literals, 'because', step.constraints);
}
```

Produce the JSON on the native side:

```sh
cargo run --release --bin demystify -- \
  --model eprime/sudoku.eprime \
  --param eprime/sudoku/redditexample.param \
  --save-parsed sudoku.json
```

The wasm side never invokes Conjure or touches the filesystem — only the JSON.

## Building puzzles in JS

`WasmBuilder` exposes a small constraint vocabulary that's enough to build star
battle, sudoku, minesweeper, and similar puzzles directly in JS — no `.eprime`
file, no Conjure. See [`demystify-builder`](../demystify-builder) for the
underlying Rust crate and the constraint vocabulary in detail.

```js
import init, { WasmBuilder, WasmPlanner } from './pkg/demystify_wasm.js';
await init();

const b = new WasmBuilder();
b.kind("toy");

// Declare a 2×2 grid of booleans.  Dims are arrays of [lo, hi] pairs,
// 1-indexed inclusive.
const g = b.varBoolMatrix("g", [[1, 2], [1, 2]]);

// Declare a constraint-family activation atom.  Each $#CON guard atom
// belongs to exactly one family.
const rule = b.conBool("rule");

// Attach family + description to the atom to form a single-use guard.
const guard = b.guard(rule, "rule", "at least two of g are true");

// "rule -> sum of (g[1,1] + g[1,2] + g[2,1] + g[2,2]) >= 2".  Each
// literal in the sum is built via .pos() / .neg(); the array is consumed.
b.sumGe(
  guard,
  [g.get([1, 1]).pos(), g.get([1, 2]).pos(),
   g.get([2, 1]).pos(), g.get([2, 2]).pos()],
  2);

const puzzle = b.build();      // consumes the builder
const planner = new WasmPlanner(puzzle);
planner.quickSolve();
console.log(planner.currentState());
```

### `WasmBuilder` API

Constructor: `new WasmBuilder()`.

| Method | Purpose |
|---|---|
| `kind(s)` | Set the `$#KIND` tag (puzzle type label) |
| `info(s)` | Push a `$#INFO` string |
| `varBoolMatrix(name, dims)` | Declare a `$#VAR` matrix of bool atoms (user-deducible) → `WasmBoolMatrix` |
| `conBoolMatrix(name, dims)` | Declare a `$#CON` family of guard atoms → `WasmBoolMatrix` |
| `conBool(name)` | 0-d `$#CON` guard atom → `WasmAtom` |
| `auxBoolMatrix(name, dims)` | Declare a `$#AUX` matrix (helper deducible atoms) → `WasmBoolMatrix` |
| `auxBool(name)` | 0-d `$#AUX` atom → `WasmAtom` |
| `revealBoolMatrix(name, dims)` | Declare a `$#REVEAL`-target matrix (see below) |
| `reveal(srcName, targetName)` | Wire up a reveal cascade |
| `show(varName, role)` | `$#SHOW` directive; role is `"main"`, `"givens"`, `"cages"`, `"region_tint"`, `"cage_sums"`, `"less_than"`, `"side_labels"` |
| `andAtom(signed)` | Fresh atom equal to `AND(inputs)`; useful for multi-gate constraints |
| `guard(atom, family, description)` | Pair an atom with a `$#CON` family + description; returns a single-use `WasmGuard` |
| `sumGe(guard, signed, k)` | Post `guard.atom → sum(signed) ≥ k`; consumes `guard` |
| `sumLe(guard, signed, k)` | Post `guard.atom → sum(signed) ≤ k` |
| `sumEq(guard, signed, k)` | Post `guard.atom → sum(signed) = k` |
| `sumEqUnguarded(signed, k)` | Post `sum(signed) = k` unconditionally (no `$#CON` entry — for one-hot encodings, givens, exclusions) |
| `build()` | Finalise → `WasmPuzzle`. Throws on a second call. |

`WasmBoolMatrix` methods: `name()`, `get([i, j, …])` → `WasmAtom`,
`atoms()` → `WasmAtom[]` (row-major).

`WasmAtom` methods: `pos()` and `neg()` return a `WasmSigned` for use in the
arrays passed to `sumGe` / `sumLe` / `sumEq` / `sumEqUnguarded`. Each `WasmAtom`
takes `&self` in pos/neg, so a single atom can produce as many `WasmSigned`
handles as you need.

**Important marshalling note**: `WasmSigned` handles and `WasmGuard` are
*consumed* when the sum_* method is called on the wasm side. Build them
inline rather than holding the JS handles across calls.

```js
// OK — fresh handles each call.
b.sumGe(b.guard(rule, "rule", "..."), [a.pos(), b.neg()], 1);

// NOT OK — `g` is invalidated after the first sumGe.
const g = b.guard(rule, "rule", "...");
b.sumGe(g, [a.pos()], 1);
b.sumGe(g, [b.pos()], 1);   // error: handle consumed
```

### Multi-gate constraints with `andAtom`

To express `g1 ∧ g2 ∧ ... → constraint`, fold the gates into a fresh atom with
`andAtom` and pass that atom to `guard`:

```js
const gate = b.andAtom([
  sumcheck.get([i, j]).pos(),
  facts.get([i, j, 0]).pos(),
]);
const g = b.guard(gate, "sumcheck", "...");
b.sumEq(g, neighbours, nMines);
```

`andAtom` creates a fresh anonymous SAT atom `c` such that `c ↔ AND(inputs)`.
The atom is not classified as `$#VAR` / `$#AUX` / `$#CON` / `$#REVEAL` — it's
purely a SAT-level helper. When passed to `guard()`, it gets registered into
the named `$#CON` family.

### Build-time validation

`build()` rejects:

- A `$#VAR` matrix never referenced by any constraint (`UnusedVar`).
- A `$#CON` family declared but never used (`UnusedFamily`).
- A reveal target declared without a matching `reveal(...)` call
  (`UnusedRevealTarget`).
- Duplicate constraint descriptions (each `description` passed to `guard()`
  must be unique across the whole puzzle).
- A `guard()` call whose atom is already registered in a *different* `$#CON`
  family.

These surface as `JsError` exceptions on the JS side.

### `$#REVEAL` cascades

Minesweeper is the canonical use case: when `grid[i,j]=v` is deduced, you want
the planner to *also* mark `facts[i,j,v]` known, without that counting as a
separate user-visible deduction step. The cascade is two steps:

```js
const grid = b.varBoolMatrix("grid", [[1, h], [1, w]]);

// facts has one extra trailing dim covering the source's {0, 1} domain.
const facts = b.revealBoolMatrix("facts", [[1, h], [1, w], [0, 1]]);

b.reveal("grid", "facts");
```

`facts[i,j,d]` becomes a regular boolean atom — you can use it in `sum_*`
constraints exactly like a `$#VAR` or `$#AUX` atom. The "reveal" part is that
whenever the planner adds `grid[i,j]=d` to its known set, it also adds
`facts[i,j,d]=true`, so any constraint guarded by `facts[i,j,d]` activates
automatically.

The reveal-target matrix's dims must be the source's dims with one trailing
`[0, 1]` dim appended (since source vars are booleans).

See `demystify-wasm/tests/builder_minesweeper.rs` for a full 3×3 example with
the standard guard-by-`facts` encoding.

## Examples

Working end-to-end tests in `demystify-wasm/tests/`:

- `smoke.rs` — load a pre-parsed JSON and run the planner.
- `builder_smoke.rs` — minimal `WasmBuilder` usage.
- `builder_sudoku.rs` — 4×4 sudoku built and solved in wasm.
- `builder_minesweeper.rs` — 3×3 minesweeper with `$#REVEAL`.

The Rust integration tests in `demystify-builder/tests/` cover the same shapes
without the wasm layer; useful when chasing a builder-side issue.

## API mirror

Method names mirror [`demystify-lua`](../demystify-lua/src/lib.rs) but in
camelCase (`isSolved`, `bestStep`, `fixLiteral`, `numClauses`, `varBoolMatrix`,
`revealBoolMatrix`, etc.). Struct names keep the `Wasm` prefix (`WasmBuilder`,
`WasmPlanner`, `WasmPuzzle`, `WasmAtom`, `WasmSigned`, `WasmBoolMatrix`).

## Debugging from the browser

When something behaves differently in the browser than in
`wasm-pack test --node` or native Rust, two helpers are exposed:

```js
import init, { enableConsoleLogs, WasmPlanner } from "./pkg/demystify_wasm.js";
await init();

// Pipe demystify's tracing logs (info!/debug!) to the browser console.
// Call once at startup.  Levels: "trace" | "debug" | "info" (default) |
// "warn" | "error".  Idempotent.
enableConsoleLogs("debug");

const planner = new WasmPlanner(puzzle);

// Console-logs each phase; returns the same difficulties map plus
// per-phase counts so JS can assert.  The returned object is:
//   { provable, dict_entries, lits_with_sized_mus, min_mus_size, difficulties }
const dbg = planner.difficultiesDebug();
console.log(dbg);
```

`difficultiesDebug()` emits four lines per call: provable-lit count, the
size of the dict `all_muses_with_larger` returned, how many entries had
a size-≥1 MUS, and the final mapping length.  Combined with the tracing
logs from `enableConsoleLogs`, that's enough to tell whether the search
ran at all, whether the tiny scan found size-1 MUSes, and whether the
cores/main-search phases produced anything.

## License

MPL-2.0. See [`LICENSE.txt`](LICENSE.txt).

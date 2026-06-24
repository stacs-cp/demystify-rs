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
wasm-pack build --target web demystify-wasm
```

`pkg/` then contains the `.wasm`, JS shim, and TypeScript declarations.

## Running the test suite

```sh
wasm-pack test --node demystify-wasm
```

Drives the planner + builder through node's V8. Tests live in
`demystify-wasm/tests/`.

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

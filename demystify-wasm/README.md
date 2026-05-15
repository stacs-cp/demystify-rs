# demystify-wasm

WebAssembly bindings for [`demystify`](../demystify), a constraint-solving tool
that produces step-by-step explanations of puzzle deductions. Mirrors the
[`demystify-lua`](../demystify-lua) API: load a pre-parsed puzzle JSON, build a
planner, step through deductions.

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

## Quick start

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

## Producing the JSON

Use the native CLI to pre-parse a puzzle:

```sh
cargo run --release --bin demystify -- \
  --model eprime/sudoku.eprime \
  --param eprime/sudoku/redditexample.param \
  --save-parsed sudoku.json
```

The wasm side never invokes Conjure or touches the filesystem — only the JSON.

## API

Mirror of [`demystify-lua`](../demystify-lua/src/lib.rs). Methods are camelCase
in JS where the Lua method was snake_case (`isSolved`, `bestStep`,
`fixLiteral`, `numClauses`, etc.); other names match the Lua interface.

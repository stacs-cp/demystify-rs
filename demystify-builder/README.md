# demystify-builder

Build [`demystify`](https://crates.io/crates/demystify) puzzle inputs directly
in Rust, bypassing Conjure.

`demystify` normally consumes `.eprime` + `.param` files via Conjure +
Savilerow, which compile the model to DIMACS CNF. That toolchain is too large
(and not viable) for the [`demystify-lua`](https://github.com/stacs-cp/demystify-rs/tree/main/demystify-lua)
and [`demystify-wasm`](https://crates.io/crates/demystify-wasm) embeddings,
but those embeddings still sometimes want to solve puzzles.

This crate provides a small builder API for those callers. It supports a
deliberately tiny constraint vocabulary — enough for star battle,
minesweeper, sudoku, binairo, and easily extensible — and does not aim to
replace Conjure.

## Supported constraint vocabulary

- Boolean variables (declared individually or as N-dimensional matrices).
- Multi-valued (integer-domain) variables via `var_int_matrix`: a one-hot
  direct encoding that renders / explains as `grid[i,j] = 2`.
- `sum(lits) >= k`, `sum(lits) <= k`, and `sum(lits) = k`, with optional
  negated literals.
- Guarded versions: `guard -> (sum >= k)`, mirroring the eprime `$#CON`
  activation pattern.
- `table`: a forbidden-tuples (negative table) constraint over one-hot
  `IntCell` columns, registered as a single `$#CON`.
- `$#REVEAL` cascades — `reveal_bool_matrix` + `set_reveal` give
  minesweeper-style "deduce `grid[i,j]=v` ⇒ also mark `facts[i,j,v]`
  known" without a separate deduction step.

## Example

```rust
use demystify_builder::{PuzzleBuilder, ShowRole};

let mut b = PuzzleBuilder::new();
b.kind("toy");
let g = b.var_bool_matrix("g", &[1..=2, 1..=2]);
b.show("g", ShowRole::Main);
let rule = b.con_bool("rule");
let guard = b.guard(rule, "rule", "at least 2 cells are true").unwrap();
b.sum_ge(
    guard,
    &[
        g.get(&[1, 1]).pos(),
        g.get(&[1, 2]).pos(),
        g.get(&[2, 1]).pos(),
        g.get(&[2, 2]).pos(),
    ],
    2,
);
let puzzle = b.build().expect("build");
```

See the [API documentation](https://docs.rs/demystify-builder) for the full
surface, and the workspace's
[`demystify-wasm`](https://crates.io/crates/demystify-wasm) crate for
example puzzle constructions (minesweeper, star battle, sudoku, binairo).

## License

MPL-2.0. See [`LICENSE.txt`](LICENSE.txt).

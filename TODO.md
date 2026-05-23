# TODO

The remaining items here are *mystify-side* work — they need changes in
`../ms` (model annotations or model rewrites), not in this repo.  The
demystify-rs renderer is ready for them.

- **Skyscrapers** `puz_grid` (interior numeric clues) in
  `ms/examples/skyscrapers.eprime`: `[D, D]` of VALUES, with `0` =
  no clue.  No new role needed — annotate with
  `$#SHOW puz_grid givens` and add `$#DEC blank_input_val=0` so cells
  with value 0 render blank.

- **Tents** `puz_trees` in `ms/examples/tents.eprime`: `[D, D]` of
  TLAB (`0..maxTrees`).  demystify-rs has no tree-icon rendering —
  even our own `eprime/tents.eprime` falls back to integers (trees
  encoded as negative numbers in the user variable).  Two paths:
  1. **Quickest:** align mystify with our convention — drop
     `puz_trees` and encode trees inside `grid` as negative values,
     so the existing `$#SHOW grid main` covers them.  Renders as
     numbers (ugly, but functional).
  2. **Proper fix:** add a `trees` SVG glyph renderer to
     `demystify/src/web/puzsvg.rs` plus a new `ShowRole::Trees`
     variant.  Benefits both models.  Defer until someone wants to
     improve tents rendering generally.

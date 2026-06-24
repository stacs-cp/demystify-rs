# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/stacs-cp/demystify-rs/compare/demystify-v0.3.0...demystify-v0.4.0) - 2026-06-16

### Added

- `--greedy` mode: find the puzzle's largest MUS (its difficulty) as fast as
  possible, applying every deduction whose MUS is at most the largest size seen
  so far and paying the full smallest-MUS search only when forced to raise that
  maximum (`PuzzlePlanner::quick_solve_greedy`). On hard puzzles this is much
  faster and far more stable run-to-run, with the same maximum MUS size.
- `is_uniquely_solvable`: a cheap unique-solution check, generalised from a
  partial-assignment solvability check.

### Performance

- `FindVarConnections` sped up by indexing clauses and unioning lazily.

### Changed

- Replaced the crate-wide `allow(dead_code)` with scoped exemptions.
- Literal handling made tolerant and consistent across the FFI surfaces.

## [0.3.0](https://github.com/stacs-cp/demystify-rs/compare/demystify-v0.2.0...demystify-v0.3.0) - 2026-06-05

### Added

- Persistent, multi-process parse cache: Conjure/Savile Row output is cached
  (keyed by the model + param contents and the Conjure, Savile Row and
  demystify versions) so re-running the same puzzle skips compilation. On by
  default; controlled by the `DEMYSTIFY_PARSE_CACHE` environment variable
  (a directory to relocate it, or `off` to disable).
- Declared a minimum supported Rust version (`rust-version = "1.94.1"`).

### Fixed

- The annotation parser no longer panics with an index-out-of-bounds on a
  malformed `$#VAR` / `$#PUZZLE` / `$#AUX` / `$#KIND` line; it reports a clear
  error instead.
- A multi-word `$#KIND` (e.g. `Jigsaw Sudoku`) is no longer silently truncated
  to its first word.
- `demystify.trace` is now created only when `--trace` is passed, instead of
  leaving an empty file in the working directory on every run.

## [0.2.0](https://github.com/stacs-cp/demystify-rs/compare/demystify-v0.1.3...demystify-v0.2.0) - 2026-04-04

### Added

- *(planner)* add unsolved_vars_after_solve to PuzzlePlanner
- *(solver)* add find_one mode to narrow MUS search within each batch
- *(stats)* per-SAT-call time and conflict bucket tables
- *(planner)* add max_steps field to PlannerConfig
- *(satcore)* add CaDiCaL backend alongside glucose
- *(stats)* per-function MUS timing table with time buckets
- *(bench)* miracle sudoku step-sweep benchmarks
- *(stats)* add global MUS statistics collector
- *(akari)* add wall_below decoration for thick-border black cell rendering
- *(svg)* composable decorations via \$#DEC, show blocked cells in difficulty view
- *(mosaic)* add Mosaic (Minesweeper) puzzle model and web demo
- *(bench)* add Criterion benchmarking suite for solve performance
- *(web)* add puzzle overview panel with $#INFO and constraint classes
- *(svg)* add thermometer, futoshiki, and killer sudoku rendering

### Fixed

- *(stats)* use iterator enumerate to satisfy clippy needless_range_loop
- *(stats)* replace unbounded Vec accumulation with O(1) pre-aggregated buckets
- *(planner)* refresh shows current state; deduction renders post-deduction grid
- *(planner)* show grid with message instead of bare "No MUS" text
- *(svg)* add clue_in_corner decoration for Mosaic-style puzzles
- *(loopy)* rewrite to avoid conjure_aux and function types
- *(web)* always-open overview panel and fix constraint cell hover
- *(svg)* draw futoshiki arrows as SVG chevrons instead of text

### Other

- Run cargo fmt
- Fix leaving temp files around
- replace NamedTempFile/tempdir with prefixed tempdir_in(".") to avoid sandbox write restrictions
- fmt, update package dependancies
- Fix off-by-one error in test
- broad codebase cleanup and improvements
- Fix bug in cake MUS
- *(solver)* remove upfront size-0 SAT call in get_var_mus_size_1
- *(planner)* add cross-step MUS cache to PuzzlePlanner
- add comprehensive documentation with examples
- Handle empty INFO
- Add missing method
- Add constraint_roots
- Add lua library
- Add 'INFO to store information about models
- Make to_json_map more generic
- Add to_json_map, nicer method for handing PuzVar assignments
- Do some clippy
- Add method to turn PuzVar assignments into nice JSON output

## [0.1.3](https://github.com/stacs-cp/demystify-rs/compare/demystify-v0.1.2...demystify-v0.1.3) - 2025-08-10

### Other

- Improve error message
- Add a better error message

## [0.1.2](https://github.com/stacs-cp/demystify-rs/compare/demystify-v0.1.1...demystify-v0.1.2) - 2025-08-09

### Other

- Give better names to executables, so they are more useful when installed
- Fix github rep

## [0.1.1](https://github.com/stacs-cp/demystify-rs/compare/demystify-v0.1.0...demystify-v0.1.1) - 2025-08-08

### Other

- Update Rustsat version

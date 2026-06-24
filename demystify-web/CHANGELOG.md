# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.5](https://github.com/stacs-cp/demystify-rs/compare/demystify-web-v0.1.4...demystify-web-v0.1.5) - 2026-06-16

### Changed

- Track `demystify` 0.4.0.
- README prose tidy and doc fixes.

## [0.1.4](https://github.com/stacs-cp/demystify-rs/compare/demystify-web-v0.1.3...demystify-web-v0.1.4) - 2026-04-04

### Added

- *(web)* add Minesweeper example with hidden-information reveal
- *(akari)* add wall_below decoration for thick-border black cell rendering
- *(nonogram)* add Duck and Heart test puzzles, register web demo
- *(mosaic)* add Mosaic (Minesweeper) puzzle model and web demo
- *(akari)* add Akari (Light Up) puzzle model and examples
- *(web)* add puzzle overview panel with $#INFO and constraint classes
- *(puzzles)* add Kakurasu/Skyscrapers/XSums web examples, fix Kakurasu labels
- *(svg)* add thermometer, futoshiki, and killer sudoku rendering

### Fixed

- *(planner)* refresh shows current state; deduction renders post-deduction grid
- *(akari)* replace under-constrained 5x5 instance with uniquely solvable one
- *(svg)* add clue_in_corner decoration for Mosaic-style puzzles
- *(x-sums)* remove $#DESC from ctc param (causes parse error)
- *(x-sums)* remove dead commented directives, fix template syntax, add easy demo param
- *(thermometer)* correct grid variable to row-major indexing

### Other

- replace NamedTempFile/tempdir with prefixed tempdir_in(".") to avoid sandbox write restrictions
- fmt, update package dependancies
- broad codebase cleanup and improvements

## [0.1.3](https://github.com/stacs-cp/demystify-rs/compare/demystify-web-v0.1.2...demystify-web-v0.1.3) - 2025-08-10

### Other

- updated the following local packages: demystify

## [0.1.2](https://github.com/stacs-cp/demystify-rs/compare/demystify-web-v0.1.1...demystify-web-v0.1.2) - 2025-08-09

### Other

- Give better names to executables, so they are more useful when installed
- Fix github rep

## [0.1.1](https://github.com/stacs-cp/demystify-rs/compare/demystify-web-v0.1.0...demystify-web-v0.1.1) - 2025-08-08

### Other

- Update Rustsat version

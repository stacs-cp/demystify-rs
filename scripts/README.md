# Scripts

## Testing

- **test-some.sh** — Smoke-tests the CLI on a handful of puzzles. CI-gated.
- **check-all.sh** — Prints commands for every `eprime/*` model/param combination (does not execute them).

## Benchmarking

- **bench-experiment.sh** — Runs the core-guided vs standard MUS experiment. For each puzzle category, runs instances 01–20 with both `--mus-method mus` (standard) and `--mus-method core` (core-guided), capped at 50 steps. Pass `--dry-run` to preview. Set `BENCH_OUTDIR` to control output location (default `/tmp/bench-cores-experiment/`).
- **bench-table.py** — Generates a LaTeX (or `--plain` text) comparison table from `bench-experiment.sh` results. Usage: `python3 scripts/bench-table.py <results-dir>`.

## Data collection

- **fetch-binairo.py** — Scrapes binairo puzzles from puzzle-binairo.com.
- **fetch-futoshiki.py** — Scrapes futoshiki puzzles from puzzle-futoshiki.com.
- **fetch-lightup.py** — Scrapes Light Up (Akari) puzzles from puzzle-light-up.com.
- **fetch-minesweeper.py** — Scrapes pen-and-paper minesweeper puzzles from puzzle-minesweeper.com.
- **fetch-sudoku.py** — Scrapes sudoku puzzles from puzzle-sudoku.com.
- **fetch-tents.py** — Scrapes tents-and-trees puzzles from puzzle-tents.com.

## Misc

- **copy-examples-to-web.sh** — Copies example HTML outputs into `demystify-web/examples/`.

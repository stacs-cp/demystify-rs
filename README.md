# demystify

`demystify` is a Rust-based solver designed to explain constraint satisfaction problems and puzzles. This project is a rewrite of the original `demystify` solver, which was implemented in Python. The long-term goal of `demystify` is to provide users with a robust tool for solving and understanding puzzles through detailed, human-readable explanations.

## What to use

- **Explain a fixed puzzle:** the `demystify` CLI or `demystify-web` below.
- **Generate levels:** Mystify in the sibling `ms` checkout; start with its
  `PUZZLE_GENERATION_GUIDE.md`, then its model adaptation notes and manual.
- **Embed hints:** the workspace's `demystify-builder` and `demystify-wasm` crates.
- **Study solving routes:** `demystify-solvetree` and the solve-corpus story tools.

MUS size counts grouped constraints in an explanation, not necessarily visible
clues. A MUS is inclusion-minimal, not automatically minimum-cardinality among
all proofs. Compare difficulty under the same encoding, move policy and search
settings. Maximum MUS alone does not describe an entire solve or puzzle quality.
MUS-1 levels can be useful; some games do not admit useful levels at that size.

## Installation


For Essence/Essence' input, Demystify uses Conjure and Savile Row. The runner
tries native `conjure` **and** `savilerow` on `PATH`, then Podman, then Docker.
Install the native toolchain or a working container runtime yourself; the
container path can fetch its compiler image on first use. It does not install
Docker/Podman. See the [Conjure project](https://github.com/conjure-cp/conjure).
Loading parsed JSON or using the constraint builder avoids runtime compilation.

You will also need a reasonably recent version of `rust`. There are various ways to install Rust, but the easiest is probably with [rustup](https://rustup.rs/)

If you are on **windows**, you also need LLVM, you can get it by running `winget install LLVM.LLVM`.

Once `conjure` and `rust` are installed, you can proceed to set up `demystify`.

1. Clone the `demystify` repository:
   ```sh
   git clone https://github.com/stacs-cp/demystify-rs
   cd demystify-rs
   ```


## Testing your installation

If you want to test demystify is working correctly, run its tests:

```sh
cargo test -p demystify       # core tests; use --workspace for all crates
```

Note that this may take a long time the first time you run it (including warnings about 'Tests taking longer than 30 seconds'), if docker or podman is being used, as the Conjure image must be downloaded the first time it is used.

## Quick Start - Web Interface

The easiest way to get started with `demystify` is with the web interface. Just run:

```sh
cargo run --release --bin demystify-web
```

Then go to the webpage it mentions (usually `http://localhost:8008` )

## Quick Start

To quickly get started with `demystify`, you can run the following command to solve a Sudoku puzzle and generate an explanatory HTML file:

```sh
cargo run --bin demystify --release -- --model eprime/sudoku.eprime --param eprime/sudoku/redditexample.param --html --trace > sudoku.html
```

After running this command, open `sudoku.html` in your web browser to view the solution and its detailed explanation.

## Persistent solve graphs

For persistent exploration of solving routes through small puzzles, see the
[solve-graph research guide](demystify/src/problem/solvetree/README.md). The
`demystify-solvetree` command supports batches, checkpoints, further MUS searches,
and on-demand offline viewers for move difficulty and explanation availability.
The [multi-game corpus runner](scripts/solve-corpus/README.md) exports shipped
Bloomsweep levels through their app encoders and saves batches with a SQLite
index, input provenance and zstd-compressed resumable graphs.
The [offline story statistics](scripts/solve-corpus/STORY.md) measure release
chains, choke points and variation in repeated hard moves across easiest-move
routes, with exact bounds for the saved graph and saved witness routes. Completed
graphs close the discovered evidence over easiest-move routes; they do not
certify exhaustive explanations or all possible solving paths.

## Parse cache

Compiling a model with Conjure/Savile Row is the slow part of loading a puzzle (seconds to tens of seconds on large instances). `demystify` caches the parsed result so that re-running the same model + parameter file — for example when generating many instances of one puzzle — skips that step.

The cache is **on by default** and needs no flags. It is keyed by the contents of the model and parameter files together with the Conjure, Savile Row, and `demystify` versions, so a cached entry is only ever reused when it would reproduce a fresh parse exactly.

It is controlled by the `DEMYSTIFY_PARSE_CACHE` environment variable:

* **unset** — cache in a SQLite database under the OS temporary directory (`<temp>/demystify-parse-cache/parse.sqlite`). This is fine to lose; it is rebuilt on demand.
* **a directory path** — store the cache there instead (the database is `<dir>/parse.sqlite`).
* **`off`** — disable caching entirely.

The cache is safe to share between processes running at the same time. To see where it lives, run with `--log progress`.

## Development Status

Please note that `demystify` is a work in progress. Some features are only half-completed and may change.

## Contributing

Contributions to `demystify` are welcome. Feel free to open issues and submit pull requests on the [GitHub repository](https://github.com/stacs-cp/demystify-rs).

## License

`demystify` is licensed under the MPL 2.0 License. See the `LICENSE.txt` file for more details.

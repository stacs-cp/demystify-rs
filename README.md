# demystify-rs

`demystify-rs` is a Rust-based solver designed to explain constraint satisfaction problems and puzzles. This project is a rewrite of the original `demystify` solver, which was implemented in Python. The long-term goal of `demystify-rs` is to provide users with a robust tool for solving and understanding puzzles through detailed, human-readable explanations.

## Installation

Before installing `demystify-rs`, you need to install `conjure`, a tool for constraint satisfaction and optimization problems. Follow the instructions on the [Conjure GitHub page](https://www.github.com/conjure-cp/conjure) to install `conjure`.

You will also need a reasonably recent version of `rust`. There are various ways to install Rust, but the easiest is probably with [rustup](https://rustup.rs/).

Once `conjure` and `rust` are installed, you can proceed to set up `demystify-rs`.

1. Clone the `demystify-rs` repository:
   ```sh
   git clone https://github.com/stacs-cp/demystify-rs
   cd demystify-rs
   ```

2. Build the project using Cargo:
   ```sh
   cargo build --release
   ```

## Quick Start

To quickly get started with `demystify-rs`, you can run the following command to solve a Sudoku puzzle and generate an explanatory HTML file:

```sh
cargo run --bin main --release -- --model eprime/sudoku.eprime --param eprime/sudoku/redditexample.param --html --quick --trace > sudoku.html
```

After running this command, open `sudoku.html` in your web browser to view the solution and its detailed explanation.

## Development Status

Please note that `demystify-rs` is a work in progress. Some features are currently only half-completed and may be subject to changes. Your feedback and contributions are welcome to help improve the project.

## Contributing

Contributions to `demystify-rs` are welcome. Feel free to open issues and submit pull requests on the [GitHub repository](https://github.com/stacs-cp/demystify-rs).

## License

`demystify-rs` is licensed under the MPL 2.0 License. See the `LICENSE` file for more details.

---

Happy puzzling!
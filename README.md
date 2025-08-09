# demystify

`demystify` is a Rust-based solver designed to explain constraint satisfaction problems and puzzles. This project is a rewrite of the original `demystify` solver, which was implemented in Python. The long-term goal of `demystify` is to provide users with a robust tool for solving and understanding puzzles through detailed, human-readable explanations.

## Installation


`demystify` requires `conjure`, a tool for constraint satisfaction and optimization problems. There are 2 ways to run conjure:

* Follow the instructions on the [Conjure GitHub page](https://www.github.com/conjure-cp/conjure) to install `conjure`.
* If you have docker, or podman, installed then if `conjure` isn't in your path it will be automatically downloaded via docker/podman. On **windows**, you must use docker.

You will also need a reasonably recent version of `rust`. There are various ways to install Rust, but the easiest is probably with [rustup](https://rustup.rs/)

If you are on **windows**, you also need LLVM, you can get it by running `winget install LLVM.LLVM`.

Once `conjure` and `rust` are installed, you can proceed to set up `demystify`.

1. Clone the `demystify` repository:
   ```sh
   git clone https://github.com/stacs-cp/demystify-rs
   cd demystify
   ```


## Testing your installation

If you want to test demystify is working correctly, run it's tests:

```sh
cargo test --workspace
```

Note that this may take a long time the first time you run it (including warnings about 'Tests taking longer than 30 seconds'), if docker or podman is being used, as the Conjure image must be downloaded the first time it is used.

## Quick Start - Web Interface

The easiest way to get started with `demystify` is with the web interface. Just run:

```sh
cargo run --release --bin demystify-web
```

The go to the webpage it mentions (usually `https://localhost:8008` )

## Quick Start

To quickly get started with `demystify`, you can run the following command to solve a Sudoku puzzle and generate an explanatory HTML file:

```sh
cargo run --bin demystify --release -- --model eprime/sudoku.eprime --param eprime/sudoku/redditexample.param --html --quick --trace > sudoku.html
```

After running this command, open `sudoku.html` in your web browser to view the solution and its detailed explanation.

## Development Status

Please note that `demystify` is a work in progress. Some features are currently only half-completed and may be subject to changes. Your feedback and contributions are welcome to help improve the project.

## Contributing

Contributions to `demystify` are welcome. Feel free to open issues and submit pull requests on the [GitHub repository](https://github.com/stacs-cp/demystify-rs).

## License

`demystify` is licensed under the MPL 2.0 License. See the `LICENSE` file for more details.

---

Happy puzzling!

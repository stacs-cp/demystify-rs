# demystify-web

`demystify-web` is the web interface for the `demystify` constraint satisfaction problem solver. It aims to provide an easy-to-use browser interface for solving and understanding puzzles through detailed, human-readable explanations.

## Overview

This package offers a convenient web-based frontend to the core `demystify` solver. If you're looking for low-level access to the solving engine, please install the main [`demystify`](https://github.com/stacs-cp/demystify) package instead.

## Installation

### Prerequisites

* A reasonably recent version of `rust`. Install with [rustup](https://rustup.rs/)
* If you're on **Windows**, you'll need LLVM: `winget install LLVM.LLVM`

The web interface will automatically handle the installation of `conjure` (via Docker/Podman if needed) when you run it.

### Setup

1. Clone the repository:
   ```sh
   git clone https://github.com/stacs-cp/demystify
   cd demystify-web
   ```

## Running the Web Interface

To start the web server:

```sh
cargo run --release --bin serve
```

Then navigate to the URL displayed in your terminal (typically `http://localhost:8008`).

## Testing

To verify that everything is working correctly:

```sh
cargo test --workspace
```

Note: The first test run may take longer if Docker/Podman needs to download the Conjure image.

## Development Status

`demystify-web` is under active development. Some features may be incomplete or subject to change.

## Contributing

Contributions are welcome! Please feel free to open issues and submit pull requests on the [GitHub repository](https://github.com/stacs-cp/demystify).

## License

Licensed under the MPL 2.0 License. See the `LICENSE` file for details.

---

Happy puzzling through the web interface!

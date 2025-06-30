#!/usr/bin/env bash

set -eux

cargo run --release --bin main -- --model eprime/sudoku.eprime --param eprime/sudoku/puzzlingexample.param --merge 5 > /dev/null
cargo run --release --bin main -- --model eprime/sudoku.eprime --param eprime/sudoku/puzzlingexample.param --merge 5 --html > /dev/null
cargo run --release --bin main -- --model eprime/star-battle.eprime --param eprime/star-battle/FATAtalkexample.param --merge 1 > /dev/null
cargo run --release --bin main -- --model eprime/star-battle.eprime --param eprime/star-battle/FATAtalkexample.param --merge 1 --html > /dev/null

# cargo run --release --bin main -- --model eprime/loopy.essence --param eprime/loopy/loopy-01.param --merge 2 > /dev/null
# cargo run --release --bin main -- --model eprime/loopy.essence --param eprime/loopy/loopy-01.param --merge 2 --html > /dev/null

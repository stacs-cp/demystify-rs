#!/usr/bin/env bash
# Run miracle sudoku benchmarks for each step count as independent cargo bench
# invocations, smallest first.  Each completes and saves its criterion report
# independently, so killing this script partway still leaves all finished tiers
# with usable data.
#
# Usage:
#   nohup bash scripts/bench-miracle-sweep.sh &> scripts/logs/miracle-sweep-$(date +%Y%m%d-%H%M%S).log &
#
# Results land in:
#   target/criterion/miracle_sudoku_NN/solve/

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

LOGDIR="scripts/logs"
mkdir -p "$LOGDIR"

echo "========================================================"
echo "miracle sudoku step sweep  $(date)"
echo "========================================================"

cargo build --release --bench solve
echo "Build done."
echo ""

run_tier() {
    local name="$1"
    echo "--------------------------------------------------------"
    echo "Starting $name  $(date)"
    echo "--------------------------------------------------------"
    cargo bench --bench solve -- --noplot "$name"
    echo "Finished $name  $(date)"
    echo ""
}

run_tier miracle_sudoku_01
run_tier miracle_sudoku_02
run_tier miracle_sudoku_05
run_tier miracle_sudoku_10
run_tier miracle_sudoku_20
run_tier miracle_sudoku_50

echo "========================================================"
echo "All tiers complete  $(date)"
echo "========================================================"

#!/usr/bin/env bash
# Core-guided vs standard MUS experiment.
#
# For each puzzle category, runs instances 01–20 with both --mus-method mus
# (standard) and --mus-method core (core-guided).
#
# Results go to $OUTDIR (default /tmp/bench-cores-experiment/).
# Each run produces three files:
#   <name>-<NN>.<method>.{stdout,stderr,time}
# where <method> is "standard" or "core".
#
# Usage:
#   bash scripts/bench-experiment.sh              # full run
#   bash scripts/bench-experiment.sh --dry-run    # print what would run

set -eu

OUTDIR="${BENCH_OUTDIR:-/tmp/bench-cores-experiment}"
INSTANCES="01 02 03 04 05 06 07 08 09 10 11 12 13 14 15 16 17 18 19 20"
EXTRA_ARGS="${BENCH_EXTRA_ARGS:-}"
DRY_RUN=false
[ "${1:-}" = "--dry-run" ] && DRY_RUN=true

mkdir -p "$OUTDIR"

BINARY="$(cd "$(dirname "$0")/.." && pwd)/target/release/demystify"

if [ "$DRY_RUN" = false ]; then
    cargo build --release --bin demystify
    if [ ! -x "$BINARY" ]; then
        echo "ERROR: $BINARY not found after build" >&2
        exit 1
    fi
fi

run_one() {
    local name="$1"
    local instance="$2"
    local method="$3"   # "standard", "core", or "core+mus"
    local model="$4"
    local param="$5"
    local extra="${6:-}"

    local method_flag
    case "$method" in
        standard)  method_flag="--mus-method mus" ;;
        core)      method_flag="--mus-method core" ;;
        core+mus)  method_flag="--mus-method core+mus" ;;
    esac

    local tag="${name}-${instance}.${method}"
    local outfile="$OUTDIR/$tag"

    if [ "$DRY_RUN" = true ]; then
        echo "  $tag: $BINARY --model $model --param $param $method_flag --log progress $extra"
        return
    fi

    # Use /usr/bin/time to capture CPU time and max RSS.
    # On macOS: -l gives detailed stats; on Linux: -v.
    local time_flag="-l"
    [ "$(uname)" = "Linux" ] && time_flag="-v"

    /usr/bin/time $time_flag \
        "$BINARY" --model "$model" --param "$param" \
        $method_flag --log progress $EXTRA_ARGS $extra \
        > "${outfile}.stdout" 2> "${outfile}.stderr" || true

    # Extract wall/user/sys times from the time output at the end of stderr.
    python3 -c "
import re, sys
text = open('${outfile}.stderr').read()
# macOS /usr/bin/time -l format
wall = re.search(r'([\d.]+)\s+real', text)
user = re.search(r'([\d.]+)\s+user', text)
syst = re.search(r'([\d.]+)\s+sys', text)
rss  = re.search(r'(\d+)\s+maximum resident set size', text)
if not wall:
    # Linux /usr/bin/time -v format
    wall = re.search(r'Elapsed.*?:\s*([\d:.]+)', text)
    user = re.search(r'User time.*?:\s*([\d.]+)', text)
    syst = re.search(r'System time.*?:\s*([\d.]+)', text)
    rss  = re.search(r'Maximum resident.*?:\s*(\d+)', text)
d = {}
if wall: d['wall'] = wall.group(1)
if user: d['user'] = user.group(1)
if syst: d['sys']  = syst.group(1)
if rss:  d['rss_kb'] = rss.group(1)
# Phase timings from demystify itself
conj = re.search(r'conjure/savilerow completed in ([\d.]+)s', text)
setup = re.search(r'parse setup completed in ([\d.]+)s', text)
solve = re.search(r'solve completed in ([\d.]+)s', text)
if conj:  d['t_conjure'] = conj.group(1)
if setup: d['t_setup'] = setup.group(1)
if solve: d['t_solve'] = solve.group(1)
for k, v in d.items():
    print(f'{k}={v}')
" > "${outfile}.time"
}

run_category() {
    local name="$1"
    local model="$2"
    local param_dir="$3"
    local param_pattern="$4"  # e.g. "6x6-easy"
    local extra="${5:-}"

    echo -n "  ${name}-${param_pattern} "
    for nn in $INSTANCES; do
        local param="${param_dir}/${param_pattern}-${nn}.param"
        if [ ! -f "$param" ] && [ "$DRY_RUN" = false ]; then
            echo -n "?"
            continue
        fi
        run_one "$name-$param_pattern" "$nn" standard "$model" "$param" "$extra"
        run_one "$name-$param_pattern" "$nn" core "$model" "$param" "$extra"
        run_one "$name-$param_pattern" "$nn" "core+mus" "$model" "$param" "$extra"
        echo -n "."
    done
    echo " done"
}

EPRIME="eprime"

echo "=== Binairo (6x6, 10x10, 20x20) ==="
for diff in 6x6-easy 6x6-hard 10x10-easy 10x10-hard 20x20-easy 20x20-hard; do
    run_category binairo "$EPRIME/binairo.essence" "$EPRIME/binairo/puzzle-binairo-com" "$diff" "--only-assign"
done

echo ""
echo "=== Sudoku (all difficulties) ==="
for diff in basic easy intermediate advanced extreme evil; do
    run_category sudoku "$EPRIME/sudoku.eprime" "$EPRIME/sudoku/puzzle-sudoku-com" "$diff"
done

echo ""
echo "=== Futoshiki (4x4, 5x5, 9x9) ==="
for diff in 4x4-easy 5x5-easy 5x5-normal 5x5-hard 9x9-easy 9x9-normal 9x9-hard; do
    run_category futoshiki "$EPRIME/futoshiki.eprime" "$EPRIME/futoshiki/puzzle-futoshiki-com" "$diff"
done

echo ""
echo "=== Akari (7x7, 10x10, 14x14) ==="
for diff in 7x7-easy 7x7-normal 7x7-hard 10x10-easy 10x10-normal 10x10-hard 14x14-easy 14x14-normal 14x14-hard; do
    run_category akari "$EPRIME/akari.eprime" "$EPRIME/akari/puzzle-light-up-com" "$diff"
done

echo ""
echo "=== Minesweeper (5x5, 10x10, 20x20) ==="
for diff in 5x5-easy 5x5-hard 10x10-easy 10x10-hard 20x20-easy 20x20-hard; do
    run_category minesweeper "$EPRIME/ppminesweeper.eprime" "$EPRIME/ppminesweeper/puzzle-minesweeper-com" "$diff"
done

echo ""
echo "=== Tents (6x6, 10x10) ==="
for diff in 6x6-easy 6x6-hard 10x10-easy 10x10-hard; do
    run_category tents "$EPRIME/tents.eprime" "$EPRIME/tents/puzzle-tents-com" "$diff"
done

echo ""
echo "=== Futoshiki --only-assign (4x4, 5x5, 9x9) ==="
for diff in 4x4-easy 5x5-easy 5x5-normal 5x5-hard 9x9-easy 9x9-normal 9x9-hard; do
    run_category futoshiki-oa "$EPRIME/futoshiki.eprime" "$EPRIME/futoshiki/puzzle-futoshiki-com" "$diff" "--only-assign"
done

echo ""
echo "=== Tents --only-assign (6x6, 10x10) ==="
for diff in 6x6-easy 6x6-hard 10x10-easy 10x10-hard; do
    run_category tents-oa "$EPRIME/tents.eprime" "$EPRIME/tents/puzzle-tents-com" "$diff" "--only-assign"
done

echo ""
echo "All experiments done. Results in $OUTDIR"
echo "Generate table: python3 scripts/bench-table.py $OUTDIR"

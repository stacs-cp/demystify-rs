#!/usr/bin/env bash
# Full benchmark: normal MUS search vs core-guided mode.
# Captures time, steps, MUS sizes. Results go to /tmp/claude/bench-cores/

set -eu

OUTDIR="/tmp/claude/bench-cores"
mkdir -p "$OUTDIR"

cargo build --release --bin demystify 2>/dev/null

BINARY="cargo run --release --bin demystify --"

run_one() {
    local name="$1"
    local mode="$2"  # "normal" or "cores"
    local model="$3"
    local param="$4"
    local extra="${5:-}"

    local flag=""
    [ "$mode" = "cores" ] && flag="--cores"

    local outfile="$OUTDIR/${name}.${mode}"

    local t0 t1
    t0=$(python3 -c 'import time; print(time.time())')
    $BINARY --model "$model" --param "$param" $flag $extra \
        > "${outfile}.stdout" 2> "${outfile}.stderr" || true
    t1=$(python3 -c 'import time; print(time.time())')
    python3 -c "print(f'{$t1 - $t0:.3f}')" > "${outfile}.time"
}

run_both() {
    local name="$1"
    local model="$2"
    local param="$3"
    local extra="${4:-}"

    echo -n "  $name... "
    run_one "$name" normal "$model" "$param" "$extra"
    echo -n "normal done, "
    run_one "$name" cores "$model" "$param" "$extra"
    echo "cores done"
}

echo "=== Running benchmarks ==="
echo ""

run_both "sudoku-reddit"       eprime/sudoku.eprime eprime/sudoku/redditexample.param
run_both "sudoku-puzzling"     eprime/sudoku.eprime eprime/sudoku/puzzlingexample.param
run_both "starbattle-fata"     eprime/star-battle.eprime eprime/star-battle/FATAtalkexample.param
run_both "starbattle-krazydad" eprime/star-battle.eprime eprime/star-battle/krazydad-tutorial.param
run_both "starbattle-logicmasters" eprime/star-battle.eprime eprime/star-battle/logicmastersindia-example.param
run_both "starbattle-rohanrao" eprime/star-battle.eprime eprime/star-battle/rohanrao-blogspot-example.param
run_both "starbattle-1"        eprime/star-battle.eprime eprime/star-battle/star-battle-1.param
run_both "binairo-1"           eprime/binairo.essence eprime/binairo/binairo-1.param "--only-assign"
run_both "binairo-diiscu"      eprime/binairo.essence eprime/binairo/diiscu.param "--only-assign"
run_both "futoshiki-1"         eprime/futoshiki.eprime eprime/futoshiki/nfutoshiki-1.param
run_both "futoshiki-2"         eprime/futoshiki.eprime eprime/futoshiki/nfutoshiki-2.param
run_both "futoshiki-3"         eprime/futoshiki.eprime eprime/futoshiki/nfutoshiki-3.param
run_both "futoshiki-wiki"      eprime/futoshiki.eprime eprime/futoshiki/wikipedia-example.param
run_both "kakuro-5x5"          eprime/kakuro.eprime eprime/kakuro/kakuro-5-5.param
run_both "minesweeper"         eprime/minesweeper.eprime eprime/minesweeper/minesweeperPrinted.param
run_both "mosaic-5x5"          eprime/mosaic.eprime eprime/mosaic/mosaic-5x5.param
run_both "nonogram-5x5"        eprime/nonogram.eprime eprime/nonogram/nonogram-5x5.param
run_both "nonogram-duck"       eprime/nonogram.eprime eprime/nonogram/duck-8x9.param
run_both "nonogram-heart"      eprime/nonogram.eprime eprime/nonogram/heart-10x10.param
run_both "skyscrapers-1"       eprime/skyscrapers.eprime eprime/skyscrapers/skyscrapers-1.param
run_both "skyscrapers-2"       eprime/skyscrapers.eprime eprime/skyscrapers/skyscrapers-2.param
run_both "battleship-1"        eprime/solitairebattleship.eprime eprime/solitairebattleship/solitaire_battleship1.param
run_both "battleship-c1"       eprime/solitairebattleship.eprime eprime/solitairebattleship/solitaire_battleship_conceptis_1.param
run_both "battleship-c2"       eprime/solitairebattleship.eprime eprime/solitairebattleship/solitaire_battleship_conceptis_2.param
run_both "thermometer-1"       eprime/thermometer.eprime eprime/thermometer/thermometer-1.param
run_both "akari-3x3"           eprime/akari.eprime eprime/akari/akari-3x3.param
run_both "akari-5x5"           eprime/akari.eprime eprime/akari/akari-5x5.param
run_both "garam"               eprime/garam.eprime eprime/garam/template.param
run_both "kakurasu"            eprime/kakurasu.eprime eprime/kakurasu/kakurasu.param
run_both "origin"              eprime/origin.eprime eprime/origin/origin.param
run_both "pasta"               eprime/pasta.eprime eprime/pasta/pasta.param
run_both "stitch-cupcake"      eprime/stitch.eprime eprime/stitch/cupcake1.param
run_both "stitch-hard1"        eprime/stitch.eprime eprime/stitch/hard1.param
run_both "stitch-hard2"        eprime/stitch.eprime eprime/stitch/hard2.param
run_both "tents-1234567"       eprime/tents.eprime eprime/tents/tectonic-1-2-3-4-5-6-7.param
run_both "tents-8"             eprime/tents.eprime eprime/tents/tectonic-8.param
run_both "tents-9"             eprime/tents.eprime eprime/tents/tectonic-9.param
run_both "xsums-easy"          eprime/x-sums.eprime eprime/x-sums/easy-xsums.param
run_both "killersudoku-std"    eprime/killersudoku.eprime eprime/killersudoku/killersudoku.param

echo ""
echo "All benchmarks done. Results in $OUTDIR"
echo "Run: python3 scripts/analyse-bench.py"

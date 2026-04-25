#!/usr/bin/env bash
# Run core vs MUS comparison across all benchmark puzzles.
# Output goes to /tmp/claude/core-comparison-results/

set -eu

OUTDIR="/tmp/claude/core-comparison-results"
mkdir -p "$OUTDIR"

BINARY="cargo run --release --bin demystify --"
TIMEOUT=1800  # 30 minutes per instance

# Build once up front
cargo build --release --bin demystify 2>/dev/null

run_instance() {
    local name="$1"
    local model="$2"
    local param="$3"
    local extra="${4:-}"
    local outfile="$OUTDIR/$name.txt"

    echo -n "Running $name... "
    if timeout "$TIMEOUT" cargo run --release --bin demystify -- \
        --model "$model" --param "$param" \
        --compare-cores --quiet $extra \
        > /dev/null 2> "$outfile"; then
        steps=$(grep -c "^Step" "$outfile" 2>/dev/null || echo 0)
        echo "done ($steps steps)"
    else
        echo "TIMEOUT or ERROR"
        echo "TIMEOUT after ${TIMEOUT}s" >> "$outfile"
    fi
}

# Sudoku
run_instance "sudoku-reddit"       eprime/sudoku.eprime eprime/sudoku/redditexample.param
run_instance "sudoku-puzzling"     eprime/sudoku.eprime eprime/sudoku/puzzlingexample.param

# Star Battle
run_instance "starbattle-fata"     eprime/star-battle.eprime eprime/star-battle/FATAtalkexample.param
run_instance "starbattle-krazydad" eprime/star-battle.eprime eprime/star-battle/krazydad-tutorial.param
run_instance "starbattle-logicmasters" eprime/star-battle.eprime eprime/star-battle/logicmastersindia-example.param
run_instance "starbattle-rohanrao" eprime/star-battle.eprime eprime/star-battle/rohanrao-blogspot-example.param
run_instance "starbattle-1"        eprime/star-battle.eprime eprime/star-battle/star-battle-1.param
run_instance "starbattle-tectonic-group" eprime/star-battle.eprime eprime/star-battle/tectonic-grouping-of-rows-or-columns.param
run_instance "starbattle-tectonic-elim" eprime/star-battle.eprime eprime/star-battle/tectonic-row-or-col-elimination.param

# Binairo
run_instance "binairo-1"           eprime/binairo.essence eprime/binairo/binairo-1.param "--only-assign"
run_instance "binairo-diiscu"      eprime/binairo.essence eprime/binairo/diiscu.param "--only-assign"

# Futoshiki
run_instance "futoshiki-1"         eprime/futoshiki.eprime eprime/futoshiki/nfutoshiki-1.param
run_instance "futoshiki-2"         eprime/futoshiki.eprime eprime/futoshiki/nfutoshiki-2.param
run_instance "futoshiki-3"         eprime/futoshiki.eprime eprime/futoshiki/nfutoshiki-3.param
run_instance "futoshiki-wiki"      eprime/futoshiki.eprime eprime/futoshiki/wikipedia-example.param

# Kakuro
run_instance "kakuro-5x5"         eprime/kakuro.eprime eprime/kakuro/kakuro-5-5.param

# Killer Sudoku
run_instance "killersudoku-crack"  eprime/killersudoku.eprime eprime/killersudoku/crackingkillersudoku.param
run_instance "killersudoku-extreme" eprime/killersudoku.eprime eprime/killersudoku/extremekillersudokuwiki.param
run_instance "killersudoku-std"    eprime/killersudoku.eprime eprime/killersudoku/killersudoku.param

# Minesweeper
run_instance "minesweeper"         eprime/minesweeper.eprime eprime/minesweeper/minesweeperPrinted.param

# Mosaic
run_instance "mosaic-5x5"         eprime/mosaic.eprime eprime/mosaic/mosaic-5x5.param

# Nonogram
run_instance "nonogram-5x5"       eprime/nonogram.eprime eprime/nonogram/nonogram-5x5.param
run_instance "nonogram-duck"      eprime/nonogram.eprime eprime/nonogram/duck-8x9.param
run_instance "nonogram-heart"     eprime/nonogram.eprime eprime/nonogram/heart-10x10.param

# Skyscrapers
run_instance "skyscrapers-1"       eprime/skyscrapers.eprime eprime/skyscrapers/skyscrapers-1.param
run_instance "skyscrapers-2"       eprime/skyscrapers.eprime eprime/skyscrapers/skyscrapers-2.param

# Solitaire Battleship
run_instance "battleship-1"        eprime/solitairebattleship.eprime eprime/solitairebattleship/solitaire_battleship1.param
run_instance "battleship-c1"       eprime/solitairebattleship.eprime eprime/solitairebattleship/solitaire_battleship_conceptis_1.param
run_instance "battleship-c2"       eprime/solitairebattleship.eprime eprime/solitairebattleship/solitaire_battleship_conceptis_2.param

# Thermometer
run_instance "thermometer-1"       eprime/thermometer.eprime eprime/thermometer/thermometer-1.param

# X-Sums
run_instance "xsums-easy"          eprime/x-sums.eprime eprime/x-sums/easy-xsums.param
run_instance "xsums-ctc"           eprime/x-sums.eprime eprime/x-sums/ctc-best-xsums.param

# Akari
run_instance "akari-3x3"           eprime/akari.eprime eprime/akari/akari-3x3.param
run_instance "akari-5x5"           eprime/akari.eprime eprime/akari/akari-5x5.param

# Garam
run_instance "garam"               eprime/garam.eprime eprime/garam/template.param

# Kakurasu
run_instance "kakurasu"             eprime/kakurasu.eprime eprime/kakurasu/kakurasu.param

# Origin
run_instance "origin"               eprime/origin.eprime eprime/origin/origin.param

# Pasta
run_instance "pasta"                eprime/pasta.eprime eprime/pasta/pasta.param

# Stitch
run_instance "stitch-cupcake"       eprime/stitch.eprime eprime/stitch/cupcake1.param
run_instance "stitch-hard1"         eprime/stitch.eprime eprime/stitch/hard1.param
run_instance "stitch-hard2"         eprime/stitch.eprime eprime/stitch/hard2.param

# Tents
run_instance "tents-1234567"        eprime/tents.eprime eprime/tents/tectonic-1-2-3-4-5-6-7.param
run_instance "tents-8"              eprime/tents.eprime eprime/tents/tectonic-8.param
run_instance "tents-9"              eprime/tents.eprime eprime/tents/tectonic-9.param

# Miracle (likely slow)
run_instance "miracle"              eprime/miracle.eprime eprime/miracle/original.param

# Loopy (a few)
run_instance "loopy-01"             eprime/loopy.essence eprime/loopy/loopy-01.param
run_instance "loopy-02"             eprime/loopy.essence eprime/loopy/loopy-02.param
run_instance "loopy-03"             eprime/loopy.essence eprime/loopy/loopy-03.param

echo ""
echo "All done. Results in $OUTDIR"

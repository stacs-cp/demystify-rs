# SudokuWiki domain puzzles

Puzzles extracted from [sudokuwiki.org](https://www.sudokuwiki.org/),
via the old Python demystify's `sudoku-wiki.py` test suite.

These param files use the **domain** format: each cell specifies which
values are candidates (rather than a single fixed value). This lets you
test demystify on partially-reduced puzzles where specific solving
techniques are expected to apply.

## Models

| Directory | Model file | Puzzle type |
|-----------|-----------|-------------|
| `sudokuwiki-domains/` | `sudoku-domains.eprime` | Standard sudoku |
| `../xsudoku/sudokuwiki/` | `xsudoku-domains.eprime` | X-sudoku |
| `../jigsaw/sudokuwiki/` | `jigsaw-domains.eprime` | Jigsaw sudoku |

## Domain format

The `allowed` parameter is a 9x9x9 binary matrix where
`allowed[row][col][d-1]` is 1 if value d is a candidate for cell (row,col).

Jigsaw puzzles additionally have a `regions` parameter: a 9x9 matrix
giving the region number (1-9) for each cell.

## Example usage

```sh
cargo run --release --bin demystify -- \
    --model eprime/sudoku-domains.eprime \
    --param eprime/sudoku/sudokuwiki-domains/x-wing-strategy-1a.param \
    --html --quick --trace > xwing.html
```

## Puzzles by technique

### Custom (3 puzzles)

- `chris-bonus-q-2-alldiff.param` (sudoku-domains): Chris Bonus Q (2 AllDiff)
- `chris-bonus-2-alldiff.param` (sudoku-domains): Chris Bonus (2 AllDiff)
- `chris-bonus-2-2-alldiff.param` (sudoku-domains): Chris Bonus 2 (2 AllDiff)

### X Sudoku (3 puzzles)

- `xsudoku-pointing-pair.param` (xsudoku-domains): XSudoku - Pointing Pair
- `xsudoku-pointing-pair-2.param` (xsudoku-domains): XSudoku - Pointing Pair 2
- `xsudoku-pointing-pair-3.param` (xsudoku-domains): XSudoku - Pointing Pair 3

### Jigsaw (3 puzzles)

- `jigsaw-double-pointing-pair-1.param` (jigsaw-domains): Jigsaw - double pointing pair 1
- `jigsaw-double-pointing-pair-2.param` (jigsaw-domains): Jigsaw - double pointing pair 2
- `jigsaw-double-pointing-pair-3.param` (jigsaw-domains): Jigsaw - double pointing pair 3

### Naked (5 puzzles)

- `naked-pairs-example-1.param` (sudoku-domains): Naked Pairs, example 1
- `naked-pairs-example-2.param` (sudoku-domains): Naked Pairs, example 2
- `naked-triples-example-1.param` (sudoku-domains): Naked Triples, example 1
- `naked-triples-example-2.param` (sudoku-domains): Naked Triples, example 2
- `naked-quad-example.param` (sudoku-domains): Naked Quad example

### Hidden (5 puzzles)

- `hidden-pair-example.param` (sudoku-domains): Hidden Pair example
- `three-hidden-pairs.param` (sudoku-domains): Three hidden pairs
- `two-hidden-triples.param` (sudoku-domains): Two Hidden Triples
- `hidden-quad-1-identifying-h7-6-only.param` (sudoku-domains): Hidden Quad 1 (identifying H7,6 only)
- `hidden-quad-2.param` (sudoku-domains): Hidden Quad 2

### Pointing (3 puzzles)

- `pointing-pairs-example-1.param` (sudoku-domains): Pointing Pairs example 1
- `pointing-pairs-example-2.param` (sudoku-domains): Pointing Pairs example 2
- `pointing-triple.param` (sudoku-domains): Pointing Triple

### Box/Line Reduction (2 puzzles)

- `box-line-reduction-example-2.param` (sudoku-domains): Box/Line Reduction example 2
- `triple-blr-only-one-of-those-is-considered-by-us.param` (sudoku-domains): Triple BLR (only one of those is considered by us)

### X Wing (4 puzzles)

- `x-wing-strategy-1a.param` (sudoku-domains): X_Wing_Strategy 1A
- `x-wing-strategy-1b.param` (sudoku-domains): X_Wing_Strategy 1B
- `x-wing-strategy-2.param` (sudoku-domains): X_Wing_Strategy 2
- `x-wing-strategy-3.param` (sudoku-domains): X_Wing_Strategy 3

### Simple Colouring (3 puzzles)

- `simple-colouring-single-chains-twice-in-a-unit-pre.param` (sudoku-domains): Simple Colouring (single chains) - Twice in a unit (Pre)
- `simple-colouring-single-chains-two-colours-elsewhere.param` (sudoku-domains): Simple Colouring (single chains) - Two colours 'elsewhere'
- `simple-colouring-single-chains-two-colours-elsewhere-2.param` (sudoku-domains): Simple Colouring (single chains) - Two colours 'elsewhere' - 2

### Y-Wing (2 puzzles)

- `y-wing-1.param` (sudoku-domains): Y-Wing 1
- `y-wing-2.param` (sudoku-domains): Y-Wing 2

### Swordfish (3 puzzles)

- `swordfish-1.param` (sudoku-domains): Swordfish 1
- `swordfish-2.param` (sudoku-domains): Swordfish 2
- `swordfish-3.param` (sudoku-domains): Swordfish 3

### XYZ Wing (2 puzzles)

- `xyz-wing-1.param` (sudoku-domains): XYZ Wing 1
- `xyz-wing-2.param` (sudoku-domains): XYZ Wing 2

### X-Cycle (4 puzzles)

- `x-cycle-part-1.param` (sudoku-domains): X-Cycle (part 1)
- `x-cycle-part-2-fig-1.param` (sudoku-domains): X-Cycle (part 2) - fig 1
- `x-cycle-part-2-fig-2.param` (sudoku-domains): X-Cycle (part 2) - fig 2
- `x-cycle-part-2-fig-3.param` (sudoku-domains): X-Cycle (part 2) - fig 3

### XY-Chain (3 puzzles)

- `xy-chains-example-1-detected-as-wxyz-wing.param` (sudoku-domains): XY-Chains example 1 (detected as WXYZ-Wing)
- `xy-chains-example-2.param` (sudoku-domains): XY-Chains example 2
- `same-cells-different-xy-chain.param` (sudoku-domains): Same cells - different XY-Chain

### 3D Medusa (8 puzzles)

- `3d-medusa-rule-1.param` (sudoku-domains): 3D Medusa Rule 1
- `3d-medusa-rule-2.param` (sudoku-domains): 3D Medusa Rule 2
- `3d-medusa-rule-4-1.param` (sudoku-domains): 3D Medusa Rule 4 1
- `3d-medusa-rule-4-2.param` (sudoku-domains): 3D Medusa Rule 4 2
- `3d-medusa-rule-5.param` (sudoku-domains): 3D Medusa Rule 5
- `3d-medusa-rule-6-1.param` (sudoku-domains): 3D Medusa Rule 6 1
- `3d-medusa-rule-6-2-using-3-candidates-in-a-cell.param` (sudoku-domains): 3D Medusa Rule 6 2, using 3 candidates in a cell
- `3d-medusa-37-eliminations-by-rule-1.param` (sudoku-domains): 3D Medusa 37 Eliminations by Rule 1

### Jellyfish (4 puzzles)

- `example-jellyfish.param` (sudoku-domains): Example Jellyfish
- `18-elimination-jellyfish.param` (sudoku-domains): 18 elimination Jellyfish
- `perfect-jellyfish.param` (sudoku-domains): Perfect Jellyfish
- `jellyfish-20-eliminations.param` (sudoku-domains): Jellyfish, 20 eliminations

### SK Loop (3 puzzles)

- `sk-loop-easter-monster.param` (sudoku-domains): SK Loop, Easter Monster
- `type-3-1-3-1-3-1-3-1-sk-loop.param` (sudoku-domains): Type 3-1-3-1-3-1-3-1- SK Loop
- `sk-loop-with-solved-cells.param` (sudoku-domains): SK Loop with Solved Cells

### WXYZ-Wing (4 puzzles)

- `wxyz-wing-example-1.param` (sudoku-domains): WXYZ-Wing example 1
- `wxyz-wing-example-2.param` (sudoku-domains): WXYZ-Wing example 2
- `wxyz-wing-example-3-detected-as-y-wing.param` (sudoku-domains): WXYZ-Wing example 3 (detected as Y-Wing)
- `wxyz-wing-example-4.param` (sudoku-domains): WXYZ-Wing example 4

### Aligned Pair (4 puzzles)

- `ape-example-1-simpler-y-wing.param` (sudoku-domains): APE example 1 (simpler Y-Wing)
- `ape-example-2-detected-as-wxyz-wing.param` (sudoku-domains): APE example 2 (detected as WXYZ-Wing)
- `ape-example-5.param` (sudoku-domains): APE example 5
- `an-eight-cell-aligned-pair.param` (sudoku-domains): An Eight-Cell Aligned Pair

### Exocet (1 puzzles)

- `exocet-rule-1.param` (sudoku-domains): Exocet Rule 1

### Grouped X-Cycles (6 puzzles)

- `grouped-x-cycles-nice-loops-1-4-cycle-with-grouped-cells-detected-as-swordfish.param` (sudoku-domains): Grouped X-Cycles, Nice Loops 1, 4-Cycle with Grouped Cells (detected as Swordfish)
- `grouped-x-cycles-nice-loops-1-grouped-8-cycle-different-to-the-example.param` (sudoku-domains): Grouped X-Cycles, Nice Loops 1, Grouped 8-Cycle, different to the example
- `grouped-x-cycles-nice-loops-2-grouped-8-cycle.param` (sudoku-domains): Grouped X-Cycles, Nice Loops 2, Grouped 8-Cycle
- `grouped-x-cycles-nice-loops-3-2-cycle-with-grouped-cells.param` (sudoku-domains): Grouped X-Cycles, Nice Loops 3, 2-Cycle with Grouped Cells
- `grouped-x-cycles-consecutive-grouped-cells-x-cycle-on-2.param` (sudoku-domains): Grouped X-Cycles, Consecutive Grouped Cells, X-Cycle on 2
- `grouped-x-cycles-consecutive-grouped-cells-grouped-x-cycle-with-8-nodes-a-lot-more-involved-than-shown.param` (sudoku-domains): Grouped X-Cycles, Consecutive Grouped Cells, Grouped X-Cycle with 8 nodes, a lot more involved than shown

## Regenerating

```sh
python3 scripts/extract-sudokuwiki.py
```

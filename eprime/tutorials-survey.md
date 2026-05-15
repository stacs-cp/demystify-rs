# Tutorial Benchmark Survey

A complete inventory of every tutorial benchmark referenced by the JAIR paper
(`text/overleaf/600aea0f61b6de49eeda409c/main.tex`, Table 1) plus everything
present in `demystify-ijcai-final/{examples,eprime}` and the current
`demystify-rs/eprime` tree.  Goal: confirm nothing has been lost in the
transition between repos, give every tutorial a permanent home, and capture
every source URL / verbatim comment so the artefact can later be matched
against the original webpage.

## Paper Table 1 (target counts)

| Puzzle | Source | Source URL | # techniques (paper) |
|---|---|---|---:|
| Binairo | conceptis | https://www.conceptispuzzles.com | 7 |
| Binairo | tectonic | http://www.tectonicpuzzel.eu | 6 |
| Futoshiki | futoshikiorg | https://www.futoshiki.org/how-to-solve | 6 |
| Futoshiki | tectonic | http://www.tectonicpuzzel.eu | 7 |
| Jigsaw Sudoku | sudokuwiki | https://www.sudokuwiki.org | 3 |
| Kakuro | conceptis | https://www.conceptispuzzles.com | 11 |
| Kakuro | kakuro.com | http://www.kakuro.com/techniques.php | 2 |
| Kakuro | kakuros.org | https://www.kakuros.com/solve | 3 |
| Skyscrapers | conceptis | https://www.conceptispuzzles.com | 14 |
| Starbattle | krazydad | https://krazydad.com/starbattle/tutorial/tutorial_10x10.php | 4 |
| Starbattle | tectonic | http://www.tectonicpuzzel.eu | 2 |
| Starbattle | rohanrao | http://rohanrao.blogspot.com/2011/07/solving-star-battle-solved-example-1.html | 14 |
| Starbattle | logicmasters | https://logicmastersindia.com/forum/forums/thread-view.asp?tid=140 | 4 |
| Sudoku basic/tough | sudokuwiki | https://www.sudokuwiki.org | 29 |
| Sudoku diabolical | sudokuwiki | https://www.sudokuwiki.org | 29 |
| Tents+Trees | tectonic | http://www.tectonicpuzzel.eu | 9 |
| Thermometers | innoludic | https://www.innoludic.com/2015-04-30-13-56-29/thermometers/56-rules-of-thermometers.html | 2 |
| Thermometers | tectonic | http://www.tectonicpuzzel.eu | 5 |
| X-Sudoku | sudokuwiki | https://www.sudokuwiki.org | 3 |
| **Total** | | | **160** |

## What we have, by source

### Sudoku family (Python — `examples/sudoku-wiki.py`)

The Python file holds **75 `dotest()` calls**, with this breakdown.  Note the
file has nested `if False:` blocks the user said may need flipping to `True`:

| Variant | active by default | gated by `if False:` | total |
|---|---:|---:|---:|
| basic Sudoku | 43 | 26 | 69 |
| basic X-Sudoku | 0 | 3 | 3 |
| Jigsaw (jigsawH) | 0 | 2 | 2 |
| Jigsaw (zigzag) | 0 | 1 | 1 |
| **Total** | **43** | **32** | **75** |

So once the `if False:` blocks are switched on, all 75 are reachable.

Paper Table 1 expects 29 + 29 + 3 + 3 = 64 sudoku-family techniques.
**75 - 64 = 11 extras.**  The 3 Chris-Bonus tests are clearly extras (not
sudokuwiki-sourced).  The remaining ~8 are likely close-variant duplicates
(`X_Wing 1A` + `1B`, `(Pre)` setups, named variants like "Three hidden
pairs" / "Two hidden triples").  Will need cross-checking at conversion
time, but every test is *present* — nothing is lost.

A line-by-line table of all 75 (with comments preserved verbatim and
active/inactive flags) lives at `/tmp/claude/survey/sudoku-python.tsv`
(76 rows including header).

#### Existing param-form versions in current `demystify-rs` tree

The current repo already mirrors the Python tutorials as `.param` files,
in two layouts:

- `eprime/sudoku/sudokuwiki-domains/` — 69 flat files, **old format** per
  the README in that dir.  These are the basic-Sudoku ones (matches the
  Python's 69 `basicSudoku` dotests).
- `eprime/sudoku/sudokuwiki/<technique>/*.param` — 76 files **organised by
  technique** (3dmedusa, alignedpairexclusion, jellyfish, etc).  This is
  the newer layout.
- `eprime/xsudoku/sudokuwiki/*.param` — 3 X-Sudoku files (matches Python).
- `eprime/jigsaw/sudokuwiki/*.param` — 3 Jigsaw files (matches Python).

The `eprime/sudoku/sudokuwiki/` dir contains **technique categories not
counted in the paper** (paper explicitly excludes "unique rectangles"):
- `BUG/` 2, `extuniqrectangles/` 3, `hiddenuniqrectangles/` 4,
  `uniquerectangles/` 11.  Total: 20 unique-rectangle-family files.
  These are *additional* to the 64 paper-counted Sudoku techniques.

So total Sudoku-family `.param` files in repo: 76 + 3 + 3 = 82.
**82 = 64 paper + 20 unique-rectangle-family − 2 reconciliation.**
The reconciliation will need verifying against the actual file list.

### Non-Sudoku puzzles (eprime params)

Every non-sudoku tutorial sits at
`eprime/<kind>/solving_techniques/<source>/*.param`.  These files are
identical between `demystify-ijcai-final/eprime/` and the current
`demystify-rs/eprime/` (verified via `diff -rq`).

| Puzzle | Source dir | # files in repo | paper expects | gap? |
|---|---|---:|---:|---|
| Binairo | `conceptispuzzles/` | 7 | 7 | ✓ |
| Binairo | `tectonicpuzzel/` | 6 | 6 | ✓ |
| Futoshiki | `futoshikiorg/` | 6 | 6 | ✓ |
| Futoshiki | `tectonicpuzzleeu/` | 7 | 7 | ✓ |
| Kakuro | `conceptispuzzles.param` (single file) | 1 | 11 | **multi-step single file** |
| Kakuro | `kakurocom/` | 2 | 2 | ✓ |
| Kakuro | `kakurosorg/` | 3 | 3 | ✓ |
| Skyscrapers | `conceptispuzzlescom/` | 14 | 14 | ✓ |
| Skyscrapers | `tectonicpuzzel/` | 2 | — | **extra (not in paper)** |
| Skyscrapers | `brainbashers-walkthrough.param` | 1 | — | **extra (commented out in paper)** |
| Starbattle | `krazydad-tutorial.param` | 1 | 4 | **multi-step** |
| Starbattle | `tectonic-grouping-of-rows-or-columns.param`, `tectonic-row-or-col-elimination.param` | 2 | 2 | ✓ |
| Starbattle | `rohanrao-blogspot-example.param` | 1 | 14 | **multi-step** |
| Starbattle | `logicmastersindia-example.param` | 1 | 4 | **multi-step** |
| Starbattle | `FATAtalkexample.param`, `puzzle-star-battle-com/`, `star-battle-1.param` | several | — | **extras (not in paper)** |
| Tents | `tectonic-1-2-3-4-5-6-7.param`, `tectonic-8.param`, `tectonic-9.param` | 3 | 9 | **multi-step (file 1 covers steps 1–7)** |
| Thermometer | `innoludic/1-some-tips.param` | 1 | 2 | **multi-step or 1 missing?** |
| Thermometer | `tectonicpuzzel/all-techniques.eprime` | 1 (mis-named, actually a param) | 5 | **multi-step single file** |
| **Total non-sudoku files** | | **~58** | **96 steps** | needs per-file step labelling |

**Key insight:** several sources hold *one param file with multiple deduction steps*
(Kakuro/conceptis = 11 steps in one file, Tents/tectonic-1-2-3-4-5-6-7 = 7 steps,
Starbattle/rohanrao = 14 steps, etc).  The paper's 96 non-sudoku step count
expands from ~58 files via these multi-step puzzles.  At conversion time we'll
need to identify each step's target deduction; this is where source comments and
URLs become critical for matching against the original tutorial pages.

### Showcase scripts (not in paper Table 1)

`examples/*.py` (Python) — full-puzzle solve traces, not per-step tutorials:

| File | Source URL | Notes |
|---|---|---|
| `miracle-cascade.py` | (no URL in file; canonical: https://youtu.be/yKf9aUIxdb4 — Cracking the Cryptic) | Miracle Sudoku full solve |
| `thermo-no-digits-cascade.py` | https://www.youtube.com/watch?v=KTth49YrQVU | Thermometer Sudoku full solve |
| `thermo-250000-subscriptions-cascade.py` | https://www.youtube.com/watch?v=U99ZFz_X4TU | Thermometer Sudoku full solve |
| `wizard-cascade.py` | https://www.youtube.com/watch?v=QNzltTzv0fc&t=79s | Wizard / Knight's-move Sudoku full solve |
| `latimes.py` | (LA Times daily Sudoku) | CLI driver for many newspaper instances |

These are *demonstrations*, not the per-step technique benchmarks the
paper Table 1 measures.  Probably don't need to migrate but worth keeping
URLs noted somewhere.

### Things in repo that are NOT in paper Table 1

- **Killer Sudoku** — `eprime/killersudoku/solving_techniques/` (10 files).  
  Paper §6.something explicitly excludes Killer Sudoku ("we found we were
  unable to represent them … problems with implicit arithmetic reasoning").  
  Keep but mark as research-deferred.
- **Garam** — `eprime/garam/instances/`.  Not mentioned in paper.
- **Solitaire Battleship** — `eprime/solitairebattleship/{instances,solving_techniques}/`.  Not mentioned in paper.
- **Star battle extras** — `FATAtalkexample`, `puzzle-star-battle-com/`,
  `star-battle-1.param`.  Not in paper count.
- **Sudoku unique-rectangle family** (`BUG`, `extuniqrectangles`,
  `hiddenuniqrectangles`, `uniquerectangles`) — 20 files.  Paper explicitly
  excludes these from the count (note `†` in Table 1: "we exclude Unique
  Rectangle techniques").  Keep but mark.
- **Thermometer/tectonic + skyscrapers/tectonic + skyscrapers/brainbashers**
  — present in repo, listed in Python ListOfTutorials.txt and earlier paper
  comments, but not in the final Table 1.  Probably abandoned during paper
  revisions.  Keep but mark.

## Counts summary

| Bucket | Count |
|---|---:|
| Paper Table 1 expected | 160 |
| Sudoku Python `dotest` calls (matches if 11 extras excluded) | 75 |
| Sudoku param files in repo (matches if 18 unique-rect excluded) | 82 |
| Non-sudoku param files in repo (multi-step) | ~58 files / ~96 steps |
| Repo-only extras (killer, garam, solitaire, etc.) | ~25 files |

## Proposed organisation

For a clean, source-traceable layout:

```
eprime/
  <kind>/
    <kind>.eprime                            # canonical model, kept as-is
    tutorials/                               # NEW: all per-tutorial scripts
      <source>/                              # e.g. sudokuwiki, conceptispuzzles
        <id>.toml                            # walkthrough script
    instances/                               # existing — non-tutorial puzzles
    solving_techniques/                      # existing — keep as legacy archive
                                             # (later: prune once tutorials/ is complete)
  sudoku/
    sudokuwiki-domains/                      # existing — legacy flat layout
    sudokuwiki/                              # existing — by-technique flat layout
    tutorials/                               # NEW canonical home
      sudokuwiki/<id>.toml                   # one walkthrough per Python dotest
      sudokuwiki-extras/                     # Chris Bonus, unique rectangles
```

The new `tutorials/` dir uses the existing `<kind>.eprime` model files as-is
(no copies, no model edits) — each walkthrough TOML just references it.

A top-level `eprime/tutorials.toml` indexes every walkthrough with metadata
(source URL, technique, paper-table row, status), so re-running the whole
benchmark = walking that file.

## Gaps to flag for the user

1. **Sudoku-wiki.py 11 extras** — need to identify which 64 of the 75 are
   the paper-counted ones; the other 11 are presumably variant/setup tests
   that the paper rolled into the same row.  Will need access to either the
   paper supplementary materials or a manual cross-check against the
   `printrow` categories.

2. **Per-step labelling for multi-step params**.  The kakuro/conceptis,
   starbattle/rohanrao, tents/tectonic-1-7, thermometer/all-techniques files
   each cover multiple deductions in one puzzle.  We don't know which target
   literals correspond to which paper-counted steps.  Need the original
   webpages or supplementary table to map them.

3. **Thermometer/innoludic count mismatch** — repo has 1 file, paper says 2
   techniques.  Either innoludic teaches 2 techniques on one puzzle, or one
   tutorial was lost.

4. **Skyscrapers/tectonic + brainbashers** — included in repo, mentioned in
   ListOfTutorials.txt, but NOT in Paper Table 1.  Were they cut?  Worth
   keeping if they correspond to real tutorials, even if they didn't make
   the published table.

5. **Miracle / Wizard / Thermo cascade scripts** — full-solve demonstrations,
   not technique-by-technique.  If we want them in the corpus they need a
   different walkthrough form (no targeted MUS, just step-by-step with
   `show_mus` per round).

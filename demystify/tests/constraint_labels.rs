//! Checks that every `$#CON` label in the puzzle library names the cells,
//! rows and columns its clauses actually touch.
//!
//! Demystify draws a 2D variable `x[a, b]` with `a` as the row (counting from
//! 1 at the top) and `b` as the column (counting from 1 at the left), so a
//! label must write cells as `(row,col)`. For each constraint, the cells of
//! the model's `$#VAR` grids in its scope are compared with the label:
//!
//! - every `(a,b)` or `(a, b)` in the label is a cell in the scope, or lies
//!   within one cell of the scope's bounding box: anchors such as a given, a
//!   tree, a clue cell or the corner of a box are not in the scope (a given is
//!   fixed, so its literal is simplified out of the clauses) but are next to it.
//!   A label naming two or more cells must have at least one in the scope, so
//!   a label whose cells are all shifted by one is still caught;
//! - "box starting at (r,c)": every scope cell lies in the 3x3 box with that
//!   top-left corner (the corner itself is not checked as a cell);
//! - "same row" / "same column": every scope cell shares that row / column;
//! - "row N" / "column N" (or "col N"), when the label names only one kind of
//!   line: every scope cell lies in that line.
//!
//! Constraints whose scope has no `$#VAR` grid cell are not checked.

use std::collections::BTreeSet;
use std::path::PathBuf;

use demystify::problem::parse::parse_essence;
use regex::Regex;

/// One instance per model with `$#CON` labels that mention grid positions.
const MODELS: &[(&str, &str)] = &[
    ("akari.eprime", "akari/akari-3x3.param"),
    ("binairo.essence", "binairo/diiscu.param"),
    ("futoshiki.eprime", "futoshiki/nfutoshiki-2.param"),
    ("hexbinairo.eprime", "hexbinairo/hexbinairo-1.param"),
    ("kakurasu.eprime", "kakurasu/kakurasu.param"),
    ("kakuro.eprime", "kakuro/kakuro-5-5.param"),
    ("minesweeper.eprime", "minesweeper/minesweeperPrinted.param"),
    ("mosaic.eprime", "mosaic/mosaic-5x5.param"),
    ("nonogram.eprime", "nonogram/duck-8x9.param"),
    (
        "ppminesweeper.eprime",
        "ppminesweeper/puzzle-minesweeper-com/15x15-easy-07.param",
    ),
    ("skyscrapers.eprime", "skyscrapers/skyscrapers-1.param"),
    ("star-battle.eprime", "star-battle/FATAtalkexample.param"),
    (
        "star-battle-pairwise.eprime",
        "star-battle/FATAtalkexample.param",
    ),
    ("sudoku.eprime", "sudoku/redditexample.param"),
    (
        "sudoku-domains.eprime",
        "sudoku/sudokuwiki-domains/naked-pairs-example-1.param",
    ),
    ("tents.eprime", "tents/tectonic-8.param"),
    ("tents-binary.eprime", "tents/tectonic-8.param"),
    ("tents-celladj.eprime", "tents/tectonic-8.param"),
    ("thermometer.eprime", "thermometer/thermometer-1.param"),
];

/// Models too slow to parse on every test run: each takes from seconds to
/// minutes in a debug build (Killer Sudoku about 2 minutes even in release).
/// Their label patterns also occur in faster models above. Run with
/// `cargo test --release -p demystify --test constraint_labels -- --ignored`.
const SLOW_MODELS: &[(&str, &str)] = &[
    (
        "jigsaw-domains.eprime",
        "jigsaw/sudokuwiki/jigsaw-double-pointing-pair-2.param",
    ),
    ("miracle.eprime", "miracle/original.param"),
    ("sudoku-pairwise.eprime", "sudoku/redditexample.param"),
    ("x-sums.eprime", "x-sums/easy-xsums.param"),
    (
        "xsudoku-domains.eprime",
        "xsudoku/sudokuwiki/xsudoku-pointing-pair.param",
    ),
    ("killersudoku.eprime", "killersudoku/killersudoku.param"),
];

/// The web app's own copies of the models (`demystify-web/examples/eprime`),
/// checked with the instance the web app ships.
const WEB_MODELS: &[(&str, &str)] = &[
    ("akari.eprime", "akari/akari-5x5.param"),
    ("binairo.essence", "binairo/diiscu.param"),
    ("futoshiki.eprime", "futoshiki/nfutoshiki-1.param"),
    ("hexbinairo.eprime", "hexbinairo/hexbinairo-1.param"),
    ("kakurasu.eprime", "kakurasu/kakurasu.param"),
    ("minesweeper.eprime", "minesweeper/minesweeper-5x5.param"),
    ("mosaic.eprime", "mosaic/mosaic-5x5.param"),
    ("nonogram.eprime", "nonogram/duck-8x9.param"),
    ("skyscrapers.eprime", "skyscrapers/skyscrapers-1.param"),
    ("star-battle.eprime", "star-battle/FATAtalkexample.param"),
    ("thermometer.eprime", "thermometer/thermometer-1.param"),
];

const SLOW_WEB_MODELS: &[(&str, &str)] = &[
    ("killersudoku.eprime", "killersudoku/killersudoku.param"),
    ("miracle.eprime", "miracle/original.param"),
    ("sudoku.eprime", "sudoku/puzzlingexample.param"),
    ("x-sums.eprime", "x-sums/easy-xsums.param"),
];

fn eprime_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../eprime")
}

fn web_eprime_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../demystify-web/examples/eprime")
}

/// Every problem with `desc`, given the `$#VAR` grid cells in its scope.
fn label_problems(desc: &str, cells: &BTreeSet<(i64, i64)>) -> Vec<String> {
    let cell = Regex::new(r"\((\d+), ?(\d+)\)").unwrap();
    let row = Regex::new(r"(?i)\brow \(?(\d+)\)?").unwrap();
    let col = Regex::new(r"(?i)\b(?:col|column) \(?(\d+)\)?").unwrap();
    let mentions_row = Regex::new(r"(?i)\brows?\b").unwrap();
    let mentions_col = Regex::new(r"(?i)\b(?:cols?|columns?)\b").unwrap();

    let rows: BTreeSet<i64> = cells.iter().map(|&(r, _)| r).collect();
    let cols: BTreeSet<i64> = cells.iter().map(|&(_, c)| c).collect();
    let near = |(r, c): (i64, i64)| {
        rows.first().unwrap() - 1 <= r
            && r <= rows.last().unwrap() + 1
            && cols.first().unwrap() - 1 <= c
            && c <= cols.last().unwrap() + 1
    };
    let box_corner = Regex::new(r"box starting at \((\d+), ?(\d+)\)").unwrap();
    let mut problems = vec![];
    let mut checked_desc = desc.to_string();
    if let Some(b) = box_corner.captures(desc) {
        let (r0, c0): (i64, i64) = (b[1].parse().unwrap(), b[2].parse().unwrap());
        if !cells
            .iter()
            .all(|&(r, c)| (r0..r0 + 3).contains(&r) && (c0..c0 + 3).contains(&c))
        {
            problems.push(format!("names box ({r0},{c0}) but touches {cells:?}"));
        }
        checked_desc = desc.replace(&b[0], "");
    }
    let named: Vec<(i64, i64)> = cell
        .captures_iter(&checked_desc)
        .map(|c| (c[1].parse().unwrap(), c[2].parse().unwrap()))
        .collect();
    for &rc in &named {
        if !cells.contains(&rc) && !near(rc) {
            problems.push(format!(
                "names cell {rc:?}, which is not in or next to its scope"
            ));
        }
    }
    if named.len() >= 2 && !named.iter().any(|rc| cells.contains(rc)) {
        problems.push(format!(
            "names cells {named:?}, none of which is in its scope"
        ));
    }
    if desc.contains("same row") && rows.len() != 1 {
        problems.push(format!("says \"same row\" but touches rows {rows:?}"));
    }
    if desc.contains("same column") && cols.len() != 1 {
        problems.push(format!("says \"same column\" but touches columns {cols:?}"));
    }
    let one_kind = mentions_row.is_match(desc) != mentions_col.is_match(desc);
    if one_kind {
        if let Some(c) = row.captures(desc) {
            let n: i64 = c[1].parse().unwrap();
            if rows != BTreeSet::from([n]) {
                problems.push(format!("names row {n} but touches rows {rows:?}"));
            }
        }
        if let Some(c) = col.captures(desc) {
            let n: i64 = c[1].parse().unwrap();
            if cols != BTreeSet::from([n]) {
                problems.push(format!("names column {n} but touches columns {cols:?}"));
            }
        }
    }
    problems
}

#[test]
fn constraint_labels_match_their_scope() {
    check_models(&eprime_dir(), MODELS);
}

#[test]
#[ignore = "slow: run with --ignored"]
fn constraint_labels_match_their_scope_slow_models() {
    check_models(&eprime_dir(), SLOW_MODELS);
}

#[test]
fn web_constraint_labels_match_their_scope() {
    check_models(&web_eprime_dir(), WEB_MODELS);
}

#[test]
#[ignore = "slow: run with --ignored"]
fn web_constraint_labels_match_their_scope_slow_models() {
    check_models(&web_eprime_dir(), SLOW_WEB_MODELS);
}

fn check_models(dir: &std::path::Path, models: &[(&str, &str)]) {
    let mut failures = vec![];
    for (model, param) in models {
        let puz = parse_essence(&dir.join(model), &dir.join(param))
            .unwrap_or_else(|e| panic!("parsing {model} with {param}: {e:#}"));
        let grid_cell = |v: &demystify::problem::PuzVar| {
            (puz.eprime.vars.contains(v.name()) && v.indices().len() == 2)
                .then(|| (v.indices()[0], v.indices()[1]))
        };
        let mut checked = 0;
        for (lit, desc) in puz.constraints.iter() {
            let cells: BTreeSet<(i64, i64)> = puz
                .constraint_scope_for_lit(lit)
                .iter()
                .filter_map(|vv| grid_cell(vv.var()))
                .collect();
            if cells.is_empty() {
                continue;
            }
            checked += 1;
            for p in label_problems(desc, &cells) {
                failures.push(format!("{model}: \"{desc}\" {p}"));
            }
        }
        assert!(
            checked > 0,
            "{model}: no constraint touches a $#VAR grid cell"
        );
    }
    assert!(
        failures.is_empty(),
        "{} label(s) disagree with their constraint:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

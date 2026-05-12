//! End-to-end tests for loading and solving externally-generated puzzles —
//! the workflow demystify uses to ingest a `mystify` model, whose clue cells
//! are `find` variables that a `.param` file cannot assign.
//!
//! Each test parses the Essence model with `conjure`/`savilerow` (like the
//! rest of the suite), pins the generated assignment, and checks the solve.
//!
//! Fixtures live in `demystify/tst/`:
//!   - `{pp,}jigsawminesweeper.eprime` / `.param` — copied from `mystify`'s
//!     `examples/` directory (the colour-region minesweeper kinds).
//!   - `{pp,}jigsawminesweeper-instance.json` — a slimmed `mystify` output
//!     JSON (`{ "puzzle": <assignment>, "solution": { "mines": ... } }`) for
//!     one solvable seed of each.

use std::collections::BTreeSet;
use std::path::PathBuf;

use demystify::problem::parse::{mystify_puzzle_assignment, parse_essence_with_assignment};
use demystify::problem::planner::PuzzlePlanner;
use demystify::problem::solver::{PuzzleSolver, SolverConfig};
use demystify::problem::{PuzLit, PuzVar, VarValPair};
use serde_json::json;

fn tst(name: &str) -> PathBuf {
    PathBuf::from(format!("{}/tst/{}", env!("CARGO_MANIFEST_DIR"), name))
}

fn read_instance(name: &str) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(tst(name)).expect("reading instance fixture"))
        .expect("parsing instance fixture")
}

/// Parse `model`/`param` (no pinning) just to get at the model's variable
/// table, for the validation tests.
fn parse_only(stem: &str) -> PuzzleSolver {
    parse_essence_with_assignment(
        &tst(&format!("{stem}.eprime")),
        &tst(&format!("{stem}.param")),
        &json!({}),
        SolverConfig::default(),
    )
    .expect("parse should succeed")
}

/// Solve a pinned instance to completion and assert every `mines` cell was
/// deduced to the value recorded in the fixture's `solution`.
fn check_fully_solves(stem: &str, instance_file: &str, expected_cells: usize) {
    let inst = read_instance(instance_file);
    let assignment = mystify_puzzle_assignment(&inst).expect("instance has a `puzzle` object");

    let solver = parse_essence_with_assignment(
        &tst(&format!("{stem}.eprime")),
        &tst(&format!("{stem}.param")),
        assignment,
        SolverConfig::default(),
    )
    .expect("parse + pin should succeed");

    let mut planner = PuzzlePlanner::new(solver);
    let _ = planner.quick_solve();

    let known: BTreeSet<_> = planner.get_all_known_lits().iter().copied().collect();
    let mines = inst["solution"]["mines"]
        .as_object()
        .expect("solution.mines object");
    let mut checked = 0usize;
    for (r, row) in mines {
        let r: i64 = r.parse().unwrap();
        for (c, v) in row.as_object().expect("solution.mines row object") {
            let c: i64 = c.parse().unwrap();
            let v = v.as_i64().expect("mine value is an integer");
            let lit = planner
                .solver()
                .puzlit_to_lit(&PuzLit::new_eq(VarValPair::new(
                    &PuzVar::new("mines", vec![r, c]),
                    v,
                )));
            assert!(
                known.contains(&lit),
                "{stem}: mines[{r},{c}] should have been deduced to {v}"
            );
            checked += 1;
        }
    }
    assert_eq!(
        checked, expected_cells,
        "{stem}: unexpected number of cells"
    );
}

#[test]
fn ppjigsawminesweeper_pinned_instance_fully_solves() {
    // Pen-and-paper jigsaw minesweeper: static clues (`puz_grid`, -1..8) plus
    // colour regions (`puz_colour`).  5x5 fixture.
    check_fully_solves(
        "ppjigsawminesweeper",
        "ppjigsawminesweeper-instance.json",
        25,
    );
}

#[test]
fn jigsawminesweeper_pinned_instance_fully_solves() {
    // Reveal-style jigsaw minesweeper: hidden truth (`puz_mines`), initial
    // reveals (`puz_initial`), colour headers (`puz_header`).  This also
    // exercises the `$#REVEAL mines facts` mechanism after pinning — clues
    // only become active as `mines` cells are deduced safe.  5x5 fixture.
    check_fully_solves("jigsawminesweeper", "jigsawminesweeper-instance.json", 25);
}

#[test]
fn ppjigsawminesweeper_colour_regions_render_as_tints() {
    // The fixture model uses `$#SHOW puz_colour region_tint`.  Once the
    // generated `puz_colour` is pinned, the rendered SVG should tint the
    // coloured cells; with `maxColours = 2` the two colours map to the first
    // two palette entries (`#85586f`, `#d6efed` — see puzsvg's `colours_list`).
    let inst = read_instance("ppjigsawminesweeper-instance.json");
    let assignment = mystify_puzzle_assignment(&inst).unwrap();
    let solver = parse_essence_with_assignment(
        &tst("ppjigsawminesweeper.eprime"),
        &tst("ppjigsawminesweeper.param"),
        assignment,
        SolverConfig::default(),
    )
    .expect("parse + pin should succeed");

    let mut planner = PuzzlePlanner::new(solver);
    let (problem, _) = planner.refresh_problem();

    let tints = problem
        .puzzle
        .region_tint
        .as_ref()
        .expect("region_tint should be populated from the pinned puz_colour");
    let coloured = tints.iter().flatten().filter(|c| c.is_some()).count();
    assert!(coloured > 0, "fixture has coloured cells");
    // `region_tint` draws no borders, so the `cages` role must be absent.
    assert!(problem.puzzle.cages.is_none());

    let svg = demystify::web::puzsvg::PuzzleDraw::new(&problem.puzzle.kind)
        .draw_puzzle(&problem)
        .to_string();
    assert!(
        svg.contains("#85586f"),
        "colour-1 tint should appear in the SVG"
    );
    assert!(
        svg.contains("#d6efed"),
        "colour-2 tint should appear in the SVG"
    );
}

#[test]
fn pin_assignment_rejects_non_object() {
    let mut solver = parse_only("ppjigsawminesweeper");
    let err = solver.pin_assignment(&json!([1, 2, 3])).unwrap_err();
    assert!(
        err.to_string().contains("expected a JSON object"),
        "got: {err}"
    );
}

#[test]
fn pin_assignment_rejects_non_integer_leaf() {
    let mut solver = parse_only("ppjigsawminesweeper");
    let err = solver
        .pin_assignment(&json!({ "puz_grid": { "1": { "1": "oops" } } }))
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("expected nested objects ending in integers"),
        "got: {err}"
    );
}

#[test]
fn pin_assignment_rejects_unknown_variable() {
    let mut solver = parse_only("ppjigsawminesweeper");
    // Out-of-range indices name a variable element that the model does not declare.
    let err = solver
        .pin_assignment(&json!({ "puz_grid": { "99": { "99": 0 } } }))
        .unwrap_err();
    assert!(err.to_string().contains("no variable"), "got: {err}");
}

#[test]
fn pin_assignment_rejects_out_of_domain_value() {
    let mut solver = parse_only("ppjigsawminesweeper");
    // `puz_grid` has domain int(-1..8); 100 is out of range.
    let err = solver
        .pin_assignment(&json!({ "puz_grid": { "1": { "1": 100 } } }))
        .unwrap_err();
    assert!(err.to_string().contains("outside the domain"), "got: {err}");
}

#[test]
fn parse_essence_with_assignment_rejects_unsatisfiable_assignment() {
    // Pin the real puzzle plus a deliberately contradictory `mines` value: the
    // model forbids a mine on a numbered cell (`puz_grid[r,c] >= 0 -> mines[r,c] = 0`),
    // and the fixture has `puz_grid[1,2] = 5`, so `mines[1,2] = 1` is impossible.
    let inst = read_instance("ppjigsawminesweeper-instance.json");
    let mut assignment = mystify_puzzle_assignment(&inst).unwrap().clone();
    assignment["mines"] = json!({ "1": { "2": 1 } });

    let err = parse_essence_with_assignment(
        &tst("ppjigsawminesweeper.eprime"),
        &tst("ppjigsawminesweeper.param"),
        &assignment,
        SolverConfig::default(),
    )
    .err()
    .expect("contradictory assignment should be rejected");
    assert!(err.to_string().contains("unsatisfiable"), "got: {err}");
}

//! End-to-end smoke test for the walkthrough executor.
//!
//! Builds tiny TOML scripts in-memory, parses them, drives the
//! `Executor` against the `binairo` fixture, and asserts that
//! rendering steps produce the expected number of sections and
//! that errors surface with their originating step.

use std::path::PathBuf;
use std::sync::Arc;

use demystify::problem::parse::parse_essence;
use demystify::problem::planner::PuzzlePlanner;
use demystify::problem::solver::PuzzleSolver;
use demystify::walkthrough::{Executor, Script};

fn tst(name: &str) -> PathBuf {
    PathBuf::from(format!("{}/tst/{}", env!("CARGO_MANIFEST_DIR"), name))
}

/// Build a planner against the `binairo` / `binairo-1` fixture.  Row 3
/// of that fixture has whites at columns 1, 3, 5, so the half-half rule
/// forces `grid[3,2] = 0` (false) — used as a provable target by the
/// show_mus tests below.
fn build_binairo_planner() -> PuzzlePlanner {
    let parse = parse_essence(&tst("binairo.eprime"), &tst("binairo-1.param"))
        .expect("parse_essence should succeed on binairo fixture");
    let solver = PuzzleSolver::new(Arc::new(parse)).expect("solver init");
    PuzzlePlanner::new(solver)
}

#[test]
fn show_mus_renders_one_section() {
    let toml = r#"
        model = "tst/binairo.eprime"
        param = "tst/binairo-1.param"
        [[step]]
        op = "show_mus"
        lit = "grid[3,2]=0"
        title = "row 3 is already full of whites"
    "#;
    let script: Script = toml::from_str(toml).expect("script parses");

    let mut planner = build_binairo_planner();
    let mut executor = Executor::new(&mut planner);
    executor.run(&script).expect("executor runs without error");
    let sections = executor.into_sections();

    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].title, "row 3 is already full of whites");
}

#[test]
fn pin_step_renders_nothing_but_advances_state() {
    // `pin` uses add_not_provable_known_lit so it works on any lit
    // — we don't need to know what's deducible.  Then heatmap renders
    // exactly one section.
    let toml = r#"
        model = "tst/binairo.eprime"
        param = "tst/binairo-1.param"
        [[step]]
        op = "pin"
        lit = "grid[3,2]=0"
        [[step]]
        op = "heatmap"
        title = "after pinning"
    "#;
    let script: Script = toml::from_str(toml).expect("script parses");

    let mut planner = build_binairo_planner();
    let mut executor = Executor::new(&mut planner);
    executor.run(&script).expect("executor runs without error");
    let sections = executor.into_sections();

    assert_eq!(sections.len(), 1, "pin emits no section; heatmap emits one");
    assert_eq!(sections[0].title, "after pinning");
}

#[test]
fn show_smallest_muses_caps_at_max() {
    let toml = r#"
        model = "tst/binairo.eprime"
        param = "tst/binairo-1.param"
        [[step]]
        op = "show_smallest_muses"
        lit = "grid[3,2]=0"
        max = 1
    "#;
    let script: Script = toml::from_str(toml).expect("script parses");

    let mut planner = build_binairo_planner();
    let mut executor = Executor::new(&mut planner);
    executor.run(&script).expect("executor runs without error");
    let sections = executor.into_sections();

    assert!(
        sections.len() <= 1,
        "max=1 should produce at most one section, got {}",
        sections.len()
    );
}

#[test]
fn malformed_lit_surfaces_as_step_error() {
    // A malformed lit should fail parsing inside the executor — not
    // panic, not silently skip.  Catches regressions where step
    // errors are swallowed instead of bubbled up.
    let toml = r#"
        model = "tst/binairo.eprime"
        param = "tst/binairo-1.param"
        [[step]]
        op = "show_mus"
        lit = "this is not a literal"
    "#;
    let script: Script = toml::from_str(toml).expect("script parses");

    let mut planner = build_binairo_planner();
    let mut executor = Executor::new(&mut planner);
    let err = executor
        .run(&script)
        .expect_err("malformed lit should surface as an error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("step 1") || msg.contains("show_mus"),
        "error should mention which step failed: {msg}"
    );
}

#[test]
fn script_defaults_thread_through_to_planner_config() {
    // Top-level `repeats` and `strategy` in the TOML override the
    // executor's tutorial defaults.  Check that the parsed Script
    // exposes them; the actual planner mutation is covered by the
    // executor's run path under the show_mus test.
    let toml = r#"
        model = "tst/binairo.eprime"
        param = "tst/binairo-1.param"
        repeats = 7
        strategy = "cake"
        hide_untouched_candidates = true
    "#;
    let script: Script = toml::from_str(toml).expect("script parses");

    assert_eq!(script.repeats, Some(7));
    assert_eq!(script.strategy.as_deref(), Some("cake"));
    assert_eq!(script.hide_untouched_candidates, Some(true));
    assert!(script.step.is_empty());
}

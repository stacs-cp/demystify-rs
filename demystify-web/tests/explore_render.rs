//! Explore mode: clicking a cell enumerates its alternative MUSes, and the
//! solver stage lists every one by size (smallest first) as a jump button.
//!
//! Bypasses HTTP like `game_smoke.rs`, but renders the real template.

use std::path::PathBuf;
use std::sync::Arc;

use demystify::problem::{self, planner::PuzzlePlanner, solver::PuzzleSolver};
use demystify_web::util::{DEFAULT_EXPLORE_SEARCHES, ExploreState, MAX_SEARCHES, SolverSession};
use demystify_web::wrap::build_solver_stage_context_pub;

fn sudoku_session() -> SolverSession {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let puzzle = problem::parse::parse_essence(
        &root.join("eprime/sudoku.eprime"),
        &root.join("eprime/sudoku/redditexample.param"),
    )
    .expect("parse_essence");
    let solver = PuzzleSolver::new(Arc::new(puzzle)).expect("solver::new");
    let planner = PuzzlePlanner::new(solver)
        .with_database(Arc::new(demystify::named_strategy::Database::empty()));
    SolverSession::new(planner)
}

fn render_stage(session: &SolverSession, problem: &demystify::json::Problem) -> String {
    let mut tera = tera::Tera::new(&format!(
        "{}/templates/**/*.html",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("templates parse");
    tera.autoescape_on(vec![]);
    let ctx = build_solver_stage_context_pub(problem, 0, session);
    tera.render("partials/solver_stage.html", &ctx)
        .expect("solver stage renders")
}

#[test]
fn explore_lists_every_alternative_by_size() {
    let mut session = sudoku_session();

    // A cell whose explanations are all larger than size 1, so there are
    // genuinely different alternatives to enumerate.
    let all_muses = session
        .planner
        .all_muses_for_literal_with_repeats(vec![4, 2, 1], 30);
    assert!(
        all_muses.len() > 1,
        "expected several alternatives, got {}",
        all_muses.len()
    );
    let sizes: Vec<usize> = all_muses.iter().map(|m| m.mus_len()).collect();
    assert!(sizes.is_sorted(), "alternatives not sorted: {sizes:?}");

    let current = 2;
    let problem = session.planner.preview_mus(&all_muses[current]);
    session.explore = Some(ExploreState {
        all_muses,
        current_index: current,
    });

    let html = render_stage(&session, &problem);

    assert_eq!(
        html.matches("<button class=\"he-explore-size").count(),
        sizes.len(),
        "one jump button per alternative"
    );
    assert_eq!(html.matches("he-explore-size current").count(), 1);
    let current_button = format!(
        "title=\"Explanation {}: size {}\"",
        current + 1,
        sizes[current]
    );
    let at = html.find(&current_button).expect("current button present");
    let start = html[..at].rfind("<button").unwrap();
    assert!(
        html[start..at].contains("current"),
        "button {current_button} should be marked current"
    );
    assert!(html.contains(&format!("\"X-Explore-Index\": \"{current}\"")));
}

/// The search-effort settings reach the planner, survive restoring a history
/// snapshot (which replaces the planner and its config), reject out-of-range
/// values without changing anything, and show in the rail.
#[test]
fn search_settings_apply_persist_and_render() {
    let mut session = sudoku_session();
    let default_step = session.step_searches();
    assert_eq!(session.explore_searches(), DEFAULT_EXPLORE_SEARCHES);

    session.snapshot();
    session.set_searches(7, 55).expect("valid settings");
    assert_eq!(session.planner.config_mut().mus_config.repeats, 7);

    // Snapshot 0 was taken with the default; restoring it must keep 7.
    session.goto_step(0).expect("goto");
    assert_eq!(session.planner.config_mut().mus_config.repeats, 7);
    assert_eq!(session.explore_searches(), 55);

    for (step, explore) in [(0, 30), (5, 0), (MAX_SEARCHES + 1, 30), (-1, 30)] {
        assert!(session.set_searches(step, explore).is_err());
        assert_eq!(session.step_searches(), 7);
        assert_eq!(session.explore_searches(), 55);
        assert_eq!(session.planner.config_mut().mus_config.repeats, 7);
    }
    assert_ne!(default_step, 7, "test should change the setting");

    let (problem, _) = session.planner.refresh_problem();
    let html = render_stage(&session, &problem);
    assert!(html.contains(
        r#"name="step_searches" min="1" max="1000" step="1"
                 value="7""#
    ));
    assert!(html.contains(
        r#"name="explore_searches" min="1" max="1000" step="1"
                 value="55""#
    ));
}

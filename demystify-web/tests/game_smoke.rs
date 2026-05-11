//! Smoke test: every level in the catalogue parses cleanly, exposes
//! provable literals at start, accepts a correct click and rejects the
//! same click on a second attempt (the "undeducible literal" path that
//! game mode relies on for both wrong-sign clicks and Minesweeper-style
//! mid-puzzle uncertainty).
//!
//! This intentionally bypasses HTTP — the router wiring in `serve.rs` is
//! a few lines and verified by hand. What this catches is level-file
//! breakage (e.g. a `.param` that no longer parses, or that the bundled
//! eprime/essence still produces a non-trivial puzzle).

use std::sync::Arc;

use demystify_web::game::{
    self, ClickOutcome, LEVELS, apply_click, compute_heatmap_tiers, find_level,
    level_temp_dir_for_test,
};
use demystify_web::util::GameState;

use demystify::problem::{self, planner::PuzzlePlanner, solver::PuzzleSolver};

fn planner_for(level: &game::LevelInfo) -> PuzzlePlanner {
    let (_tmp, model, param) =
        level_temp_dir_for_test(level).expect("level temp dir should construct");
    let puzzle = problem::parse::parse_essence(&model, &param).expect("parse_essence");
    let puzzle = Arc::new(puzzle);
    let solver = PuzzleSolver::new(puzzle).expect("solver::new");
    PuzzlePlanner::new(solver).with_database(Arc::new(demystify::named_strategy::Database::empty()))
}

#[test]
fn levels_catalogue_is_non_empty_and_well_formed() {
    assert!(!LEVELS.is_empty());
    for l in LEVELS {
        assert!(!l.id.is_empty());
        assert!(!l.name.is_empty());
        assert!(!l.difficulty.is_empty());
        assert!(!l.model.is_empty());
        assert!(!l.param.is_empty());
        assert!(find_level(l.id).is_some());
    }
}

#[test]
fn every_level_parses_and_has_provable_literals() {
    for level in LEVELS {
        let mut planner = planner_for(level);
        let provable_count = planner.solver().get_provable_varlits().len();
        assert!(
            provable_count > 0,
            "level '{}' has no provable literals at start",
            level.id
        );
    }
}

#[test]
fn every_level_supports_correct_click_then_rejects_repeat() {
    for level in LEVELS {
        let mut planner = planner_for(level);
        let mut game = GameState::new(level.id.to_string());

        // Pick the first provable lit's representative puzlit.
        let provable = planner.solver().get_provable_varlits().clone();
        let lit = *provable.iter().next().unwrap();
        let puzlit = planner
            .solver()
            .lit_to_puzlit(&lit)
            .iter()
            .next()
            .expect("lit has at least one puzlit")
            .clone();
        let mut idx = puzlit.var().indices().clone();
        idx.push(puzlit.val());
        let sign = puzlit.sign();

        let first = apply_click(&mut planner, &mut game, &idx, sign);
        assert!(
            matches!(first, ClickOutcome::Accepted | ClickOutcome::Won),
            "level '{}' first click should be accepted",
            level.id
        );
        assert_eq!(game.failures, 0);

        // The same click again is no longer deducible — must reject.
        let second = apply_click(&mut planner, &mut game, &idx, sign);
        assert_eq!(
            second,
            ClickOutcome::Rejected,
            "level '{}' repeat click should be rejected",
            level.id
        );
        assert_eq!(game.failures, 1);
    }
}

#[test]
fn every_level_computes_heatmap_tiers_without_panic() {
    for level in LEVELS {
        let mut planner = planner_for(level);
        let tiers = compute_heatmap_tiers(&mut planner);
        // Disjoint tier sets.
        for a in tiers.tier1.iter() {
            assert!(!tiers.tier2.contains(a));
            assert!(!tiers.tier3.contains(a));
        }
    }
}

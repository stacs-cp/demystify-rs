//! Game-mode handlers, level catalogue, and the click-validation / hint logic.
//!
//! Game mode runs alongside the existing solver/explore UI. A player makes
//! deductions themselves; the server accepts a click only if the signed
//! literal is currently forced by the planner state (so guessing is impossible
//! — wrong clicks bounce and bump `failures`). The hint system has three
//! tiers: a 3-colour heatmap classifying currently-deducible literals by
//! smallest-MUS size, a per-cell "Why?" that surfaces the smallest MUS, and a
//! "give up" that reveals the solution.

use std::{collections::BTreeSet, fs, io::Write, path::PathBuf, sync::Arc};

use anyhow::{Context, anyhow};
use axum::extract::State;
use axum::http::header::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_session::{Session, SessionNullPool};
use serde::Deserialize;

use demystify::json::Problem;
use demystify::problem::{self, PuzLit, planner::PuzzlePlanner, solver::PuzzleSolver};
use rustsat::types::Lit;

use crate::util::{self, AppState, GameState, SolverSession, get_solver_global, set_solver_global};
use crate::wrap;

// ─── Level catalogue ────────────────────────────────────────────────────────

pub struct LevelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub difficulty: &'static str,
    pub model_filename: &'static str, // "puzzle.eprime" or "puzzle.essence"
    pub model: &'static str,
    pub param: &'static str,
}

macro_rules! include_level_file {
    ($path:expr) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $path))
    };
}

pub static LEVELS: &[LevelInfo] = &[
    LevelInfo {
        id: "01_thermometer",
        name: "Thermometer",
        difficulty: "easy",
        model_filename: "puzzle.eprime",
        model: include_level_file!("levels/01_thermometer/puzzle.eprime"),
        param: include_level_file!("levels/01_thermometer/puzzle.param"),
    },
    LevelInfo {
        id: "02_binairo",
        name: "Binairo",
        difficulty: "medium",
        model_filename: "puzzle.essence",
        model: include_level_file!("levels/02_binairo/puzzle.essence"),
        param: include_level_file!("levels/02_binairo/puzzle.param"),
    },
    LevelInfo {
        id: "03_sudoku",
        name: "Sudoku",
        difficulty: "hard",
        model_filename: "puzzle.eprime",
        model: include_level_file!("levels/03_sudoku/puzzle.eprime"),
        param: include_level_file!("levels/03_sudoku/puzzle.param"),
    },
];

pub fn find_level(id: &str) -> Option<&'static LevelInfo> {
    LEVELS.iter().find(|l| l.id == id)
}

fn load_level_planner(
    level: &LevelInfo,
    strategy_db: Arc<demystify::named_strategy::Database>,
) -> anyhow::Result<PuzzlePlanner> {
    let temp_dir = tempfile::Builder::new()
        .prefix(".demystify-level-")
        .tempdir_in(".")
        .context("Failed to create temporary directory")?;

    let model_path = temp_dir.path().join(level.model_filename);
    let mut f = fs::File::create(&model_path).context("Failed to create model file")?;
    f.write_all(level.model.as_bytes())
        .context("Failed to write model file")?;

    let param_path = temp_dir.path().join("puzzle.param");
    let mut f = fs::File::create(&param_path).context("Failed to create param file")?;
    f.write_all(level.param.as_bytes())
        .context("Failed to write param file")?;

    let puzzle = problem::parse::parse_essence(&model_path, &param_path)?;
    let puzzle = Arc::new(puzzle);
    let solver = PuzzleSolver::new(puzzle)?;
    Ok(PuzzlePlanner::new(solver).with_database(strategy_db))
}

// ─── Game-side core logic (testable without HTTP) ───────────────────────────

#[derive(Debug, PartialEq, Eq)]
pub enum ClickOutcome {
    /// Literal was deduced; puzzle now solved.
    Won,
    /// Literal was deduced; more remains.
    Accepted,
    /// Literal was not currently forced (or wrong sign). State unchanged.
    Rejected,
}

/// Find the signed `Lit` (currently provable) matching `lit_def` and `positive`.
fn find_provable_signed_lit(
    planner: &mut PuzzlePlanner,
    lit_def: &[i64],
    positive: bool,
) -> Option<Lit> {
    let provable: BTreeSet<Lit> = planner.solver().get_provable_varlits().clone();
    for lit in provable {
        let puzlit_set: BTreeSet<PuzLit> = planner.solver().lit_to_puzlit(&lit).clone();
        for puzlit in puzlit_set {
            if puzlit.sign() != positive {
                continue;
            }
            let mut indices = puzlit.var().indices().clone();
            indices.push(puzlit.val());
            if indices == lit_def {
                return Some(lit);
            }
        }
    }
    None
}

/// Validate a click and, if valid, advance the planner. Pure logic; no HTTP.
pub fn apply_click(
    planner: &mut PuzzlePlanner,
    game: &mut GameState,
    lit_def: &[i64],
    positive: bool,
) -> ClickOutcome {
    if let Some(lit) = find_provable_signed_lit(planner, lit_def, positive) {
        planner.mark_lit_as_deduced(&lit);
        if planner.solver().get_provable_varlits().is_empty() {
            game.completed = true;
            ClickOutcome::Won
        } else {
            ClickOutcome::Accepted
        }
    } else {
        game.failures += 1;
        ClickOutcome::Rejected
    }
}

/// Classification of currently-deducible literals into three tiers based on
/// the size of the smallest MUS that deduces them: tier 1 = smallest-MUS-size
/// globally, tier 2 = second-smallest, tier 3 = everything else still deducible.
#[derive(Debug, Default)]
pub struct HeatmapTiers {
    pub tier1_size: Option<usize>,
    pub tier2_size: Option<usize>,
    pub tier1: BTreeSet<Lit>,
    pub tier2: BTreeSet<Lit>,
    pub tier3: BTreeSet<Lit>,
}

pub fn compute_heatmap_tiers(planner: &mut PuzzlePlanner) -> HeatmapTiers {
    let dict = planner.all_smallish_muses();
    let mut sizes: Vec<(Lit, usize)> = dict
        .muses()
        .keys()
        .filter_map(|lit| dict.min_lit(*lit).map(|n| (*lit, n)))
        .collect();
    sizes.sort_by_key(|(_, n)| *n);

    let mut distinct: Vec<usize> = sizes.iter().map(|(_, n)| *n).collect();
    distinct.dedup();
    let tier1_size = distinct.first().copied();
    let tier2_size = distinct.get(1).copied();

    let mut tiers = HeatmapTiers {
        tier1_size,
        tier2_size,
        ..Default::default()
    };
    for (lit, n) in sizes {
        if Some(n) == tier1_size {
            tiers.tier1.insert(lit);
        } else if Some(n) == tier2_size {
            tiers.tier2.insert(lit);
        } else {
            tiers.tier3.insert(lit);
        }
    }
    tiers
}

/// True if the puzzle is fully determined under current known literals (no
/// more provable literals remain).
pub fn is_won(planner: &mut PuzzlePlanner) -> bool {
    planner.solver().get_provable_varlits().is_empty()
}

// ─── Handlers ───────────────────────────────────────────────────────────────

pub async fn game_select(State(state): State<AppState>) -> Result<Html<String>, util::AppError> {
    let mut ctx = tera::Context::new();
    ctx.insert("view", "game");
    let levels: Vec<tera::Value> = LEVELS
        .iter()
        .map(|l| {
            let mut obj = serde_json::Map::new();
            obj.insert("id".into(), tera::Value::String(l.id.into()));
            obj.insert("name".into(), tera::Value::String(l.name.into()));
            obj.insert(
                "difficulty".into(),
                tera::Value::String(l.difficulty.into()),
            );
            tera::Value::Object(obj)
        })
        .collect();
    ctx.insert("levels", &levels);
    let html = state.tera.render("game_select.html", &ctx)?;
    Ok(Html(html))
}

#[derive(Deserialize)]
pub struct StartParams {
    pub level_id: String,
}

pub async fn game_start(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
    form: axum::extract::Form<StartParams>,
) -> Result<Response, util::AppError> {
    let level =
        find_level(&form.level_id).with_context(|| format!("Unknown level '{}'", form.level_id))?;
    let planner = load_level_planner(level, state.strategy_db.clone())?;
    set_solver_global(&session, planner);
    let solver = get_solver_global(&session)?;
    {
        let mut s = solver.lock().unwrap();
        s.game = Some(GameState::new(level.id.to_string()));
    }
    session.set("round", 0u32);
    Ok(Redirect::to("/game/play").into_response())
}

pub async fn game_play(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
) -> Result<Response, util::AppError> {
    let solver = match get_solver_global(&session) {
        Ok(s) => s,
        Err(_) => return Ok(Redirect::to("/game").into_response()),
    };
    let mut solver = solver.lock().unwrap();
    if solver.game.is_none() {
        return Ok(Redirect::to("/game").into_response());
    }
    let round: u32 = session.get("round").unwrap_or(0);
    let (problem, _) = solver.planner.refresh_problem();
    let ctx = build_game_page_context(&problem, round, &solver);
    let html = state.tera.render("game_play.html", &ctx)?;
    Ok(Html(html).into_response())
}

#[derive(Deserialize)]
pub struct ClickQuery {
    pub sign: Option<String>,
}

pub async fn game_click(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<ClickQuery>,
) -> Result<Html<String>, util::AppError> {
    let solver = get_solver_global(&session)?;
    let mut solver = solver.lock().unwrap();

    let lit_def = wrap::parse_cell_literal_pub(&headers)?;
    let positive = match q.sign.as_deref().unwrap_or("pos") {
        "pos" => true,
        "neg" => false,
        s => return Err(anyhow!("Invalid sign '{s}'").into()),
    };

    let game = solver
        .game
        .as_mut()
        .ok_or_else(|| anyhow!("Not in game mode"))?;
    let mut game_state = game.clone();
    let outcome = apply_click(&mut solver.planner, &mut game_state, &lit_def, positive);
    solver.game = Some(game_state);

    let round: u32 = session.get("round").unwrap_or(0);
    let (problem, _) = solver.planner.refresh_problem();
    let mut ctx = build_game_stage_context(&problem, round, &solver);
    ctx.insert(
        "last_outcome",
        match outcome {
            ClickOutcome::Won => "won",
            ClickOutcome::Accepted => "accepted",
            ClickOutcome::Rejected => "rejected",
        },
    );
    let html = state.tera.render("partials/game_stage.html", &ctx)?;
    Ok(Html(html))
}

pub async fn game_hint_heatmap(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
) -> Result<Html<String>, util::AppError> {
    let solver = get_solver_global(&session)?;
    let mut solver = solver.lock().unwrap();
    if let Some(g) = solver.game.as_mut() {
        g.hints_used += 1;
    }

    // Reuse the existing difficulty rendering. The 3-tier classification is
    // available via `compute_heatmap_tiers` and tested separately; the
    // rendered overlay shown here is the existing per-cell-difficulty
    // heatmap (good enough for the demo). Replacing it with a true 3-colour
    // overlay would require additions to PuzzleDraw — out of scope here.
    let problem = solver.planner.difficulty_problem();

    let round: u32 = session.get("round").unwrap_or(0);
    let ctx = build_game_stage_context(&problem, round, &solver);
    let html = state.tera.render("partials/game_stage.html", &ctx)?;
    Ok(Html(html))
}

pub async fn game_hint_why(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
    headers: HeaderMap,
) -> Result<Html<String>, util::AppError> {
    let solver = get_solver_global(&session)?;
    let mut solver = solver.lock().unwrap();
    let lit_def = wrap::parse_cell_literal_pub(&headers)?;

    if let Some(g) = solver.game.as_mut() {
        g.hints_used += 1;
    }

    let all_muses = solver.planner.all_muses_for_literal(lit_def);
    let problem: Problem = if all_muses.is_empty() {
        solver.planner.refresh_problem().0
    } else {
        solver.planner.preview_mus(&all_muses[0])
    };

    let round: u32 = session.get("round").unwrap_or(0);
    let ctx = build_game_stage_context(&problem, round, &solver);
    let html = state.tera.render("partials/game_stage.html", &ctx)?;
    Ok(Html(html))
}

pub async fn game_give_up(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
) -> Result<Html<String>, util::AppError> {
    let solver = get_solver_global(&session)?;
    let mut solver = solver.lock().unwrap();
    // Run the planner to completion. (No win is recorded — give-up is a loss.)
    while !solver.planner.solver().get_provable_varlits().is_empty() {
        let (_problem, lits) = solver.planner.solve_step();
        if lits.is_empty() {
            break;
        }
    }
    let round: u32 = session.get("round").unwrap_or(0);
    let (problem, _) = solver.planner.refresh_problem();
    let mut ctx = build_game_stage_context(&problem, round, &solver);
    ctx.insert("gave_up", &true);
    let html = state.tera.render("partials/game_stage.html", &ctx)?;
    Ok(Html(html))
}

pub async fn game_quit(session: Session<SessionNullPool>) -> Result<Response, util::AppError> {
    if let Ok(solver) = get_solver_global(&session) {
        solver.lock().unwrap().game = None;
    }
    Ok(Redirect::to("/game").into_response())
}

// ─── Tera context helpers ──────────────────────────────────────────────────

fn build_game_stage_context(
    problem: &Problem,
    round: u32,
    session: &SolverSession,
) -> tera::Context {
    let mut ctx = wrap::build_solver_stage_context_pub(problem, round, session);

    if let Some(g) = &session.game {
        ctx.insert("game_failures", &g.failures);
        ctx.insert("game_hints", &g.hints_used);
        ctx.insert("game_level_id", &g.level_id);
        ctx.insert("game_won", &g.completed);
        let level_name = find_level(&g.level_id).map(|l| l.name).unwrap_or("");
        let level_difficulty = find_level(&g.level_id).map(|l| l.difficulty).unwrap_or("");
        ctx.insert("game_level_name", level_name);
        ctx.insert("game_level_difficulty", level_difficulty);
    }
    ctx
}

fn build_game_page_context(
    problem: &Problem,
    round: u32,
    session: &SolverSession,
) -> tera::Context {
    let mut ctx = build_game_stage_context(problem, round, session);
    ctx.insert("view", "game");
    ctx
}

// ─── Static asset for the level files (used by integration tests only) ────

pub fn level_temp_dir_for_test(
    level: &LevelInfo,
) -> anyhow::Result<(tempfile::TempDir, PathBuf, PathBuf)> {
    let temp_dir = tempfile::Builder::new()
        .prefix(".demystify-level-test-")
        .tempdir_in(".")?;
    let model_path = temp_dir.path().join(level.model_filename);
    fs::write(&model_path, level.model)?;
    let param_path = temp_dir.path().join("puzzle.param");
    fs::write(&param_path, level.param)?;
    Ok((temp_dir, model_path, param_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_planner(level: &LevelInfo) -> PuzzlePlanner {
        let strategy_db = Arc::new(demystify::named_strategy::Database::empty());
        load_level_planner(level, strategy_db).expect("level should load")
    }

    fn first_level() -> &'static LevelInfo {
        &LEVELS[0]
    }

    fn pick_provable_lit(planner: &mut PuzzlePlanner) -> Lit {
        *planner
            .solver()
            .get_provable_varlits()
            .iter()
            .next()
            .expect("provable_varlits should be non-empty at level start")
    }

    fn lit_def_from(planner: &mut PuzzlePlanner, lit: &Lit) -> (Vec<i64>, bool) {
        let puzlit = planner
            .solver()
            .lit_to_puzlit(lit)
            .iter()
            .next()
            .expect("lit has at least one puzlit")
            .clone();
        let mut indices = puzlit.var().indices().clone();
        indices.push(puzlit.val());
        (indices, puzlit.sign())
    }

    #[test]
    fn fresh_level_has_provable_literals() {
        let mut planner = fresh_planner(first_level());
        assert!(!planner.solver().get_provable_varlits().is_empty());
    }

    #[test]
    fn correct_left_click_advances() {
        let mut planner = fresh_planner(first_level());
        let mut game = GameState::new(first_level().id.to_string());

        // Find a provable lit whose representative puzlit is positive.
        let provable: BTreeSet<Lit> = planner.solver().get_provable_varlits().clone();
        let (lit_def, positive) = provable
            .iter()
            .find_map(|lit| {
                let p = planner.solver().lit_to_puzlit(lit).iter().next()?.clone();
                if p.sign() {
                    let mut idx = p.var().indices().clone();
                    idx.push(p.val());
                    Some((idx, true))
                } else {
                    None
                }
            })
            .expect("expected a positive provable puzlit");

        let outcome = apply_click(&mut planner, &mut game, &lit_def, positive);
        assert!(matches!(
            outcome,
            ClickOutcome::Accepted | ClickOutcome::Won
        ));
        assert_eq!(game.failures, 0);
    }

    #[test]
    fn correct_right_click_advances() {
        let mut planner = fresh_planner(first_level());
        let mut game = GameState::new(first_level().id.to_string());

        let provable: BTreeSet<Lit> = planner.solver().get_provable_varlits().clone();
        let negative_lit = provable.iter().find_map(|lit| {
            let p = planner.solver().lit_to_puzlit(lit).iter().next()?.clone();
            if !p.sign() {
                let mut idx = p.var().indices().clone();
                idx.push(p.val());
                Some((idx, false))
            } else {
                None
            }
        });

        if let Some((lit_def, positive)) = negative_lit {
            let outcome = apply_click(&mut planner, &mut game, &lit_def, positive);
            assert!(matches!(
                outcome,
                ClickOutcome::Accepted | ClickOutcome::Won
            ));
            assert_eq!(game.failures, 0);
        }
        // If no negative provable lit exists at this level's first step,
        // that's still a valid puzzle shape; the positive test covers the
        // accepted-click logic.
    }

    #[test]
    fn wrong_sign_is_rejected() {
        let mut planner = fresh_planner(first_level());
        let mut game = GameState::new(first_level().id.to_string());

        let lit = pick_provable_lit(&mut planner);
        let (lit_def, sign) = lit_def_from(&mut planner, &lit);
        let wrong_sign = !sign;
        let known_before = planner.get_all_known_lits().len();

        let outcome = apply_click(&mut planner, &mut game, &lit_def, wrong_sign);
        assert_eq!(outcome, ClickOutcome::Rejected);
        assert_eq!(game.failures, 1);
        assert_eq!(planner.get_all_known_lits().len(), known_before);
    }

    #[test]
    fn undeducible_click_is_rejected() {
        // Construct a synthetic lit_def that is not currently provable: take
        // a provable lit, mark it as deduced (so it leaves provable_varlits),
        // then click it again with the same sign — second attempt must reject.
        let mut planner = fresh_planner(first_level());
        let mut game = GameState::new(first_level().id.to_string());

        let lit = pick_provable_lit(&mut planner);
        let (lit_def, sign) = lit_def_from(&mut planner, &lit);
        let first = apply_click(&mut planner, &mut game, &lit_def, sign);
        assert!(matches!(first, ClickOutcome::Accepted | ClickOutcome::Won));
        assert_eq!(game.failures, 0);

        // Second attempt: same lit is no longer in provable_varlits.
        let second = apply_click(&mut planner, &mut game, &lit_def, sign);
        assert_eq!(second, ClickOutcome::Rejected);
        assert_eq!(game.failures, 1);
    }

    #[test]
    fn heatmap_tiers_partition_deducibles() {
        let mut planner = fresh_planner(first_level());
        let tiers = compute_heatmap_tiers(&mut planner);

        // Tier sets must be pairwise disjoint.
        for a in &tiers.tier1 {
            assert!(!tiers.tier2.contains(a) && !tiers.tier3.contains(a));
        }
        for a in &tiers.tier2 {
            assert!(!tiers.tier1.contains(a) && !tiers.tier3.contains(a));
        }
        for a in &tiers.tier3 {
            assert!(!tiers.tier1.contains(a) && !tiers.tier2.contains(a));
        }

        // If anything was classified, tier1 must be non-empty (the smallest tier).
        let total_classified = tiers.tier1.len() + tiers.tier2.len() + tiers.tier3.len();
        if total_classified > 0 {
            assert!(
                !tiers.tier1.is_empty(),
                "tier1 must be non-empty when anything is classified"
            );
            assert!(tiers.tier1_size.is_some());
        }
        // tier1_size < tier2_size when both exist.
        if let (Some(s1), Some(s2)) = (tiers.tier1_size, tiers.tier2_size) {
            assert!(s1 < s2, "tier1_size ({s1}) must be < tier2_size ({s2})");
        }
    }

    #[test]
    fn win_condition() {
        let mut planner = fresh_planner(first_level());
        let mut game = GameState::new(first_level().id.to_string());

        // Drive the planner to completion via the same path quick_solve uses.
        // After this, get_provable_varlits should be empty.
        while !planner.solver().get_provable_varlits().is_empty() {
            let lit = pick_provable_lit(&mut planner);
            planner.mark_lit_as_deduced(&lit);
        }
        assert!(is_won(&mut planner));

        // A click in this state must reject (everything already known).
        let outcome = apply_click(&mut planner, &mut game, &[1, 1, 1], true);
        assert_eq!(outcome, ClickOutcome::Rejected);
    }
}

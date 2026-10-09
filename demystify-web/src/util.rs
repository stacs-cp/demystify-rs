use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::bail;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

use axum_session::{Session, SessionNullPool};
use demystify::{
    problem::{musdict::MusContext, planner::PuzzlePlanner},
    snapshot::{SessionSnapshot, export_snapshot, import_snapshot},
};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub tera: Arc<tera::Tera>,
    pub strategy_db: Arc<demystify::named_strategy::Database>,
}

pub struct ExploreState {
    pub all_muses: Vec<MusContext>,
    pub current_index: usize,
}

/// Randomised MUS searches per click in explore mode, until the user changes
/// it. Explore exists to show alternative explanations, so it searches much
/// harder than a solve step.
pub const DEFAULT_EXPLORE_SEARCHES: i64 = 30;

/// Upper limit on either search setting. Cost is roughly linear in searches
/// (30 explore searches take up to ~1.2s per click on `miracle`), so this
/// keeps a typo from tying up the server for minutes.
pub const MAX_SEARCHES: i64 = 1000;

pub struct SolverSession {
    pub planner: PuzzlePlanner,
    pub explore: Option<ExploreState>,
    pub explore_enabled: bool,
    pub history: Vec<PuzzlePlanner>,
    pub game: Option<GameState>,
    /// Randomised MUS searches per literal on each solve step. Owned here, not
    /// only by `planner`, because restoring a history snapshot replaces the
    /// planner (and its config) wholesale.
    step_searches: i64,
    /// Randomised MUS searches per literal when explore mode explains a cell.
    explore_searches: i64,
}

#[derive(Debug, Clone)]
pub struct GameState {
    pub level_id: String,
    pub failures: u32,
    pub hints_used: u32,
    pub completed: bool,
}

impl GameState {
    pub fn new(level_id: String) -> Self {
        Self {
            level_id,
            failures: 0,
            hints_used: 0,
            completed: false,
        }
    }
}

impl SolverSession {
    pub fn new(mut planner: PuzzlePlanner) -> Self {
        let snapshot = planner
            .fork()
            .expect("Failed to fork initial planner state");
        let step_searches = planner.config_mut().mus_config.repeats;
        Self {
            planner,
            explore: None,
            explore_enabled: false,
            history: vec![snapshot],
            game: None,
            step_searches,
            explore_searches: DEFAULT_EXPLORE_SEARCHES,
        }
    }

    pub fn step_searches(&self) -> i64 {
        self.step_searches
    }

    pub fn explore_searches(&self) -> i64 {
        self.explore_searches
    }

    /// Set how hard solve steps and explore clicks search for MUSes. Takes
    /// effect from the next step or click; explanations already shown are kept.
    pub fn set_searches(&mut self, step: i64, explore: i64) -> anyhow::Result<()> {
        for (name, n) in [("step", step), ("explore", explore)] {
            anyhow::ensure!(
                (1..=MAX_SEARCHES).contains(&n),
                "{name} searches must be between 1 and {MAX_SEARCHES}, got {n}"
            );
        }
        self.step_searches = step;
        self.explore_searches = explore;
        self.planner.config_mut().mus_config.repeats = step;
        Ok(())
    }

    pub fn snapshot(&mut self) {
        let snapshot = self.planner.fork().expect("Failed to fork planner state");
        self.history.push(snapshot);
    }

    pub fn goto_step(&mut self, step: usize) -> anyhow::Result<()> {
        anyhow::ensure!(step < self.history.len(), "Step {step} out of range");
        self.planner = self.history[step].fork()?;
        self.planner.config_mut().mus_config.repeats = self.step_searches;
        self.history.truncate(step + 1);
        self.explore = None;
        self.explore_enabled = false;
        Ok(())
    }
}

pub struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

fn solver_global(
    uuid: Uuid,
    set_solver: Option<Arc<Mutex<SolverSession>>>,
) -> Option<Arc<Mutex<SolverSession>>> {
    type GlobalPuzzleStorage = Mutex<HashMap<Uuid, Arc<Mutex<SolverSession>>>>;
    static SOLVER: OnceLock<GlobalPuzzleStorage> = OnceLock::new();
    let m = SOLVER.get_or_init(|| Mutex::new(HashMap::new()));

    if let Some(solver) = set_solver {
        m.lock().unwrap().insert(uuid, solver);
        None
    } else {
        m.lock().unwrap().get(&uuid).cloned()
    }
}

pub fn get_solver_global(
    session: &Session<SessionNullPool>,
) -> anyhow::Result<Arc<Mutex<SolverSession>>> {
    let uuid = session.get_session_id().uuid();
    let solver = solver_global(uuid, None);
    if let Some(solver) = solver {
        Ok(solver)
    } else {
        bail!("No solver -- have you uploaded files?");
    }
}

pub fn set_solver_global(session: &Session<SessionNullPool>, set_solver: PuzzlePlanner) {
    let uuid = session.get_session_id().uuid();
    solver_global(
        uuid,
        Some(Arc::new(Mutex::new(SolverSession::new(set_solver)))),
    );
}

pub fn set_solver_global_session(
    session: &Session<SessionNullPool>,
    solver_session: SolverSession,
) {
    let uuid = session.get_session_id().uuid();
    solver_global(uuid, Some(Arc::new(Mutex::new(solver_session))));
}

// ─── Export / import ───
//
// Snapshot data structures and the JSON-replay logic live in
// `demystify::snapshot`. Here we expose thin web-shaped wrappers around them:
// `SolverSession::export_snapshot` works on the session's `history`, and
// `import_snapshot_into_session` rebuilds a `SolverSession` from a replayed
// planner-history.

impl SolverSession {
    pub fn export_snapshot(&self) -> anyhow::Result<SessionSnapshot> {
        export_snapshot(&self.history)
    }
}

pub fn import_snapshot_into_session(
    snapshot: SessionSnapshot,
    strategy_db: Arc<demystify::named_strategy::Database>,
) -> anyhow::Result<SolverSession> {
    let history = import_snapshot(snapshot, strategy_db)?;
    let mut live = history
        .last()
        .expect("import_snapshot returns at least one entry")
        .fork()?;
    let step_searches = live.config_mut().mus_config.repeats;
    Ok(SolverSession {
        planner: live,
        explore: None,
        explore_enabled: false,
        history,
        game: None,
        step_searches,
        explore_searches: DEFAULT_EXPLORE_SEARCHES,
    })
}

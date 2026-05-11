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

pub struct SolverSession {
    pub planner: PuzzlePlanner,
    pub explore: Option<ExploreState>,
    pub explore_enabled: bool,
    pub history: Vec<PuzzlePlanner>,
    pub game: Option<GameState>,
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
    pub fn new(planner: PuzzlePlanner) -> Self {
        let snapshot = planner
            .fork()
            .expect("Failed to fork initial planner state");
        Self {
            planner,
            explore: None,
            explore_enabled: false,
            history: vec![snapshot],
            game: None,
        }
    }

    pub fn snapshot(&mut self) {
        let snapshot = self.planner.fork().expect("Failed to fork planner state");
        self.history.push(snapshot);
    }

    pub fn goto_step(&mut self, step: usize) -> anyhow::Result<()> {
        anyhow::ensure!(step < self.history.len(), "Step {step} out of range");
        self.planner = self.history[step].fork()?;
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
    let live = history
        .last()
        .expect("import_snapshot returns at least one entry")
        .fork()?;
    Ok(SolverSession {
        planner: live,
        explore: None,
        explore_enabled: false,
        history,
        game: None,
    })
}

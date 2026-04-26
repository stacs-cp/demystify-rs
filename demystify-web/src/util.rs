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
use demystify::problem::{musdict::MusContext, planner::PuzzlePlanner};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub tera: Arc<tera::Tera>,
}

pub struct ExploreState {
    pub all_muses: Vec<MusContext>,
    pub current_index: usize,
}

pub struct SolverSession {
    pub planner: PuzzlePlanner,
    pub explore: Option<ExploreState>,
    pub explore_enabled: bool,
}

impl SolverSession {
    pub fn new(planner: PuzzlePlanner) -> Self {
        Self {
            planner,
            explore: None,
            explore_enabled: false,
        }
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

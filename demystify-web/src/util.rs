use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::{Context, bail};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rustsat::types::Lit;
use serde::{Deserialize, Serialize};

use axum_session::{Session, SessionNullPool};
use demystify::problem::{
    musdict::MusContext,
    planner::{PlannerConfig, PuzzlePlanner},
    serialize::SerializablePuzzleParse,
    solver::{PuzzleSolver, SolverConfig},
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

#[derive(Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub puzzle: SerializablePuzzleParse,
    pub planner_config: PlannerConfig,
    pub solver_config: SolverConfig,
    pub steps: Vec<StepSnapshot>,
}

#[derive(Serialize, Deserialize)]
pub struct StepSnapshot {
    pub deduced_lits: Vec<i32>,
}

impl SolverSession {
    pub fn export_snapshot(&self) -> anyhow::Result<SessionSnapshot> {
        let puzzle = SerializablePuzzleParse::try_from(self.planner.puzzle())?;
        let planner_config = self.planner.planner_config();
        let solver_config = self.planner.solver_config();

        let mut steps = Vec::new();

        // Step 0: initial/trivial lits
        let step0_lits: Vec<i32> = self.history[0]
            .get_all_known_lits()
            .iter()
            .map(|lit| lit.to_ipasir())
            .collect();
        steps.push(StepSnapshot {
            deduced_lits: step0_lits,
        });

        // Steps 1..N: diff from previous
        for i in 1..self.history.len() {
            let prev: BTreeSet<Lit> = self.history[i - 1]
                .get_all_known_lits()
                .iter()
                .copied()
                .collect();
            let deduced: Vec<i32> = self.history[i]
                .get_all_known_lits()
                .iter()
                .filter(|lit| !prev.contains(lit))
                .map(|lit| lit.to_ipasir())
                .collect();
            steps.push(StepSnapshot {
                deduced_lits: deduced,
            });
        }

        Ok(SessionSnapshot {
            puzzle,
            planner_config,
            solver_config,
            steps,
        })
    }
}

pub fn import_snapshot(
    snapshot: SessionSnapshot,
    strategy_db: Arc<demystify::named_strategy::Database>,
) -> anyhow::Result<SolverSession> {
    let puzzle: demystify::problem::parse::PuzzleParse = snapshot.puzzle.try_into()?;
    let puzzle = Arc::new(puzzle);

    let solver = PuzzleSolver::new_with_config(puzzle, snapshot.solver_config)?;
    let planner =
        PuzzlePlanner::new_with_config(solver, snapshot.planner_config).with_database(strategy_db);

    let mut session = SolverSession::new(planner);

    for (i, step) in snapshot.steps.iter().enumerate().skip(1) {
        let lits: Vec<Lit> = step
            .deduced_lits
            .iter()
            .map(|&ipasir| Lit::from_ipasir(ipasir))
            .collect::<Result<Vec<_>, _>>()
            .context(format!("Invalid literal in step {i}"))?;

        for lit in &lits {
            session.planner.solver().add_not_provable_known_lit(*lit);
        }
        session.snapshot();
    }

    Ok(session)
}

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::time::{Duration, Instant};

use rustsat::instances::Cnf;
#[cfg_attr(target_arch = "wasm32", allow(unused_imports))]
use rustsat::solvers::{GetInternalStats, Solve, SolveIncremental, SolverResult};
use rustsat::types::{Assignment, Lit};
use tracing::{info, warn};

use std::sync::atomic::Ordering::Relaxed;

// ===== Solver backend selection =====
// All solver-specific code is isolated here; the rest of the file uses `Solver` uniformly.

/// Which SAT solver backend to use.
///
/// On `wasm32-unknown-unknown` only `BatSat` exists (Glucose/CaDiCaL are C/C++
/// FFI and don't build for wasm).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SolverBackend {
    #[cfg(not(target_arch = "wasm32"))]
    Glucose,
    #[cfg(not(target_arch = "wasm32"))]
    CaDiCaL,
    BatSat,
}

#[cfg(not(target_arch = "wasm32"))]
static SOLVER_BACKEND: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0); // 0=Glucose, 1=CaDiCaL, 2=BatSat

/// Set the SAT solver backend. Should be called before any [`SatCore`] is created.
///
/// On `wasm32`, the choice is fixed to BatSat and this setter is ignored.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_solver_backend(backend: SolverBackend) {
    SOLVER_BACKEND.store(backend as u8, Relaxed);
}

#[cfg(target_arch = "wasm32")]
pub fn set_solver_backend(_backend: SolverBackend) {}

#[cfg(target_arch = "wasm32")]
fn current_backend() -> SolverBackend {
    SolverBackend::BatSat
}

#[cfg(not(target_arch = "wasm32"))]
fn current_backend() -> SolverBackend {
    match SOLVER_BACKEND.load(Relaxed) {
        0 => SolverBackend::Glucose,
        1 => SolverBackend::CaDiCaL,
        _ => SolverBackend::BatSat,
    }
}

pub enum Solver {
    #[cfg(not(target_arch = "wasm32"))]
    Glucose(rustsat_glucose::core::Glucose),
    #[cfg(not(target_arch = "wasm32"))]
    CaDiCaL(rustsat_cadical::CaDiCaL<'static, 'static>),
    BatSat(Box<rustsat_batsat::BasicSolver>),
}

// SAFETY: BatSat's `BasicCallbacks` carries an `Option<Box<dyn Fn() -> bool>>`
// (a `stop` predicate) which is `!Send`. We never set that callback, so the
// field stays `None` for the lifetime of the solver, and is trivially safe to
// transfer between threads. `Solver` is also always held behind `Mutex<Solver>`
// inside `SatCore`, so concurrent access is already serialized.
unsafe impl Send for Solver {}

impl Default for Solver {
    fn default() -> Self {
        match current_backend() {
            #[cfg(not(target_arch = "wasm32"))]
            SolverBackend::Glucose => Solver::Glucose(Default::default()),
            #[cfg(not(target_arch = "wasm32"))]
            SolverBackend::CaDiCaL => Solver::CaDiCaL(Default::default()),
            SolverBackend::BatSat => Solver::BatSat(Box::default()),
        }
    }
}

impl Solver {
    fn add_cnf(&mut self, cnf: Cnf) -> anyhow::Result<()> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Solver::Glucose(s) => s.add_cnf(cnf)?,
            #[cfg(not(target_arch = "wasm32"))]
            Solver::CaDiCaL(s) => s.add_cnf(cnf)?,
            Solver::BatSat(s) => s.add_cnf(cnf)?,
        }
        Ok(())
    }

    fn add_unit(&mut self, lit: Lit) -> anyhow::Result<()> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Solver::Glucose(s) => s.add_unit(lit)?,
            #[cfg(not(target_arch = "wasm32"))]
            Solver::CaDiCaL(s) => s.add_unit(lit)?,
            Solver::BatSat(s) => s.add_unit(lit)?,
        }
        Ok(())
    }

    fn solve_assumps(&mut self, lits: &[Lit]) -> anyhow::Result<SolverResult> {
        Ok(match self {
            #[cfg(not(target_arch = "wasm32"))]
            Solver::Glucose(s) => s.solve_assumps(lits)?,
            #[cfg(not(target_arch = "wasm32"))]
            Solver::CaDiCaL(s) => s.solve_assumps(lits)?,
            Solver::BatSat(s) => s.solve_assumps(lits)?,
        })
    }

    fn full_solution(&self) -> anyhow::Result<Assignment> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Solver::Glucose(s) => s.full_solution(),
            #[cfg(not(target_arch = "wasm32"))]
            Solver::CaDiCaL(s) => s.full_solution(),
            Solver::BatSat(s) => s.full_solution(),
        }
    }

    fn core(&mut self) -> anyhow::Result<Vec<Lit>> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Solver::Glucose(s) => s.core(),
            #[cfg(not(target_arch = "wasm32"))]
            Solver::CaDiCaL(s) => s.core(),
            Solver::BatSat(s) => s.core(),
        }
    }

    fn set_conflict_limit(&mut self, limit: i64) {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Solver::Glucose(s) => s.set_limit(rustsat_glucose::Limit::Conflicts(limit)),
            #[cfg(not(target_arch = "wasm32"))]
            Solver::CaDiCaL(s) => s
                .set_limit(rustsat_cadical::Limit::Conflicts(limit as i32))
                .expect("CaDiCaL set_limit failed"),
            // BatSat has no conflict-limit API in rustsat; runs uninterrupted.
            Solver::BatSat(_) => {
                let _ = limit;
            }
        }
    }

    fn clear_conflict_limit(&mut self) {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Solver::Glucose(s) => s.set_limit(rustsat_glucose::Limit::None),
            #[cfg(not(target_arch = "wasm32"))]
            Solver::CaDiCaL(s) => s
                .set_limit(rustsat_cadical::Limit::Conflicts(-1))
                .expect("CaDiCaL set_limit failed"),
            Solver::BatSat(_) => {}
        }
    }

    fn conflicts(&self) -> usize {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Solver::Glucose(s) => s.conflicts(),
            #[cfg(not(target_arch = "wasm32"))]
            Solver::CaDiCaL(s) => s.conflicts(),
            // BatSat doesn't expose a conflict counter via rustsat traits.
            // The auto-ramp logic that uses this becomes a no-op; that's
            // acceptable because BatSat has no conflict limit anyway.
            Solver::BatSat(_) => 0,
        }
    }
}

// ===== End solver backend selection =====

/// Represents a SAT solver core.
/// The public interface to the solver is stateless.
/// Internally, we fix some values in the solver (represented by the)
/// 'fixed' set. Whenever we need to remove values from this set,
/// we restart the solver. This is not externally visible.
pub struct SatCore {
    pub solver: Arc<Mutex<Solver>>,
    pub cnf: Arc<Cnf>,
    pub fixed: RefCell<HashSet<Lit>>,
}

// Solvers can sometimes time out, so we add a conflict limit.
// We also set a 'counter', which checks if the solver is frequently hitting it's limit, if so
// we increase the limit
static CONFLICT_LIMIT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1000);
/// Upper bound on the auto-ramp conflict limit — prevents overflow on very long runs.
const MAX_CONFLICT_LIMIT: i64 = 100_000_000;
/// Number of limited SAT calls since the last ramp check.
static LIMITED_CALLS: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
/// Number of interrupted calls since the last ramp check.
static LIMITED_INTERRUPTED: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
/// Warmup period: wait this many limited calls before checking the interrupt ratio.
const RAMP_WARMUP: i64 = 50;
/// If the interrupt ratio exceeds this threshold, multiply the limit by 10.
const RAMP_THRESHOLD: f64 = 0.10;
static SOLVER_CALLS: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

// --- Diagnostic timers for `_no_limit` path.
//
// Accumulated in nanoseconds across all threads.  `print_phase_breakdown` emits
// totals at shutdown.  The solve itself is `PHASE_SOLVE_NS`; the rest is
// everything around the solver call (mutex, fix_values, stats bookkeeping).
static PHASE_FIX_VALUES_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PHASE_MUTEX_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PHASE_SOLVE_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PHASE_POST_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PHASE_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn print_phase_breakdown() {
    let calls = PHASE_CALLS.load(Relaxed);
    if calls == 0 {
        return;
    }
    let fv = PHASE_FIX_VALUES_NS.load(Relaxed);
    let mu = PHASE_MUTEX_NS.load(Relaxed);
    let sv = PHASE_SOLVE_NS.load(Relaxed);
    let po = PHASE_POST_NS.load(Relaxed);
    let tot = fv + mu + sv + po;
    let pct = |x: u64| {
        if tot == 0 {
            0.0
        } else {
            100.0 * x as f64 / tot as f64
        }
    };
    eprintln!("=== SAT-call phase breakdown (_no_limit path, summed across threads) ===");
    eprintln!("  Calls timed: {calls}");
    eprintln!(
        "  fix_values       {:>10.3} s  ({:5.1}%)",
        fv as f64 / 1e9,
        pct(fv)
    );
    eprintln!(
        "  mutex acquire    {:>10.3} s  ({:5.1}%)",
        mu as f64 / 1e9,
        pct(mu)
    );
    eprintln!(
        "  solve_assumps    {:>10.3} s  ({:5.1}%)",
        sv as f64 / 1e9,
        pct(sv)
    );
    eprintln!(
        "  post (stats etc) {:>10.3} s  ({:5.1}%)",
        po as f64 / 1e9,
        pct(po)
    );
    eprintln!("========================================================================");
}

/// Set the global conflict limit used for the SAT
/// solver (0 = no limit)
pub fn set_global_conflict_limit(val: i64) {
    CONFLICT_LIMIT.store(val, Relaxed);
}

/// Multiply the global conflict limit by `factor`, saturating at `i64::MAX`,
/// and return `(old, new)`.
///
/// Used to escalate when a whole MUS-search pass found nothing within the
/// current budget: a bigger budget lets the (still smallest-first) search reach
/// MUSes that were just out of reach, without ever committing to an arbitrary
/// large one.  A limit of `0` means "no limit" and is left unchanged (so
/// `new == old == 0`); callers treat `new == old` as "already unlimited, cannot
/// raise further".  Note this is *not* capped at [`MAX_CONFLICT_LIMIT`] (which
/// only bounds the automatic ramp): escalation must be free to try as hard as it
/// takes to find a provable literal's smallest MUS.
pub fn multiply_global_conflict_limit(factor: i64) -> (i64, i64) {
    let old = CONFLICT_LIMIT.load(Relaxed);
    if old <= 0 {
        return (old, old);
    }
    let new = old.saturating_mul(factor);
    CONFLICT_LIMIT.store(new, Relaxed);
    (old, new)
}

/// Get the number of solver calls made.
pub fn get_solver_calls() -> i64 {
    SOLVER_CALLS.load(Relaxed)
}

/// Reset the solver call counter to zero.
///
/// Intended for use between benchmark iterations so per-run call counts can be measured.
pub fn reset_solver_calls() {
    SOLVER_CALLS.store(0, Relaxed);
}

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SearchError {
    #[error("The SAT solver reached it's search limit")]
    Limit,
}

pub type SearchResult<T> = std::result::Result<T, SearchError>;

impl SatCore {
    /// Creates a new `SatCore` instance.
    ///
    /// # Arguments
    ///
    /// * `cnf` - The CNF formula to solve.
    ///
    /// # Returns
    ///
    /// A `SatCore` instance.
    pub fn new(cnf: Arc<Cnf>) -> anyhow::Result<SatCore> {
        let timing_on = tracing::enabled!(target: "satcore_build", tracing::Level::INFO);
        let t_total = timing_on.then(Instant::now);

        let t_solver = timing_on.then(Instant::now);
        let mut solver = Solver::default();
        if let Some(t) = t_solver {
            let e = t.elapsed();
            info!(target: "satcore_build", "SatCore::new: Solver::default() took {:?}", e);
        }

        let t_clone = timing_on.then(Instant::now);
        let cnf_clone = cnf.as_ref().clone();
        let n_clauses = cnf_clone.len();
        if let Some(t) = t_clone {
            let e = t.elapsed();
            info!(target: "satcore_build",
                "SatCore::new: cnf.clone() took {:?} ({} clauses)", e, n_clauses);
        }

        let t_addcnf = timing_on.then(Instant::now);
        solver.add_cnf(cnf_clone)?;
        if let Some(t) = t_addcnf {
            let e = t.elapsed();
            info!(target: "satcore_build",
                "SatCore::new: solver.add_cnf({} clauses) took {:?}", n_clauses, e);
        }

        if let Some(t) = t_total {
            let e = t.elapsed();
            // Warn if total construction was unusually long so it surfaces even
            // when only `satcore_build=warn` is set.
            if e.as_secs_f64() > 0.5 {
                warn!(target: "satcore_build",
                    "SatCore::new: total {:?} ({} clauses)", e, n_clauses);
            } else {
                info!(target: "satcore_build",
                    "SatCore::new: total {:?} ({} clauses)", e, n_clauses);
            }
        }

        Ok(SatCore {
            solver: Arc::new(Mutex::new(solver)),
            cnf,
            fixed: RefCell::new(HashSet::new()),
        })
    }

    /// Fix the follow list of literals. As search progresses, we often want to fix a list
    /// of literals (the known values), but as solvers are in a threadpool, we want to
    /// treat solvers as memoryless. Therefore, we fix values, and also reboot the solver if
    /// we discover that we need to fix less literals than the already fixed list
    /// (stored in fixed)
    fn fix_values(&self, lits: &[Lit]) {
        let timing_on = tracing::enabled!(target: "satcore_build", tracing::Level::INFO);
        let t_total = timing_on.then(Instant::now);
        let mut fixed = self.fixed.borrow_mut();
        let fixed_before = fixed.len();

        {
            let mut solver = self.solver.lock().unwrap();

            for &l in lits {
                if !fixed.contains(&l) {
                    solver.add_unit(l).expect("FATAL: Solver bug 1");
                    fixed.insert(l);
                }
            }
        }

        // After adding all of `lits`, `fixed` should be exactly `lits`.
        // If fixed has extra entries, a previous call added lits that are no
        // longer in the current known set — the solver must be rebooted.
        assert!(
            lits.iter().all(|l| fixed.contains(l)),
            "fix_values: lits contains entries not in fixed (should be impossible after the loop above)"
        );
        let rebooted = fixed.len() > lits.len();
        if rebooted {
            let t_reboot = timing_on.then(Instant::now);
            let mut solver = Solver::default();
            solver
                .add_cnf(self.cnf.as_ref().clone())
                .expect("FATAL: Solver bug 2");
            fixed.clear();
            for &l in lits {
                if !fixed.contains(&l) {
                    solver.add_unit(l).expect("FATAL: Solver bug 3");
                    fixed.insert(l);
                }
            }
            let mut mutex_solver = self.solver.lock().unwrap();
            *mutex_solver = solver;
            if let Some(t) = t_reboot {
                let e = t.elapsed();
                warn!(target: "satcore_build",
                    "fix_values: REBOOTED solver — fixed_before={}, lits={}, rebuild took {:?}",
                    fixed_before, lits.len(), e);
            }
        }

        if let Some(t) = t_total {
            let e = t.elapsed();
            // Only chatter at info level when something interesting happened
            // or the call was non-trivial.
            if rebooted || e.as_secs_f64() > 0.05 {
                info!(target: "satcore_build",
                    "fix_values: total {:?} (fixed_before={}, lits={}, rebooted={})",
                    e, fixed_before, lits.len(), rebooted);
            }
        }
    }

    /// Variant of [`do_solve_assumps`] that clears any conflict limit before
    /// invoking the solver.  Because no limit is in place, the solver will
    /// never return `Interrupted`.  Used by the `*_no_limit` public methods.
    fn do_solve_assumps_no_limit(solver: &mut MutexGuard<Solver>, lits: &[Lit]) -> SolverResult {
        solver.clear_conflict_limit();
        SOLVER_CALLS.fetch_add(1, Relaxed);
        let conflicts_before = solver.conflicts();
        let call_start = Instant::now();
        let solve = solver.solve_assumps(lits).unwrap();
        let call_duration = call_start.elapsed();
        let conflicts_delta = solver.conflicts().saturating_sub(conflicts_before);
        solver.clear_conflict_limit();
        crate::stats::record_sat_call(call_duration, conflicts_delta, solve);
        Self::warn_long_call(call_duration, conflicts_delta, &solve, lits, "no_limit");
        solve
    }

    /// Compute the per-call conflict limit for a given work multiplier.
    /// Returns 0 to mean "no limit" (matching the convention used by
    /// solver.set_conflict_limit / clear_conflict_limit downstream).
    fn effective_limit(work_mult: f64) -> i64 {
        let base = CONFLICT_LIMIT.load(Relaxed);
        if base <= 0 || work_mult <= 0.0 || !work_mult.is_finite() {
            return 0;
        }
        let scaled = (base as f64) * work_mult;
        if scaled >= i64::MAX as f64 {
            i64::MAX
        } else {
            (scaled as i64).max(1)
        }
    }

    fn do_solve_assumps(
        solver: &mut MutexGuard<Solver>,
        lits: &[Lit],
        work_mult: f64,
    ) -> SolverResult {
        let limit = Self::effective_limit(work_mult);
        if limit > 0 {
            solver.set_conflict_limit(limit);
        } else {
            solver.clear_conflict_limit();
        }
        SOLVER_CALLS.fetch_add(1, Relaxed);
        let conflicts_before = solver.conflicts();
        let call_start = Instant::now();
        let solve = solver.solve_assumps(lits).unwrap();
        let call_duration = call_start.elapsed();
        let conflicts_delta = solver.conflicts().saturating_sub(conflicts_before);
        solver.clear_conflict_limit();

        crate::stats::record_sat_call(call_duration, conflicts_delta, solve);
        Self::warn_long_call(call_duration, conflicts_delta, &solve, lits, "limited");

        // Only feed the auto-ramp from default-budget calls.  Callers that
        // pass a non-unit multiplier are deliberately picking a custom
        // budget (e.g. random_solution's escalation chain); their
        // interruptions shouldn't push the global limit up for unrelated
        // MUS work happening in other threads.
        if (work_mult - 1.0).abs() < f64::EPSILON {
            if matches!(solve, SolverResult::Interrupted) {
                LIMITED_INTERRUPTED.fetch_add(1, Relaxed);
            }
            let total = LIMITED_CALLS.fetch_add(1, Relaxed) + 1;
            if total >= RAMP_WARMUP {
                let interrupted = LIMITED_INTERRUPTED.load(Relaxed);
                let ratio = interrupted as f64 / total as f64;
                if ratio >= RAMP_THRESHOLD {
                    let limit = CONFLICT_LIMIT.load(Relaxed);
                    let new_limit = (limit * 10).min(MAX_CONFLICT_LIMIT);
                    if new_limit > limit {
                        eprintln!(
                            "Auto-ramp: {interrupted}/{total} calls interrupted ({:.0}%), increasing conflict limit from {limit} to {new_limit}",
                            ratio * 100.0,
                        );
                        CONFLICT_LIMIT.store(new_limit, Relaxed);
                    }
                    LIMITED_CALLS.store(0, Relaxed);
                    LIMITED_INTERRUPTED.store(0, Relaxed);
                }
            }
        }

        solve
    }

    const LONG_CALL_THRESHOLD: Duration = Duration::from_secs(10);

    fn warn_long_call(
        duration: Duration,
        conflicts: usize,
        result: &SolverResult,
        lits: &[Lit],
        path: &str,
    ) {
        if duration >= Self::LONG_CALL_THRESHOLD {
            eprintln!(
                "LONG SAT call ({path}): {:.1}s, {conflicts} conflicts, result={result:?}, assumptions={} lits",
                duration.as_secs_f64(),
                lits.len(),
            );
        }
    }

    /// Solve under the global conflict limit, scaled by `work_mult`.
    /// `work_mult == 1.0` keeps the current default behaviour; larger
    /// values grant the solver more conflicts; `<= 0` or non-finite is
    /// treated as "no limit".  Returns `Err(SearchError::Limit)` when the
    /// solver hits the chosen budget without deciding.
    pub fn assumption_solve(
        &self,
        known: &[Lit],
        lits: &[Lit],
        work_mult: f64,
    ) -> SearchResult<bool> {
        let t0 = Instant::now();
        self.fix_values(known);
        let t1 = Instant::now();
        let mut solver = self.solver.lock().unwrap();
        let t2 = Instant::now();
        let solve = SatCore::do_solve_assumps(&mut solver, lits, work_mult);
        let t3 = Instant::now();
        let result = match solve {
            rustsat::solvers::SolverResult::Sat => Ok(true),
            rustsat::solvers::SolverResult::Unsat => Ok(false),
            rustsat::solvers::SolverResult::Interrupted => Err(SearchError::Limit),
        };
        let t4 = Instant::now();
        PHASE_FIX_VALUES_NS.fetch_add((t1 - t0).as_nanos() as u64, Relaxed);
        PHASE_MUTEX_NS.fetch_add((t2 - t1).as_nanos() as u64, Relaxed);
        PHASE_SOLVE_NS.fetch_add((t3 - t2).as_nanos() as u64, Relaxed);
        PHASE_POST_NS.fetch_add((t4 - t3).as_nanos() as u64, Relaxed);
        PHASE_CALLS.fetch_add(1, Relaxed);
        info!(target: "solver", "Solution to {:?} is {:?}", lits, result);
        result
    }

    /// Solves the CNF formula with the given assumptions and returns the full solution.
    ///
    /// # Arguments
    ///
    /// * `known` - The known literals.
    /// * `lits` - The assumptions to use during solving.
    ///
    /// # Returns
    ///
    /// The full solution if the formula is satisfiable, `None` if it is unsatisfiable.
    pub fn assumption_solve_solution(
        &self,
        known: &[Lit],
        lits: &[Lit],
        work_mult: f64,
    ) -> SearchResult<Option<Assignment>> {
        let t0 = Instant::now();
        self.fix_values(known);
        let t1 = Instant::now();
        let mut solver = self.solver.lock().unwrap();
        let t2 = Instant::now();
        let solve = SatCore::do_solve_assumps(&mut solver, lits, work_mult);
        let t3 = Instant::now();
        let result = match solve {
            rustsat::solvers::SolverResult::Sat => Ok(Some(solver.full_solution().unwrap())),
            rustsat::solvers::SolverResult::Unsat => Ok(None),
            rustsat::solvers::SolverResult::Interrupted => Err(SearchError::Limit),
        };
        let t4 = Instant::now();
        PHASE_FIX_VALUES_NS.fetch_add((t1 - t0).as_nanos() as u64, Relaxed);
        PHASE_MUTEX_NS.fetch_add((t2 - t1).as_nanos() as u64, Relaxed);
        PHASE_SOLVE_NS.fetch_add((t3 - t2).as_nanos() as u64, Relaxed);
        PHASE_POST_NS.fetch_add((t4 - t3).as_nanos() as u64, Relaxed);
        PHASE_CALLS.fetch_add(1, Relaxed);
        info!(target: "solver", "Solution to {:?} is {:?}", lits, result);
        result
    }

    /// Solve as an assumption problem with **no conflict limit applied**.
    ///
    /// Returns `true` for SAT, `false` for UNSAT.  There is no `Interrupted`
    /// branch: without a limit, the solver will run until it produces an
    /// answer.  Use this variant in code that has no useful fallback when a
    /// conflict-limit trips: the caller should simply wait.
    ///
    /// MUS-finding algorithms that *do* have a sensible fallback (try another
    /// dive, skip this literal, etc.) should continue to use the limited
    /// [`Self::assumption_solve`] and handle the `Err(SearchError::Limit)` case
    /// explicitly.
    pub fn assumption_solve_no_limit(&self, known: &[Lit], lits: &[Lit]) -> bool {
        let t0 = Instant::now();
        self.fix_values(known);
        let t1 = Instant::now();
        let mut solver = self.solver.lock().unwrap();
        let t2 = Instant::now();
        let solve = SatCore::do_solve_assumps_no_limit(&mut solver, lits);
        let t3 = Instant::now();
        let result = match solve {
            rustsat::solvers::SolverResult::Sat => true,
            rustsat::solvers::SolverResult::Unsat => false,
            rustsat::solvers::SolverResult::Interrupted => {
                unreachable!("assumption_solve_no_limit must not hit a limit")
            }
        };
        let t4 = Instant::now();
        PHASE_FIX_VALUES_NS.fetch_add((t1 - t0).as_nanos() as u64, Relaxed);
        PHASE_MUTEX_NS.fetch_add((t2 - t1).as_nanos() as u64, Relaxed);
        PHASE_SOLVE_NS.fetch_add((t3 - t2).as_nanos() as u64, Relaxed);
        PHASE_POST_NS.fetch_add((t4 - t3).as_nanos() as u64, Relaxed);
        PHASE_CALLS.fetch_add(1, Relaxed);
        result
    }

    /// Solve as an assumption problem with **no conflict limit applied**, and
    /// return the full model when SAT.  See [`Self::assumption_solve_no_limit`].
    pub fn assumption_solve_solution_no_limit(
        &self,
        known: &[Lit],
        lits: &[Lit],
    ) -> Option<Assignment> {
        self.fix_values(known);
        let mut solver = self.solver.lock().unwrap();
        let solve = SatCore::do_solve_assumps_no_limit(&mut solver, lits);
        match solve {
            rustsat::solvers::SolverResult::Sat => Some(solver.full_solution().unwrap()),
            rustsat::solvers::SolverResult::Unsat => None,
            rustsat::solvers::SolverResult::Interrupted => {
                unreachable!("assumption_solve_solution_no_limit must not hit a limit")
            }
        }
    }

    /// True iff the CNF is satisfiable under `assumps` with the extra
    /// disjunctive `clause` added.
    ///
    /// Runs on a fresh throwaway solver built from `self.cnf`, so the cached
    /// incremental solver — whose learned clauses must stay sound for later
    /// memoryless calls, and whose public interface only ever adds *unit*
    /// literals — is never mutated by this disjunctive clause.  No conflict
    /// limit is applied: the caller waits for a definitive answer.
    pub fn solve_with_clause_no_limit(&self, assumps: &[Lit], clause: &[Lit]) -> bool {
        let mut cnf = self.cnf.as_ref().clone();
        cnf.add_clause(clause.iter().copied().collect());
        let mut solver = Solver::default();
        solver
            .add_cnf(cnf)
            .expect("FATAL: solver build in solve_with_clause_no_limit");
        SOLVER_CALLS.fetch_add(1, Relaxed);
        solver.clear_conflict_limit();
        matches!(
            solver.solve_assumps(assumps).unwrap(),
            rustsat::solvers::SolverResult::Sat
        )
    }

    /// Solves the CNF formula with the given assumptions and returns the unsatisfiable core.
    ///
    /// # Arguments
    ///
    /// * `known` - The known literals.
    /// * `lits` - The assumptions to use during solving.
    ///
    /// # Returns
    ///
    /// The unsatisfiable core if the formula is unsatisfiable, `None` if it is satisfiable.
    pub fn assumption_solve_with_core(
        &self,
        known: &[Lit],
        lits: &[Lit],
    ) -> SearchResult<Option<Vec<Lit>>> {
        let t0 = Instant::now();
        self.fix_values(known);
        let t1 = Instant::now();
        PHASE_FIX_VALUES_NS.fetch_add((t1 - t0).as_nanos() as u64, Relaxed);
        self.raw_assumption_solve_with_core_timed(lits, t1)
    }

    /// Solves the CNF formula with the given assumptions and returns the unsatisfiable core.
    /// *Not memoryless*: Uses whatever set of values are already fixed in the solver.
    fn raw_assumption_solve_with_core(&self, lits: &[Lit]) -> SearchResult<Option<Vec<Lit>>> {
        self.raw_assumption_solve_with_core_timed(lits, Instant::now())
    }

    /// Shared body of the core-returning solve.  Accepts an anchor `Instant`
    /// so the phase breakdown can split time between fix_values (before the
    /// anchor) and the rest of the call.
    fn raw_assumption_solve_with_core_timed(
        &self,
        lits: &[Lit],
        t_after_fix: Instant,
    ) -> SearchResult<Option<Vec<Lit>>> {
        let mut solver = self.solver.lock().unwrap();
        let t2 = Instant::now();
        let solve = SatCore::do_solve_assumps(&mut solver, lits, 1.0);
        let t3 = Instant::now();
        let result = match solve {
            rustsat::solvers::SolverResult::Sat => Ok(None),
            rustsat::solvers::SolverResult::Unsat => Ok(Some(
                solver.core().unwrap().into_iter().map(|l| !l).collect(),
            )),
            rustsat::solvers::SolverResult::Interrupted => Err(SearchError::Limit),
        };
        let t4 = Instant::now();
        // The caller recorded fix_values's start; we recover its duration here.
        // When the raw entry point is used without a fix_values step, we still
        // record mutex/solve/post honestly.
        PHASE_MUTEX_NS.fetch_add((t2 - t_after_fix).as_nanos() as u64, Relaxed);
        PHASE_SOLVE_NS.fetch_add((t3 - t2).as_nanos() as u64, Relaxed);
        PHASE_POST_NS.fetch_add((t4 - t3).as_nanos() as u64, Relaxed);
        PHASE_CALLS.fetch_add(1, Relaxed);
        result
    }

    /// Greedy minimisation loop: given an initial UNSAT core and the universe of
    /// lits it was drawn from, try removing each element.  Assumes `fix_values`
    /// has already been called.
    fn greedy_minimise(
        &self,
        initial_core: Vec<Lit>,
        max_size: Option<i64>,
    ) -> SearchResult<Option<Vec<Lit>>> {
        let mut core = initial_core;

        // Bulk shrinking: partition the core into max_size+1 groups and
        // greedily drop the first group whose removal keeps UNSAT.
        // By pigeonhole, if a MUS of size ≤ max_size exists, at least one
        // group contains no MUS elements.
        if let Some(max_size) = max_size {
            let num_groups = max_size as usize + 1;
            while core.len() > num_groups * 2 {
                let mut shrank = false;
                for i in 0..num_groups {
                    let remaining: Vec<Lit> = core
                        .iter()
                        .enumerate()
                        .filter_map(
                            |(j, &lit)| {
                                if j % num_groups == i { None } else { Some(lit) }
                            },
                        )
                        .collect();
                    let candidate = self.raw_assumption_solve_with_core(&remaining)?;
                    if let Some(found) = candidate {
                        tracing::info!(target: "musdetail",
                            "bulk shrink: {} -> {} (group {}/{})",
                            core.len(), found.len(), i, num_groups);
                        core = found;
                        shrank = true;
                        break;
                    }
                }
                if !shrank {
                    return Ok(None);
                }
            }
        }

        // Element-by-element minimisation over the (now small) core.
        let candidates = core.clone();
        let mut known_core = Vec::new();
        let mut known_size: i64 = 0;
        for &lit in &candidates {
            let location = core.iter().position(|&x| x == lit);
            if let Some(location) = location {
                let mut check_core = core.clone();
                check_core.remove(location);
                let candidate = self.raw_assumption_solve_with_core(&check_core)?;
                if let Some(found) = candidate {
                    core = found;
                } else {
                    known_size += 1;
                    known_core.push(lit);
                    if let Some(max_size) = max_size
                        && known_size == max_size
                    {
                        assert!(known_core.len() as i64 == max_size);
                        let core = self.raw_assumption_solve_with_core(&known_core)?;
                        if let Some(found) = core {
                            assert!(found.len() as i64 == known_size);
                            return Ok(Some(found));
                        }
                        return Ok(None);
                    }
                }
            }
        }
        Ok(Some(core))
    }

    /// Takes a known-unsatisfiable subset and greedily minimises it by trying
    /// to remove each element.  Returns a MUS (or a smaller US if `max_size`
    /// terminates the search early).
    ///
    /// Panics if `us` is satisfiable under `known`.
    pub fn minimise_us(
        &self,
        known: &[Lit],
        us: &[Lit],
        max_size: Option<i64>,
    ) -> SearchResult<Vec<Lit>> {
        Ok(self
            .minimise_us_bounded(known, us, max_size)?
            .expect("minimise_us: greedy_minimise must succeed (input is UNSAT)"))
    }

    /// Bounded variant of [`Self::minimise_us`].  Returns `Some(mus)` for a MUS
    /// of size at most `max_size`, or `None` if greedy minimisation could not
    /// bring the subset down to that size.  Unlike `minimise_us`, the "no MUS
    /// that small" outcome is returned rather than panicked on, so callers can
    /// cheaply test "is there a MUS of size ≤ N for this literal?".
    ///
    /// Panics if `us` is satisfiable under `known`.
    pub fn minimise_us_bounded(
        &self,
        known: &[Lit],
        us: &[Lit],
        max_size: Option<i64>,
    ) -> SearchResult<Option<Vec<Lit>>> {
        self.fix_values(known);
        let initial = self.raw_assumption_solve_with_core(us)?;
        let core = initial.expect("minimise_us: input must be an unsatisfiable subset");
        self.greedy_minimise(core, max_size)
    }

    /// Finds a minimal unsatisfiable subset (MUS) of literals given a set of known literals.
    ///
    /// # Arguments
    ///
    /// * `known` - The known literals.
    /// * `lits` - The set of literals to search over.
    ///
    /// # Returns
    ///
    /// The minimal unsatisfiable subset (MUS) of literals, if one exists.
    pub fn quick_mus(
        &self,
        known: &[Lit],
        lits: &[Lit],
        max_size: Option<i64>,
    ) -> SearchResult<Option<Vec<Lit>>> {
        self.fix_values(known);
        let core = self.raw_assumption_solve_with_core(lits)?;
        match core {
            None => Ok(None),
            Some(core) => {
                tracing::info!(target: "musdetail", "quick_mus: initial_core={} max_size={:?}",
                    core.len(), max_size);
                Ok(self.greedy_minimise(core, max_size)?)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rustsat::lit;

    use super::*;

    fn create_cnf() -> Arc<Cnf> {
        let mut cnf = Cnf::new();
        cnf.add_binary(lit![0], lit![1]);
        cnf.add_binary(lit![0], !lit![1]);
        Arc::new(cnf)
    }

    #[test]
    fn test_assumption_solve_solution() -> anyhow::Result<()> {
        let solver = SatCore::new(create_cnf())?;
        let result = solver.assumption_solve_solution(&[], &[lit![1], lit![2]], 1.0)?;
        assert!(result.is_some());
        let result = solver.assumption_solve_solution(&[], &[lit![0]], 1.0)?;
        assert!(result.is_some());
        let result = solver.assumption_solve_solution(&[], &[!lit![0]], 1.0)?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn test_assumption_solve_core() -> anyhow::Result<()> {
        let solver = SatCore::new(create_cnf())?;
        let result = solver.assumption_solve_solution(&[], &[lit![1], lit![2]], 1.0)?;
        assert!(result.is_some());
        let result = solver.assumption_solve_solution(&[], &[lit![0]], 1.0)?;
        assert!(result.is_some());
        let result = solver.assumption_solve_solution(&[], &[!lit![0]], 1.0)?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn test_assumption_quick_mus() -> anyhow::Result<()> {
        let solver = SatCore::new(create_cnf())?;
        let result = solver.quick_mus(&[], &[lit![1], lit![2]], None)?;
        assert!(result.is_none());
        let result = solver.quick_mus(&[], &[lit![0]], None)?;
        assert!(result.is_none());
        let result = solver.quick_mus(&[], &[!lit![0]], None)?;
        assert!(result.is_some());

        Ok(())
    }

    #[test]
    fn test_assumption_quick_mus_known() -> anyhow::Result<()> {
        let solver = SatCore::new(create_cnf())?;
        let result = solver.quick_mus(&[], &[lit![1], lit![2]], None)?;
        assert!(result.is_none());
        let result = solver.quick_mus(&[!lit![0]], &[lit![1], lit![2]], None)?;
        assert_eq!(result, Some(vec![]));
        let result = solver.quick_mus(&[], &[lit![1], lit![2]], None)?;
        assert!(result.is_none());

        Ok(())
    }
}

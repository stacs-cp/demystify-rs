use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard};

use rustsat::instances::Cnf;
use rustsat::solvers::{GetInternalStats, Solve, SolveIncremental, SolverResult};
use rustsat::types::{Assignment, Lit};
use tracing::info;

use std::sync::atomic::Ordering::Relaxed;

// ===== Solver backend selection =====
// All solver-specific code is isolated here; the rest of the file uses `Solver` uniformly.

/// Which SAT solver backend to use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SolverBackend {
    Glucose,
    CaDiCaL,
}

static SOLVER_BACKEND: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0); // 0=Glucose, 1=CaDiCaL

/// Set the SAT solver backend. Should be called before any [`SatCore`] is created.
pub fn set_solver_backend(backend: SolverBackend) {
    SOLVER_BACKEND.store(backend as u8, Relaxed);
}

fn current_backend() -> SolverBackend {
    if SOLVER_BACKEND.load(Relaxed) == 0 {
        SolverBackend::Glucose
    } else {
        SolverBackend::CaDiCaL
    }
}

pub enum Solver {
    Glucose(rustsat_glucose::core::Glucose),
    CaDiCaL(rustsat_cadical::CaDiCaL<'static, 'static>),
}

impl Default for Solver {
    fn default() -> Self {
        match current_backend() {
            SolverBackend::Glucose => Solver::Glucose(Default::default()),
            SolverBackend::CaDiCaL => Solver::CaDiCaL(Default::default()),
        }
    }
}

impl Solver {
    fn add_cnf(&mut self, cnf: Cnf) -> anyhow::Result<()> {
        match self {
            Solver::Glucose(s) => s.add_cnf(cnf)?,
            Solver::CaDiCaL(s) => s.add_cnf(cnf)?,
        }
        Ok(())
    }

    fn add_unit(&mut self, lit: Lit) -> anyhow::Result<()> {
        match self {
            Solver::Glucose(s) => s.add_unit(lit)?,
            Solver::CaDiCaL(s) => s.add_unit(lit)?,
        }
        Ok(())
    }

    fn solve_assumps(&mut self, lits: &[Lit]) -> anyhow::Result<SolverResult> {
        Ok(match self {
            Solver::Glucose(s) => s.solve_assumps(lits)?,
            Solver::CaDiCaL(s) => s.solve_assumps(lits)?,
        })
    }

    fn full_solution(&self) -> anyhow::Result<Assignment> {
        match self {
            Solver::Glucose(s) => s.full_solution(),
            Solver::CaDiCaL(s) => s.full_solution(),
        }
    }

    fn core(&mut self) -> anyhow::Result<Vec<Lit>> {
        match self {
            Solver::Glucose(s) => s.core(),
            Solver::CaDiCaL(s) => s.core(),
        }
    }

    fn set_conflict_limit(&mut self, limit: i64) {
        match self {
            Solver::Glucose(s) => s.set_limit(rustsat_glucose::Limit::Conflicts(limit)),
            Solver::CaDiCaL(s) => s
                .set_limit(rustsat_cadical::Limit::Conflicts(limit as i32))
                .expect("CaDiCaL set_limit failed"),
        }
    }

    fn clear_conflict_limit(&mut self) {
        match self {
            Solver::Glucose(s) => s.set_limit(rustsat_glucose::Limit::None),
            Solver::CaDiCaL(s) => s
                .set_limit(rustsat_cadical::Limit::Conflicts(-1))
                .expect("CaDiCaL set_limit failed"),
        }
    }

    fn conflicts(&self) -> usize {
        match self {
            Solver::Glucose(s) => s.conflicts(),
            Solver::CaDiCaL(s) => s.conflicts(),
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
static CONFLICT_COUNT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
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
        let mut solver = Solver::default();
        solver.add_cnf(cnf.as_ref().clone())?;

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
        let mut fixed = self.fixed.borrow_mut();

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
        if fixed.len() > lits.len() {
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
        }
    }

    /// Variant of [`do_solve_assumps`] that clears any conflict limit before
    /// invoking the solver.  Because no limit is in place, the solver will
    /// never return `Interrupted`.  Used by the `*_no_limit` public methods.
    fn do_solve_assumps_no_limit(solver: &mut MutexGuard<Solver>, lits: &[Lit]) -> SolverResult {
        solver.clear_conflict_limit();
        SOLVER_CALLS.fetch_add(1, Relaxed);
        let conflicts_before = solver.conflicts();
        let call_start = std::time::Instant::now();
        let solve = solver.solve_assumps(lits).unwrap();
        let call_duration = call_start.elapsed();
        let conflicts_delta = solver.conflicts().saturating_sub(conflicts_before);
        solver.clear_conflict_limit();
        crate::stats::record_sat_call(call_duration, conflicts_delta, solve);
        solve
    }

    fn do_solve_assumps(solver: &mut MutexGuard<Solver>, lits: &[Lit]) -> SolverResult {
        // A non-positive CONFLICT_LIMIT means "no limit": go straight to
        // clear_conflict_limit so the caller does not receive Interrupted.
        let limit = CONFLICT_LIMIT.load(Relaxed);
        if limit > 0 {
            solver.set_conflict_limit(limit);
        } else {
            solver.clear_conflict_limit();
        }
        SOLVER_CALLS.fetch_add(1, Relaxed);
        let conflicts_before = solver.conflicts();
        let call_start = std::time::Instant::now();
        let solve = solver.solve_assumps(lits).unwrap();
        let call_duration = call_start.elapsed();
        let conflicts_delta = solver.conflicts().saturating_sub(conflicts_before);
        solver.clear_conflict_limit();

        crate::stats::record_sat_call(call_duration, conflicts_delta, solve);

        if matches!(solve, SolverResult::Interrupted) {
            //eprintln!("SAT solver limit tripped");
            // This code may well have some race conditions, but
            // if we are in this situation, I don't mind if we
            // end up increasing the limit even more than intended,
            // as long as it is increased, and the counter reset.
            let count = CONFLICT_COUNT.fetch_add(1, Relaxed);
            if count > 1000 {
                let limit = CONFLICT_LIMIT.load(Relaxed);
                eprintln!(
                    "Warning: The puzzle is hard to solve, increasing limits in SAT solver from {} to {}",
                    limit,
                    limit * 10
                );
                CONFLICT_LIMIT.store(
                    (CONFLICT_LIMIT.load(Relaxed) * 10).min(MAX_CONFLICT_LIMIT),
                    Relaxed,
                );
                CONFLICT_COUNT.store(0, Relaxed);
            } else {
                let _ = CONFLICT_COUNT.fetch_update(Relaxed, Relaxed, |count| {
                    if count > 0 { Some(count - 1) } else { Some(0) }
                });
            }
        }

        solve
    }

    /// Solves the CNF formula with the given assumption and known
    /// values.
    ///
    /// # Arguments
    ///
    /// * `known` - Literal known to be true
    /// * `lits` - The assumptions to use during solving.
    ///
    /// # Returns
    ///
    /// `true` if the formula is satisfiable, `false` if it is unsatisfiable.
    pub fn assumption_solve(&self, known: &[Lit], lits: &[Lit]) -> SearchResult<bool> {
        let t0 = std::time::Instant::now();
        self.fix_values(known);
        let t1 = std::time::Instant::now();
        let mut solver = self.solver.lock().unwrap();
        let t2 = std::time::Instant::now();
        let solve = SatCore::do_solve_assumps(&mut solver, lits);
        let t3 = std::time::Instant::now();
        let result = match solve {
            rustsat::solvers::SolverResult::Sat => Ok(true),
            rustsat::solvers::SolverResult::Unsat => Ok(false),
            rustsat::solvers::SolverResult::Interrupted => Err(SearchError::Limit),
        };
        let t4 = std::time::Instant::now();
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
    ) -> SearchResult<Option<Assignment>> {
        let t0 = std::time::Instant::now();
        self.fix_values(known);
        let t1 = std::time::Instant::now();
        let mut solver = self.solver.lock().unwrap();
        let t2 = std::time::Instant::now();
        let solve = SatCore::do_solve_assumps(&mut solver, lits);
        let t3 = std::time::Instant::now();
        let result = match solve {
            rustsat::solvers::SolverResult::Sat => Ok(Some(solver.full_solution().unwrap())),
            rustsat::solvers::SolverResult::Unsat => Ok(None),
            rustsat::solvers::SolverResult::Interrupted => Err(SearchError::Limit),
        };
        let t4 = std::time::Instant::now();
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
    /// [`assumption_solve`] and handle the `Err(SearchError::Limit)` case
    /// explicitly.
    pub fn assumption_solve_no_limit(&self, known: &[Lit], lits: &[Lit]) -> bool {
        let t0 = std::time::Instant::now();
        self.fix_values(known);
        let t1 = std::time::Instant::now();
        let mut solver = self.solver.lock().unwrap();
        let t2 = std::time::Instant::now();
        let solve = SatCore::do_solve_assumps_no_limit(&mut solver, lits);
        let t3 = std::time::Instant::now();
        let result = match solve {
            rustsat::solvers::SolverResult::Sat => true,
            rustsat::solvers::SolverResult::Unsat => false,
            rustsat::solvers::SolverResult::Interrupted => {
                unreachable!("assumption_solve_no_limit must not hit a limit")
            }
        };
        let t4 = std::time::Instant::now();
        PHASE_FIX_VALUES_NS.fetch_add((t1 - t0).as_nanos() as u64, Relaxed);
        PHASE_MUTEX_NS.fetch_add((t2 - t1).as_nanos() as u64, Relaxed);
        PHASE_SOLVE_NS.fetch_add((t3 - t2).as_nanos() as u64, Relaxed);
        PHASE_POST_NS.fetch_add((t4 - t3).as_nanos() as u64, Relaxed);
        PHASE_CALLS.fetch_add(1, Relaxed);
        result
    }

    /// Solve as an assumption problem with **no conflict limit applied**, and
    /// return the full model when SAT.  See [`assumption_solve_no_limit`].
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
        let t0 = std::time::Instant::now();
        self.fix_values(known);
        let t1 = std::time::Instant::now();
        PHASE_FIX_VALUES_NS.fetch_add((t1 - t0).as_nanos() as u64, Relaxed);
        self.raw_assumption_solve_with_core_timed(lits, t1)
    }

    /// Solves the CNF formula with the given assumptions and returns the unsatisfiable core.
    /// *Not memoryless*: Uses whatever set of values are already fixed in the solver.
    fn raw_assumption_solve_with_core(&self, lits: &[Lit]) -> SearchResult<Option<Vec<Lit>>> {
        self.raw_assumption_solve_with_core_timed(lits, std::time::Instant::now())
    }

    /// Shared body of the core-returning solve.  Accepts an anchor `Instant`
    /// so the phase breakdown can split time between fix_values (before the
    /// anchor) and the rest of the call.
    fn raw_assumption_solve_with_core_timed(
        &self,
        lits: &[Lit],
        t_after_fix: std::time::Instant,
    ) -> SearchResult<Option<Vec<Lit>>> {
        let mut solver = self.solver.lock().unwrap();
        let t2 = std::time::Instant::now();
        let solve = SatCore::do_solve_assumps(&mut solver, lits);
        let t3 = std::time::Instant::now();
        let result = match solve {
            rustsat::solvers::SolverResult::Sat => Ok(None),
            rustsat::solvers::SolverResult::Unsat => Ok(Some(
                solver.core().unwrap().into_iter().map(|l| !l).collect(),
            )),
            rustsat::solvers::SolverResult::Interrupted => Err(SearchError::Limit),
        };
        let t4 = std::time::Instant::now();
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
        self.fix_values(known);
        let initial = self.raw_assumption_solve_with_core(us)?;
        let core = initial.expect("minimise_us: input must be an unsatisfiable subset");
        Ok(self
            .greedy_minimise(core, max_size)?
            .expect("minimise_us: greedy_minimise must succeed (input is UNSAT)"))
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
        let result = solver.assumption_solve_solution(&[], &[lit![1], lit![2]])?;
        assert!(result.is_some());
        let result = solver.assumption_solve_solution(&[], &[lit![0]])?;
        assert!(result.is_some());
        let result = solver.assumption_solve_solution(&[], &[!lit![0]])?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn test_assumption_solve_core() -> anyhow::Result<()> {
        let solver = SatCore::new(create_cnf())?;
        let result = solver.assumption_solve_solution(&[], &[lit![1], lit![2]])?;
        assert!(result.is_some());
        let result = solver.assumption_solve_solution(&[], &[lit![0]])?;
        assert!(result.is_some());
        let result = solver.assumption_solve_solution(&[], &[!lit![0]])?;
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

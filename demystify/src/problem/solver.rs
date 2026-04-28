use std::collections::BTreeSet;
use std::ops::Neg;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use itertools::Itertools;
use rand::seq::SliceRandom;
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha20Rng;
use rayon::iter::{IntoParallelRefIterator, ParallelBridge, ParallelIterator};
use rustsat::types::Lit;
use thread_local::ThreadLocal;
use tracing::info;

use crate::problem::musdict::MusContext;
use crate::{
    problem::{PuzVar, VarValPair},
    satcore::{SatCore, SearchResult},
};

use super::{PuzLit, musdict::MusDict, parse::PuzzleParse};

/// The strategy to use when finding a minimal unsatisfiable subset (MUS)
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum Strategy {
    /// Uses a quick algorithm that may find larger MUSes
    Quick,
    /// Uses a slicing technique to find smaller MUSes
    Slice,
    /// Uses a "cake cutting" technique to find small MUSes
    Cake,
    /// Uses 'cake cutting' for smaller MUSes, slice for larger
    #[default]
    Dynamic,
}

#[derive(Copy, Clone)]
pub struct MusConfig {
    pub base_size_mus: i64,
    pub mus_add_step: i64,
    pub mus_mult_step: i64,
    pub repeats: i64,
    pub find_bigger: bool,
    pub find_one: bool,
    /// When true, the returned `MusDict` retains every MUS the search produces,
    /// including strictly larger ones. Intended for analysing whether a literal has
    /// alternative explanations of different sizes. Has no effect on search order.
    pub keep_all_muses: bool,
    pub strategy: Strategy,
}

impl Default for MusConfig {
    fn default() -> Self {
        Self {
            base_size_mus: 2,
            mus_add_step: 1,
            mus_mult_step: 2,
            repeats: 2,
            find_bigger: false,
            find_one: true,
            keep_all_muses: false,
            strategy: Strategy::default(),
        }
    }
}

impl MusConfig {
    #[must_use]
    pub fn new_with_repeats(repeats: i64) -> Self {
        Self {
            base_size_mus: 2,
            mus_add_step: 1,
            mus_mult_step: 2,
            repeats,
            find_bigger: false,
            find_one: true,
            keep_all_muses: false,
            strategy: Strategy::default(),
        }
    }
}

#[derive(Copy, Clone, Default)]
pub struct SolverConfig {
    pub only_assignments: bool,
}

/// Represents a puzzle solver.
pub struct PuzzleSolver {
    satcore: ThreadLocal<SatCore>,
    puzzleparse: Arc<PuzzleParse>,

    knownlits: Vec<Lit>,
    tosolvelits: Option<BTreeSet<Lit>>,

    solver_config: SolverConfig,
}

impl PuzzleSolver {
    /// Creates a new `PuzzleSolver` instance.
    ///
    /// # Arguments
    ///
    /// * `puzzleparse` - The `PuzzleParse` instance containing puzzle information.
    ///
    /// # Returns
    ///
    /// A `PuzzleSolver` instance.
    pub fn new(puzzleparse: Arc<PuzzleParse>) -> anyhow::Result<PuzzleSolver> {
        Ok(PuzzleSolver {
            satcore: ThreadLocal::new(),
            puzzleparse,
            tosolvelits: None,
            knownlits: Vec::new(),
            solver_config: SolverConfig::default(),
        })
    }

    /// Creates a new `PuzzleSolver` instance from a config
    ///
    /// # Arguments
    ///
    /// * `puzzleparse` - The `PuzzleParse` instance containing puzzle information.
    /// * `solverconfig` - A `SolverConfig` object
    ///
    /// # Returns
    ///
    /// A `PuzzleSolver` instance.
    pub fn new_with_config(
        puzzleparse: Arc<PuzzleParse>,
        solver_config: SolverConfig,
    ) -> anyhow::Result<PuzzleSolver> {
        Ok(PuzzleSolver {
            satcore: ThreadLocal::new(),
            puzzleparse,
            tosolvelits: None,
            knownlits: Vec::new(),
            solver_config,
        })
    }

    /// Retrieves the `SatCore` instance associated with the `PuzzleSolver`.
    ///
    /// # Returns
    ///
    /// A reference to the `SatCore` instance.
    fn get_satcore(&self) -> &SatCore {
        self.satcore
            .get_or(|| SatCore::new(self.puzzleparse.cnf.clone().unwrap()).unwrap())
    }

    /// Converts a `PuzLit` instance to a `Lit`.
    ///
    /// # Arguments
    ///
    /// * `puzlit` - The `PuzLit` instance to convert.
    ///
    /// # Returns
    ///
    /// The corresponding `Lit` instance.
    pub fn puzlit_to_lit(&self, puzlit: &PuzLit) -> Lit {
        *self
            .puzzleparse
            .direct
            .litmap
            .get(puzlit)
            .unwrap_or_else(|| {
                panic!("Expected to find the following variable, but could not find it: {puzlit}");
            })
    }

    /// Converts a `Lit` instance to a reference to the set of `PuzLit` instances it represents.
    ///
    /// # Arguments
    ///
    /// * `lit` - The `Lit` instance to convert.
    ///
    /// # Returns
    ///
    /// A reference to the set of `PuzLit` instances.
    pub fn lit_to_puzlit(&self, lit: &Lit) -> &BTreeSet<PuzLit> {
        self.puzzleparse
            .direct
            .invlitmap
            .get(lit)
            .unwrap_or_else(|| panic!("Missing lit: {lit}"))
    }

    /// Determines if the current puzzle state is solvable under the current assumptions. This only checks if the puzzle has at least one solution, not that the solution is unique.
    ///
    /// Note that for multi-step puzzles (like minesweeper), this only
    /// checks if the current state of the puzzle has at least one solution.
    ///
    /// This method combines the literals from the puzzle's constraint set (`conset_lits`)
    /// and the known literals (`knownlits`) to form a set of assumptions. It then attempts
    /// to solve the puzzle using these assumptions. If the solver finds a solution, it
    /// indicates that the puzzle is currently solvable under these assumptions.
    ///
    /// # Returns
    ///
    /// Returns `true` if the puzzle is solvable under the current assumptions, otherwise `false`.
    pub fn is_currently_solvable(&mut self) -> bool {
        let mut litorig: Vec<Lit> = self
            .puzzleparse
            .constraints
            .lits()
            .iter()
            .copied()
            .collect();
        litorig.extend_from_slice(&self.knownlits);
        // Feasibility check with no conflict limit: a Limit interrupt would be
        // meaningless here — the caller has no alternative behaviour and must
        // wait for a definitive answer.
        self.get_satcore()
            .assumption_solve_no_limit(self.get_known_lits(), &litorig)
    }

    /// Retrieves variable literals which can be proved.
    ///
    /// # Returns
    ///
    /// A vector containing the provable variable literals.
    #[must_use]
    pub fn get_provable_varlits(&mut self) -> &BTreeSet<Lit> {
        if self.tosolvelits.is_none() {
            let mut litorig: Vec<Lit> = self
                .puzzleparse
                .constraints
                .lits()
                .iter()
                .copied()
                .collect();
            litorig.extend_from_slice(&self.knownlits);
            let lits = self.get_literals_to_try_solving();
            let provable: BTreeSet<_> = lits
                .par_iter()
                .filter_map(|&lit| {
                    if !(self.knownlits.contains(&lit) || self.knownlits.contains(&!lit)) {
                        let mut lits = litorig.clone();
                        lits.push(lit);
                        // No limit: the caller expects a definitive answer for
                        // every literal.  Silently skipping a literal on limit
                        // would produce an incomplete (and silently wrong)
                        // provable set.
                        if !self
                            .get_satcore()
                            .assumption_solve_no_limit(self.get_known_lits(), &lits)
                        {
                            return Some(!lit);
                        }
                    }
                    None
                })
                .collect();

            self.tosolvelits = Some(provable);
        }

        self.tosolvelits.as_ref().unwrap()
    }

    /// Retrieves literals which can be proved by a particular MUS.
    ///
    /// # Returns
    ///
    /// A vector containing the provable variable literals.
    #[must_use]
    pub fn get_varlits_provable_by_mus(
        &mut self,
        candidates: &BTreeSet<Lit>,
        mc: &MusContext,
    ) -> BTreeSet<Lit> {
        let mus = &mc.mus;
        assert!(
            mus.iter()
                .all(|c| self.puzzleparse.constraints.lits().contains(c))
        );

        let mut litorig = mus.clone();
        for &lit in &self.knownlits {
            litorig.insert(lit);
        }

        candidates
            .iter()
            .filter_map(|&lit| {
                let lit = !lit;
                if !(self.knownlits.contains(&lit) || self.knownlits.contains(&!lit)) {
                    let mut lits = litorig.iter().copied().collect_vec();
                    lits.push(lit);
                    if !self
                        .get_satcore()
                        .assumption_solve_no_limit(self.get_known_lits(), &lits)
                    {
                        return Some(!lit);
                    }
                }
                None
            })
            .collect()
    }

    /// Returns all literals in the scope of a MUS.
    ///
    /// This method collects all literals that are in the scope of the given MUS. The scope
    /// is determined by looking at all constraints in the MUS and finding all literals that
    /// are affected by those constraints.
    ///
    /// # Arguments
    ///
    /// * `base` - The base literal that is being proved by the MUS.
    /// * `mus` - The Minimal Unsatisfiable Subset (MUS) as a vector of literals.
    ///
    /// # Returns
    ///
    /// A vector of literals that are in the scope of the given MUS.
    fn get_all_lits_in_scope_for_mus(&mut self, mc: &MusContext) -> BTreeSet<Lit> {
        // First get all lits in the scopes of all constraints in the MUS
        let mut lits = BTreeSet::new();

        for m in &mc.mus {
            for l in self.puzzleparse().constraints.var_lits(m) {
                lits.insert(*l);
            }
        }

        // Then get the vars of all those lits
        let mut vars = BTreeSet::new();

        for l in lits {
            for vvp in self.puzzleparse().direct_or_ordered_lit_to_varvalpair(&l) {
                vars.insert(vvp.var().clone());
            }
        }

        // Then get the lits we still need to find, and check if they are in any of those variables
        let mut check_lits = BTreeSet::new();
        // This should always be in here, but let's add it just in case something goes wrong.
        for l in &mc.lits {
            check_lits.insert(*l);
        }

        for l in self.get_provable_varlits().clone() {
            // Get all variables which refer to that literal
            for vvp in self.puzzleparse().direct_or_ordered_lit_to_varvalpair(&l) {
                if vars.contains(vvp.var()) {
                    check_lits.insert(l);
                }
            }
        }

        check_lits
    }

    /// Returns all literals that a given MUS can deduce.
    ///
    /// This method collects all literals that are in the scope of the given MUS, then
    /// checks which of them can be deduced by `mus`.
    ///
    /// # Arguments
    ///
    /// * `base` - The base literal that is being proved by the MUS.
    /// * `mc` - The Minimal Unsatisfiable Subset (MUS).
    ///
    /// # Returns
    ///
    /// A new MUS.
    pub fn get_all_lits_solved_by_mus(&mut self, mc: &MusContext) -> MusContext {
        let candidates = self.get_all_lits_in_scope_for_mus(mc);
        let filtered = self.get_varlits_provable_by_mus(&candidates, mc);
        let result = MusContext::new_with_more_lits(filtered.clone(), mc);

        if cfg!(debug_assertions) {
            let mus_cons: Vec<Lit> = result.mus.iter().copied().collect();
            for &lit in &mc.lits {
                self.verify_mus(lit, &mus_cons);
            }
            for &lit in &filtered {
                if !mc.lits.contains(&lit) {
                    self.verify_mus_provability(lit, &mus_cons);
                }
            }
        }

        result
    }

    /// Generate a random solution.  Does not enforce uniqueness, only existence:
    /// the solution is built by a random dive through `$#VAR` literals.
    ///
    /// All `REVEAL` variables are forced to `true`.
    ///
    /// `steps` controls how many variable assignments are made randomly before
    /// the remaining variables are filled in with whatever the SAT solver
    /// returns.  `None` means "keep going randomly for every variable" (most
    /// random); `Some(n)` means "flip n vars randomly, then extend".
    ///
    /// # Return value
    ///
    /// Returns `None` when the problem as presented to the solver is
    /// unsatisfiable under the current known-literal set — for example, when a
    /// neighbourhood constraint or the caller's pinned lits leave no feasible
    /// assignment.  Callers that expect a solution (e.g. initial sampling from
    /// a fresh unconstrained model) should unwrap with `.expect(...)`; callers
    /// that tolerate failure (e.g. neighbourhood mutation, where the requested
    /// distance may have no feasible neighbour) should handle the `None` case
    /// by retrying at a different distance or giving up this step.
    pub fn random_solution(
        &mut self,
        rng: &mut ChaCha20Rng,
        mut steps: Option<usize>,
    ) -> Option<BTreeSet<Lit>> {
        let mut solution = vec![];

        let mut litorig: Vec<Lit> = self
            .puzzleparse
            .constraints
            .lits()
            .iter()
            .copied()
            .collect();
        litorig.extend_from_slice(&self.knownlits);

        let reveal_lits: Vec<_> = self.puzzleparse.reveal_map.values().copied().collect();
        litorig.extend_from_slice(&reveal_lits);

        // Random sampling and the read-out treat two sets differently:
        // - `lits_to_check` is shuffled and visited for random polarity
        //   choices on the first `steps` iterations.  Only $#VAR lits go
        //   here; framework-special `demystify_*` AUX vars are derived from
        //   the puzzle's design and should not be randomly fixed first.
        // - `lits_to_read` is the union: it is what we read out of the
        //   final solution to populate the returned BTreeSet.  Special
        //   AUX values are captured here so callers (e.g. Mystify) can
        //   use them for design control or fitness signalling.
        let mut lits_to_check = self
            .puzzleparse
            .var_lits
            .positive()
            .iter()
            .copied()
            .collect_vec();
        lits_to_check.shuffle(rng);
        let mut lits_to_read = lits_to_check.clone();
        lits_to_read.extend(self.puzzleparse.var_lits.special().iter().copied());

        for &l in &lits_to_check {
            let mut lits = litorig.clone();
            let test_lit = if rng.random_bool(0.5) { l } else { l.neg() };

            lits.push(test_lit);

            // Random sampling uses unlimited-conflict solves: there is no
            // useful fall-back when a feasibility check times out here, and
            // silent timeouts would silently bias the sampled solution.
            if self
                .get_satcore()
                .assumption_solve_no_limit(self.get_known_lits(), &lits)
            {
                solution.push(test_lit);
                litorig.push(test_lit);
            } else {
                // Try the opposite polarity.
                let test_lit = test_lit.neg();
                let mut lits = litorig.clone();
                lits.push(test_lit);
                if self
                    .get_satcore()
                    .assumption_solve_no_limit(self.get_known_lits(), &lits)
                {
                    solution.push(test_lit);
                    litorig.push(test_lit);
                } else {
                    // Neither polarity is feasible given the committed
                    // assumptions: the problem is unsatisfiable under the
                    // caller's current constraints.  Report None so the
                    // caller can recover (for example, by trying a larger
                    // neighbourhood).
                    return None;
                }
            }

            if steps == Some(0) {
                let sol = self
                    .get_satcore()
                    .assumption_solve_solution_no_limit(self.get_known_lits(), &litorig)
                    .expect("Must be a solution, from previous call");

                for &l in &lits_to_read {
                    match sol.lit_value(l) {
                        rustsat::types::TernaryVal::True => {
                            solution.push(l);
                        }
                        rustsat::types::TernaryVal::False => {}
                        rustsat::types::TernaryVal::DontCare => panic!("Missing assignment??!?"),
                    }
                }
                return Some(solution.into_iter().collect());
            }
            steps = steps.map(|x| x - 1);
        }

        Some(solution.into_iter().collect())
    }

    /// Returns the set of literals which we should still try solving (may be true, or false)
    pub fn get_literals_to_try_solving(&mut self) -> BTreeSet<Lit> {
        let lits = if self.solver_config.only_assignments {
            &self.puzzleparse.var_lits.negative()
        } else {
            &self.puzzleparse.var_lits.positive()
        };
        lits.iter()
            .copied()
            .filter(|&lit| !(self.knownlits.contains(&lit) || self.knownlits.contains(&!lit)))
            .collect()
    }

    /// Sets a literal as known, which could previously be proved.
    ///
    /// # Arguments
    ///
    /// * `lit` - The literal to add.
    pub fn add_known_lit(&mut self, lit: Lit) {
        if self.knownlits.contains(&lit) {
            return;
        }
        // The puzzle may have become unsolvable (in which case there are no
        // solvable lits), but we didn't realise yet (as we don't check that
        // at every addition of a known lit).
        debug_assert!(self.get_provable_varlits().contains(&lit) || !self.is_currently_solvable());
        self.add_known_lit_unchecked(lit);
    }

    /// Adds a literal which is known to be true, but cannot be proved true.
    /// This exists because it invalidates a number of internal caches.
    ///
    /// # Arguments
    ///
    /// * `lit` - The literal to add.
    pub fn add_not_provable_known_lit(&mut self, lit: Lit) {
        self.add_known_lit_unchecked(lit);
        self.tosolvelits = None;
    }

    pub fn fork_with_known_lits(
        puzzleparse: Arc<PuzzleParse>,
        known_lits: &[Lit],
        solver_config: SolverConfig,
    ) -> anyhow::Result<PuzzleSolver> {
        let mut solver = PuzzleSolver::new_with_config(puzzleparse, solver_config)?;
        for &lit in known_lits {
            solver.add_known_lit_unchecked(lit);
        }
        Ok(solver)
    }

    pub(crate) fn add_known_lit_unchecked(&mut self, lit: Lit) {
        if self.knownlits.contains(&lit) {
            return;
        }
        self.add_known_lit_internal(lit);
        // When we add 'x=i' literal, automatically add 'x != j'
        // for all 'j != i'. This isn't required, but it speeds
        // up solving, and cleans up the output.
        let puzlit_set = self.lit_to_puzlit(&lit).clone();
        for puzlit in puzlit_set {
            if puzlit.sign() {
                let var = puzlit.var();
                let val = puzlit.val();
                let domain = self
                    .puzzleparse()
                    .direct
                    .domainmap
                    .get(&var)
                    .expect("Fatal error getting var")
                    .clone();
                for d in domain {
                    if d != val {
                        let new_puzlit = PuzLit::new_neq(VarValPair {
                            var: var.clone(),
                            val: d,
                        });
                        let new_lit = self.puzlit_to_lit(&new_puzlit);
                        if !self.knownlits.contains(&new_lit) {
                            self.add_known_lit_internal(new_lit);
                        }
                    }
                }
            }
        }
    }

    fn add_known_lit_internal(&mut self, lit: Lit) {
        if let Some(tosolvelits) = self.tosolvelits.as_mut() {
            tosolvelits.remove(&lit);
        }
        self.knownlits.push(lit);

        let lits = self.lit_to_puzlit(&lit).clone();

        for l in lits {
            // Only reveal from positive varvalpairs
            if !l.sign() {
                continue;
            }

            let name = l.varval().var().name().clone();
            if let Some(value) = self.puzzleparse.eprime.reveal.get(&name) {
                // Build the 'reveal' variable
                let value = value.clone();

                let mut vec = l.varval().var().indices().clone();
                vec.push(l.varval().val());

                let vvpair = VarValPair::new(&PuzVar::new(&value, vec), 1);
                let imply_lit = PuzLit::new_eq(vvpair);
                info!(target: "solver", "{l} reveals {imply_lit}");

                let puzlit = self
                    .puzzleparse()
                    .direct
                    .litmap
                    .get(&imply_lit)
                    .expect("REVEAL variable missing: {imply_lit}");
                self.knownlits.push(*puzlit);
                self.tosolvelits = None;
            }
        }
    }

    /// Get all literals known to be true.
    pub fn get_known_lits(&self) -> &Vec<Lit> {
        &self.knownlits
    }

    fn get_var_mus_size_1_loop(
        &self,
        lit: Lit,
        count: Option<usize>,
        lits: &[Lit],
        muses: &mut BTreeSet<Vec<Lit>>,
    ) -> SearchResult<()> {
        if lits.is_empty() || count.is_some_and(|x| muses.len() >= x) || muses.contains(&vec![])
        // size-0 MUS already found; every subset is UNSAT
        {
            return Ok(());
        }

        let mut lit_cpy = lits.to_vec();
        lit_cpy.push(!lit);

        let solvable = self
            .get_satcore()
            .assumption_solve_with_core(self.get_known_lits(), &lit_cpy)?;

        if let Some(core) = solvable {
            // Check for size-0 MUS: core contains only !lit, no constraint needed.
            if !core.iter().any(|&x| x != !lit) {
                muses.insert(vec![]);
                return Ok(());
            }

            if lits.len() == 1 {
                // The solver's core isn't guaranteed minimal: it may include the
                // constraint even when !lit alone suffices (size-0 MUS). Do one
                // final cheap check to find out which case we're in.
                let just_neg_lit = vec![!lit];
                let size0 = self
                    .get_satcore()
                    .assumption_solve(self.get_known_lits(), &just_neg_lit)?;
                if size0 {
                    muses.insert(lits.to_vec()); // constraint is needed: size-1 MUS
                } else {
                    muses.insert(vec![]); // !lit alone is UNSAT: size-0 MUS
                }
            } else {
                // This core can be found early. We might find it again later,
                // but we add it here as it might make us find enough cores (in particular
                // if we only want one))
                if core.len() == 2 {
                    let mus = core
                        .iter()
                        .copied()
                        .filter(|x| lits.contains(x))
                        .collect_vec();
                    assert!(mus.len() == 1);
                    muses.insert(mus);
                }
                let mid = lits.len() / 2;
                let (left, right) = lits.split_at(mid);
                self.get_var_mus_size_1_loop(lit, count, left, muses)?;
                self.get_var_mus_size_1_loop(lit, count, right, muses)?;
            }
        }

        Ok(())
    }

    /// Retrieves MUSes of size 0 or 1 for a given literal
    ///
    /// # Arguments
    ///
    /// * `lit` - The literal to find a proof for (so we invert for the MUS).
    /// * `count` - the largest number of MUSes to return (or None for all MUSes)
    ///
    /// # Returns
    ///
    /// An optional vector of vectors, containing the MUS of variables, or `None` if no MUS is found.
    pub fn get_var_mus_size_1(
        &self,
        lit: Lit,
        count: Option<usize>,
    ) -> SearchResult<Vec<Vec<Lit>>> {
        let mut conset = self
            .puzzleparse
            .constraints
            .lits()
            .iter()
            .copied()
            .collect_vec();

        let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(2);
        conset.shuffle(&mut rng);

        let mut muses: BTreeSet<Vec<Lit>> = BTreeSet::new();

        let mid = conset.len() / 2;
        let (left, right) = conset.split_at(mid);
        self.get_var_mus_size_1_loop(lit, count, left, &mut muses)?;
        self.get_var_mus_size_1_loop(lit, count, right, &mut muses)?;
        Ok(muses.into_iter().collect_vec())
    }

    /// Check if there is a MUS of size 0 for a given literal
    ///
    /// # Arguments
    ///
    /// * `lit` - The literal to find a proof for (so we invert for the MUS).
    ///
    /// # Returns
    ///
    /// A boolean, true if there is a MUS of size 0 for this literal.
    pub fn check_var_mus_size_0(&self, lit: Lit) -> bool {
        // First of all, check if there is a MUS of size 0,
        // mainly because it makes the rest of this algorithm
        // degenerate.
        let just_lit = vec![!lit];

        let solvable = self
            .get_satcore()
            .assumption_solve(self.get_known_lits(), &just_lit);

        if let Ok(solvable) = solvable {
            !solvable
        } else {
            // Treat a solver timeout as 'no MUS'
            false
        }
    }

    pub fn verify_mus_provability(&self, target_lit: Lit, mus_cons: &[Lit]) {
        let fresh_core =
            SatCore::new(self.puzzleparse.cnf.clone().unwrap()).expect("failed to create SatCore");

        let mut assumps: Vec<Lit> = mus_cons.to_vec();
        assumps.extend_from_slice(self.get_known_lits());
        assumps.push(!target_lit);

        let sat = fresh_core.assumption_solve_no_limit(self.get_known_lits(), &assumps);
        if sat {
            let target_name: Vec<_> = self
                .lit_to_puzlit(&target_lit)
                .iter()
                .map(|p| format!("{:?}", p))
                .collect();
            let con_names: Vec<_> = mus_cons
                .iter()
                .map(|c| {
                    self.puzzleparse()
                        .constraints
                        .try_description(c)
                        .cloned()
                        .unwrap_or_else(|| format!("unknown({})", c))
                })
                .collect();
            panic!(
                "MUS verification failed: MUS does not prove {}.\n  Target: {:?}\n  Constraints: {:?}",
                target_lit, target_name, con_names
            );
        }
    }

    pub fn verify_mus(&self, target_lit: Lit, mus_cons: &[Lit]) {
        self.verify_mus_provability(target_lit, mus_cons);

        let target_name: Vec<_> = self
            .lit_to_puzlit(&target_lit)
            .iter()
            .map(|p| format!("{:?}", p))
            .collect();
        let con_names: Vec<_> = mus_cons
            .iter()
            .map(|c| {
                self.puzzleparse()
                    .constraints
                    .try_description(c)
                    .cloned()
                    .unwrap_or_else(|| format!("unknown({})", c))
            })
            .collect();

        for i in 0..mus_cons.len() {
            let mut reduced: Vec<Lit> = mus_cons.to_vec();
            reduced.remove(i);
            reduced.push(!target_lit);

            let sat = self
                .get_satcore()
                .assumption_solve_no_limit(self.get_known_lits(), &reduced);
            assert!(
                sat,
                "MUS is not minimal: removing '{}' still UNSAT for {}.\n  Target: {:?}\n  Constraints: {:?}",
                con_names[i], target_lit, target_name, con_names
            );
        }
    }

    /// Retrieves a minimal unsatisfiable subset (MUS) of variables which proves
    /// a given literal is required
    ///
    /// # Arguments
    ///
    /// * `lit` - The literal to find a proof for.
    ///
    /// # Returns
    ///
    /// An optional vector containing the MUS of variables, or `None` if no MUS is found.
    pub fn get_var_mus_quick(
        &self,
        lit: Lit,
        max_size: Option<i64>,
    ) -> SearchResult<Option<Vec<Lit>>> {
        assert!(self.puzzleparse.var_lits.positive().contains(&lit));

        let mut lits: Vec<Lit> = vec![];
        lits.extend(self.puzzleparse.constraints.lits().iter());
        lits.push(!lit);
        let mus = self
            .get_satcore()
            .quick_mus(&self.knownlits, &lits, max_size.map(|x| x + 1))?;
        Ok(mus.map(|m| {
            m.into_iter()
                .filter(|x| self.puzzleparse.constraints.lits().contains(x))
                .collect()
        }))
    }

    pub fn get_var_mus_slice(
        &self,
        lit: Lit,
        max_size: Option<i64>,
    ) -> SearchResult<Option<Vec<Lit>>> {
        // let _t = QuickTimer::new(format!("get_var_mus_quick {:?}", lit));
        assert!(self.puzzleparse.var_lits.positive().contains(&lit));

        let mut lits: Vec<Lit> = vec![];

        let mut conset = self
            .puzzleparse
            .constraints
            .lits()
            .iter()
            .copied()
            .collect_vec();

        conset.shuffle(&mut rand::rng());

        // This code tries to deduce how many elements we can drop from 'conset', such that
        // we will still have an 80% chance of leaving a MUS of size 'max_size'.
        // The code is a bit more horrible than the simplest version, to make sure we do
        // not break when very large, or small, MUSes are required.

        let mut percentage_reduce = 0.4;

        if let Some(size) = max_size
            && size > 0
        {
            percentage_reduce = 1.0 - (size as f64) / (conset.len() as f64);
        }

        percentage_reduce = percentage_reduce.clamp(0.4, 0.9999);

        let trims = (0.8_f64.ln() / (percentage_reduce.ln())) as i64;

        let trims = trims.clamp(0, (conset.len() as i64) / 2);

        info!(target: "solver", "trimming {} from {} because max_size = {:?}", trims, conset.len(), max_size);

        lits.extend(conset.into_iter().skip(trims as usize));

        lits.push(!lit);
        let mus = self
            .get_satcore()
            .quick_mus(&self.knownlits, &lits, max_size.map(|x| x + 1))?;
        Ok(mus.map(|m| {
            m.into_iter()
                .filter(|x| self.puzzleparse.constraints.lits().contains(x))
                .collect()
        }))
    }

    fn search_one_mus(
        &self,
        lit: Lit,
        mus_test_size: i64,
        strategy: Strategy,
    ) -> (SearchResult<Option<Vec<Lit>>>, crate::stats::MusFunction) {
        match strategy {
            Strategy::Slice => (
                self.get_var_mus_slice(lit, Some(mus_test_size)),
                crate::stats::MusFunction::Slice,
            ),
            Strategy::Cake => (
                self.get_var_mus_cake(lit, mus_test_size),
                crate::stats::MusFunction::Cake,
            ),
            Strategy::Quick => (
                self.get_var_mus_quick(lit, Some(mus_test_size)),
                crate::stats::MusFunction::Quick,
            ),
            Strategy::Dynamic => {
                if mus_test_size < 5 {
                    (
                        self.get_var_mus_cake(lit, mus_test_size),
                        crate::stats::MusFunction::Cake,
                    )
                } else {
                    (
                        self.get_var_mus_slice(lit, Some(mus_test_size)),
                        crate::stats::MusFunction::Slice,
                    )
                }
            }
        }
    }

    pub fn get_var_mus_cake(&self, lit: Lit, max_size: i64) -> SearchResult<Option<Vec<Lit>>> {
        assert!(self.puzzleparse.var_lits.positive().contains(&lit));

        let mut conset = self
            .puzzleparse
            .constraints
            .lits()
            .iter()
            .copied()
            .collect_vec();

        conset.shuffle(&mut rand::rng());

        let num_groups = max_size as usize + 1;
        let conset_chunks: Vec<Vec<Lit>> = (0..num_groups)
            .map(|i| {
                conset
                    .iter()
                    .enumerate()
                    .filter_map(
                        |(j, &lit)| {
                            if j % num_groups == i { None } else { Some(lit) }
                        },
                    )
                    .collect()
            })
            .collect();

        for (i, chunk) in conset_chunks.iter().enumerate() {
            let mut lits: Vec<Lit> = chunk.clone();
            lits.push(!lit);
            let t0 = Instant::now();
            let mus = self
                .get_satcore()
                .quick_mus(&self.knownlits, &lits, Some(max_size + 1))?;
            info!(target: "musdetail", "cake chunk {}/{} lit={:?} bound={} chunk_size={} result={} {:.1?}",
                  i, num_groups, lit, max_size,
                  chunk.len(),
                  if mus.is_some() { format!("found({})", mus.as_ref().unwrap().len()) } else { "none".to_string() },
                  t0.elapsed());
            if let Some(m) = mus {
                return Ok(Some(
                    m.into_iter()
                        .filter(|x| self.puzzleparse.constraints.lits().contains(x))
                        .collect(),
                ));
            }
        }

        Ok(None)
    }

    pub fn core_size_summary(&self, lits: &BTreeSet<Lit>) -> (Option<usize>, usize) {
        let cores = self.get_all_cores(lits);
        let min = cores.iter().map(|(_, core)| core.len()).min();
        let count_1 = cores.iter().filter(|(_, core)| core.len() <= 1).count();
        (min, count_1)
    }

    /// For each provable literal, extract a raw SAT core (constraint-only).
    /// Returns `(lit, core)` pairs; lits where the SAT call fails are omitted.
    pub fn get_all_cores(&self, lits: &BTreeSet<Lit>) -> Vec<(Lit, Vec<Lit>)> {
        lits.par_iter()
            .filter_map(|&lit| {
                let mut assumptions: Vec<Lit> = self
                    .puzzleparse
                    .constraints
                    .lits()
                    .iter()
                    .copied()
                    .collect();
                assumptions.push(!lit);
                match self
                    .get_satcore()
                    .assumption_solve_with_core(&self.knownlits, &assumptions)
                {
                    Ok(Some(core)) => {
                        let con_core: Vec<Lit> = core
                            .into_iter()
                            .filter(|x| self.puzzleparse.constraints.lits().contains(x))
                            .collect();
                        Some((lit, con_core))
                    }
                    _ => None,
                }
            })
            .collect()
    }

    /// Minimise a raw core for a single literal into a true MUS.
    /// The `core` should contain only constraint lits (not `!lit`).
    pub fn minimise_core_for_lit(&self, lit: Lit, core: &[Lit]) -> SearchResult<Vec<Lit>> {
        let mut us = core.to_vec();
        us.push(!lit);
        let minimised = self.get_satcore().minimise_us(&self.knownlits, &us, None)?;
        Ok(minimised
            .into_iter()
            .filter(|x| self.puzzleparse.constraints.lits().contains(x))
            .collect())
    }

    /// Minimise a batch of `(lit, core)` pairs in parallel, returning a [`MusDict`].
    pub fn minimise_cores(&self, cores: &[(Lit, Vec<Lit>)]) -> MusDict {
        let results: Vec<_> = cores
            .par_iter()
            .filter_map(|(lit, core)| match self.minimise_core_for_lit(*lit, core) {
                Ok(mus) => Some((*lit, mus.into_iter().collect::<BTreeSet<Lit>>())),
                Err(_) => None,
            })
            .collect();
        let mut dict = MusDict::new();
        for (lit, mus) in results {
            dict.add_mus(lit, mus);
        }
        dict
    }

    pub fn get_many_vars_mus_size_0(&self, lits: &BTreeSet<Lit>) -> BTreeSet<Lit> {
        lits.par_iter()
            .filter(|&x| self.check_var_mus_size_0(*x))
            .cloned()
            .collect()
    }

    /// Retrieves an explanation for each element of a list of literals. This will often be
    /// much bigger than the minimum possible MUS size.
    ///
    /// # Arguments
    ///
    /// * `lits` - The literals to find the explanations for.
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains a literal and its corresponding MUS of variables.
    /// Literals where no MUS was found are omitted from the output.
    pub fn get_many_vars_mus_first(
        &self,
        lits: &BTreeSet<Lit>,
        musdict: Option<MusDict>,
    ) -> MusDict {
        let muses: Vec<_> = lits
            .par_iter()
            .map(|&x| (x, self.get_var_mus_quick(x, None)))
            .filter(|(_, y)| y.is_ok())
            .map(|(x, y)| (x, y.unwrap()))
            .filter(|(_, mus)| mus.is_some())
            .map(|(lit, mus)| (lit, mus.unwrap()))
            .collect();
        let mut md = musdict.unwrap_or_default();
        for (k, v) in muses {
            let bts: BTreeSet<Lit> = v.iter().copied().collect();
            md.add_mus(k, bts);
        }
        md
    }

    /// Retrieves small MUSes for each element of a list of literals
    ///
    /// # Arguments
    ///
    /// * `lits` - The literals to find the MUS for.
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains a literal and its corresponding MUS of variables.
    /// Literals with large MUSes are skipped. The exact set of returned literals may vary.
    pub fn get_many_vars_small_mus_quick(
        &self,
        lits: &BTreeSet<Lit>,
        config: &MusConfig,
        musdict: Option<MusDict>,
    ) -> MusDict {
        let md =
            Mutex::new(musdict.unwrap_or_else(|| MusDict::with_keep_all(config.keep_all_muses)));

        let _batch_timer = crate::stats::PhaseTimer::batch_mus();

        info!(target: "solve", "scanning for tiny muses");

        // Tiny scan: search every lit for a size-1 MUS unless one is already cached.
        let tiny_scan_lits: BTreeSet<Lit> = if config.find_bigger {
            lits.clone()
        } else {
            let g = md.lock().unwrap();
            lits.iter()
                .copied()
                .filter(|&lit| g.min_lit(lit).is_none_or(|s| s > 1))
                .collect()
        };

        tiny_scan_lits.iter().par_bridge().for_each(|&x| {
            let t0 = Instant::now();
            let ret = self.get_var_mus_size_1(x, Some(1));
            let elapsed = t0.elapsed();
            let outcome = match &ret {
                Ok(v) if !v.is_empty() => crate::stats::MusOutcome::Found(1),
                Ok(_) => crate::stats::MusOutcome::NotFound,
                Err(_) => crate::stats::MusOutcome::Timeout,
            };
            info!(target: "musdetail", "tiny  lit={:?} size=1 {:?} {:.1?}", x, outcome, elapsed);
            crate::stats::record_mus_search(elapsed, outcome, crate::stats::MusFunction::Size1);
            if let Ok(v) = ret
                && !v.is_empty()
            {
                let bts: BTreeSet<Lit> = v[0].iter().copied().collect();
                md.lock().unwrap().add_mus(x, bts);
            }
        });

        // If the tiny scan landed any new size-1 MUS, that's enough to make progress;
        // skip the larger search entirely. find_bigger wants the larger MUSes too,
        // so it always falls through.
        if !config.find_bigger && md.lock().unwrap().min_filtered(&tiny_scan_lits) == Some(1) {
            info!(target: "solve", "found tiny muses");
            return md.into_inner().unwrap();
        }

        // Core scan: get raw SAT cores for all lits. The minimum core size
        // is an upper bound on the minimum MUS size.
        let core_t0 = Instant::now();
        let cores = self.get_all_cores(lits);
        let core_elapsed = core_t0.elapsed();
        let min_core = cores.iter().map(|(_, core)| core.len()).min();
        let max_core = cores.iter().map(|(_, core)| core.len()).max();
        let mus_size =
            (min_core.unwrap_or(config.base_size_mus as usize) as i64).max(config.base_size_mus);
        info!(target: "solver", "scanning for {} muses, core bound = {:?}, mus_size = {}",
              lits.len(), min_core, mus_size);
        info!(target: "musdetail", "cores: {} lits, min={:?} max={:?} {:.1?}",
              cores.len(), min_core, max_core, core_elapsed);

        // Minimise the smallest cores into actual MUSes. These seed the
        // MusDict so the main search can tighten its bound immediately.
        if let Some(min) = min_core {
            let smallest: Vec<_> = cores
                .into_iter()
                .filter(|(_, core)| core.len() == min)
                .collect();
            info!(target: "musdetail", "minimising {} cores of size {}", smallest.len(), min);
            let min_t0 = Instant::now();
            let minimised = self.minimise_cores(&smallest);
            let min_elapsed = min_t0.elapsed();
            let n_muses: usize = minimised.muses().values().map(|s| s.len()).sum();
            let min_mus_size = minimised.min();
            info!(target: "musdetail", "minimised: {} muses, min_size={:?} {:.1?}",
                  n_muses, min_mus_size, min_elapsed);
            let mut g = md.lock().unwrap();
            for (lit, mus_set) in minimised.muses() {
                for mc in mus_set {
                    g.add_mus(*lit, mc.mus.clone());
                }
            }
        }

        let search_t0 = Instant::now();
        lits.iter()
            .flat_map(|&x| std::iter::repeat_n(x, config.repeats as usize))
            .par_bridge()
            .for_each(|x| {
                let mus_test_size = if config.find_bigger {
                    mus_size + 9
                } else {
                    let g = md.lock().unwrap();
                    match g.min_filtered(lits) {
                        Some(found) => {
                            let bound = (found as i64).min(mus_size);
                            if config.find_one { bound - 1 } else { bound }
                        }
                        None => mus_size,
                    }
                };

                if mus_test_size <= 1 {
                    info!(target: "musdetail", "skip  lit={:?} bound={} (<=1)", x, mus_test_size);
                    return;
                }

                let t0 = Instant::now();
                let (ret, func) = self.search_one_mus(x, mus_test_size, config.strategy);
                let elapsed = t0.elapsed();
                let outcome = match &ret {
                    Ok(Some(mus)) => crate::stats::MusOutcome::Found(mus.len()),
                    Ok(None) => crate::stats::MusOutcome::NotFound,
                    Err(_) => crate::stats::MusOutcome::Timeout,
                };
                info!(target: "musdetail", "search lit={:?} algo={:?} bound={} {:?} {:.1?}",
                      x, func, mus_test_size, outcome, elapsed);
                crate::stats::record_mus_search(elapsed, outcome, func);

                if let Ok(Some(y)) = ret {
                    let bts: BTreeSet<Lit> = y.iter().copied().collect();
                    md.lock().unwrap().add_mus(x, bts);
                }
            });
        info!(target: "musdetail", "main search done {:.1?}", search_t0.elapsed());

        if config.find_bigger {
            // find_bigger needs to keep growing beyond the initial core bound.
            let mus_min = md.lock().unwrap().min_filtered(lits);
            let met_target = mus_min.is_some_and(|m| (m as i64) * 3 + 3 <= mus_size);
            if !met_target {
                let mut grow_size = mus_size * config.mus_mult_step + config.mus_add_step;
                while grow_size <= i64::from(i32::MAX) {
                    info!(target: "solver", "find_bigger: scanning at size {}", grow_size);
                    lits.iter()
                        .flat_map(|&x| std::iter::repeat_n(x, config.repeats as usize))
                        .par_bridge()
                        .for_each(|x| {
                            let mus_test_size = grow_size + 9;
                            let t0 = Instant::now();
                            let (ret, func) =
                                self.search_one_mus(x, mus_test_size, config.strategy);
                            let elapsed = t0.elapsed();
                            let outcome = match &ret {
                                Ok(Some(mus)) => crate::stats::MusOutcome::Found(mus.len()),
                                Ok(None) => crate::stats::MusOutcome::NotFound,
                                Err(_) => crate::stats::MusOutcome::Timeout,
                            };
                            crate::stats::record_mus_search(elapsed, outcome, func);

                            if let Ok(Some(y)) = ret {
                                let bts: BTreeSet<Lit> = y.iter().copied().collect();
                                md.lock().unwrap().add_mus(x, bts);
                            }
                        });
                    let mus_min = md.lock().unwrap().min_filtered(lits);
                    if mus_min.is_some_and(|m| (m as i64) * 3 + 3 <= grow_size) {
                        break;
                    }
                    grow_size = grow_size * config.mus_mult_step + config.mus_add_step;
                }
            }
        }

        info!(target: "solver", "muses found!");
        md.into_inner().unwrap()
    }

    /// Retrieves a reference to the `PuzzleParse` instance associated with the `PuzzleSolver`.
    ///
    /// # Returns
    ///
    /// A reference to the `PuzzleParse` instance.
    #[must_use]
    pub fn puzzleparse(&self) -> &PuzzleParse {
        &self.puzzleparse
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeSet, HashSet},
        sync::Arc,
    };

    use crate::problem::solver::{MusConfig, PuzzleSolver, SolverConfig};

    use rand::SeedableRng;
    use test_log::test;

    #[test]
    fn test_parse_essence() -> anyhow::Result<()> {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/little1.eprime",
            "./tst/little1.param",
        );

        let result = Arc::new(result);

        let mut puz = PuzzleSolver::new(result)?;

        let varlits = puz.get_provable_varlits().clone();

        insta::assert_debug_snapshot!(varlits);
        insta::assert_debug_snapshot!(puz.get_literals_to_try_solving());

        assert_eq!(puz.get_known_lits(), &vec![]);

        let l = *varlits.first().unwrap();

        puz.add_known_lit(l);

        insta::assert_debug_snapshot!(puz.get_provable_varlits().clone());
        insta::assert_debug_snapshot!(puz.get_literals_to_try_solving());

        assert!(puz.get_known_lits().contains(&l));
        assert_eq!(puz.get_known_lits().len(), 5);

        assert_eq!(varlits.len(), 16);

        // Do a basic check we get a MUS for every varlit
        for &lit in &varlits {
            let mus = puz.get_var_mus_quick(lit, None)?;
            let mus_limit = puz.get_var_mus_quick(lit, Some(100))?;
            assert!(mus.is_some());
            assert!(mus_limit.is_some());
            println!("{lit:?} {mus:?}");
        }
        Ok(())
    }

    #[test]
    fn test_parse_essence_config() -> anyhow::Result<()> {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/little1.eprime",
            "./tst/little1.param",
        );

        let result = Arc::new(result);

        let mut puz = PuzzleSolver::new_with_config(
            result,
            SolverConfig {
                only_assignments: true,
            },
        )?;

        let varlits = puz.get_provable_varlits().clone();

        assert_eq!(puz.get_known_lits(), &vec![]);

        let l = *varlits.first().unwrap();

        puz.add_known_lit(l);

        assert!(puz.get_known_lits().contains(&l));
        assert_eq!(puz.get_known_lits().len(), 5);

        assert_eq!(varlits.len(), 4);

        // Do a basic check we get a MUS for every varlit
        for &lit in &varlits {
            let mus = puz.get_var_mus_quick(lit, None)?;
            let mus_limit = puz.get_var_mus_quick(lit, Some(100))?;
            assert!(mus.is_some());
            assert!(mus_limit.is_some());
            println!("{lit:?} {mus:?}");
        }
        Ok(())
    }

    #[test]
    fn test_known_lits() -> anyhow::Result<()> {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/little1.eprime",
            "./tst/little1.param",
        );

        let result = Arc::new(result);

        let mut puz = PuzzleSolver::new(result)?;

        let varlits = puz.get_provable_varlits().clone();

        assert_eq!(varlits.len(), 16);
        for &lit in &varlits {
            let puzlit = puz.lit_to_puzlit(&lit);
            for p in puzlit {
                let indices = p.var().indices;
                assert_eq!(indices.len(), 1);
                // In the solution, forAll i, x[i]=i
                // and the lits are the 'provable' lits
                assert_eq!(indices[0] == p.val(), p.sign());
            }
        }

        // Do a basic check we get a MUS for every varlit
        for &lit in &varlits {
            let mus = puz.get_var_mus_quick(lit, None)?.unwrap();
            let mus_limit = puz.get_var_mus_quick(lit, Some(100))?.unwrap();
            let tiny_muses = puz.get_var_mus_size_1(lit, None)?;
            let tiny_muses_1 = puz.get_var_mus_size_1(lit, Some(1))?;
            let cake_mus = puz.get_var_mus_cake(lit, 4)?.unwrap();
            assert_eq!(mus.len() == 1, !tiny_muses.is_empty());
            assert_eq!(!tiny_muses_1.is_empty(), !tiny_muses.is_empty());
            if mus.len() == 1 {
                assert!(tiny_muses.iter().any(|x| x == &mus));
                assert!(tiny_muses.iter().any(|x| x == &mus_limit));
                assert!(tiny_muses.iter().any(|x| x == &tiny_muses_1[0]));
                assert_eq!(cake_mus.len(), 1);
            }
            println!("{lit:?} {mus:?}");
        }

        // Check their negations have no mus (this isn't always true,
        // only for puzzles with only one solution)
        for &lit in &varlits {
            let lit = !lit;
            let mus = puz.get_var_mus_quick(lit, None)?;
            let mus_limit = puz.get_var_mus_quick(lit, Some(100))?;
            let tiny_muses = puz.get_var_mus_size_1(lit, None)?;
            let tiny_muses_1 = puz.get_var_mus_size_1(lit, Some(1))?;
            let cake_mus = puz.get_var_mus_cake(lit, 2)?;
            assert!(mus.is_none());
            assert!(mus_limit.is_none());
            assert!(tiny_muses.is_empty());
            assert!(tiny_muses_1.is_empty());
            assert!(cake_mus.is_none());
        }
        Ok(())
    }

    #[test]
    fn test_many_lits() -> anyhow::Result<()> {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/little1.eprime",
            "./tst/little1.param",
        );

        let result = Arc::new(result);

        let mut puz = PuzzleSolver::new(result)?;

        let varlits = puz.get_provable_varlits().clone();

        assert_eq!(varlits.len(), 16);
        for &lit in &varlits {
            let puzlit = puz.lit_to_puzlit(&lit);
            for p in puzlit {
                let indices = p.var().indices;
                assert_eq!(indices.len(), 1);
                // In the solution, forAll i, x[i]=i
                // and the lits are the 'provable' lits
                assert_eq!(indices[0] == p.val(), p.sign());
            }
        }

        let muses = puz.get_many_vars_mus_first(&varlits, None);
        let muses_quick = puz.get_many_vars_small_mus_quick(&varlits, &MusConfig::default(), None);

        assert!(!muses.is_empty());
        assert!(!muses_quick.is_empty());

        let muses_2 = puz.get_many_vars_mus_first(
            &(varlits.iter().map(|&x| !x).collect()),
            Some(muses.clone()),
        );
        let muses_quick_2 = puz.get_many_vars_mus_first(
            &(varlits.iter().map(|&x| !x).collect()),
            Some(muses_quick.clone()),
        );

        assert!(!muses_2.is_empty());
        assert!(!muses_quick_2.is_empty());

        assert_eq!(muses.min(), muses_2.min());
        assert_eq!(muses_quick.min(), muses_quick_2.min());

        for (l, btree) in muses_2.muses() {
            for mus in btree {
                let list = puz.get_varlits_provable_by_mus(&varlits, mus);
                let scopelist = puz.get_all_lits_solved_by_mus(mus);
                assert!(&list.contains(l));
                assert!(&scopelist.lits.contains(l));
                assert_eq!(
                    list.iter().collect::<HashSet<_>>(),
                    scopelist.lits.iter().collect::<HashSet<_>>()
                );
            }
        }

        let neg_muses = puz.get_many_vars_mus_first(&(varlits.iter().map(|&x| !x).collect()), None);
        let neg_muses_quick =
            puz.get_many_vars_mus_first(&(varlits.iter().map(|&x| !x).collect()), None);

        assert!(neg_muses.is_empty());
        assert!(neg_muses_quick.is_empty());

        let neg_muses_2 = puz.get_many_vars_mus_first(
            &(varlits.iter().map(|&x| !x).collect()),
            Some(neg_muses.clone()),
        );
        let neg_muses_quick_2 = puz.get_many_vars_mus_first(
            &(varlits.iter().map(|&x| !x).collect()),
            Some(neg_muses_quick.clone()),
        );

        assert!(neg_muses_2.is_empty());
        assert!(neg_muses_quick_2.is_empty());

        Ok(())
    }

    #[test]
    fn test_random_solution_little() -> anyhow::Result<()> {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/little1.eprime",
            "./tst/little1.param",
        );

        let result = Arc::new(result);

        let mut gens = BTreeSet::new();

        for i in 0..11 {
            let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(i);

            let mut puz = PuzzleSolver::new(result.clone())?;

            let sol = if i == 11 {
                puz.random_solution(&mut rng, None)
                    .expect("unconstrained little1 must have a solution")
            } else {
                puz.random_solution(&mut rng, Some(i as usize))
                    .expect("unconstrained little1 must have a solution")
            };

            gens.insert(sol);
        }

        assert_eq!(gens.len(), 1);

        let sol = gens.into_iter().next().unwrap();

        insta::assert_debug_snapshot!(sol);

        let puz = PuzzleSolver::new(result)?;

        let puzsol: BTreeSet<_> = sol
            .into_iter()
            .flat_map(|lit| puz.lit_to_puzlit(&lit))
            .collect();

        insta::assert_debug_snapshot!(puzsol);

        Ok(())
    }

    #[test]
    fn test_random_solution_wall() -> anyhow::Result<()> {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/minesweeper.eprime",
            "./tst/minesweeperWall.param",
        );

        let result = Arc::new(result);

        let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(2);

        let mut puz = PuzzleSolver::new(result)?;

        let sol = puz
            .random_solution(&mut rng, None)
            .expect("minesweeperWall must have a solution");

        insta::assert_debug_snapshot!(sol);

        let puzsol: BTreeSet<_> = sol
            .into_iter()
            .flat_map(|lit| puz.lit_to_puzlit(&lit))
            .collect();

        insta::assert_debug_snapshot!(puzsol);

        Ok(())
    }
}

use std::ops::Neg;
use std::sync::Arc;
use std::{collections::BTreeSet, sync::atomic::AtomicI64};

use std::sync::atomic::Ordering::Relaxed;

use itertools::Itertools;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
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
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Strategy {
    /// Uses a quick algorithm that may find larger MUSes
    Quick,
    /// Uses a slicing technique to find smaller MUSes
    Slice,
    /// Uses a "cake cutting" technique to find small MUSes
    Cake,
    /// Uses 'cake cutting' for smaller MUSes, slice for larger
    Dynamic,
}

impl Default for Strategy {
    fn default() -> Self {
        Self::Dynamic
    }
}

#[derive(Copy, Clone)]
pub struct MusConfig {
    pub base_size_mus: i64,
    pub mus_add_step: i64,
    pub mus_mult_step: i64,
    pub repeats: i64,
    pub find_bigger: bool,
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
        *self.puzzleparse.litmap.get(puzlit).unwrap_or_else(|| {
            dbg!(&self.puzzleparse.litmap);
            panic!("Missing puzlit: {puzlit}");
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
            .invlitmap
            .get(lit)
            .unwrap_or_else(|| panic!("Mizzing lit: {lit}"))
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
        let mut litorig: Vec<Lit> = self.puzzleparse.conset_lits.iter().copied().collect();
        litorig.extend_from_slice(&self.knownlits);
        self.get_satcore()
            .assumption_solve(self.get_known_lits(), &litorig)
            .expect("Solving the basic problem took too long, solver timed out (type 2)")
    }

    /// Retrieves variable literals which can be proved.
    ///
    /// # Returns
    ///
    /// A vector containing the provable variable literals.
    #[must_use]
    pub fn get_provable_varlits(&mut self) -> &BTreeSet<Lit> {
        if self.tosolvelits.is_none() {
            let mut litorig: Vec<Lit> = self.puzzleparse.conset_lits.iter().copied().collect();
            litorig.extend_from_slice(&self.knownlits);
            let lits = self.get_literals_to_try_solving();
            let provable: BTreeSet<_> = lits
                .par_iter()
                .filter_map(|&lit| {
                    if !(self.knownlits.contains(&lit) || self.knownlits.contains(&!lit)) {
                        let mut lits = litorig.clone();
                        lits.push(lit);
                        if !self
                            .get_satcore()
                            .assumption_solve(self.get_known_lits(), &lits)
                            .expect("Solving the basic problem took too long, solver timed out")
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
        assert!(mus.iter().all(|c| self.puzzleparse.conset_lits.contains(c)));

        let mut litorig = mus.clone();
        for &lit in &self.knownlits {
            litorig.insert(lit);
        }

        let provable: BTreeSet<_> = candidates
            .iter()
            .filter_map(|&lit| {
                // This literal should be provable, so we invert it for testing
                let lit = !lit;
                if !(self.knownlits.contains(&lit) || self.knownlits.contains(&!lit)) {
                    let mut lits = litorig.iter().copied().collect_vec();
                    lits.push(lit);
                    if !self
                        .get_satcore()
                        .assumption_solve(self.get_known_lits(), &lits)
                        .expect("Solving the basic problem took too long, solver timed out")
                    {
                        return Some(!lit);
                    }
                }
                None
            })
            .collect();

        provable
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
            for l in self.puzzleparse().varlits_in_con.get(m).unwrap() {
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
        MusContext::new_with_more_lits(filtered, mc)
    }

    /// Generate a random solution. This will not enforce that the problem
    /// has a unique solution, only that it has a solution. The solution
    /// is generated by a random dive through all literals.
    ///
    /// All 'REVEAL' variables are forced to 'true'.
    /// 'steps' is how many variables to assign randomly, if 'None',
    /// then all variables are assigned randomly (which achieves the most
    /// randomness).
    pub fn random_solution(
        &mut self,
        rng: &mut ChaCha20Rng,
        mut steps: Option<usize>,
    ) -> BTreeSet<Lit> {
        let mut solution = vec![];

        let mut litorig: Vec<Lit> = self.puzzleparse.conset_lits.iter().copied().collect();
        litorig.extend_from_slice(&self.knownlits);

        let reveal_lits: Vec<_> = self.puzzleparse.reveal_map.values().copied().collect();
        litorig.extend_from_slice(&reveal_lits);

        let mut lits_to_check = self.puzzleparse.varset_lits.iter().copied().collect_vec();

        lits_to_check.shuffle(rng);

        for &l in &lits_to_check {
            let mut lits = litorig.clone();
            let test_lit = if rng.random_bool(0.5) { l } else { l.neg() };

            lits.push(test_lit);

            if self
                .get_satcore()
                .assumption_solve(self.get_known_lits(), &lits)
                .expect("??? solver took too long")
            {
                solution.push(test_lit);
                litorig.push(test_lit);
            } else {
                // This should never fail, but let's check
                let test_lit = test_lit.neg();
                let mut lits = litorig.clone();
                lits.push(test_lit);
                if self
                    .get_satcore()
                    .assumption_solve(self.get_known_lits(), &lits)
                    .expect("??? solver took too long")
                {
                    solution.push(test_lit);
                    litorig.push(test_lit);
                } else {
                    panic!("Trying to find a solution to a problem with no answer??!??")
                }
            }

            if steps == Some(0) {
                let sol = self
                    .get_satcore()
                    .assumption_solve_solution(self.get_known_lits(), &litorig)
                    .expect("Solver too slow...?!?!")
                    .expect("Must be a solution, from previous call??!?");

                for &l in &lits_to_check {
                    match sol.lit_value(l) {
                        rustsat::types::TernaryVal::True => {
                            solution.push(l);
                        }
                        rustsat::types::TernaryVal::False => {}
                        rustsat::types::TernaryVal::DontCare => panic!("Missing assignment??!?"),
                    }
                }
                return solution.into_iter().collect();
            }
            steps = steps.map(|x| x - 1);
        }

        solution.into_iter().collect()
    }

    /// Returns the set of literals which we should still try solving (may be true, or false)
    pub fn get_literals_to_try_solving(&mut self) -> BTreeSet<Lit> {
        let lits = if self.solver_config.only_assignments {
            &self.puzzleparse.varset_lits_neg
        } else {
            &self.puzzleparse.varset_lits
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

    /// Forces a literal to be true, without checking if
    /// this can be logically deduced.
    ///
    /// # Arguments
    ///
    /// * `lit` - The literal to add.
    fn add_known_lit_unchecked(&mut self, lit: Lit) {
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
        if lits.is_empty() || count.is_some_and(|x| muses.len() >= x) {
            return Ok(());
        }

        let mut lit_cpy = lits.to_vec();
        lit_cpy.push(!lit);

        let solvable = self
            .get_satcore()
            .assumption_solve_with_core(self.get_known_lits(), &lit_cpy)?;

        if let Some(core) = solvable {
            if lits.len() == 1 {
                muses.insert(lits.to_vec());
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
        // First of all, check if there is a MUS of size 0,
        // mainly because it makes the rest of this algorithm
        // degenerate.
        let just_lit = vec![!lit];

        let solvable = self
            .get_satcore()
            .assumption_solve(self.get_known_lits(), &just_lit)?;

        if !solvable {
            return Ok(vec![vec![]]);
        }

        let mut conset = self.puzzleparse.conset_lits.iter().copied().collect_vec();

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
        assert!(self.puzzleparse.varset_lits.contains(&lit));

        let mut lits: Vec<Lit> = vec![];
        lits.extend(self.puzzleparse.conset_lits.iter());
        lits.push(!lit);
        let mus = self
            .get_satcore()
            .quick_mus(&self.knownlits, &lits, max_size.map(|x| x + 1))?;
        Ok(mus.map(|m| {
            m.into_iter()
                .filter(|x| self.puzzleparse.conset_lits.contains(x))
                .collect()
        }))
    }

    pub fn get_var_mus_slice(
        &self,
        lit: Lit,
        max_size: Option<i64>,
    ) -> SearchResult<Option<Vec<Lit>>> {
        // let _t = QuickTimer::new(format!("get_var_mus_quick {:?}", lit));
        assert!(self.puzzleparse.varset_lits.contains(&lit));

        let mut lits: Vec<Lit> = vec![];

        let mut conset = self.puzzleparse.conset_lits.iter().copied().collect_vec();

        conset.shuffle(&mut rand::rng());

        // This code tries to deduce how many elements we can drop from 'conset', such that
        // we will still have an 80% chance of leaving a MUS of size 'max_size'.
        // The code is a bit more horrible than the simplest version, to make sure we do
        // not break when very large, or small, MUSes are required.

        let mut percentage_reduce = 0.4;

        if let Some(size) = max_size {
            if size > 0 {
                percentage_reduce = 1.0 - (size as f64) / (conset.len() as f64);
            }
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
                .filter(|x| self.puzzleparse.conset_lits.contains(x))
                .collect()
        }))
    }

    pub fn get_var_mus_cake(&self, lit: Lit, max_size: i64) -> SearchResult<Option<Vec<Lit>>> {
        // let _t = QuickTimer::new(format!("get_var_mus_quick {:?}", lit));
        assert!(self.puzzleparse.varset_lits.contains(&lit));

        let mut conset = self.puzzleparse.conset_lits.iter().copied().collect_vec();

        conset.shuffle(&mut rand::rng());

        let conset_chunks: Vec<Vec<Lit>> = (0..=max_size)
            .map(|i| {
                conset
                    .iter()
                    .enumerate()
                    .filter_map(|(j, &lit)| {
                        if j % (max_size as usize + 1) == i as usize {
                            None
                        } else {
                            Some(lit)
                        }
                    })
                    .collect()
            })
            .collect();

        for chunk in conset_chunks {
            let mut lits: Vec<Lit> = vec![];
            lits.extend(chunk);
            lits.push(!lit);
            let mus = self
                .get_satcore()
                .quick_mus(&self.knownlits, &lits, Some(max_size + 1))?;
            if let Some(m) = mus {
                return Ok(Some(
                    m.into_iter()
                        .filter(|x| self.puzzleparse.conset_lits.contains(x))
                        .collect(),
                ));
            }
            lits.clear();
        }

        Ok(None)
    }

    /// Retrieves the literals which can be solved with a size 0 MUS.
    ///
    /// # Arguments
    ///
    /// * `lits` - The literals to check MUSes for.
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains a literal and its corresponding MUS of variables.
    /// Literals where no MUS was found are omitted from the output.
    pub fn get_many_vars_mus_size_0(&self, lits: &BTreeSet<Lit>) -> BTreeSet<Lit> {
        let lits = lits
            .par_iter()
            .filter(|&x| self.check_var_mus_size_0(*x))
            .cloned()
            .collect();

        lits
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
        let mut md = musdict.unwrap_or_default();

        let mut mus_size = config.base_size_mus;
        let best_mus_size = AtomicI64::new(config.base_size_mus);

        info!(target: "solve", "scanning for tiny muses");

        let muses: Vec<_> = lits
            .iter()
            .par_bridge()
            .map(|&x| {
                let ret = self.get_var_mus_size_1(x, Some(1));
                (x, ret)
            })
            .filter(|(_, y)| y.is_ok())
            .map(|(x, y)| (x, y.unwrap()))
            .filter(|(_, mus)| !mus.is_empty())
            .map(|(lit, mus)| (lit, mus[0].clone()))
            .collect();

        if !muses.is_empty() && !config.find_bigger {
            info!(target: "solve", "found tiny muses");
            for (k, v) in muses {
                let bts = v.iter().copied().collect();
                md.add_mus(k, bts);
            }
            return md;
        }

        info!(target: "solver", "scanning for {} muses", lits.len());
        loop {
            info!(target: "solver", "scanning for muses size {}", mus_size);
            best_mus_size.store(mus_size, Relaxed);
            let muses: Vec<_> = lits
                .iter()
                .flat_map(|x| std::iter::repeat_n(x, config.repeats as usize))
                .par_bridge()
                .map(|&x| {
                    let mus_test_size = best_mus_size.load(Relaxed);
                    let mus_test_size = if config.find_bigger {
                        mus_test_size + 3 * 3
                    } else {
                        mus_test_size
                    };

                    let ret = match config.strategy {
                        Strategy::Slice => self.get_var_mus_slice(x, Some(mus_test_size)),
                        Strategy::Cake => self.get_var_mus_cake(x, mus_test_size),
                        Strategy::Quick => self.get_var_mus_quick(x, Some(mus_test_size)),
                        Strategy::Dynamic => {
                            if mus_test_size < 5 {
                                self.get_var_mus_cake(x, mus_test_size)
                            } else {
                                self.get_var_mus_slice(x, Some(mus_test_size))
                            }
                        }
                    };
                    if let Ok(Some(y)) = &ret {
                        best_mus_size.fetch_min(y.len() as i64, Relaxed);
                    }
                    (x, ret)
                })
                .filter(|(_, y)| y.is_ok())
                .map(|(x, y)| (x, y.unwrap()))
                .filter(|(_, mus)| mus.is_some())
                .map(|(lit, mus)| (lit, mus.unwrap()))
                .collect();

            for (k, v) in muses {
                let bts = v.iter().copied().collect();
                md.add_mus(k, bts);
            }

            if let Some(mus_min) = md.min() {
                let met_target = if config.find_bigger {
                    (mus_min as i64) * 3 + 3 <= mus_size
                } else {
                    mus_min as i64 <= mus_size
                };
                if met_target {
                    info!(target: "solver", "muses found!");
                    return md;
                }
            }
            // Make sure we stop, if something stupid has happened
            if mus_size > i64::from(i32::MAX) {
                info!(target: "solver", "no muses found!");
                return md;
            }
            mus_size = mus_size * config.mus_mult_step + config.mus_add_step;
        }
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
            let cake_mus = puz.get_var_mus_cake(lit, 3)?.unwrap();
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
            } else {
                puz.random_solution(&mut rng, Some(i as usize))
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

        let sol = puz.random_solution(&mut rng, None);

        insta::assert_debug_snapshot!(sol);

        let puzsol: BTreeSet<_> = sol
            .into_iter()
            .flat_map(|lit| puz.lit_to_puzlit(&lit))
            .collect();

        insta::assert_debug_snapshot!(puzsol);

        Ok(())
    }
}

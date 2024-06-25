use std::collections::BTreeSet;

use itertools::Itertools;
use rand::seq::SliceRandom;
use rayon::iter::{IntoParallelRefIterator, ParallelBridge, ParallelIterator};
use rustsat::types::Lit;
use thread_local::ThreadLocal;
use tracing::info;

use crate::{
    problem::{PuzVar, VarValPair},
    satcore::SatCore,
};

use super::{musdict::MusDict, parse::PuzzleParse, PuzLit};

pub struct MusConfig {
    pub base_size_mus: i64,
    pub mus_add_step: i64,
    pub mus_mult_step: i64,
    pub repeats: i64,
}

impl Default for MusConfig {
    fn default() -> Self {
        Self {
            base_size_mus: 2,
            mus_add_step: 1,
            mus_mult_step: 2,
            repeats: 5,
        }
    }
}

/// Represents a puzzle solver.
pub struct PuzzleSolver {
    satcore: ThreadLocal<SatCore>,
    puzzleparse: PuzzleParse,

    knownlits: Vec<Lit>,
    tosolvelits: Option<BTreeSet<Lit>>,
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
    pub fn new(puzzleparse: PuzzleParse) -> anyhow::Result<PuzzleSolver> {
        Ok(PuzzleSolver {
            satcore: ThreadLocal::new(),
            puzzleparse,
            tosolvelits: None,
            knownlits: Vec::new(),
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
        *self.puzzleparse.litmap.get(puzlit).unwrap()
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
        self.puzzleparse.invlitmap.get(lit).unwrap()
    }

    /// Retrieves variable literals which can be proved.
    ///
    /// # Returns
    ///
    /// A vector containing the provable variable literals.
    #[must_use]
    pub fn get_provable_varlits(&mut self) -> &BTreeSet<Lit> {
        if self.tosolvelits.is_none() {
            let mut provable = BTreeSet::new();

            let mut litorig: Vec<Lit> = self.puzzleparse.conset_lits.iter().copied().collect();
            litorig.extend_from_slice(&self.knownlits);

            for &lit in &self.puzzleparse.varset_lits {
                if !self.knownlits.contains(&!lit) {
                    let mut lits = litorig.clone();
                    lits.push(lit);
                    if !self.get_satcore().assumption_solve(&lits) {
                        provable.insert(!lit);
                    }
                }
            }

            self.tosolvelits = Some(provable);
        }

        self.tosolvelits.as_ref().unwrap()
    }

    /// Adds a literal which is known to be true.
    ///
    /// # Arguments
    ///
    /// * `lit` - The literal to add.
    pub fn add_known_lit(&mut self, lit: Lit) {
        assert!(self.get_provable_varlits().contains(&lit));
        self.add_known_lit_unchecked(lit);
    }

    /// Forces a literal to be true, without checking if
    /// this can be logically deduced.
    ///
    /// # Arguments
    ///
    /// * `lit` - The literal to add.
    pub fn add_known_lit_unchecked(&mut self, lit: Lit) {
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

    /// Retrieves the minimal unsatisfiable subset (MUS) of variables which proves
    /// a given literal is required
    ///
    /// # Arguments
    ///
    /// * `lit` - The literal to find a proof for (so we invert for the MUS).
    ///
    /// # Returns
    ///
    /// An optional vector containing the MUS of variables, or `None` if no MUS is found.
    #[must_use]
    pub fn get_var_mus_quick(&self, lit: Lit, max_size: Option<i64>) -> Option<Vec<Lit>> {
        assert!(self.puzzleparse.varset_lits.contains(&lit));

        let mut lits: Vec<Lit> = vec![];
        lits.extend(self.puzzleparse.conset_lits.iter());
        lits.push(!lit);
        let mus = self
            .get_satcore()
            .quick_mus(&self.knownlits, &lits, max_size.map(|x| x + 1));
        mus.map(|m| {
            m.into_iter()
                .filter(|x| self.puzzleparse.conset_lits.contains(x))
                .collect()
        })
    }

    #[must_use]
    pub fn get_var_mus_slice(&self, lit: Lit, max_size: Option<i64>) -> Option<Vec<Lit>> {
        assert!(self.puzzleparse.varset_lits.contains(&lit));

        let mut lits: Vec<Lit> = vec![];

        let mut conset = self.puzzleparse.conset_lits.iter().copied().collect_vec();

        conset.shuffle(&mut rand::thread_rng());

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
            .quick_mus(&self.knownlits, &lits, max_size.map(|x| x + 1));
        mus.map(|m| {
            m.into_iter()
                .filter(|x| self.puzzleparse.conset_lits.contains(x))
                .collect()
        })
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
    pub fn get_many_vars_mus_first(&self, lits: &BTreeSet<Lit>) -> MusDict {
        let muses: Vec<_> = lits
            .par_iter()
            .map(|&x| (x, self.get_var_mus_quick(x, None)))
            .filter(|(_, mus)| mus.is_some())
            .map(|(lit, mus)| (lit, mus.unwrap()))
            .collect();
        let mut md = MusDict::new();
        for (k, v) in muses {
            md.add_mus(k, v);
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
    ) -> MusDict {
        let mut md = MusDict::new();
        let mut mus_size = config.base_size_mus;
        info!(target: "solver", "scanning for {} muses", lits.len());
        loop {
            info!(target: "solver", "scanning for muses size {}", mus_size);
            let muses: Vec<_> = lits
                .iter()
                .flat_map(|x| std::iter::repeat(x).take(config.repeats as usize))
                .par_bridge()
                .map(|&x| (x, self.get_var_mus_slice(x, Some(mus_size))))
                .filter(|(_, mus)| mus.is_some())
                .map(|(lit, mus)| (lit, mus.unwrap()))
                .collect();

            for (k, v) in muses {
                md.add_mus(k, v);
            }

            if let Some(mus_min) = md.min() {
                if mus_min as i64 <= mus_size {
                    info!(target: "solver", "muses found!");
                    return md;
                }
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
    use crate::problem::solver::{MusConfig, PuzzleSolver};

    use test_log::test;

    #[test]
    fn test_parse_essence() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/little1.eprime",
            "./tst/little1.param",
        );

        let mut puz = PuzzleSolver::new(result).unwrap();

        let varlits = puz.get_provable_varlits().clone();

        assert_eq!(puz.get_known_lits(), &vec![]);

        let l = *varlits.first().unwrap();

        puz.add_known_lit(l);

        assert!(puz.get_known_lits().contains(&l));
        assert_eq!(puz.get_known_lits().len(), 2);

        assert_eq!(varlits.len(), 16);

        // Do a basic check we get a MUS for every varlit
        for &lit in &varlits {
            let mus = puz.get_var_mus_quick(lit, None);
            let mus_limit = puz.get_var_mus_quick(lit, Some(100));
            assert!(mus.is_some());
            assert!(mus_limit.is_some());
            println!("{lit:?} {mus:?}");
        }
    }

    #[test]
    fn test_known_lits() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/little1.eprime",
            "./tst/little1.param",
        );

        let mut puz = PuzzleSolver::new(result).unwrap();

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
            let mus = puz.get_var_mus_quick(lit, None);
            let mus_limit = puz.get_var_mus_quick(lit, Some(100));
            assert!(mus.is_some());
            assert!(mus_limit.is_some());
            println!("{lit:?} {mus:?}");
        }

        // Check their negations have no mus (this isn't always true,
        // only for puzzles with only one solution)
        for &lit in &varlits {
            let lit = !lit;
            let mus = puz.get_var_mus_quick(lit, None);
            let mus_limit = puz.get_var_mus_quick(lit, Some(100));
            assert!(mus.is_none());
            assert!(mus_limit.is_none());
        }
    }

    #[test]
    fn test_many_lits() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/little1.eprime",
            "./tst/little1.param",
        );

        let mut puz = PuzzleSolver::new(result).unwrap();

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

        let muses = puz.get_many_vars_mus_first(&varlits);
        let muses_quick = puz.get_many_vars_small_mus_quick(&varlits, &MusConfig::default());

        assert!(!muses.is_empty());
        assert!(!muses_quick.is_empty());

        let neg_muses = puz.get_many_vars_mus_first(&(varlits.iter().map(|&x| !x).collect()));
        let neg_muses_quick = puz.get_many_vars_mus_first(&(varlits.iter().map(|&x| !x).collect()));

        assert!(neg_muses.is_empty());
        assert!(neg_muses_quick.is_empty());
    }
}

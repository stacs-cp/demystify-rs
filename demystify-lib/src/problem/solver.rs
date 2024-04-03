use std::collections::BTreeSet;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rustsat::types::Lit;
use thread_local::ThreadLocal;

use crate::satcore::SatCore;

use super::{parse::PuzzleParse, PuzLit};

pub struct PuzzleSolver {
    satcore: ThreadLocal<SatCore>,
    puzzleparse: PuzzleParse,
    knownlits: Vec<Lit>,
}

impl PuzzleSolver {
    pub fn new(puzzleparse: PuzzleParse) -> anyhow::Result<PuzzleSolver> {
        Ok(PuzzleSolver {
            satcore: ThreadLocal::new(),
            puzzleparse,
            knownlits: Vec::new(),
        })
    }

    fn get_satcore(&self) -> &SatCore {
        self.satcore
            .get_or(|| SatCore::new(self.puzzleparse.cnf.clone().unwrap()).unwrap())
    }

    fn puzlit_to_lit(&self, puzlit: PuzLit) -> Lit {
        *self.puzzleparse.litmap.get(&puzlit).unwrap()
    }

    fn lit_to_puzlit(&self, lit: Lit) -> &BTreeSet<PuzLit> {
        self.puzzleparse.invlitmap.get(&lit).unwrap()
    }

    #[must_use]
    pub fn get_unsatisfiable_varlits(&self) -> Vec<Lit> {
        let mut satisfied = vec![];

        let mut litorig: Vec<Lit> = self.puzzleparse.conset_lits.iter().copied().collect();
        litorig.extend_from_slice(&self.knownlits);

        for &lit in &self.puzzleparse.varset_lits {
            let mut lits = litorig.clone();
            lits.push(lit);
            if !self.get_satcore().assumption_solve(&lits) {
                satisfied.push(lit);
            }
        }

        satisfied
    }

    pub fn add_known_lit(&mut self, lit: Lit) {
        self.knownlits.push(lit);
        // we could add the literal to the solver, but then it can't
        // be backtracked.. but it might be faster! Investigate later.
        //self.get_satcore().add_lit(lit);
    }

    #[must_use]
    pub fn get_var_mus_quick(&self, lit: Lit) -> Option<Vec<Lit>> {
        assert!(self.puzzleparse.varset_lits.contains(&lit));

        let mut lits: Vec<Lit> = vec![];
        lits.extend(self.puzzleparse.conset_lits.iter());
        lits.push(lit);
        let mus = self.get_satcore().quick_mus(&self.knownlits, &lits);
        mus.map(|m| {
            m.into_iter()
                .filter(|x| self.puzzleparse.conset_lits.contains(x))
                .collect()
        })
    }

    pub fn get_many_vars_mus_quick(&self, lits: &[Lit]) -> Vec<(Lit, Vec<Lit>)> {
        let muses: Vec<_> = lits
            .par_iter()
            .map(|&x| (x, self.get_var_mus_quick(x).unwrap()))
            .collect();
        muses
    }

    #[must_use]
    pub fn puzzleparse(&self) -> &PuzzleParse {
        &self.puzzleparse
    }
}

#[cfg(test)]
mod tests {
    use crate::problem::solver::PuzzleSolver;
    use test_log::test;

    #[test]
    fn test_parse_essence() {
        let result = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/little1.eprime",
            "./tst/little1.param",
        );

        let puz = PuzzleSolver::new(result).unwrap();

        let varlits = puz.get_unsatisfiable_varlits();

        assert_eq!(varlits.len(), 16);
        for &lit in &varlits {
            let puzlit = puz.lit_to_puzlit(lit);
            for p in puzlit {
                let indices = p.var().indices;
                assert_eq!(indices.len(), 1);
                // In the solution, forAll i, x[i]=i
                // and the lits are the 'unsatisfiable' lits
                assert_eq!(indices[0] == p.val(), !p.sign());
            }
        }

        // Do a basic check we get a MUS for every varlit
        for &lit in &varlits {
            let mus = puz.get_var_mus_quick(lit);
            assert!(mus.is_some());
            print!("{lit:?} {mus:?}");
        }

        // Check their negations have no mus (this isn't always true,
        // only for puzzles with only one solution)
        for &lit in &varlits {
            let lit = !lit;
            let mus = puz.get_var_mus_quick(lit);
            assert!(mus.is_none());
        }
    }
}

use std::collections::BTreeSet;

use rustsat::types::Lit;

use crate::satcore::SatCore;

use super::{parse::PuzzleParse, PuzLit};

pub struct PuzzleSolver {
    satcore: SatCore,
    puzzleparse: PuzzleParse,
}

impl PuzzleSolver {
    fn new(puzzleparse: PuzzleParse) -> anyhow::Result<PuzzleSolver> {
        let satcore = SatCore::new(puzzleparse.satinstance.clone().as_cnf().0)?;
        Ok(PuzzleSolver {
            satcore,
            puzzleparse,
        })
    }

    fn puzlit_to_lit(&self, puzlit: PuzLit) -> Lit {
        *self.puzzleparse.litmap.get(&puzlit).unwrap()
    }

    fn lit_to_puzlit(&self, lit: Lit) -> &BTreeSet<PuzLit> {
        self.puzzleparse.invlitmap.get(&lit).unwrap()
    }

    fn get_unsatisfiable_varlits(&self) -> Vec<Lit> {
        let mut satisfied = vec![];

        for &lit in &self.puzzleparse.varset_lits {
            let mut lits: Vec<Lit> = self.puzzleparse.conset_lits.iter().cloned().collect();
            lits.push(lit);
            if !self.satcore.assumption_solve(&lits) {
                satisfied.push(lit);
            }
        }

        satisfied
    }
}

#[cfg(test)]
mod tests {
    use crate::problem::{parse::parse_essence, solver::PuzzleSolver};
    use std::fs;
    use test_log::test;

    #[test]
    fn test_parse_essence() {
        let eprime_path = "./tst/little1.eprime";
        let eprimeparam_path = "./tst/little1.param";

        // Create temporary directory for test files
        let temp_dir = tempfile::tempdir().expect("Failed to create temporary directory");

        // Copy eprime file to temporary directory
        let temp_eprime_path = temp_dir.path().join("little1.eprime");
        fs::copy(eprime_path, &temp_eprime_path).expect("Failed to copy eprime file");

        // Copy eprimeparam file to temporary directory
        let temp_eprimeparam_path = temp_dir.path().join("little1.param");
        fs::copy(eprimeparam_path, &temp_eprimeparam_path)
            .expect("Failed to copy eprimeparam file");

        // Call parse_essence function
        let result = parse_essence(&temp_eprime_path, &temp_eprimeparam_path).unwrap();

        let puz = PuzzleSolver::new(result).unwrap();

        let varlits = puz.get_unsatisfiable_varlits();

        assert_eq!(varlits.len(), 16);
        for lit in varlits {
            let puzlit = puz.lit_to_puzlit(lit);
            for p in puzlit {
                let indices = p.var().indices;
                assert_eq!(indices.len(), 1);
                // In the solution, forAll i, x[i]=i
                // and the lits are the 'unsatisfiable' lits
                assert_eq!(indices[0] == p.val(), !p.sign());
            }
        }
        // Clean up temporary directory
        temp_dir
            .close()
            .expect("Failed to clean up temporary directory");
    }
}

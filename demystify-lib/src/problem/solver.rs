use crate::satcore::SatCore;

use super::parse::PuzzleParse;

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
}

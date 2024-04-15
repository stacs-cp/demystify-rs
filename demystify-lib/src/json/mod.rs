use std::collections::BTreeSet;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::problem::{parse::PuzzleParse, solver::PuzzleSolver, PuzLit};

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Puzzle {
    pub kind: String,
    pub width: i64,
    pub height: i64,
    pub start_grid: Option<Vec<Vec<Option<i64>>>>,
    pub solution_grid: Option<Vec<Vec<Option<i64>>>>,
    pub cages: Option<Vec<Vec<Option<i64>>>>,
}

impl Puzzle {
    pub fn new_from_puzzle(problem: &PuzzleParse) -> anyhow::Result<Puzzle> {
        let kind = problem.eprime.kind.clone().unwrap_or("Unknown".to_string());

        let mut width = None;
        let mut height = None;

        if problem.eprime.has_param("width") {
            width = Some(problem.eprime.param_i64("width")?);
        }

        if problem.eprime.has_param("height") {
            height = Some(problem.eprime.param_i64("height")?);
        }

        if problem.eprime.has_param("height") {
            height = Some(problem.eprime.param_i64("height")?);
        }

        let mut start_grid = None;
        let mut cages = None;

        if problem.eprime.has_param("start_grid") {
            start_grid = Some(problem.eprime.param_vec_vec_option_i64("start_grid")?);
        }

        if problem.eprime.has_param("fixed") {
            start_grid = Some(problem.eprime.param_vec_vec_option_i64("fixed")?);
        }

        if problem.eprime.has_param("cages") {
            cages = Some(problem.eprime.param_vec_vec_option_i64("fixed")?);
        }

        if width.is_none() || height.is_none() {
            if start_grid.is_some() {
                width = Some(start_grid.as_ref().unwrap()[0].len() as i64);
                height = Some(start_grid.as_ref().unwrap().len() as i64);
            } else if cages.is_some() {
                width = Some(cages.as_ref().unwrap()[0].len() as i64);
                height = Some(cages.as_ref().unwrap().len() as i64);
            }
        }

        Ok(Puzzle {
            kind,
            width: width.context("'width' not given as a param, and unable to deduce")?,
            height: height.context("'height' not given as a param, and unable to deduce")?,
            start_grid,
            solution_grid: None,
            cages,
        })
    }
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct StateLit {
    pub val: u32,
    pub classes: Option<Vec<String>>,
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct State {
    pub knowledge_grid: Option<Vec<Vec<Option<Vec<StateLit>>>>>,
    pub statements: Option<Vec<Statement>>,
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Statement {
    pub content: String,
    pub classes: Option<Vec<String>>,
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Problem {
    pub puzzle: Puzzle,
    pub state: Option<State>,
}

impl Problem {
    pub fn new_from_puzzle(problem: &PuzzleParse) -> anyhow::Result<Problem> {
        let puzzle = Puzzle::new_from_puzzle(problem)?;
        Ok(Problem {
            puzzle,
            state: None,
        })
    }

    pub fn new_from_puzzle_and_mus(
        solver: &PuzzleSolver,
        lits: &BTreeSet<PuzLit>,
        constraints: &Vec<String>,
    ) -> anyhow::Result<Problem> {
        let puzzle = Puzzle::new_from_puzzle(solver.puzzleparse())?;

        Ok(Problem {
            puzzle,
            state: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use test_log::test;

    use crate::json::Puzzle;

    #[test]
    fn test_parse_essence_binairo() -> anyhow::Result<()> {
        let eprime_path = "./tst/binairo.eprime";
        let eprimeparam_path = "./tst/binairo-1.param";

        let puz =
            crate::problem::util::test_utils::build_puzzleparse(eprime_path, eprimeparam_path);

        let p = Puzzle::new_from_puzzle(&puz)?;

        assert_eq!(p.kind, "Binairo");
        assert_eq!(p.width, 6);
        assert_eq!(p.height, 6);

        Ok(())
    }
}

/// This module contains the definitions and implementations related to JSON serialization and deserialization for the demystify library.
use std::collections::{BTreeSet, HashMap};

use anyhow::Context;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::problem::{parse::PuzzleParse, solver::PuzzleSolver, PuzLit, VarValPair};

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Puzzle {
    pub kind: String,
    pub width: i64,
    pub height: i64,
    pub start_grid: Option<Vec<Vec<Option<i64>>>>,
    pub solution_grid: Option<Vec<Vec<Option<i64>>>>,
    pub cages: Option<Vec<Vec<Option<i64>>>>,
    pub top_labels: Option<Vec<String>>,
    pub bottom_labels: Option<Vec<String>>,
    pub left_labels: Option<Vec<String>>,
    pub right_labels: Option<Vec<String>>,
}

impl Puzzle {
    pub fn new_from_puzzle(problem: &PuzzleParse) -> anyhow::Result<Puzzle> {
        let kind = problem.eprime.kind.clone().unwrap_or("Unknown".to_string());

        let mut width = None;
        let mut height = None;

        if problem.eprime.has_param("width") {
            width = Some(problem.eprime.param_i64("width")?);
        } else if problem.eprime.has_param("x") {
            width = Some(problem.eprime.param_i64("x")?);
        }

        if problem.eprime.has_param("height") {
            height = Some(problem.eprime.param_i64("height")?);
        } else if problem.eprime.has_param("y") {
            height = Some(problem.eprime.param_i64("y")?);
        }

        if problem.eprime.has_param("grid_size") {
            height = Some(problem.eprime.param_i64("grid_size")?);
            width = Some(problem.eprime.param_i64("grid_size")?);
        }

        if problem.eprime.has_param("size") {
            height = Some(problem.eprime.param_i64("size")?);
            width = Some(problem.eprime.param_i64("size")?);
        }

        let mut start_grid = None;
        let mut cages = None;

        let mut top_labels = None;
        let mut bottom_labels = None;
        let mut left_labels = None;
        let mut right_labels = None;

        for label in ["row_labels", "top_labels", "row_sums"] {
            if problem.eprime.has_param(label) {
                top_labels = Some(problem.eprime.param_vec_string(label)?);
            }
        }

        for label in ["col_labels", "left_labels", "col_sums"] {
            if problem.eprime.has_param(label) {
                left_labels = Some(problem.eprime.param_vec_string(label)?);
            }
        }

        for label in ["bottom_labels"] {
            if problem.eprime.has_param(label) {
                bottom_labels = Some(problem.eprime.param_vec_string(label)?);
            }
        }

        for label in ["right_labels"] {
            if problem.eprime.has_param(label) {
                right_labels = Some(problem.eprime.param_vec_string(label)?);
            }
        }

        if problem.eprime.has_param("side_labels") {
            let side_labels = problem.eprime.param_vec_vec_string("side_labels")?;
            left_labels = Some(side_labels[0].clone());
            top_labels = Some(side_labels[1].clone());
            right_labels = Some(side_labels[2].clone());
            bottom_labels = Some(side_labels[3].clone());
        }

        if problem.eprime.has_param("start_grid") {
            start_grid = Some(problem.eprime.param_vec_vec_option_i64("start_grid")?);
        }

        if problem.eprime.has_param("fixed") {
            start_grid = Some(problem.eprime.param_vec_vec_option_i64("fixed")?);
        }

        if problem.eprime.has_param("cages") {
            cages = Some(problem.eprime.param_vec_vec_option_i64("cages")?);
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
            top_labels,
            bottom_labels,
            left_labels,
            right_labels,
        })
    }
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct StateLit {
    pub val: i64,
    pub classes: Option<BTreeSet<String>>,
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct State {
    pub knowledge_grid: Option<Vec<Vec<Option<Vec<StateLit>>>>>,
    pub statements: Option<Vec<Statement>>,
    pub description: Option<String>,
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
        tosolve: &BTreeSet<VarValPair>,
        known: &BTreeSet<PuzLit>,
        deduced_lits: &BTreeSet<PuzLit>,
        constraints: &[String],
        description: &str,
    ) -> anyhow::Result<Problem> {
        let puzzle = Puzzle::new_from_puzzle(solver.puzzleparse())?;

        let mut knowledgegrid: Vec<Vec<Option<Vec<StateLit>>>> =
            vec![
                vec![None; usize::try_from(puzzle.width).context("width is negative")?];
                usize::try_from(puzzle.height).context("height is negative")?
            ];

        let mut constraint_tags: HashMap<VarValPair, BTreeSet<String>> = HashMap::new();

        // Start by getting a map of all the constraints which need tagging
        for (i, con) in constraints.iter().enumerate() {
            let scope = solver.puzzleparse().constraint_scope(con);
            for p in scope {
                let tags = constraint_tags.entry(p).or_default();
                tags.insert(format!("highlight_con{i}"));
                tags.insert("js_highlighter".to_string());
            }
        }

        let all_lits = solver.puzzleparse().all_var_varvals();

        for l in all_lits {
            if !(tosolve.contains(&l) || known.contains(&PuzLit::new_eq(l.clone()))) {
                continue;
            }

            // TODO: Handle more than one variable matrix?
            let index = l.var().indices().clone();
            assert_eq!(index.len(), 2);
            let i = usize::try_from(index[0]).context("negative index 0?")?;
            let j = usize::try_from(index[1]).context("negative index 1?")?;

            assert!(i > 0, "Variables should be 1-indexed");
            assert!(j > 0, "Variables should be 1-indexed");

            let i = i - 1;
            let j = j - 1;

            let mut tags = BTreeSet::new();

            if let Some(val) = constraint_tags.get(&l) {
                tags.extend(val.clone());
                tags.insert("litinmus".to_string());
            }

            if deduced_lits.contains(&PuzLit::new_eq(l.clone())) {
                tags.insert("litpos".to_string());
            }

            if deduced_lits.contains(&PuzLit::new_neq(l.clone())) {
                tags.insert("litneg".to_string());
            }

            if known.contains(&PuzLit::new_eq(l.clone())) {
                tags.insert("litknown".to_string());
            }

            if knowledgegrid[i][j].is_none() {
                knowledgegrid[i][j] = Some(vec![]);
            }

            knowledgegrid[i][j].as_mut().unwrap().push(StateLit {
                val: l.val(),
                classes: Some(tags),
            });
        }

        let statements = constraints
            .iter()
            .enumerate()
            .map(|(i, con)| Statement {
                content: con.clone(),
                classes: Some(vec![
                    format!("highlight_con{}", i),
                    "js_highlighter".to_string(),
                ]),
            })
            .collect_vec();

        let state = State {
            knowledge_grid: Some(knowledgegrid),
            statements: Some(statements),
            description: Some(description.to_owned()),
        };

        Ok(Problem {
            puzzle,
            state: Some(state),
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

/// This module contains the definitions and implementations related to JSON serialization and deserialization for the demystify library.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::Context;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::problem::{PuzLit, VarValPair, parse::PuzzleParse, solver::PuzzleSolver};

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

        for label in ["width", "x", "x_dim"] {
            if problem.eprime.has_param(label) {
                width = Some(problem.eprime.param_i64(label)?);
            }
        }

        for label in ["height", "y", "y_dim"] {
            if problem.eprime.has_param(label) {
                height = Some(problem.eprime.param_i64(label)?);
            }
        }

        if problem.eprime.has_param("grid_size") {
            height = Some(problem.eprime.param_i64("grid_size")?);
            width = Some(problem.eprime.param_i64("grid_size")?);
        }

        if problem.eprime.has_param("size") {
            height = Some(problem.eprime.param_i64("size")?);
            width = Some(problem.eprime.param_i64("size")?);
        }

        // If there is only one 'VAR', then it might tell us what to draw
        if height.is_none() && width.is_none() && problem.eprime.vars.len() == 1 {
            let var = problem.eprime.vars.iter().next().unwrap();

            let indices = problem.get_matrix_indices(var);
            if let Some(v) = indices {
                if v.len() == 2 {
                    width = Some(v[1]);
                    height = Some(v[0]);
                }
            }
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
    pub classes: Vec<String>,
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Problem {
    pub puzzle: Puzzle,
    pub state: Option<State>,
}

pub struct DescriptionStatement {
    pub result: String,
    pub constraints: Vec<String>,
}

impl DescriptionStatement {
    pub fn new(result: String, constraints: Vec<String>) -> Self {
        Self {
            result,
            constraints,
        }
    }
}

impl Problem {
    pub fn new_from_puzzle(problem: &PuzzleParse) -> anyhow::Result<Problem> {
        let puzzle = Puzzle::new_from_puzzle(problem)?;
        Ok(Problem {
            puzzle,
            state: None,
        })
    }

    pub fn new_from_puzzle_and_state(
        solver: &PuzzleSolver,
        tosolve: &BTreeSet<VarValPair>,
        known: &BTreeSet<PuzLit>,
        deduced_lits: &BTreeSet<PuzLit>,
        comments: &str,
    ) -> anyhow::Result<Problem> {
        Self::new_from_puzzle_and_mus(solver, tosolve, known, deduced_lits, &[], comments)
    }

    pub fn new_from_puzzle_and_mus(
        solver: &PuzzleSolver,
        tosolve: &BTreeSet<VarValPair>,
        known: &BTreeSet<PuzLit>,
        deduced_lits: &BTreeSet<PuzLit>,
        deduction_list: &[DescriptionStatement],
        comments: &str,
    ) -> anyhow::Result<Problem> {
        let puzzle = Puzzle::new_from_puzzle(solver.puzzleparse())?;

        let varnames = tosolve
            .iter()
            .map(|x| x.var().name().clone())
            .chain(known.iter().map(|x| x.var().name().clone()))
            .collect::<HashSet<String>>();

        let allowed_names: HashSet<String> = if varnames.len() == 1 {
            varnames
        } else if varnames.contains("grid") {
            {
                let mut set = HashSet::new();
                set.insert("grid".to_string());
                set
            }
        } else {
            return Err(anyhow::anyhow!(
                "More than one variable matrix, and none called 'grid', so not sure what to print: {:?}",
                varnames
            ));
        };

        let mut knowledgegrid: Vec<Vec<Option<Vec<StateLit>>>> =
            vec![
                vec![None; usize::try_from(puzzle.width).context("width is negative")?];
                usize::try_from(puzzle.height).context("height is negative")?
            ];

        // Start by getting a list of all constraints, and assigning a number to each of them.
        let mut constraint_num: HashMap<String, usize> = HashMap::new();
        // Make a list of the tags we need to attach to each varvalpair in the scope of each constraint
        let mut constraint_tags: HashMap<VarValPair, BTreeSet<String>> = HashMap::new();

        for deduction in deduction_list {
            for constraint in &deduction.constraints {
                // constraint_num makes sure we only tag each constraint once
                if !constraint_num.contains_key(constraint) {
                    let len = constraint_num.len();
                    constraint_num.insert(constraint.clone(), len);
                    let scope = solver.puzzleparse().constraint_scope(constraint);
                    for p in scope {
                        let tags = constraint_tags.entry(p).or_default();
                        tags.insert(format!("highlight_con{len}"));
                        tags.insert("js_highlighter".to_string());
                    }
                }
            }
        }

        let all_lits = solver.puzzleparse().all_var_varvals();

        for l in all_lits {
            if !(tosolve.contains(&l) || known.contains(&PuzLit::new_eq(l.clone()))) {
                continue;
            }

            if !allowed_names.contains(l.var().name()) {
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

        let mut statements = Vec::new();

        for deduction in deduction_list {
            statements.push(Statement {
                content: deduction.result.clone(),
                classes: vec![],
            });
            for constraint in &deduction.constraints {
                let num = constraint_num.get(constraint).unwrap();
                statements.push(Statement {
                    content: constraint.clone(),
                    classes: vec![
                        format!("highlight_con{}", num),
                        "js_highlighter".to_string(),
                    ],
                });
            }
        }

        let state = State {
            knowledge_grid: Some(knowledgegrid),
            statements: Some(statements),
            description: Some(comments.to_owned()),
        };

        Ok(Problem {
            puzzle,
            state: Some(state),
        })
    }

    pub fn new_from_puzzle_and_difficulty(
        solver: &PuzzleSolver,
        tosolve: &BTreeSet<VarValPair>,
        known: &BTreeSet<PuzLit>,
        complexity: &BTreeMap<VarValPair, usize>,
        description: &str,
    ) -> anyhow::Result<Problem> {
        let puzzle = Puzzle::new_from_puzzle(solver.puzzleparse())?;

        let varnames = tosolve
            .iter()
            .map(|x| x.var().name().clone())
            .chain(known.iter().map(|x| x.var().name().clone()))
            .collect::<HashSet<String>>();

        let allowed_names: HashSet<String> = if varnames.len() == 1 {
            varnames
        } else if varnames.contains("grid") {
            {
                let mut set = HashSet::new();
                set.insert("grid".to_string());
                set
            }
        } else {
            return Err(anyhow::anyhow!(
                "More than one variable matrix, and none called 'grid', so not sure what to print: {:?}",
                varnames
            ));
        };

        let mut knowledgegrid: Vec<Vec<Option<Vec<StateLit>>>> =
            vec![
                vec![None; usize::try_from(puzzle.width).context("width is negative")?];
                usize::try_from(puzzle.height).context("height is negative")?
            ];

        let all_lits = solver.puzzleparse().all_var_varvals();

        let complexity_vals: BTreeSet<_> = complexity.values().collect();

        for l in all_lits {
            if !(tosolve.contains(&l) || known.contains(&PuzLit::new_eq(l.clone()))) {
                continue;
            }

            if !allowed_names.contains(l.var().name()) {
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

            if let Some(val) = complexity.get(&l) {
                let i = complexity_vals.iter().position(|&v| v == val).unwrap_or(0);
                tags.insert(format!("highlight_con{i}"));
                tags.insert("js_highlighter".to_string());
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

        let statements = complexity_vals
            .iter()
            .enumerate()
            .map(|(i, consize)| Statement {
                content: format!("MUS size {consize}"),
                classes: vec![format!("highlight_con{}", i), "js_highlighter".to_string()],
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

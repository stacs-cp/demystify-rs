use serde::{Deserialize, Serialize};

use crate::problem::parse::PuzzleParse;

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Puzzle {
    pub kind: String,
    pub width: u32,
    pub height: u32,
    pub start_grid: Option<Vec<Vec<Option<u32>>>>,
    pub solution_grid: Option<Vec<Vec<Option<u32>>>>,
    pub cages: Option<Vec<Vec<Option<u32>>>>,
}

impl Puzzle {
    pub fn new_from_puzzle(problem: &PuzzleParse) -> Puzzle {
        let kind = problem.eprime.kind.clone().unwrap_or("Unknown".to_string());

        panic!();
        /*        Puzzle {
            kind
        }*/
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
    pub statements: Option<Vec<Statement>>,
}

impl Problem {
    pub fn new_from_puzzle(problem: &PuzzleParse) -> Problem {
        panic!();
    }
}

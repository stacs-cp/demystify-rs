use serde::{Deserialize, Serialize};

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Puzzle {
    kind: String,
    width: u32,
    height: u32,
    start_grid: Option<Vec<Vec<Option<u32>>>>,
    solution_grid: Option<Vec<Vec<Option<u32>>>>,
    cages: Option<Vec<Vec<Option<u32>>>>,
}

pub struct StateLit {
    val: u32,
    classes: Option<Vec<String>>,
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct State {
    knowledge_grid: Vec<Vec<Option<u32>>>,
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Statement {
    content: String,
    classes: Option<Vec<String>>,
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Problem {
    puzzle: Puzzle,
    state: Option<State>,
    statements: Option<Vec<Statement>>,
}

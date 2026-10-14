//! Usage: check_solvable MODEL PARAM [MODEL PARAM ...]
//! Prints, per puzzle: UNSAT, UNIQUE, or MULTIPLE(n unfixed literals).
use std::sync::Arc;

use demystify::problem::{parse::parse_essence, planner::PuzzlePlanner, solver::PuzzleSolver};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(
        !args.is_empty() && args.len() % 2 == 0,
        "usage: MODEL PARAM [MODEL PARAM ...]"
    );
    for pair in args.chunks(2) {
        let puz = parse_essence(
            &std::path::PathBuf::from(&pair[0]),
            &std::path::PathBuf::from(&pair[1]),
        )?;
        let solver = PuzzleSolver::new(Arc::new(puz))?;
        let mut planner = PuzzlePlanner::new(solver);
        let verdict = match planner.check_solvability() {
            None => "UNSAT".to_string(),
            Some(0) => "UNIQUE".to_string(),
            Some(n) => format!("MULTIPLE({n} unfixed literals)"),
        };
        println!("{verdict}\t{}\t{}", pair[0], pair[1]);
    }
    Ok(())
}

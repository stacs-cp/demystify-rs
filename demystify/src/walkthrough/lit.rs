//! Tiny parser for literal references in walkthrough scripts.
//!
//! Accepts strings of the form `<name>[<idx>(,<idx>)*]<op><val>` where
//! `<op>` is `=` or `!=`.  Examples:
//!
//! - `field[3,5]=2`
//! - `grid[1,1]!=7`
//! - `puz_cages[1,1]=4`
//!
//! Used by the executor to translate authored literal references into
//! `PuzLit`s before passing them to `PuzzleSolver`/`PuzzlePlanner`.

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::problem::{PuzLit, PuzVar, VarValPair};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLit {
    pub name: String,
    pub indices: Vec<i64>,
    pub val: i64,
    pub is_eq: bool,
}

impl ParsedLit {
    /// Construct the corresponding `PuzLit`.  The puzzle solver later
    /// verifies the variable name and value lie within its declared domain.
    #[must_use]
    pub fn to_puzlit(&self) -> PuzLit {
        let var = PuzVar::new(&self.name, self.indices.clone());
        let vvp = VarValPair::new(&var, self.val);
        if self.is_eq {
            PuzLit::new_eq(vvp)
        } else {
            PuzLit::new_neq(vvp)
        }
    }

    /// Lit_def for the planner's literal-targeted methods
    /// (`solve_step_for_literal`, `all_muses_for_literal`).  Format:
    /// `[idx1, idx2, ..., val]`.  Equality only — the planner's API does not
    /// take a sign, callers must invert into the search filter themselves.
    #[must_use]
    pub fn lit_def(&self) -> Vec<i64> {
        let mut v = self.indices.clone();
        v.push(self.val);
        v
    }
}

pub fn parse_lit(s: &str) -> Result<ParsedLit> {
    // Lazy-init via OnceLock would be cleaner but avoids a dep on `once_cell`;
    // these strings are short and parsing happens once per step, so the
    // recompile cost is irrelevant.
    let re = Regex::new(
        r"^([a-zA-Z][a-zA-Z0-9_]*)\[\s*(-?\d+(?:\s*,\s*-?\d+)*)\s*\]\s*(!=|=)\s*(-?\d+)\s*$",
    )
    .expect("walkthrough lit regex");
    let caps = re
        .captures(s.trim())
        .with_context(|| format!("malformed lit '{s}': expected name[i,j,...]=val or !=val"))?;

    let name = caps.get(1).unwrap().as_str().to_string();
    let indices: Result<Vec<i64>> = caps
        .get(2)
        .unwrap()
        .as_str()
        .split(',')
        .map(|s| {
            s.trim()
                .parse::<i64>()
                .with_context(|| format!("non-integer index in '{s}'"))
        })
        .collect();
    let indices = indices?;
    let is_eq = caps.get(3).unwrap().as_str() == "=";
    let val = caps
        .get(4)
        .unwrap()
        .as_str()
        .parse::<i64>()
        .with_context(|| format!("non-integer value in '{s}'"))?;

    if indices.is_empty() {
        bail!("lit '{s}' must have at least one index");
    }
    Ok(ParsedLit {
        name,
        indices,
        val,
        is_eq,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_eq() {
        let p = parse_lit("grid[3,5]=2").unwrap();
        assert_eq!(p.name, "grid");
        assert_eq!(p.indices, vec![3, 5]);
        assert_eq!(p.val, 2);
        assert!(p.is_eq);
        assert_eq!(p.lit_def(), vec![3, 5, 2]);
    }

    #[test]
    fn parses_neq() {
        let p = parse_lit("field[1,1]!=7").unwrap();
        assert_eq!(p.name, "field");
        assert_eq!(p.indices, vec![1, 1]);
        assert_eq!(p.val, 7);
        assert!(!p.is_eq);
    }

    #[test]
    fn parses_underscored_name() {
        let p = parse_lit("puz_cages[1,1]=4").unwrap();
        assert_eq!(p.name, "puz_cages");
    }

    #[test]
    fn parses_higher_arity() {
        let p = parse_lit("box_alldiff[1,2,3,4]=1").unwrap();
        assert_eq!(p.indices, vec![1, 2, 3, 4]);
    }

    #[test]
    fn parses_with_whitespace() {
        let p = parse_lit("  grid[ 3 , 5 ] = 2  ").unwrap();
        assert_eq!(p.indices, vec![3, 5]);
    }

    #[test]
    fn rejects_no_indices() {
        assert!(parse_lit("grid[]=2").is_err());
    }

    #[test]
    fn rejects_non_int_index() {
        assert!(parse_lit("grid[a,b]=2").is_err());
    }

    #[test]
    fn rejects_no_op() {
        assert!(parse_lit("grid[1,1]2").is_err());
    }

    #[test]
    fn rejects_bare_name() {
        assert!(parse_lit("grid").is_err());
    }
}

//! Build `demystify` puzzle inputs directly in Rust, bypassing Conjure.
//!
//! `demystify` normally consumes `.eprime` + `.param` files via Conjure +
//! Savilerow, which compile the model to DIMACS CNF.  That toolchain is too
//! large (and not viable) for the `demystify-lua` and `demystify-wasm`
//! embeddings, but those embeddings still sometimes want to solve puzzles.
//!
//! This crate provides a small builder API for those callers.  It supports a
//! deliberately tiny constraint vocabulary — enough for star battle, and
//! easily extensible — and does not aim to replace Conjure.
//!
//! # Supported constraint vocabulary
//!
//! - Boolean variables (declared individually or as N-dimensional matrices).
//! - `sum(lits) >= k` and `sum(lits) <= k`, where each lit may be negated.
//! - Guarded versions of the above: `guard -> (sum >= k)` /
//!   `guard -> (sum <= k)`.  The guard mirrors the eprime `$#CON` activation
//!   pattern.
//! - `$#REVEAL` cascades — `reveal_bool_matrix` + `set_reveal` give
//!   minesweeper-style "deduce `grid[i,j]=v` ⇒ also mark `facts[i,j,v]`
//!   known" without a separate deduction step.
//!
//! Out of scope (cleanly addable later): `sum = k`, integer-domain
//! variables.
//!
//! # Example
//!
//! ```no_run
//! use demystify_builder::{PuzzleBuilder, ShowRole};
//!
//! let mut b = PuzzleBuilder::new();
//! b.kind("toy");
//! let g = b.var_bool_matrix("g", &[1..=2, 1..=2]);
//! b.show("g", ShowRole::Main);
//! let act = b.aux_bool("rule");
//! // "at least 2 of g[1,1], g[1,2], g[2,1], g[2,2]"
//! b.sum_ge(
//!     act,
//!     &[
//!         g.get(&[1, 1]).pos(),
//!         g.get(&[1, 2]).pos(),
//!         g.get(&[2, 1]).pos(),
//!         g.get(&[2, 2]).pos(),
//!     ],
//!     2,
//!     "rule",
//!     "at least 2 cells are true",
//! );
//! let _puzzle = b.build().expect("build");
//! ```

mod builder;
mod cardinality;
mod error;

pub use builder::{Atom, BoolMatrix, ConstraintHandle, FamilyMut, PuzzleBuilder, Signed};
pub use demystify::problem::parse::{PuzzleParse, ShowRole};
pub use error::BuildError;
pub use rustsat::types::Lit;

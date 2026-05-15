//! Tutorial-walkthrough scripting layer.
//!
//! A walkthrough script is a TOML document describing a sequence of state
//! mutations and renderings against a single puzzle.  The executor drives a
//! `PuzzlePlanner` through the steps; the HTML composer turns the resulting
//! sections into a single self-contained web page.
//!
//! Public API:
//! - [`schema::Script`] / [`schema::Step`] — TOML schema (`serde::Deserialize`).
//! - [`lit::parse_lit`] — `"field[3,5]=2"` → `ParsedLit` → `PuzLit`.
//! - [`executor::Executor`] — drives a planner, accumulates `Section`s.
//! - [`html::compose_page`] — turns sections into a self-contained HTML page.
//!
//! See `demystify-walkthrough` (the CLI binary) for a typical caller; mystify's
//! `build_guide` is intended to share `executor::Executor` + `html::render_section_body`
//! so corpus-mining output and hand-authored tutorials use the same renderer.

pub mod executor;
pub mod html;
pub mod lit;
pub mod schema;

pub use executor::{Executor, Section};
pub use html::{compose_page, render_section_body};
pub use lit::{ParsedLit, parse_lit};
pub use schema::{Script, Step};

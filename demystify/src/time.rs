//! Centralised time primitives.
//!
//! Re-exports `Instant` and `Duration` from `web_time`, the wasm-aware
//! drop-in for `std::time`.  On non-wasm targets `web_time::Instant` is
//! literally `std::time::Instant`; on `wasm32-unknown-unknown` it uses
//! `performance.now()` instead of the (unsupported) monotonic clock.
//!
//! **Always import time types from here**:
//!
//! ```ignore
//! use crate::time::{Instant, Duration};
//! ```
//!
//! The repo has been bitten more than once by `use std::time::Instant;`
//! slipping into a hot path and panicking at the first `Instant::now()`
//! under wasm.  Routing everything through this module is the
//! convention; the wasm-pack tests (notably `quick_solve` and the
//! bloomsweeper repro) will fail on the wasm target if a stray
//! `std::time::Instant::now` is reintroduced, so the regression is
//! caught at test time.
//!
//! We can't enforce this with `clippy::disallowed_types` because
//! `web_time::Instant` is a type alias for `std::time::Instant` on
//! non-wasm — clippy resolves the alias and would flag every
//! centralised import as well.

pub use web_time::{Duration, Instant};

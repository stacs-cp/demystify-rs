//! TOML schema for walkthrough scripts.
//!
//! A walkthrough script is a self-contained TOML document describing a
//! sequence of state mutations and renderings against a single puzzle:
//!
//! ```toml
//! model = "eprime/sudoku.eprime"
//! param = "eprime/sudoku/redditexample.param"
//! title = "Box-line reduction example 2"
//! repeats = 5            # default MusConfig::repeats for all show_* ops
//! strategy = "cake"      # default MusConfig::strategy
//!
//! [[step]]
//! op = "deduce"
//! lit = "grid[3,2]!=4"
//!
//! [[step]]
//! op = "show_mus"
//! lit = "grid[6,2]=7"
//! title = "Hidden single in column 2"
//! ```
//!
//! Steps are tagged by `op`. State-mutation ops (`deduce`, `pin`) advance the
//! solver but emit no HTML; rendering ops (`show_mus`, `show_smallest_muses`,
//! `show_all_size_muses`, `heatmap`) emit one or more sections in the output.

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct Script {
    pub model: PathBuf,
    pub param: PathBuf,
    /// Page title; falls back to the script's filename if omitted.
    #[serde(default)]
    pub title: Option<String>,
    /// Default `MusConfig::repeats` for every `show_*` step.  Per-step
    /// overrides take precedence.
    #[serde(default)]
    pub repeats: Option<i64>,
    /// Default `MusConfig::strategy` (`"quick"`/`"slice"`/`"cake"`/`"dynamic"`).
    #[serde(default)]
    pub strategy: Option<String>,
    /// Per-SAT-call conflict limit, mirroring the main CLI's `--conflict-limit`.
    #[serde(default)]
    pub conflict_limit: Option<i64>,
    /// Mirrors the main CLI's `--only-assign`: when true the solver only
    /// targets positive value-assignment lits (`grid[i,j] = k`) and never
    /// individual `≠` lits.  Useful for tents-style puzzles where players
    /// place tents/Xs as full cell values rather than eliminating
    /// candidates one tree at a time.
    #[serde(default)]
    pub only_assignments: Option<bool>,
    /// When true, cells that have no known literal yet (positive or
    /// negative) render as a "?" placeholder instead of the full
    /// candidate list.  Cuts visual noise in tutorial walkthroughs at
    /// the cost of losing the difficulty-heatmap signal on cells that
    /// haven't been touched yet.  Tutorial-only — the interactive GUI
    /// keeps the busy-but-clickable rendering by default.
    #[serde(default)]
    pub hide_untouched_candidates: Option<bool>,
    #[serde(default)]
    pub step: Vec<Step>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Step {
    /// Pin a literal as known via `add_known_lit`.  Use for literals the
    /// puzzle's own constraints already imply — adds them to the planner's
    /// state without proof, so subsequent search starts from a more
    /// advanced position.  The walkthrough emits no HTML for this step.
    Deduce { lit: String },

    /// Pin a literal via `add_not_provable_known_lit` — bypasses any
    /// provability check.  For design-side variables (`puz_*`) and other
    /// cases where the literal is asserted, not proven.
    Pin { lit: String },

    /// Render a single picture showing the smallest MUS that explains
    /// `lit` (delegates to `Planner::solve_step_for_literal`).
    ShowMus {
        lit: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        repeats: Option<i64>,
        #[serde(default)]
        strategy: Option<String>,
        /// Free-text annotation describing how this deduction's MUS
        /// differs from the source tutorial's reasoning (e.g. "produces
        /// a smaller MUS using a different all-different block").
        /// Captured verbatim in the rendered HTML and intended to be
        /// aggregated across the corpus to count match/mismatch
        /// categories for the paper.
        #[serde(default)]
        note: Option<String>,
    },

    /// Render one picture per minimum-sized MUS for `lit`.  Uses
    /// `Planner::all_muses_for_literal`, then filters to MUSes of the
    /// minimum size.  `max` caps the number of pictures.
    ShowSmallestMuses {
        lit: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        repeats: Option<i64>,
        #[serde(default)]
        strategy: Option<String>,
        #[serde(default)]
        max: Option<usize>,
        /// See `ShowMus::note`.
        #[serde(default)]
        note: Option<String>,
    },

    /// Render one picture per MUS for `lit`, including non-minimum-sized
    /// MUSes up to `max_size`.  Uses `Planner::all_muses_for_literal`
    /// (which sets `find_bigger = true`).  `max` caps the number of
    /// pictures returned.
    ShowAllSizeMuses {
        lit: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        repeats: Option<i64>,
        #[serde(default)]
        strategy: Option<String>,
        /// Stop searching for MUSes larger than this size.
        #[serde(default)]
        max_size: Option<usize>,
        #[serde(default)]
        max: Option<usize>,
        /// See `ShowMus::note`.
        #[serde(default)]
        note: Option<String>,
    },

    /// Render the difficulty heat-map for the current state.
    Heatmap {
        #[serde(default)]
        title: Option<String>,
        /// See `ShowMus::note`.
        #[serde(default)]
        note: Option<String>,
        /// Free-text narrative commentary — *not* a divergence flag.
        /// Use this for human prose that should appear under the section
        /// heading without being conflated with `note` (which is reserved
        /// for "this MUS doesn't match the source page's reasoning").
        /// Rendered in a distinct visual style.
        #[serde(default)]
        commentary: Option<String>,
    },

    /// Silently advance state by repeatedly applying the smallest-MUS
    /// deductions while the minimum MUS size remains `<= max_mus_size`.
    /// Use for "trivial cleanup" phases — e.g. once two stars are placed
    /// in a row of a Star Battle puzzle, all neighbouring cells and the
    /// other row cells become forbidden via size-1 MUSes; rendering each
    /// of those individually would clutter the walkthrough.
    ///
    /// Behaviour:
    ///   1. Compute the current smallest MUSes.  If empty → stop.
    ///   2. If the smallest MUS size exceeds `max_mus_size`, ABORT with
    ///      an error (the puzzle has work that wasn't trivial — you
    ///      probably need a real `show_*` step).
    ///   3. Otherwise apply every literal those MUSes prove and loop.
    ///
    /// Renders one summary section showing the post-state heatmap with
    /// a title noting how many cells were deduced.  No per-cell SVG.
    DeduceTrivial {
        /// Highest acceptable smallest-MUS size.  Most "trivial" phases
        /// use `1`; some single-step propagations need `2`.
        max_mus_size: usize,
        #[serde(default)]
        title: Option<String>,
        /// See `ShowMus::note`.
        #[serde(default)]
        note: Option<String>,
        /// Literals to *not* auto-apply even if their MUS is within the
        /// threshold — typically lits that a later `show_*` step wants to
        /// demonstrate explicitly.  Without this, the cleanup might grab
        /// "interesting" deductions whose MUS happens to also be small,
        /// and the later show step would find nothing left to explain.
        #[serde(default)]
        skip: Vec<String>,
    },
}

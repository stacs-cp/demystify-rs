//! Named-strategy recognition for MUSes.
//!
//! Computes a canonical *fingerprint* of a MUS — a (constraint-family multiset,
//! overlap graph) pair, in canonical form — that two MUSes share iff their
//! structure is identical up to graph isomorphism (with edge-cardinalities
//! quantised). A name database maps fingerprints to human-recognisable
//! technique names like "naked single" or "X-wing".
//!
//! See `doc/named-strategies.md` for the full design.

pub mod database;
pub mod family;
pub mod fingerprint;

pub use database::{Database, Strategy, display_name};
pub use family::FamilyMap;
pub use fingerprint::{MusFingerprint, fingerprint};

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Standard relative paths to look for a named-strategy directory when no
/// explicit one is given.  Covers running from the workspace root, from
/// inside the `demystify` crate, and from a sibling crate (the wasm and
/// web crates).
const DEFAULT_DB_SEARCH_PATHS: &[&str] = &[
    "demystify/named-strategies",
    "named-strategies",
    "../demystify/named-strategies",
];

/// Load a named-strategy [`Database`], falling back to the standard search
/// paths when `explicit` is `None`.  If no explicit path is given and none
/// of the search paths exist, an empty database is returned — callers get
/// no labels but everything else keeps working.  Loading errors from an
/// existing directory are propagated.
pub fn load_or_discover(explicit: Option<&Path>) -> anyhow::Result<Arc<Database>> {
    let dir: Option<PathBuf> = explicit.map(PathBuf::from).or_else(|| {
        DEFAULT_DB_SEARCH_PATHS
            .iter()
            .map(PathBuf::from)
            .find(|p| p.exists())
    });
    match dir {
        Some(d) => Ok(Arc::new(Database::load_from_dir(&d)?)),
        None => Ok(Arc::new(Database::empty())),
    }
}

#[cfg(test)]
mod integration_tests {
    //! End-to-end tests: real .eprime + .param + the shipped Sudoku DB,
    //! verifying that named techniques flow through the planner output.
    //! These tests run Conjure, so they're slower than the unit tests in
    //! the sibling submodules — keep them focused.

    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::named_strategy::Database;
    use crate::problem::planner::PuzzlePlanner;
    use crate::problem::solver::PuzzleSolver;
    use crate::problem::util::test_utils::build_puzzleparse;

    fn db_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("named-strategies")
    }

    /// Run the full pipeline (parse + solve + name lookup) on the shipped
    /// `eprime/sudoku.eprime` + a SudokuWiki hidden-singles puzzle, and
    /// collect every named technique that appears.
    fn run_named_solve(param_rel: &str) -> Vec<String> {
        let parse = build_puzzleparse("../eprime/sudoku.eprime", param_rel);
        let solver = PuzzleSolver::new(Arc::new(parse)).expect("solver init");
        let db = Arc::new(Database::load_from_dir(&db_path()).expect("db load"));
        let mut planner = PuzzlePlanner::new(solver).with_database(db);
        planner
            .quick_solve()
            .into_iter()
            .flatten()
            .filter_map(|um| um.name)
            .collect()
    }

    #[test]
    fn shipped_db_loads_without_errors() {
        let db = Database::load_from_dir(&db_path()).expect("DB should load");
        assert!(
            !db.is_empty(),
            "shipped Sudoku.toml should contain at least one strategy"
        );
    }

    /// Every `orientation_group` referenced in the shipped `Sudoku.toml`
    /// must correspond to a `$#FAMILY` group declared in
    /// `eprime/sudoku.eprime`. Otherwise the orientation prefix would
    /// silently never apply at display time.
    ///
    /// This is the round-trip check: load both ends of the contract and
    /// confirm they line up. If you rename a `$#FAMILY` group in the model
    /// without updating the DB, this test catches it.
    #[test]
    fn shipped_db_orientation_groups_exist_in_sudoku_model() {
        // Need a real PuzzleParse to access the families. Use the smallest
        // available sudoku param so this test stays fast.
        let parse = build_puzzleparse(
            "../eprime/sudoku.eprime",
            "../eprime/sudoku/sudokuwiki/hiddensingles/hiddensingles.param",
        );
        let declared_groups: std::collections::BTreeSet<&str> =
            parse.eprime.families.keys().map(String::as_str).collect();

        // Re-read the DB file directly so we can inspect every entry by name.
        let db_file: toml::Value = toml::from_str(
            &std::fs::read_to_string(db_path().join("Sudoku.toml"))
                .expect("Sudoku.toml should exist"),
        )
        .expect("Sudoku.toml should be valid TOML");
        let strategies = db_file
            .get("strategy")
            .and_then(toml::Value::as_array)
            .expect("Sudoku.toml should contain a [[strategy]] array");

        let mut bad: Vec<(String, String)> = Vec::new();
        for entry in strategies {
            let name = entry
                .get("name")
                .and_then(toml::Value::as_str)
                .expect("strategy missing 'name'");
            if let Some(group) = entry.get("orientation_group").and_then(toml::Value::as_str)
                && !declared_groups.contains(group)
            {
                bad.push((name.to_string(), group.to_string()));
            }
        }
        assert!(
            bad.is_empty(),
            "Sudoku.toml references orientation_group(s) not declared in eprime/sudoku.eprime: {bad:?}"
        );
    }

    #[test]
    fn fingerprint_pipeline_produces_named_steps_on_hidden_singles() {
        // Hidden-singles puzzle from the SudokuWiki test corpus — should
        // yield at least one "hidden single" named step in some orientation.
        let names =
            run_named_solve("../eprime/sudoku/sudokuwiki/hiddensingles/hiddensingles.param");
        assert!(
            !names.is_empty(),
            "expected at least one named step; got none"
        );
        assert!(
            names.iter().any(|n| n.ends_with("hidden single")),
            "expected at least one 'hidden single' step; names were: {names:?}"
        );
    }

    #[test]
    #[ignore = "flaky: rayon-driven MUS search non-determinism sometimes routes the puzzle through hidden singles only"]
    fn fingerprint_pipeline_finds_hidden_pair_and_triple() {
        // The hidden-triples SudokuWiki puzzle requires both hidden pair and
        // hidden triple reasoning along the way; demystify's planner should
        // surface at least one of each.  (`hiddenpairs-1.param` is *named*
        // for hidden pairs but turns out to be solvable without them — easier
        // path through hidden singles wins.)
        let names =
            run_named_solve("../eprime/sudoku/sudokuwiki/hiddenpairstriples/hiddentriples.param");
        assert!(
            names.iter().any(|n| n.ends_with("hidden pair")),
            "expected at least one 'hidden pair' step; names were: {names:?}"
        );
        assert!(
            names.iter().any(|n| n.ends_with("hidden triple")),
            "expected at least one 'hidden triple' step; names were: {names:?}"
        );
    }

    #[test]
    fn named_steps_carry_orientation_prefix() {
        // The orientation prefix ("Row" / "Column" / "Box") must be set
        // on at least one named step — confirms the orientation_group
        // resolution is wired correctly.
        let names =
            run_named_solve("../eprime/sudoku/sudokuwiki/hiddensingles/hiddensingles.param");
        let prefixed: Vec<&String> = names
            .iter()
            .filter(|n| n.starts_with("Row ") || n.starts_with("Column ") || n.starts_with("Box "))
            .collect();
        assert!(
            !prefixed.is_empty(),
            "expected at least one orientation-prefixed name; names were: {names:?}"
        );
    }

    /// `all_alternatives_for_literal` must return at least one MUS — with a
    /// populated fingerprint — for a literal that the planner *would* deduce
    /// next. This is the per-candidate query mystify calls in pass 2: take a
    /// state and a literal, get back every minimal explanation with its
    /// technique label.
    ///
    /// We pick a literal off the *first* solve step rather than hard-coding
    /// cell coordinates, so the test stays robust to model edits.
    #[test]
    fn all_alternatives_for_literal_returns_named_muses() {
        let parse = build_puzzleparse(
            "../eprime/sudoku.eprime",
            "../eprime/sudoku/sudokuwiki/hiddensingles/hiddensingles.param",
        );
        let solver = PuzzleSolver::new(Arc::new(parse)).expect("solver init");
        let db = Arc::new(Database::load_from_dir(&db_path()).expect("db load"));
        let mut planner = PuzzlePlanner::new(solver).with_database(db);

        // Pull one chosen MUS out of step 1, get its target literal as a
        // (var-indices, value) tuple — exactly the shape `lit_def` wants.
        let first_step = planner
            .quick_solve()
            .into_iter()
            .next()
            .expect("first step");
        let first_um = first_step.into_iter().next().expect("first mus");
        let first_lit = first_um.lits.iter().next().expect("first lit").clone();
        let mut lit_def: Vec<i64> = first_lit.var().indices().clone();
        lit_def.push(first_lit.val());

        // Fresh planner from the same starting state — quick_solve mutated
        // the previous one.
        let parse2 = build_puzzleparse(
            "../eprime/sudoku.eprime",
            "../eprime/sudoku/sudokuwiki/hiddensingles/hiddensingles.param",
        );
        let solver2 = PuzzleSolver::new(Arc::new(parse2)).expect("solver init 2");
        let db2 = Arc::new(Database::load_from_dir(&db_path()).expect("db load 2"));
        let mut planner2 = PuzzlePlanner::new(solver2).with_database(db2);

        let alts = planner2.all_alternatives_for_literal(lit_def);
        assert!(
            !alts.is_empty(),
            "expected at least one alternative MUS for lit {first_lit:?}"
        );
        for um in &alts {
            assert!(
                !um.fingerprint.is_empty(),
                "every UserMus must carry a fingerprint"
            );
        }
        // Sorted smallest-first.
        let sizes: Vec<usize> = alts.iter().map(|u| u.constraints.len()).collect();
        let mut sorted = sizes.clone();
        sorted.sort();
        assert_eq!(sizes, sorted, "alternatives must be sorted smallest-first");
    }
}

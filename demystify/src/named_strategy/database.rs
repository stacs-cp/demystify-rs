//! Name database for matching MUS fingerprints to human-recognised techniques.
//!
//! The database is a directory of TOML files, one per `$#KIND`. Each entry
//! associates a canonical-form `MusFingerprint` string with a human technique
//! name and (optionally) the family-group whose member labels should be
//! prefixed at display time to surface the concrete orientation.
//!
//! Schema:
//! ```toml
//! [[strategy]]
//! name = "Hidden single"
//! fingerprint = "..."         # opaque MusFingerprint canonical-form string
//! orientation_group = "unit_contains"   # optional
//! ```

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::{info, warn};

use crate::problem::musdict::MusContext;
use crate::problem::parse::PuzzleParse;

use super::fingerprint::MusFingerprint;

/// One named technique entry in the database.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strategy {
    /// Human-readable technique name (e.g. "Hidden single").
    pub name: String,
    /// Canonical-form fingerprint string this strategy matches.
    pub fingerprint: String,
    /// Optional: name of the family-group whose member label should be used
    /// to prefix the technique name at display time. The display logic looks
    /// up `families[orientation_group].members[raw_family]` for each MUS
    /// constraint whose raw family is in the group, and uses the unique
    /// resulting label as the prefix.
    #[serde(default)]
    pub orientation_group: Option<String>,
}

/// Top-level structure of one strategy DB file.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct DbFile {
    #[serde(default)]
    strategy: Vec<Strategy>,
}

/// In-memory database keyed by `$#KIND` then by fingerprint string.
#[derive(Debug, Default, Clone)]
pub struct Database {
    by_kind: BTreeMap<String, BTreeMap<String, Strategy>>,
}

impl Database {
    /// Empty database — every lookup misses.  Useful for tests and the case
    /// where no DB directory is configured.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Load every `*.toml` file in `dir` as a strategy DB. The file basename
    /// (without extension) becomes the `$#KIND` key. Returns an empty
    /// database (with a warning logged) if `dir` does not exist.
    ///
    /// Per-file parse errors propagate as `Err`; a malformed DB is treated
    /// as a configuration bug, not silently swallowed.
    pub fn load_from_dir(dir: &Path) -> Result<Self> {
        let mut db = Self::empty();
        if !dir.exists() {
            warn!(target: "named_strategy",
                "strategy DB directory {:?} does not exist; named techniques disabled", dir);
            return Ok(db);
        }
        for entry in
            fs::read_dir(dir).with_context(|| format!("reading strategy DB directory {dir:?}"))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }
            let kind = path
                .file_stem()
                .and_then(|s| s.to_str())
                .with_context(|| format!("DB file {path:?} has non-UTF-8 stem"))?
                .to_string();
            let content = fs::read_to_string(&path)
                .with_context(|| format!("reading strategy DB file {path:?}"))?;
            let parsed: DbFile = toml::from_str(&content)
                .with_context(|| format!("parsing strategy DB file {path:?}"))?;
            let n = parsed.strategy.len();
            db.add_kind(&kind, parsed.strategy)?;
            info!(target: "named_strategy", "loaded {n} strategies for kind '{kind}' from {path:?}");
        }
        Ok(db)
    }

    /// Insert all strategies for a single `$#KIND`. Errors on duplicate
    /// fingerprints within the same kind.
    pub fn add_kind(&mut self, kind: &str, strategies: Vec<Strategy>) -> Result<()> {
        let entry = self.by_kind.entry(kind.to_string()).or_default();
        for s in strategies {
            if let Some(existing) = entry.get(&s.fingerprint) {
                anyhow::bail!(
                    "strategy DB '{kind}': two entries share fingerprint '{}': '{}' and '{}'",
                    s.fingerprint,
                    existing.name,
                    s.name
                );
            }
            entry.insert(s.fingerprint.clone(), s);
        }
        Ok(())
    }

    /// Look up the strategy for a fingerprint within a `$#KIND` namespace.
    #[must_use]
    pub fn lookup(&self, kind: &str, fingerprint: &MusFingerprint) -> Option<&Strategy> {
        self.by_kind.get(kind)?.get(fingerprint.as_str())
    }

    /// Return every strategy for a given `$#KIND` whose `orientation_group`
    /// is set but does not appear in `declared_groups`. Used to surface
    /// curation errors (typo in DB, or model dropped a `$#FAMILY` line)
    /// loudly rather than silently dropping prefixes at display time.
    #[must_use]
    pub fn dangling_orientation_groups<'a>(
        &'a self,
        kind: &str,
        declared_groups: &std::collections::BTreeSet<&str>,
    ) -> Vec<&'a Strategy> {
        let Some(entries) = self.by_kind.get(kind) else {
            return Vec::new();
        };
        entries
            .values()
            .filter(|s| {
                s.orientation_group
                    .as_deref()
                    .is_some_and(|g| !declared_groups.contains(g))
            })
            .collect()
    }

    /// Total number of strategies across all kinds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_kind.values().map(BTreeMap::len).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_kind.values().all(BTreeMap::is_empty)
    }
}

/// Compose the human-readable display name for a matched strategy.
///
/// If the strategy has `orientation_group` set, scan the MUS constraints,
/// take their group-relative member labels via
/// `families[orientation_group].members[raw_family]`, and prefix the
/// (unique) label to the technique name. If the matched constraints disagree
/// on label or no orientation group is set, return the bare technique name.
#[must_use]
pub fn display_name(strategy: &Strategy, mus: &MusContext, parse: &PuzzleParse) -> String {
    let Some(group_id) = strategy.orientation_group.as_deref() else {
        return strategy.name.clone();
    };
    let Some(family) = parse.eprime.families.get(group_id) else {
        return strategy.name.clone();
    };
    let labels: BTreeSet<&String> = mus
        .mus
        .iter()
        .filter_map(|lit| {
            let raw = parse.constraints.family_of(lit)?;
            family.members.get(raw.as_str())
        })
        .collect();
    if labels.len() == 1 {
        format!("{} {}", labels.iter().next().unwrap(), strategy.name)
    } else {
        strategy.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_database_misses_everything() {
        let db = Database::empty();
        let fp = MusFingerprint {
            canonical: "anything".into(),
        };
        assert!(db.lookup("Sudoku", &fp).is_none());
        assert!(db.is_empty());
    }

    #[test]
    fn lookup_within_kind() {
        let mut db = Database::empty();
        db.add_kind(
            "Sudoku",
            vec![
                Strategy {
                    name: "Naked single".into(),
                    fingerprint: "ns-fp".into(),
                    orientation_group: None,
                },
                Strategy {
                    name: "Hidden single".into(),
                    fingerprint: "hs-fp".into(),
                    orientation_group: Some("unit_contains".into()),
                },
            ],
        )
        .unwrap();

        let fp_ns = MusFingerprint {
            canonical: "ns-fp".into(),
        };
        let fp_hs = MusFingerprint {
            canonical: "hs-fp".into(),
        };
        let fp_other = MusFingerprint {
            canonical: "missing".into(),
        };
        assert_eq!(db.lookup("Sudoku", &fp_ns).unwrap().name, "Naked single");
        assert_eq!(db.lookup("Sudoku", &fp_hs).unwrap().name, "Hidden single");
        assert!(db.lookup("Sudoku", &fp_other).is_none());
        // Wrong kind: no leakage.
        assert!(db.lookup("Binairo", &fp_ns).is_none());
        assert_eq!(db.len(), 2);
    }

    #[test]
    fn duplicate_fingerprint_within_kind_fails() {
        let mut db = Database::empty();
        let result = db.add_kind(
            "Sudoku",
            vec![
                Strategy {
                    name: "A".into(),
                    fingerprint: "same".into(),
                    orientation_group: None,
                },
                Strategy {
                    name: "B".into(),
                    fingerprint: "same".into(),
                    orientation_group: None,
                },
            ],
        );
        assert!(result.is_err(), "duplicate fingerprint should fail");
    }

    #[test]
    fn load_from_dir_picks_up_toml_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("Sudoku.toml"),
            r#"
[[strategy]]
name = "Naked single"
fingerprint = "ns-fp"

[[strategy]]
name = "Hidden single"
fingerprint = "hs-fp"
orientation_group = "unit_contains"
"#,
        )
        .unwrap();
        std::fs::write(tmp.path().join("notes.md"), "ignored").unwrap();

        let db = Database::load_from_dir(tmp.path()).unwrap();
        assert_eq!(db.len(), 2);
        let fp = MusFingerprint {
            canonical: "hs-fp".into(),
        };
        let s = db.lookup("Sudoku", &fp).unwrap();
        assert_eq!(s.name, "Hidden single");
        assert_eq!(s.orientation_group.as_deref(), Some("unit_contains"));
    }

    #[test]
    fn load_from_dir_missing_dir_returns_empty() {
        let db = Database::load_from_dir(Path::new("/no/such/dir/12345")).unwrap();
        assert!(db.is_empty());
    }

    #[test]
    fn load_from_dir_malformed_file_errors() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("Bad.toml"), "this is not valid toml ===").unwrap();
        let result = Database::load_from_dir(tmp.path());
        assert!(result.is_err(), "malformed TOML should fail loudly");
    }

    #[test]
    fn dangling_orientation_groups_finds_typos() {
        let mut db = Database::empty();
        db.add_kind(
            "Sudoku",
            vec![
                Strategy {
                    name: "Naked single".into(),
                    fingerprint: "ns".into(),
                    orientation_group: None,
                },
                Strategy {
                    name: "Hidden single".into(),
                    fingerprint: "hs".into(),
                    orientation_group: Some("unit_contains".into()),
                },
                Strategy {
                    name: "Pointing pair".into(),
                    fingerprint: "pp".into(),
                    orientation_group: Some("typo_group".into()),
                },
            ],
        )
        .unwrap();

        let declared: BTreeSet<&str> = ["unit_contains", "unit_atmost"].into_iter().collect();
        let bad = db.dangling_orientation_groups("Sudoku", &declared);
        assert_eq!(bad.len(), 1, "should flag the one entry with typo");
        assert_eq!(bad[0].name, "Pointing pair");

        // Wrong kind: no entries at all to check.
        assert!(
            db.dangling_orientation_groups("Binairo", &declared)
                .is_empty()
        );

        // Empty declared set: every set orientation_group becomes dangling.
        let empty = BTreeSet::new();
        assert_eq!(db.dangling_orientation_groups("Sudoku", &empty).len(), 2);
    }

    #[test]
    fn unknown_field_in_strategy_errors() {
        // `deny_unknown_fields` should catch typos like `orientation_grup`
        // (missing 'o') so they don't silently noop.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("Sudoku.toml"),
            r#"
[[strategy]]
name = "Hidden single"
fingerprint = "row_contains;"
orientation_grup = "unit_contains"
"#,
        )
        .unwrap();
        let result = Database::load_from_dir(tmp.path());
        assert!(result.is_err(), "typo'd field should fail to load");
    }

    #[test]
    fn empty_toml_file_yields_zero_strategies() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("Empty.toml"),
            "# no strategies yet — populate via demystify-fingerprint\n",
        )
        .unwrap();
        let db = Database::load_from_dir(tmp.path()).unwrap();
        assert!(db.is_empty());
    }

    /// Minimal PuzzleParse builder for display_name tests. Inserts the given
    /// (lit, family, description) triples into the constraint store, and
    /// attaches the supplied family-group annotations. No SAT instance.
    fn build_parse_for_display(
        families: BTreeMap<String, crate::problem::parse::Family>,
        constraints: &[(rustsat::types::Lit, &str, &str)],
    ) -> PuzzleParse {
        let mut parse = PuzzleParse::new_from_eprime(
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            Some("Sudoku".into()),
            Vec::new(),
            families,
            Vec::new(),
        );
        for (lit, family, desc) in constraints {
            parse
                .constraints
                .insert(*lit, (*family).to_string(), (*desc).to_string(), Vec::new())
                .expect("insert should succeed");
        }
        parse
    }

    fn unit_contains_family() -> BTreeMap<String, crate::problem::parse::Family> {
        let mut families = BTreeMap::new();
        families.insert(
            "unit_contains".to_string(),
            crate::problem::parse::Family {
                label: "Unit contains".into(),
                members: [
                    ("row_contains".to_string(), "Row".to_string()),
                    ("con_contains".to_string(), "Column".to_string()),
                    ("box_contains".to_string(), "Box".to_string()),
                ]
                .into_iter()
                .collect(),
            },
        );
        families
    }

    #[test]
    fn display_name_with_no_orientation_group() {
        let parse = build_parse_for_display(BTreeMap::new(), &[]);
        let strategy = Strategy {
            name: "Naked single".into(),
            fingerprint: "fp".into(),
            orientation_group: None,
        };
        let mus = MusContext {
            lits: BTreeSet::new(),
            mus: BTreeSet::new(),
        };
        assert_eq!(display_name(&strategy, &mus, &parse), "Naked single");
    }

    #[test]
    fn display_name_prefixes_unique_orientation_label() {
        let lit = rustsat::types::Lit::from_ipasir(1).unwrap();
        let parse = build_parse_for_display(
            unit_contains_family(),
            &[(lit, "row_contains", "row_contains[3, 5]")],
        );
        let strategy = Strategy {
            name: "hidden single".into(),
            fingerprint: "fp".into(),
            orientation_group: Some("unit_contains".into()),
        };
        let mus = MusContext {
            lits: BTreeSet::new(),
            mus: BTreeSet::from([lit]),
        };
        assert_eq!(display_name(&strategy, &mus, &parse), "Row hidden single");
    }

    #[test]
    fn display_name_box_orientation() {
        let lit = rustsat::types::Lit::from_ipasir(7).unwrap();
        let parse = build_parse_for_display(
            unit_contains_family(),
            &[(lit, "box_contains", "box_contains[0, 1, 4]")],
        );
        let strategy = Strategy {
            name: "hidden single".into(),
            fingerprint: "fp".into(),
            orientation_group: Some("unit_contains".into()),
        };
        let mus = MusContext {
            lits: BTreeSet::new(),
            mus: BTreeSet::from([lit]),
        };
        assert_eq!(display_name(&strategy, &mus, &parse), "Box hidden single");
    }

    #[test]
    fn display_name_drops_prefix_when_labels_disagree() {
        // Defensive case: a MUS with both row_contains and con_contains active
        // would produce conflicting orientation labels — drop the prefix.
        let lit_a = rustsat::types::Lit::from_ipasir(1).unwrap();
        let lit_b = rustsat::types::Lit::from_ipasir(2).unwrap();
        let parse = build_parse_for_display(
            unit_contains_family(),
            &[
                (lit_a, "row_contains", "row_contains[3, 5]"),
                (lit_b, "con_contains", "con_contains[6, 5]"),
            ],
        );
        let strategy = Strategy {
            name: "weird thing".into(),
            fingerprint: "fp".into(),
            orientation_group: Some("unit_contains".into()),
        };
        let mus = MusContext {
            lits: BTreeSet::new(),
            mus: BTreeSet::from([lit_a, lit_b]),
        };
        assert_eq!(display_name(&strategy, &mus, &parse), "weird thing");
    }

    #[test]
    fn display_name_drops_prefix_when_orientation_group_unknown() {
        // Defensive: strategy references a group that doesn't exist in the model.
        let lit = rustsat::types::Lit::from_ipasir(1).unwrap();
        let parse = build_parse_for_display(
            unit_contains_family(),
            &[(lit, "row_contains", "row_contains[1, 2]")],
        );
        let strategy = Strategy {
            name: "Hidden single".into(),
            fingerprint: "fp".into(),
            orientation_group: Some("nonexistent_group".into()),
        };
        let mus = MusContext {
            lits: BTreeSet::new(),
            mus: BTreeSet::from([lit]),
        };
        assert_eq!(display_name(&strategy, &mus, &parse), "Hidden single");
    }

    #[test]
    fn display_name_drops_prefix_when_no_constraints_in_group() {
        // The MUS exists but contains no constraints whose raw family is in
        // the orientation group — fall back to the bare technique name.
        let lit = rustsat::types::Lit::from_ipasir(1).unwrap();
        let parse = build_parse_for_display(
            unit_contains_family(),
            &[(lit, "row_alldiff", "row_alldiff[1, 2, 3]")],
        );
        let strategy = Strategy {
            name: "Naked something".into(),
            fingerprint: "fp".into(),
            orientation_group: Some("unit_contains".into()),
        };
        let mus = MusContext {
            lits: BTreeSet::new(),
            mus: BTreeSet::from([lit]),
        };
        assert_eq!(display_name(&strategy, &mus, &parse), "Naked something");
    }
}

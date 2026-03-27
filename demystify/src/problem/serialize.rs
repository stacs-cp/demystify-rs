//! Serialization support for PuzzleParse and related types.
//!
//! This module provides JSON serialization and deserialization for puzzle data,
//! allowing pre-parsed puzzles to be saved and loaded without running external
//! tools like conjure or savilerow.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use rustsat::instances::{BasicVarManager, Cnf, SatInstance};
use rustsat::types::{Clause, Lit};
use serde::{Deserialize, Serialize};

use crate::problem::{PuzLit, PuzVar};

use super::parse::{EPrimeAnnotations, PuzzleParse};

/// Serializable version of EPrimeAnnotations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableEPrimeAnnotations {
    pub vars: BTreeSet<String>,
    pub auxvars: BTreeSet<String>,
    pub cons: BTreeMap<String, String>,
    pub reveal: BTreeMap<String, String>,
    pub reveal_values: BTreeSet<String>,
    pub params: BTreeMap<String, serde_json::Value>,
    pub kind: Option<String>,
    pub info: Vec<String>,
    #[serde(default)]
    pub decs: Vec<String>,
}

impl From<&EPrimeAnnotations> for SerializableEPrimeAnnotations {
    fn from(e: &EPrimeAnnotations) -> Self {
        Self {
            vars: e.vars.clone(),
            auxvars: e.auxvars.clone(),
            cons: e.cons.clone(),
            reveal: e.reveal.clone(),
            reveal_values: e.reveal_values.clone(),
            params: e.params().clone(),
            kind: e.kind.clone(),
            info: e.info.clone(),
            decs: e.decs.clone(),
        }
    }
}

impl From<SerializableEPrimeAnnotations> for EPrimeAnnotations {
    fn from(s: SerializableEPrimeAnnotations) -> Self {
        Self {
            vars: s.vars,
            auxvars: s.auxvars,
            cons: s.cons,
            reveal: s.reveal.clone(),
            reveal_values: s.reveal_values,
            params: s.params,
            kind: s.kind,
            info: s.info,
            decs: s.decs,
        }
    }
}

/// Serializable version of PuzzleParse
///
/// This converts rustsat types to their serializable equivalents:
/// - `Lit` -> i32 (via ipasir format), stored as String keys where needed for JSON
/// - `Cnf` -> Vec<Vec<i32>> (list of clauses)
/// - `SatInstance` is not serialized (rebuilt from cnf on load)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializablePuzzleParse {
    pub eprime: SerializableEPrimeAnnotations,
    /// CNF clauses as list of literals in ipasir format
    pub cnf_clauses: Vec<Vec<i32>>,
    /// litmap with Lit converted to i32
    pub litmap: Vec<(PuzLit, i32)>,
    /// invlitmap with Lit converted to i32 (as string key for JSON)
    pub invlitmap: BTreeMap<String, BTreeSet<PuzLit>>,
    /// domainmap
    pub domainmap: Vec<(PuzVar, BTreeSet<i64>)>,
    /// conset with Lit converted to i32 (as string key for JSON)
    pub conset: BTreeMap<String, String>,
    /// invconset with Lit converted to i32
    pub invconset: BTreeMap<String, i32>,
    /// varlits_in_con with Lit converted to i32 (as string key for JSON)
    pub varlits_in_con: BTreeMap<String, Vec<i32>>,
    /// varset_lits with Lit converted to i32
    pub varset_lits: BTreeSet<i32>,
    /// varset_lits_neg with Lit converted to i32
    pub varset_lits_neg: BTreeSet<i32>,
    /// conset_lits with Lit converted to i32
    pub conset_lits: BTreeSet<i32>,
    /// order_encoding_map
    pub order_encoding_map: Vec<(PuzVar, HashSet<i32>)>,
    /// inv_order_encoding_map with Lit converted to i32 (as string key for JSON)
    pub inv_order_encoding_map: BTreeMap<String, PuzVar>,
    /// order_encoding_all_lits with Lit converted to i32
    pub order_encoding_all_lits: BTreeSet<i32>,
    /// reveal_map with Lit converted to i32 (as string key for JSON)
    pub reveal_map: BTreeMap<String, i32>,
}

/// Convert a Lit to its ipasir i32 representation
fn lit_to_i32(lit: Lit) -> i32 {
    lit.to_ipasir()
}

/// Convert an i32 ipasir representation back to a Lit
fn i32_to_lit(i: i32) -> Result<Lit> {
    Lit::from_ipasir(i).context("Invalid literal value")
}

impl TryFrom<&PuzzleParse> for SerializablePuzzleParse {
    type Error = anyhow::Error;

    fn try_from(p: &PuzzleParse) -> Result<Self> {
        // Convert CNF to list of clauses
        let cnf_clauses: Vec<Vec<i32>> = p
            .cnf
            .as_ref()
            .map(|cnf| {
                cnf.iter()
                    .map(|clause| clause.iter().map(|&lit| lit_to_i32(lit)).collect())
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            eprime: SerializableEPrimeAnnotations::from(&p.eprime),
            cnf_clauses,
            litmap: p
                .litmap
                .iter()
                .map(|(k, &v)| (k.clone(), lit_to_i32(v)))
                .collect(),
            invlitmap: p
                .invlitmap
                .iter()
                .map(|(&k, v)| (lit_to_i32(k).to_string(), v.clone()))
                .collect(),
            domainmap: p
                .domainmap
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            conset: p
                .conset
                .iter()
                .map(|(&k, v)| (lit_to_i32(k).to_string(), v.clone()))
                .collect(),
            invconset: p
                .invconset
                .iter()
                .map(|(k, &v)| (k.clone(), lit_to_i32(v)))
                .collect(),
            varlits_in_con: p
                .varlits_in_con
                .iter()
                .map(|(&k, v)| {
                    (
                        lit_to_i32(k).to_string(),
                        v.iter().map(|&l| lit_to_i32(l)).collect(),
                    )
                })
                .collect(),
            varset_lits: p.varset_lits.iter().map(|&l| lit_to_i32(l)).collect(),
            varset_lits_neg: p.varset_lits_neg.iter().map(|&l| lit_to_i32(l)).collect(),
            conset_lits: p.conset_lits.iter().map(|&l| lit_to_i32(l)).collect(),
            order_encoding_map: p
                .order_encoding_map
                .iter()
                .map(|(k, v)| (k.clone(), v.iter().map(|&l| lit_to_i32(l)).collect()))
                .collect(),
            inv_order_encoding_map: p
                .inv_order_encoding_map
                .iter()
                .map(|(&k, v)| (lit_to_i32(k).to_string(), v.clone()))
                .collect(),
            order_encoding_all_lits: p
                .order_encoding_all_lits
                .iter()
                .map(|&l| lit_to_i32(l))
                .collect(),
            reveal_map: p
                .reveal_map
                .iter()
                .map(|(&k, &v)| (lit_to_i32(k).to_string(), lit_to_i32(v)))
                .collect(),
        })
    }
}

/// Parse a string key back to i32
fn str_to_i32(s: &str) -> Result<i32> {
    s.parse::<i32>().context("Invalid literal string")
}

impl TryFrom<SerializablePuzzleParse> for PuzzleParse {
    type Error = anyhow::Error;

    fn try_from(s: SerializablePuzzleParse) -> Result<Self> {
        // Rebuild CNF from clauses
        let mut cnf = Cnf::new();
        for clause in &s.cnf_clauses {
            let lits: Result<Vec<Lit>> = clause.iter().map(|&i| i32_to_lit(i)).collect();
            cnf.add_clause(Clause::from_iter(lits?));
        }

        // Create a SatInstance from the CNF
        let satinstance: SatInstance<BasicVarManager> = cnf.clone().into();

        Ok(Self {
            eprime: s.eprime.into(),
            satinstance,
            cnf: Some(Arc::new(cnf)),
            litmap: s
                .litmap
                .into_iter()
                .map(|(k, v)| Ok((k, i32_to_lit(v)?)))
                .collect::<Result<_>>()?,
            invlitmap: s
                .invlitmap
                .into_iter()
                .map(|(k, v)| Ok((i32_to_lit(str_to_i32(&k)?)?, v)))
                .collect::<Result<_>>()?,
            domainmap: s.domainmap.into_iter().collect(),
            conset: s
                .conset
                .into_iter()
                .map(|(k, v)| Ok((i32_to_lit(str_to_i32(&k)?)?, v)))
                .collect::<Result<_>>()?,
            invconset: s
                .invconset
                .into_iter()
                .map(|(k, v)| Ok((k, i32_to_lit(v)?)))
                .collect::<Result<_>>()?,
            varlits_in_con: s
                .varlits_in_con
                .into_iter()
                .map(|(k, v)| {
                    let lits: Result<Vec<Lit>> = v.into_iter().map(i32_to_lit).collect();
                    Ok((i32_to_lit(str_to_i32(&k)?)?, lits?))
                })
                .collect::<Result<_>>()?,
            varset_lits: s
                .varset_lits
                .into_iter()
                .map(i32_to_lit)
                .collect::<Result<_>>()?,
            varset_lits_neg: s
                .varset_lits_neg
                .into_iter()
                .map(i32_to_lit)
                .collect::<Result<_>>()?,
            conset_lits: s
                .conset_lits
                .into_iter()
                .map(i32_to_lit)
                .collect::<Result<_>>()?,
            order_encoding_map: s
                .order_encoding_map
                .into_iter()
                .map(|(k, v)| {
                    let lits: Result<HashSet<Lit>> = v.into_iter().map(i32_to_lit).collect();
                    Ok((k, lits?))
                })
                .collect::<Result<_>>()?,
            inv_order_encoding_map: s
                .inv_order_encoding_map
                .into_iter()
                .map(|(k, v)| Ok((i32_to_lit(str_to_i32(&k)?)?, v)))
                .collect::<Result<_>>()?,
            order_encoding_all_lits: s
                .order_encoding_all_lits
                .into_iter()
                .map(i32_to_lit)
                .collect::<Result<_>>()?,
            reveal_map: s
                .reveal_map
                .into_iter()
                .map(|(k, v)| Ok((i32_to_lit(str_to_i32(&k)?)?, i32_to_lit(v)?)))
                .collect::<Result<_>>()?,
        })
    }
}

impl PuzzleParse {
    /// Save the puzzle parse to a JSON file.
    ///
    /// This allows the puzzle to be loaded later without running conjure/savilerow.
    pub fn save_to_json(&self, path: &Path) -> Result<()> {
        let serializable = SerializablePuzzleParse::try_from(self)?;
        let file = File::create(path).context("Failed to create output file")?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &serializable)
            .context("Failed to serialize puzzle")?;
        Ok(())
    }

    /// Load a puzzle parse from a JSON file.
    ///
    /// This loads a puzzle that was previously saved with `save_to_json`.
    pub fn load_from_json(path: &Path) -> Result<Self> {
        let file = File::open(path).context("Failed to open input file")?;
        let reader = BufReader::new(file);
        let serializable: SerializablePuzzleParse =
            serde_json::from_reader(reader).context("Failed to deserialize puzzle")?;
        serializable.try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::problem::util::test_utils::build_puzzleparse;
    use tempfile::NamedTempFile;

    #[test]
    fn test_serialize_roundtrip_little() {
        let original = build_puzzleparse("./tst/little1.eprime", "./tst/little1.param");

        // Save to temp file
        let temp_file = NamedTempFile::new().unwrap();
        original.save_to_json(temp_file.path()).unwrap();

        // Load back
        let loaded = PuzzleParse::load_from_json(temp_file.path()).unwrap();

        // Compare key fields
        assert_eq!(original.eprime.vars, loaded.eprime.vars);
        assert_eq!(original.eprime.auxvars, loaded.eprime.auxvars);
        assert_eq!(original.eprime.cons, loaded.eprime.cons);
        assert_eq!(original.eprime.kind, loaded.eprime.kind);
        assert_eq!(original.litmap, loaded.litmap);
        assert_eq!(original.invlitmap, loaded.invlitmap);
        assert_eq!(original.domainmap, loaded.domainmap);
        assert_eq!(original.conset, loaded.conset);
        assert_eq!(original.invconset, loaded.invconset);
        assert_eq!(original.varset_lits, loaded.varset_lits);
        assert_eq!(original.conset_lits, loaded.conset_lits);
    }

    #[test]
    fn test_serialize_roundtrip_binairo() {
        let original = build_puzzleparse("./tst/binairo.eprime", "./tst/binairo-1.param");

        let temp_file = NamedTempFile::new().unwrap();
        original.save_to_json(temp_file.path()).unwrap();

        let loaded = PuzzleParse::load_from_json(temp_file.path()).unwrap();

        assert_eq!(original.eprime.vars, loaded.eprime.vars);
        assert_eq!(original.litmap, loaded.litmap);
        assert_eq!(original.conset_lits.len(), loaded.conset_lits.len());
    }

    #[test]
    fn test_serialize_solve_after_load() {
        use crate::problem::planner::PuzzlePlanner;
        use crate::problem::solver::PuzzleSolver;
        use std::sync::Arc;

        let original = build_puzzleparse("./tst/little1.eprime", "./tst/little1.param");

        let temp_file = NamedTempFile::new().unwrap();
        original.save_to_json(temp_file.path()).unwrap();

        let loaded = PuzzleParse::load_from_json(temp_file.path()).unwrap();
        let loaded = Arc::new(loaded);

        // Create solver and planner from loaded puzzle
        let solver = PuzzleSolver::new(loaded).unwrap();
        let mut planner = PuzzlePlanner::new(solver);

        // Solve the puzzle
        let steps = planner.quick_solve();

        // Should have solution steps
        assert!(!steps.is_empty());
    }
}

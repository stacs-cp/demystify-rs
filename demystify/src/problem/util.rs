use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::bail;
use itertools::Itertools;
use rustsat::{instances::SatInstance, types::Lit};
use tracing::info;

// `exec` runs `conjure`/`savilerow` via std::process and `which`. Neither
// works on `wasm32-unknown-unknown` (the `which` crate doesn't compile there),
// and pre-parsed-JSON is the only intended wasm input path anyway.
#[cfg(not(target_arch = "wasm32"))]
pub mod exec;
pub mod parsing;

pub fn safe_insert<K: Ord, V>(dict: &mut BTreeMap<K, V>, key: K, value: V) -> anyhow::Result<()> {
    if dict.insert(key, value).is_some() {
        bail!("Internal Error: Repeated Key")
    }
    Ok(())
}

pub struct FindVarConnections {
    clauses: Vec<Vec<Lit>>,
    lit_to_clauses: HashMap<Lit, HashSet<usize>>,
    all_var_lits: HashSet<Lit>,
}

impl FindVarConnections {
    #[must_use]
    pub fn new(sat: &SatInstance, all_var_lits: &HashSet<Lit>) -> FindVarConnections {
        let (cnf, _) = sat.clone().into_cnf();

        // Store clauses, and index each literal to the clauses it appears in.
        // The union of a literal's clauses (computed lazily in `get_connections`)
        // gives its co-occurring literals; building it eagerly was O(k^2) per
        // clause of size k, which blew up on very large clauses.
        let mut clauses: Vec<Vec<Lit>> = Vec::new();
        let mut lit_to_clauses: HashMap<Lit, HashSet<usize>> = HashMap::new();
        for clause in &cnf {
            let idx = clauses.len();
            let lits: Vec<Lit> = clause.iter().copied().collect();
            for &lit in &lits {
                lit_to_clauses.entry(lit).or_default().insert(idx);
            }
            clauses.push(lits);
        }

        // Blank out any literals in unit clauses
        for clause in &clauses {
            if clause.len() == 1 {
                let lit = clause[0];
                lit_to_clauses.insert(lit, HashSet::new());
                lit_to_clauses.insert(-lit, HashSet::new());
            }
        }

        FindVarConnections {
            clauses,
            lit_to_clauses,
            all_var_lits: all_var_lits.clone(),
        }
    }

    pub fn get_connections(&self, con_lit: Lit) -> Vec<Lit> {
        let mut todo: Vec<Lit> = vec![];
        let mut found: HashSet<Lit> = HashSet::new();

        if !self.lit_to_clauses.contains_key(&-con_lit) {
            return vec![];
        }

        info!("Looking for connections for: {con_lit}");

        todo.push(-con_lit);
        todo.push(con_lit);

        while let Some(todo_lit) = todo.pop() {
            info!("Todo: {}", todo_lit);
            let clause_idxs = self.lit_to_clauses.get(&todo_lit);
            if let Some(clause_idxs) = clause_idxs {
                for &idx in clause_idxs {
                    for &lit in &self.clauses[idx] {
                        let lit = -lit;
                        info!("Considering {}\n", lit.to_ipasir());
                        if !found.contains(&lit) {
                            info!("Found {}\n", lit.to_ipasir());
                            found.insert(lit);
                            if self.all_var_lits.contains(&lit) {
                                info!("In var_lits: {}\n", lit.to_ipasir());
                            } else {
                                assert!(!self.all_var_lits.contains(&-lit));
                                info!("Add to todo: {}\n", lit.to_ipasir());
                                todo.push(lit);
                            }
                        }
                    }
                }
            }
        }

        found
            .intersection(&self.all_var_lits)
            .copied()
            .collect_vec()
    }
}

pub mod json;
pub mod timer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_var_connections() {
        let eprime_path = "./tst/binairo.eprime";
        let eprimeparam_path = "./tst/binairo-1.param";

        let puz =
            crate::problem::util::test_utils::build_puzzleparse(eprime_path, eprimeparam_path);

        let fvc = FindVarConnections::new(&puz.satinstance, &puz.all_var_related_lits());

        for c in puz.constraints.lits() {
            let lits = fvc.get_connections(*c);
            let puzlits = lits
                .iter()
                .map(|l| puz.direct_or_ordered_lit_to_varvalpair(l))
                .collect_vec();
            println!("{c} {puzlits:?}");
            for l in &lits {
                println!("{l:?}");
                println!("{:?}", puz.direct.invlitmap.get(l));
                println!("{:?}", puz.order.inv_map.get(l));
            }
        }
    }
}

/// Utilities for building `PuzzleParse` instances from `.eprime`/`.param` files.
/// Used by tests and benchmarks. Skipped on wasm32 (no Conjure / no test runner).
#[cfg(not(target_arch = "wasm32"))]
pub mod test_utils {
    use std::fs;
    use std::path::Path;

    use crate::problem::parse::{PuzzleParse, parse_essence};

    /// Parse an Essence' model + parameter file pair into a `PuzzleParse`.
    ///
    /// Runs Conjure in a temporary directory.  Panics on parse failure — intended for
    /// tests and benchmarks where a bad parse is always a bug.
    #[must_use]
    pub fn build_puzzleparse(eprime_path: &str, eprimeparam_path: &str) -> PuzzleParse {
        let eprime_path = env!("CARGO_MANIFEST_DIR").to_string() + "/" + eprime_path;
        let eprimeparam_path = env!("CARGO_MANIFEST_DIR").to_string() + "/" + eprimeparam_path;

        let temp_dir = tempfile::Builder::new()
            .prefix(".demystify-")
            .tempdir_in(".")
            .expect("Failed to create temporary directory");

        // Preserve original filenames so Conjure output filenames are predictable.
        let eprime_name = Path::new(&eprime_path).file_name().unwrap();
        let param_name = Path::new(&eprimeparam_path).file_name().unwrap();
        let temp_eprime = temp_dir.path().join(eprime_name);
        let temp_param = temp_dir.path().join(param_name);

        fs::copy(&eprime_path, &temp_eprime).expect("Failed to copy eprime file");
        fs::copy(&eprimeparam_path, &temp_param).expect("Failed to copy param file");

        let result = parse_essence(&temp_eprime, &temp_param);
        assert!(result.is_ok(), "Bad parse: {result:?}");

        temp_dir
            .close()
            .expect("Failed to clean up temporary directory");
        result.unwrap()
    }
}

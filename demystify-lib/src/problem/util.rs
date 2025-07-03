use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::bail;
use itertools::Itertools;
use rustsat::{instances::SatInstance, types::Lit};
use tracing::info;

pub mod parsing;

pub fn safe_insert<K: Ord, V>(dict: &mut BTreeMap<K, V>, key: K, value: V) -> anyhow::Result<()> {
    if dict.insert(key, value).is_some() {
        bail!("Internal Error: Repeated Key")
    }
    Ok(())
}

pub struct FindVarConnections {
    lit_to_clauses: HashMap<Lit, HashSet<Lit>>,
    all_var_lits: HashSet<Lit>,
}

impl FindVarConnections {
    #[must_use]
    pub fn new(sat: &SatInstance, all_var_lits: &HashSet<Lit>) -> FindVarConnections {
        let (cnf, _) = sat.clone().into_cnf();
        let mut lit_to_clauses: HashMap<Lit, HashSet<Lit>> = HashMap::new();
        for clause in &cnf {
            for &lit in clause {
                let s = lit_to_clauses.entry(lit).or_default();
                for &l in clause.iter() {
                    s.insert(l);
                }
            }
        }

        // Blank out any literals in unit clauses
        for clause in &cnf {
            if clause.len() == 1 {
                let &lit = clause.iter().next().unwrap();
                lit_to_clauses.insert(lit, HashSet::new());
                lit_to_clauses.insert(-lit, HashSet::new());
            }
        }

        FindVarConnections {
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
            let litset = self.lit_to_clauses.get(&todo_lit);
            if let Some(litset) = litset {
                for &lit in litset {
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

        for c in &puz.conset_lits {
            let lits = fvc.get_connections(*c);
            let puzlits = lits
                .iter()
                .map(|l| puz.direct_or_ordered_lit_to_varvalpair(l))
                .collect_vec();
            println!("{c} {puzlits:?}");
            for l in &lits {
                println!("{l:?}");
                println!("{:?}", puz.invlitmap.get(l));
                println!("{:?}", puz.inv_order_encoding_map.get(l));
            }
        }
    }
}

#[cfg(test)]
pub mod test_utils {
    use std::fs;

    use crate::problem::parse::{PuzzleParse, parse_essence};

    // Here we put some utility functions used in various places
    #[must_use]
    pub fn build_puzzleparse(eprime_path: &str, eprimeparam_path: &str) -> PuzzleParse {
        // Create temporary directory for test files
        let eprime_path = env!("CARGO_MANIFEST_DIR").to_string() + "/" + eprime_path;
        let eprimeparam_path = env!("CARGO_MANIFEST_DIR").to_string() + "/" + eprimeparam_path;
        let temp_dir = tempfile::tempdir().expect("Failed to create temporary directory");

        // Copy eprime file to temporary directory
        let temp_eprime_path = temp_dir.path().join("binairo.eprime");
        fs::copy(dbg!(eprime_path), &temp_eprime_path).expect("Failed to copy eprime file");

        // Copy eprimeparam file to temporary directory
        let temp_eprimeparam_path = temp_dir.path().join("binairo-1.param");

        fs::copy(dbg!(eprimeparam_path), &temp_eprimeparam_path)
            .expect("Failed to copy eprimeparam file");

        // Call parse_essence function
        let result = parse_essence(&temp_eprime_path, &temp_eprimeparam_path);

        assert!(result.is_ok(), "Bad parse: {result:?}");
        // Assert that the function returns Ok
        assert!(result.is_ok());

        // Clean up temporary directory
        temp_dir
            .close()
            .expect("Failed to clean up temporary directory");

        result.unwrap()
    }
}

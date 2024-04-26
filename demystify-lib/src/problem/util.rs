use std::collections::{BTreeMap, HashMap, HashSet};

use super::{parse::PuzzleParse, PuzVar};
use anyhow::{bail, Context};
use itertools::Itertools;
use rustsat::{instances::SatInstance, types::Lit};
use tracing::info;

pub fn parse_savile_row_name(
    dimacs: &PuzzleParse,
    //    vars: &BTreeSet<String>,
    //    auxvars: &BTreeSet<String>,
    n: &str,
) -> anyhow::Result<Option<PuzVar>> {
    let mut matches: Vec<&String> = dimacs
        .eprime
        .vars
        .iter()
        .filter(|&v| n.starts_with(v))
        .collect();
    let conmatch: Vec<&String> = dimacs
        .eprime
        .cons
        .keys()
        .filter(|&v| n.starts_with(v))
        .collect();

    matches.extend(conmatch);

    if matches.is_empty() {
        if !dimacs.eprime.auxvars.iter().any(|v| n.starts_with(v)) {
            bail!("{} is not defined -- should it be AUX?", n);
        }
        return Ok(None);
    }
    if matches.len() > 1 {
        bail!(
            "Variables cannot have a common prefix: Can't tell if {} is {:?}",
            n,
            matches
        );
    }

    let name = matches[0].clone();

    // The variable has no indices, so we are done
    if name == n {
        return Ok(Some(PuzVar::new(&name, vec![])));
    }

    let n = &n[name.len() + 1..];

    let splits: Vec<&str> = n.split('_').collect();
    let mut args = Vec::new();
    for arg in splits {
        if !arg.is_empty() {
            let c = if let Some(strip) = arg.strip_prefix('n') {
                -(strip.parse::<i64>()?)
            } else {
                arg.parse::<i64>()?
            };
            args.push(c);
        }
    }
    Ok(Some(PuzVar::new(&name, args)))
}

pub fn parse_constraint_name(
    template: &str,
    params: &BTreeMap<String, serde_json::value::Value>,
    index: &Vec<i64>,
) -> anyhow::Result<String> {
    let mut context = tera::Context::new();
    context.insert("index", index);
    context.insert("params", params);
    tera::Tera::one_off(template, &context, false)
        .context("Could not parse description of variable or constraint")
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
        for clause in cnf.iter() {
            for &lit in clause {
                let s = lit_to_clauses.entry(lit).or_default();
                for &l in clause.iter() {
                    s.insert(l);
                }
            }
        }

        // Blank out any literals in unit clauses
        for clause in cnf.iter() {
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

        found.insert(con_lit);
        found.insert(-con_lit);
        todo.push(-con_lit);
        todo.push(con_lit);

        while let Some(todo_lit) = todo.pop() {
            info!("Todo: {}", todo_lit);
            let litset = self.lit_to_clauses.get(&todo_lit);
            if let Some(litset) = litset {
                for &lit in litset {
                    let lit = -lit;
                    info!("Considering {}\n", lit);
                    if !found.contains(&lit) {
                        info!("Found {}\n", lit);
                        found.insert(lit);
                        if !self.all_var_lits.contains(&lit) {
                            assert!(!self.all_var_lits.contains(&-lit));
                            info!("Add to todo: {}\n", lit);
                            todo.push(lit);
                        } else {
                            info!("In var_lits: {}\n", lit);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn test_parse_savile_row_name() {
        let vars: BTreeSet<String> = ["var1", "var2", "var3", "var3x"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let auxvars: BTreeSet<String> = ["aux1", "aux2", "aux3"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();

        let mut cons: BTreeMap<String, String> = BTreeMap::new();
        cons.insert("con1".to_string(), "test1".to_string());
        cons.insert("con2".to_string(), "test2".to_string());

        let params = BTreeMap::new();

        let dp = PuzzleParse::new_from_eprime(vars, auxvars, cons, params, None);

        // Test case 1: n starts with a variable in variables
        let n1 = "var1_1_2_3";
        let expected1 = Some(PuzVar::new("var1", vec![1, 2, 3]));
        assert_eq!(parse_savile_row_name(&dp, n1).unwrap(), expected1);

        let n1b = "var1_00001_00002_00010";
        let expected1b = Some(PuzVar::new("var1", vec![1, 2, 10]));
        assert_eq!(parse_savile_row_name(&dp, n1b).unwrap(), expected1b);

        let n1c = "var1_n00001_00002_n00010";
        let expected1c = Some(PuzVar::new("var1", vec![-1, 2, -10]));
        assert_eq!(parse_savile_row_name(&dp, n1c).unwrap(), expected1c);

        let n1d = "var1";
        let expected1d = Some(PuzVar::new("var1", vec![]));
        assert_eq!(parse_savile_row_name(&dp, n1d).unwrap(), expected1d);

        let ncon = "con1";
        let expectedcon = Some(PuzVar::new("con1", vec![]));
        assert_eq!(parse_savile_row_name(&dp, ncon).unwrap(), expectedcon);

        let ne = "var3x";
        assert!(parse_savile_row_name(&dp, ne).is_err());

        // Test case 2: n starts with a variable in aux_variables
        let n2 = "aux2_4_5_6";
        assert_eq!(parse_savile_row_name(&dp, n2).unwrap(), None);

        // Test case 3: n does not start with any variable
        let n3 = "not_found_7_8_9";
        assert!(parse_savile_row_name(&dp, n3).is_err());

        // Test case 4: n starts with multiple variables
        let n4 = "var1_var2_10_11_12";
        assert!(parse_savile_row_name(&dp, n4).is_err());

        // Test case 5: n starts with a variable, but the remaining part is empty
        let n5 = "var1_";
        let expected5 = Some(PuzVar::new("var1", vec![]));
        assert_eq!(parse_savile_row_name(&dp, n5).unwrap(), expected5);
    }

    #[test]
    fn test_parse_constraint_name() {
        let params = serde_json::from_str(r#"{"a":1, "b": 2, "2":7, "3": {"2": 99}}"#).unwrap();
        let index = vec![1, 2, 3];
        let template = r"Constraint {{ index }} with params {{ params.a }}";
        let expected = "Constraint [1, 2, 3] with params 1";
        assert_eq!(
            parse_constraint_name(template, &params, &index).unwrap(),
            expected
        );
        let template = r"Constraint {{ index }} with params {{ params.a + 1 }}";
        let expected = "Constraint [1, 2, 3] with params 2";
        assert_eq!(
            parse_constraint_name(template, &params, &index).unwrap(),
            expected
        );
        let template = r"Constraint {{ index }} with params {{ params.2 }}";
        let expected = "Constraint [1, 2, 3] with params 7";
        assert_eq!(
            parse_constraint_name(template, &params, &index).unwrap(),
            expected
        );

        let template = r"Constraint {{ index }} with params {{ params.3.2 }}";
        let expected = "Constraint [1, 2, 3] with params 99";
        assert_eq!(
            parse_constraint_name(template, &params, &index).unwrap(),
            expected
        );
    }

    #[test]
    fn test_find_var_connections() {
        let eprime_path = "./tst/binairo.eprime";
        let eprimeparam_path = "./tst/binairo-1.param";

        let puz =
            crate::problem::util::test_utils::build_puzzleparse(eprime_path, eprimeparam_path);

        let all_lits = puz
            .varset_lits
            .union(&puz.varset_order_lits)
            .copied()
            .collect();

        let fvc = FindVarConnections::new(&puz.satinstance, &all_lits);

        for c in &puz.conset_lits {
            let lits = fvc.get_connections(*c);
            let puzlits = puz.collect_puzlits_both_direct_and_ordered(lits.clone());
            println!("{c} {puzlits:?}");
            for l in &lits {
                println!("{l:?}");
                println!("{:?}", puz.invlitmap.get(l));
                println!("{:?}", puz.invordervarmap.get(l));
            }
        }
    }
}

#[cfg(test)]
pub mod test_utils {
    use std::fs;

    use crate::problem::parse::{parse_essence, PuzzleParse};

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

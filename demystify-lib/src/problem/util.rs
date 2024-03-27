use super::{parse::PuzzleParse, PuzVar};
use anyhow::bail;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn test_parse_savile_row_name() {
        let vars: BTreeSet<String> = ["var1", "var2", "var3", "var3x"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let auxvars: BTreeSet<String> = ["aux1", "aux2", "aux3"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let mut cons: BTreeMap<String, String> = BTreeMap::new();
        cons.insert("con1".to_string(), "test1".to_string());
        cons.insert("con2".to_string(), "test2".to_string());

        let dp = PuzzleParse::new_from_eprime(vars, auxvars, cons);

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
}

#[cfg(test)]
pub mod test_utils {
    use std::fs;

    use crate::problem::parse::{parse_essence, PuzzleParse};

    // Here we put some utility functions used in various places
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

        if result.is_err() {
            panic!("Bad parse: {:?}", result);
        }
        // Assert that the function returns Ok
        assert!(result.is_ok());

        // Clean up temporary directory
        temp_dir
            .close()
            .expect("Failed to clean up temporary directory");

        result.unwrap()
    }
}

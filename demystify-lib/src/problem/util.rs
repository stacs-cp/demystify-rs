use super::{parse::DimacsParse, VarID};
use anyhow::bail;

pub fn parse_savile_row_name(
    dimacs: &DimacsParse,
    //    vars: &BTreeSet<String>,
    //    auxvars: &BTreeSet<String>,
    n: &str,
) -> anyhow::Result<Option<VarID>> {
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
        return Ok(Some(VarID::new(&name, vec![])));
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
    Ok(Some(VarID::new(&name, args)))
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

        let dp = DimacsParse::new_from_eprime(vars, auxvars, cons);

        // Test case 1: n starts with a variable in variables
        let n1 = "var1_1_2_3";
        let expected1 = Some(VarID::new("var1", vec![1, 2, 3]));
        assert_eq!(parse_savile_row_name(&dp, n1).unwrap(), expected1);

        let n1b = "var1_00001_00002_00010";
        let expected1b = Some(VarID::new("var1", vec![1, 2, 10]));
        assert_eq!(parse_savile_row_name(&dp, n1b).unwrap(), expected1b);

        let n1c = "var1_n00001_00002_n00010";
        let expected1c = Some(VarID::new("var1", vec![-1, 2, -10]));
        assert_eq!(parse_savile_row_name(&dp, n1c).unwrap(), expected1c);

        let n1d = "var1";
        let expected1d = Some(VarID::new("var1", vec![]));
        assert_eq!(parse_savile_row_name(&dp, n1d).unwrap(), expected1d);

        let ncon = "con1";
        let expectedcon = Some(VarID::new("con1", vec![]));
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
        let expected5 = Some(VarID::new("var1", vec![]));
        assert_eq!(parse_savile_row_name(&dp, n5).unwrap(), expected5);
    }
}

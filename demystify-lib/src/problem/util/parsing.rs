use std::collections::BTreeMap;

use anyhow::{bail, Context};

use super::super::PuzVar;

use crate::problem::parse::PuzzleParse;

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
    let revealmatch: Vec<&String> = dimacs
        .eprime
        .reveal
        .values()
        .filter(|&v| n.starts_with(v))
        .collect();

    matches.extend(conmatch);
    matches.extend(revealmatch);

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

        let reveal = BTreeMap::new();

        let dp = PuzzleParse::new_from_eprime(vars, auxvars, cons, reveal, params, None);

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
}

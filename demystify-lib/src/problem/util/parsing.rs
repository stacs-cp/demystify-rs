use std::collections::BTreeMap;

use anyhow::Context;

use super::super::PuzVar;

use crate::problem::parse::PuzzleParse;

/// Splits a Savile Row name into base name and indices
///
/// Takes a name like "var_00001_n00002" and returns ("var", [1, -2])
/// This parsing is non-trivial as names can also contain _
/// This would break if someone actually named a variable something
/// like var_00001, but then so does savilerow!
fn split_savile_row_name(n: &str) -> (String, Vec<i64>) {
    let mut current = n.to_string();
    let mut indices = Vec::new();

    loop {
        // Find the last underscore
        if let Some(pos) = current.rfind('_') {
            let (base, last_part) = current.split_at(pos);
            let value_part = &last_part[1..]; // Skip the underscore

            // Check if it starts with 'n' for negation
            let (value_str, negate) = if let Some(stripped) = value_part.strip_prefix('n') {
                (stripped, true)
            } else {
                (value_part, false)
            };

            // Check if the remainder is a number with at least 5 digits
            if value_str.len() >= 5 && value_str.chars().all(|c| c.is_digit(10)) {
                if let Ok(mut num) = value_str.parse::<i64>() {
                    if negate {
                        num = -num;
                    }
                    indices.insert(0, num);
                    current = base.to_string();
                    continue;
                }
            }
        }

        // If we can't process anymore, break the loop
        break;
    }

    (current, indices)
}

pub fn parse_savile_row_name(dimacs: &PuzzleParse, n: &str) -> anyhow::Result<Option<PuzVar>> {
    let (name, indices) = split_savile_row_name(n);

    let has_match = dimacs.eprime.vars.contains(&name)
        || dimacs.eprime.cons.contains_key(&name)
        || dimacs.eprime.reveal.contains_key(&name)
        || dimacs.eprime.reveal_values.contains(&name);

    if !has_match {
        if !dimacs.eprime.auxvars.contains(&name) && !n.starts_with("conjure_aux") {
            eprintln!("Do not recognise variable '{}' -- should it be AUX?", name);
        }
        return Ok(None);
    }

    return Ok(Some(PuzVar::new(&name, indices)));
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
    fn test_parse_savile_row_name() -> anyhow::Result<()> {
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
        let n1 = "var1_00001_00002_00003";
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
        assert_eq!(
            parse_savile_row_name(&dp, ne)?,
            Some(PuzVar::new("var3x", vec![]))
        );

        // Test case 2: n starts with a variable in aux_variables
        let n2 = "aux2_4_5_6";
        assert_eq!(parse_savile_row_name(&dp, n2).unwrap(), None);

        // Test case 3: n does not start with any variable
        let n3 = "not_found_7_8_9";
        assert_eq!(parse_savile_row_name(&dp, n3)?, None);

        // Test case 4: n starts with multiple variables
        let n4 = "var1_var2_10_11_12";
        assert_eq!(parse_savile_row_name(&dp, n4)?, None);

        // Test case 5: n starts with a variable, but the remaining part is empty
        let n5 = "var1_";
        assert_eq!(parse_savile_row_name(&dp, n5).unwrap(), None);

        Ok(())
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

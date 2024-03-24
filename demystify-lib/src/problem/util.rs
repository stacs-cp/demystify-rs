use anyhow::bail;
use std::collections::BTreeSet;

pub fn parse_savile_row_name(
    vars: &BTreeSet<String>,
    auxvars: &BTreeSet<String>,
    n: &str,
) -> anyhow::Result<Option<(String, Vec<i32>)>> {
    let varmatch: Vec<&String> = vars.iter().filter(|&v| n.starts_with(v)).collect();
    if varmatch.is_empty() {
        if !auxvars.iter().any(|v| n.starts_with(v)) {
            bail!(
                "Cannot find {} in the VAR list {:?} -- should it be AUX?",
                n,
                vars
            );
        }
        return Ok(None);
    }
    if varmatch.len() > 1 {
        bail!(
            "Variables cannot have a common prefix: Can't tell if {} is {:?}",
            n,
            varmatch
        );
    }

    let varmatch = varmatch[0].clone();

    // The variable has no indices, so we are done
    if varmatch == n {
        return Ok(Some((varmatch, vec![])));
    }

    let n = &n[varmatch.len() + 1..];

    let splits: Vec<&str> = n.split("_").collect();
    let mut args = Vec::new();
    for arg in splits {
        if !arg.is_empty() {
            let c = if arg.starts_with("n") {
                -1 * arg[1..].parse::<i32>()?
            } else {
                arg.parse::<i32>()?
            };
            args.push(c);
        }
    }
    Ok(Some((varmatch, args)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_savile_row_name() {
        let variables: BTreeSet<String> = ["var1", "var2", "var3", "var3x"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let aux_variables: BTreeSet<String> = ["aux1", "aux2", "aux3"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Test case 1: n starts with a variable in variables
        let n1 = "var1_1_2_3";
        let expected1 = Some(("var1".to_string(), vec![1, 2, 3]));
        assert_eq!(
            parse_savile_row_name(&variables, &aux_variables, n1).unwrap(),
            expected1
        );

        let n1b = "var1_00001_00002_00010";
        let expected1b = Some(("var1".to_string(), vec![1, 2, 10]));
        assert_eq!(
            parse_savile_row_name(&variables, &aux_variables, n1b).unwrap(),
            expected1b
        );

        let n1c = "var1_n00001_00002_n00010";
        let expected1c = Some(("var1".to_string(), vec![-1, 2, -10]));
        assert_eq!(
            parse_savile_row_name(&variables, &aux_variables, n1c).unwrap(),
            expected1c
        );

        let n1d = "var1";
        let expected1d = Some(("var1".to_string(), vec![]));
        assert_eq!(
            parse_savile_row_name(&variables, &aux_variables, n1d).unwrap(),
            expected1d
        );

        let ne = "var3x";
        assert!(parse_savile_row_name(&variables, &aux_variables, ne).is_err());

        // Test case 2: n starts with a variable in aux_variables
        let n2 = "aux2_4_5_6";
        assert_eq!(
            parse_savile_row_name(&variables, &aux_variables, n2).unwrap(),
            None
        );

        // Test case 3: n does not start with any variable
        let n3 = "not_found_7_8_9";
        assert!(parse_savile_row_name(&variables, &aux_variables, n3).is_err());

        // Test case 4: n starts with multiple variables
        let n4 = "var1_var2_10_11_12";
        assert!(parse_savile_row_name(&variables, &aux_variables, n4).is_err());

        // Test case 5: n starts with a variable, but the remaining part is empty
        let n5 = "var1_";
        let expected5 = Some(("var1".to_string(), vec![]));
        assert_eq!(
            parse_savile_row_name(&variables, &aux_variables, n5).unwrap(),
            expected5
        );
    }
}

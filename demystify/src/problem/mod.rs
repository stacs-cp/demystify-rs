pub mod musdict;
/// Module containing problem-related functionality.
pub mod parse;
pub mod planner;
pub mod solver;
pub mod util;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use itertools::Itertools;
use serde::{Deserialize, Serialize};

/// Represents a puzzle variable.
#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PuzVar {
    name: String,
    indices: Vec<i64>,
}

impl PuzVar {
    /// Creates a new `PuzVar` instance.
    #[must_use]
    pub fn new(name: &str, indices: Vec<i64>) -> PuzVar {
        PuzVar {
            name: name.to_string(),
            indices,
        }
    }

    /// Returns the name of the variable.
    #[must_use]
    pub fn name(&self) -> &String {
        &self.name
    }

    /// Returns the indices of the variable.
    #[must_use]
    pub fn indices(&self) -> &Vec<i64> {
        &self.indices
    }

    /// Converts the name of the variable into a CSS-friendly string.
    #[must_use]
    pub fn to_css_string(&self) -> String {
        self.name.replace('.', "_").replace('-', "_")
            + &self
                .indices
                .iter()
                .map(|index| format!("_{index}"))
                .collect::<String>()
    }
}

impl fmt::Display for PuzVar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{:?}", self.name, self.indices)
    }
}

/// Represents a puzzle literal.
#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct VarValPair {
    var: PuzVar,
    val: i64,
}

impl fmt::Display for VarValPair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({},{})", self.var, self.val)
    }
}

impl VarValPair {
    /// Creates a new `VarValPair` instance.
    #[must_use]
    pub fn new(var: &PuzVar, val: i64) -> VarValPair {
        VarValPair {
            var: var.clone(),
            val,
        }
    }

    /// Returns the variable associated with the `VarValPair`.
    #[must_use]
    pub fn var(&self) -> &PuzVar {
        &self.var
    }

    /// Returns the value associated with the `VarValPair`.
    #[must_use]
    pub fn val(&self) -> i64 {
        self.val
    }

    /// Checks if the `VarValPair` is equal to a given `PuzLit`.
    #[must_use]
    pub fn is_lit(&self, puzlit: &PuzLit) -> bool {
        *self == puzlit.varval()
    }

    /// Converts the `VarValPair` into a CSS-friendly string.
    #[must_use]
    pub fn to_css_string(&self) -> String {
        format!("lit_{}__{}", self.var.to_css_string(), self.val)
    }
}

/// Represents a puzzle literal, which is the positive or negative form of a `VarValPair`.
#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PuzLit {
    varval: VarValPair,
    equal: bool,
}

impl fmt::Display for PuzLit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.equal {
            write!(f, "{}={}", self.varval.var(), self.varval.val())
        } else {
            write!(f, "{}!={}", self.varval.var(), self.varval.val())
        }
    }
}

impl PuzLit {
    /// Creates a new `PuzLit` instance representing an equality constraint.
    #[must_use]
    pub fn new_eq(varval: VarValPair) -> PuzLit {
        PuzLit {
            varval,
            equal: true,
        }
    }

    /// Creates a new `PuzLit` instance representing an inequality constraint.
    #[must_use]
    pub fn new_neq(varval: VarValPair) -> PuzLit {
        PuzLit {
            varval,
            equal: false,
        }
    }

    /// Returns the `VarValPair` associated with the `PuzLit`.
    #[must_use]
    pub fn varval(&self) -> VarValPair {
        self.varval.clone()
    }

    /// Checks if the `PuzLit` is equal to a given `VarValPair`.
    #[must_use]
    pub fn is_varval(&self, varval: &VarValPair) -> bool {
        self.varval == *varval
    }

    /// Returns the variable associated with the `PuzLit`.
    #[must_use]
    pub fn var(&self) -> PuzVar {
        self.varval.var().clone()
    }

    /// Returns the value associated with the `PuzLit`.
    #[must_use]
    pub fn val(&self) -> i64 {
        self.varval.val()
    }

    /// Returns the sign of the `PuzLit`.
    #[must_use]
    pub fn sign(&self) -> bool {
        self.equal
    }

    /// Returns the negation of the `PuzLit`.
    #[must_use]
    pub fn neg(&self) -> PuzLit {
        PuzLit {
            varval: self.varval.clone(),
            equal: !self.equal,
        }
    }

    pub fn nice_puzlit_list_html<'a, I>(puz_container: I) -> String
    where
        I: IntoIterator<Item = &'a PuzLit>,
    {
        // Group literals by variable
        let mut var_literals: BTreeMap<PuzVar, BTreeMap<i64, bool>> = BTreeMap::new();

        for lit in puz_container {
            let var = lit.var();
            let val = lit.val();
            let equal = lit.sign();

            var_literals.entry(var).or_default().insert(val, equal);
        }

        // Generate formatted strings for each variable
        let mut result_strings = Vec::new();

        for (var, val_map) in var_literals {
            // Check if there are any positive literals
            if val_map.values().any(|&equal| equal) {
                // Get all the positive values
                let positives: Vec<i64> = val_map
                    .iter()
                    .filter_map(|(&val, &equal)| if equal { Some(val) } else { None })
                    .collect();

                // Format positive literals
                for val in positives {
                    let css = "highlight_".to_owned() + &VarValPair::new(&var, val).to_css_string();

                    result_strings.push(format!(r##"<div style="display:inline" class="{css} js_highlighter">{var} = {val}</div>"##));
                }
            } else {
                // All literals are negative
                let negatives: BTreeSet<i64> = val_map
                    .iter()
                    .filter_map(|(&val, &equal)| if equal { None } else { Some(val) })
                    .collect();

                if !negatives.is_empty() {
                    let neg_values = negatives
                        .iter()
                        .map(|&val| val.to_string())
                        .collect::<Vec<_>>()
                        .join(" or ");

                    let neg_classes = negatives
                        .iter()
                        .map(|&val| {
                            "highlight_".to_owned() + &VarValPair::new(&var, val).to_css_string()
                        })
                        .collect_vec()
                        .join(" ");

                    result_strings.push(format!(r##"<div style="display:inline" class="{neg_classes} js_highlighter">{var} != {neg_values}</div>"##));
                }
            }
        }

        result_strings.join(", ")
    }
}

/// Represents a constraint identifier.
#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ConID {
    pub lit: PuzLit,
    pub name: String,
}

impl ConID {
    /// Creates a new `ConID` instance.
    fn new(lit: PuzLit, name: String) -> ConID {
        ConID { lit, name }
    }
}

#[cfg(test)]
mod tests {
    use crate::problem::VarValPair;

    use super::{PuzLit, PuzVar};
    use std::sync::Arc;

    #[test]
    fn var() {
        let v = PuzVar::new("v", vec![]);
        let v2 = PuzVar::new("v", vec![2]);
        let w = PuzVar::new("w", vec![]);
        assert_eq!(v, v);
        assert!(v != w);
        assert!(v2 != w);
        assert!(v != v2);
    }

    #[test]
    fn varval() {
        let v = Arc::new(PuzVar::new("v", vec![]));
        let w = Arc::new(PuzVar::new("w", vec![]));
        let l = VarValPair::new(&v, 2);
        let nl = VarValPair::new(&v, 3);
        let lw = VarValPair::new(&w, 2);
        assert!(l != nl);
        assert!(l != lw);
        assert!(nl != lw);
        assert_eq!(l, l);
    }

    #[test]
    fn lit() {
        let v = Arc::new(PuzVar::new("v", vec![]));
        let w = Arc::new(PuzVar::new("w", vec![]));
        let l = PuzLit::new_eq(VarValPair::new(&v, 2));
        let nl = PuzLit::new_neq(VarValPair::new(&v, 2));
        let lw = PuzLit::new_eq(VarValPair::new(&w, 2));
        assert_eq!(l, l);
        assert_eq!(l, l.neg().neg());
        assert_eq!(l, nl.neg());
        assert_eq!(l.neg(), nl);
        assert!(l != lw);
    }

    #[test]
    fn varval_lit() {
        let v = Arc::new(PuzVar::new("v", vec![]));
        let w = Arc::new(PuzVar::new("w", vec![]));
        let l = PuzLit::new_eq(VarValPair::new(&v, 2));
        let nl = PuzLit::new_neq(VarValPair::new(&v, 2));
        let lw = PuzLit::new_eq(VarValPair::new(&w, 2));

        let vvl = VarValPair::new(&v, 2);
        let vvl3 = VarValPair::new(&v, 3);
        let vvlw = VarValPair::new(&w, 2);

        assert!(l.is_varval(&vvl));
        assert!(nl.is_varval(&vvl));
        assert!(!l.is_varval(&vvl3));
        assert!(!nl.is_varval(&vvl3));
        assert!(lw.is_varval(&vvlw));
        assert!(!lw.is_varval(&vvl));

        assert!(vvl.is_lit(&l));
        assert!(vvl.is_lit(&nl));
        assert!(!vvl3.is_lit(&l));
        assert!(!vvl3.is_lit(&nl));
        assert!(!vvl.is_lit(&lw));
        assert!(!vvlw.is_lit(&l));
        assert!(vvlw.is_lit(&lw));
    }

    #[test]
    fn test_puzvar_to_css_string() {
        let v = PuzVar::new("v.name", vec![]);
        assert_eq!(v.to_css_string(), "v_name");

        let v_with_indices = PuzVar::new("v-name", vec![1, 2, 3]);
        assert_eq!(v_with_indices.to_css_string(), "v_name_1_2_3");

        let v_complex = PuzVar::new("v.name-test", vec![42]);
        assert_eq!(v_complex.to_css_string(), "v_name_test_42");
    }

    #[test]
    fn test_varvalpair_to_css_string() {
        let v = PuzVar::new("v.name", vec![1, 2]);
        let pair = VarValPair::new(&v, 42);
        assert_eq!(pair.to_css_string(), "lit_v_name_1_2__42");

        let w = PuzVar::new("w-name", vec![]);
        let pair_no_indices = VarValPair::new(&w, 7);
        assert_eq!(pair_no_indices.to_css_string(), "lit_w_name__7");
    }

    #[test]
    fn test_nice_puzlit_list_html() {
        let v = PuzVar::new("v", vec![]);
        let w = PuzVar::new("w", vec![]);
        let x = PuzVar::new("x", vec![1, 2]);

        // Test case 1: Single positive literal
        let lit1 = PuzLit::new_eq(VarValPair::new(&v, 2));
        assert!(PuzLit::nice_puzlit_list_html(&[lit1.clone()]).contains("v[] = 2"));

        // Test case 2: Multiple positive literals for different variables
        let lit2 = PuzLit::new_eq(VarValPair::new(&w, 3));
        let lit3 = PuzLit::new_eq(VarValPair::new(&x, 5));
        assert!(PuzLit::nice_puzlit_list_html(&[lit1, lit2, lit3]).contains("x[1, 2] = 5"));

        // Test case 3: Single negative literal
        let neq1 = PuzLit::new_neq(VarValPair::new(&v, 2));
        assert!(PuzLit::nice_puzlit_list_html(&[neq1.clone()]).contains("v[] != 2"));

        // Test case 4: Multiple negative literals for same variable
        let neq2 = PuzLit::new_neq(VarValPair::new(&v, 3));
        let neq3 = PuzLit::new_neq(VarValPair::new(&v, 4));
        assert!(PuzLit::nice_puzlit_list_html(&[neq1, neq2, neq3]).contains("v[] != 2 or 3 or 4"));

        // Test case 5: Mix of positive and negative literals
        let mix1 = PuzLit::new_eq(VarValPair::new(&v, 5));
        let mix2 = PuzLit::new_neq(VarValPair::new(&w, 1));
        let mix3 = PuzLit::new_neq(VarValPair::new(&w, 2));
        let mix4 = PuzLit::new_eq(VarValPair::new(&x, 7));
        assert!(
            ["v[] = 5", "w[] != 1 or 2", "x[1, 2] = 7"]
                .iter()
                .all(|s| PuzLit::nice_puzlit_list_html([&mix1, &mix2, &mix3, &mix4]).contains(s))
        );

        // Test case 6: Empty list
        assert_eq!(PuzLit::nice_puzlit_list_html(&[]), "");
    }
}

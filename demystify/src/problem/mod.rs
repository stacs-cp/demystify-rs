pub mod musdict;
/// Module containing problem-related functionality.
pub mod parse;
pub mod planner;
pub mod serialize;
pub mod solver;
pub mod solvetree;
pub mod util;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
        self.name.replace(['.', '-'], "_")
            + &self
                .indices
                .iter()
                .map(|index| format!("_{index}"))
                .collect::<String>()
    }

    /// Returns a new `PuzVar` with the given prefix added to the name.
    #[must_use]
    pub fn with_prefix(&self, prefix: &str) -> PuzVar {
        PuzVar {
            name: format!("{}{}", prefix, self.name),
            indices: self.indices.clone(),
        }
    }

    fn insert_assignment_to_json_map(json_obj: &mut serde_json::Value, puzvar: &PuzVar, val: i64) {
        let name = puzvar.name();
        let indices = puzvar.indices();

        // Start at the top-level object
        let obj = json_obj
            .as_object_mut()
            .expect("Expected a JSON object at the root");

        // Get or insert the variable name as an object
        let mut current = obj.entry(name).or_insert_with(|| json!({}));

        // Traverse or create nested objects for each index except the last
        for idx in &indices[..indices.len().saturating_sub(1)] {
            let idx_str = idx.to_string();
            if current.get(&idx_str).is_none() {
                current
                    .as_object_mut()
                    .expect("Expected object")
                    .insert(idx_str.clone(), json!({}));
            }
            current = current.get_mut(&idx_str).expect("Index missing");
        }

        // Insert the value at the last index, or directly if no indices
        if let Some(last_idx) = indices.last() {
            let last_idx_str = last_idx.to_string();
            let map = current.as_object_mut().expect("Expected object");
            if map.contains_key(&last_idx_str) {
                panic!("Assignment already exists for {:?}", puzvar);
            }
            map.insert(last_idx_str, Value::from(val));
        } else {
            // No indices: assign directly to the variable name
            if !current.is_null() && !current.as_object().is_some_and(|o| o.is_empty()) {
                panic!("Assignment already exists for {:?}", puzvar);
            }
            *current = Value::from(val);
        }
    }

    pub fn to_json_map<M>(assignments: &M) -> serde_json::Value
    where
        for<'a> &'a M: IntoIterator<Item = (&'a PuzVar, &'a i64)>,
    {
        let mut json_obj = serde_json::json!({});

        for (puzvar, val) in assignments {
            Self::insert_assignment_to_json_map(&mut json_obj, puzvar, *val);
        }

        json_obj
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

/// Format a [`PuzLit`] for the FFI surfaces (`demystify-lua`,
/// `demystify-wasm`).  Differs from [`PuzLit`]'s `Display` impl in
/// one place: scalar variables (no indices) are rendered as `"x=5"`
/// rather than `"x[]=5"`.  The HTML-output Display path retains the
/// trailing `[]` for backwards compatibility with snapshot tests.
#[must_use]
pub fn format_puzlit(lit: &PuzLit) -> String {
    let var = lit.var();
    let val = lit.val();
    let sign = if lit.sign() { "=" } else { "!=" };
    if var.indices().is_empty() {
        format!("{}{}{}", var.name(), sign, val)
    } else {
        format!("{}{:?}{}{}", var.name(), var.indices(), sign, val)
    }
}

/// Format a [`PuzVar`] for the FFI surfaces.  Scalar variables (no
/// indices) render as just the name; indexed ones as
/// `"grid[1, 2]"` — the inverse of [`parse_var_string`].
#[must_use]
pub fn format_puzvar(var: &PuzVar) -> String {
    if var.indices().is_empty() {
        var.name().clone()
    } else {
        format!(
            "{}[{}]",
            var.name(),
            var.indices()
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// Parse a string emitted by [`format_puzvar`] back into a [`PuzVar`].
/// Returns `None` when the input doesn't match `name` or `name[i, j, ...]`,
/// or when any index isn't a valid `i64`.
#[must_use]
pub fn parse_var_string(s: &str) -> Option<PuzVar> {
    let s = s.trim();

    if let Some(bracket_pos) = s.find('[') {
        let name = s[..bracket_pos].trim();
        if name.is_empty() {
            return None;
        }
        let close_bracket = s.rfind(']')?;
        if close_bracket <= bracket_pos {
            return None;
        }
        let indices_str = &s[bracket_pos + 1..close_bracket];
        let mut indices = Vec::new();
        for part in indices_str.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let idx: i64 = part.parse().ok()?;
            indices.push(idx);
        }
        Some(PuzVar::new(name, indices))
    } else {
        if s.is_empty() || !s.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return None;
        }
        Some(PuzVar::new(s, vec![]))
    }
}

#[cfg(test)]
mod tests {
    use crate::problem::VarValPair;

    use super::{PuzLit, PuzVar};
    use serde_json::json;
    use std::{collections::BTreeMap, sync::Arc};

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
        assert!(PuzLit::nice_puzlit_list_html(std::slice::from_ref(&lit1)).contains("v[] = 2"));

        // Test case 2: Multiple positive literals for different variables
        let lit2 = PuzLit::new_eq(VarValPair::new(&w, 3));
        let lit3 = PuzLit::new_eq(VarValPair::new(&x, 5));
        assert!(PuzLit::nice_puzlit_list_html(&[lit1, lit2, lit3]).contains("x[1, 2] = 5"));

        // Test case 3: Single negative literal
        let neq1 = PuzLit::new_neq(VarValPair::new(&v, 2));
        assert!(PuzLit::nice_puzlit_list_html(std::slice::from_ref(&neq1)).contains("v[] != 2"));

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

    #[test]
    fn test_with_prefix() {
        let v = PuzVar::new("foo", vec![1, 2]);
        let prefixed = v.with_prefix("bar_");
        assert_eq!(prefixed.name(), &"bar_foo".to_string());
        assert_eq!(prefixed.indices(), &vec![1, 2]);
        // Ensure original is unchanged
        assert_eq!(v.name(), &"foo".to_string());
        assert_eq!(v.indices(), &vec![1, 2]);
    }
    #[test]
    fn test_insert_assignment_no_indices() {
        let puzvar = PuzVar::new("foo", vec![]);
        let mut json_obj = json!({});

        PuzVar::insert_assignment_to_json_map(&mut json_obj, &puzvar, 42);

        assert_eq!(json_obj, json!({"foo": 42}));
    }

    #[test]
    fn test_insert_assignment_single_index() {
        let puzvar = PuzVar::new("bar", vec![1]);
        let mut json_obj = json!({});

        PuzVar::insert_assignment_to_json_map(&mut json_obj, &puzvar, 99);

        assert_eq!(json_obj, json!({"bar": {"1": 99}}));
    }

    #[test]
    fn test_insert_assignment_multiple_indices() {
        let puzvar = PuzVar::new("baz", vec![1, 2, 3]);
        let mut json_obj = json!({});

        PuzVar::insert_assignment_to_json_map(&mut json_obj, &puzvar, 55);

        assert_eq!(json_obj, json!({"baz": {"1": {"2": {"3": 55}}}}));
    }

    #[test]
    fn test_insert_multiple_assignments() {
        let var1 = PuzVar::new("var1", vec![]);
        let var2 = PuzVar::new("var2", vec![5]);
        let var3 = PuzVar::new("var3", vec![1, 2]);
        let mut json_obj = json!({});

        PuzVar::insert_assignment_to_json_map(&mut json_obj, &var1, 10);
        PuzVar::insert_assignment_to_json_map(&mut json_obj, &var2, 20);
        PuzVar::insert_assignment_to_json_map(&mut json_obj, &var3, 30);

        assert_eq!(
            json_obj,
            json!({
                "var1": 10,
                "var2": {"5": 20},
                "var3": {"1": {"2": 30}}
            })
        );
    }

    #[test]
    fn test_insert_same_variable_different_indices() {
        let grid1 = PuzVar::new("grid", vec![1, 1]);
        let grid2 = PuzVar::new("grid", vec![1, 2]);
        let grid3 = PuzVar::new("grid", vec![2, 1]);
        let mut json_obj = json!({});

        PuzVar::insert_assignment_to_json_map(&mut json_obj, &grid1, 1);
        PuzVar::insert_assignment_to_json_map(&mut json_obj, &grid2, 2);
        PuzVar::insert_assignment_to_json_map(&mut json_obj, &grid3, 3);

        assert_eq!(
            json_obj,
            json!({
                "grid": {
                    "1": {
                        "1": 1,
                        "2": 2
                    },
                    "2": {
                        "1": 3
                    }
                }
            })
        );
    }

    #[test]
    fn test_insert_into_existing_json() {
        let puzvar = PuzVar::new("new_var", vec![10]);
        let mut json_obj = json!({
            "existing_var": 42,
            "another_var": {"5": 99}
        });

        PuzVar::insert_assignment_to_json_map(&mut json_obj, &puzvar, 77);

        assert_eq!(
            json_obj,
            json!({
                "existing_var": 42,
                "another_var": {"5": 99},
                "new_var": {"10": 77}
            })
        );
    }

    #[test]
    #[should_panic(expected = "Assignment already exists")]
    fn test_insert_duplicate_no_indices() {
        let puzvar = PuzVar::new("foo", vec![]);
        let mut json_obj = json!({});

        PuzVar::insert_assignment_to_json_map(&mut json_obj, &puzvar, 1);
        // This should panic because we're inserting to the same variable again
        PuzVar::insert_assignment_to_json_map(&mut json_obj, &puzvar, 2);
    }

    #[test]
    #[should_panic(expected = "Assignment already exists")]
    fn test_insert_duplicate_with_indices() {
        let puzvar = PuzVar::new("grid", vec![1, 2]);
        let mut json_obj = json!({});

        PuzVar::insert_assignment_to_json_map(&mut json_obj, &puzvar, 5);
        // This should panic because we're inserting to the same variable with the same indices
        PuzVar::insert_assignment_to_json_map(&mut json_obj, &puzvar, 10);
    }

    #[test]
    #[should_panic(expected = "Assignment already exists")]
    fn test_insert_duplicate_with_different_depths() {
        let puzvar = PuzVar::new("grid", vec![1, 2]);
        let puzvar2 = PuzVar::new("grid", vec![1]);

        let mut json_obj = json!({});

        PuzVar::insert_assignment_to_json_map(&mut json_obj, &puzvar, 2);
        // This should panic because we're inserting to the same variable with different index depth
        PuzVar::insert_assignment_to_json_map(&mut json_obj, &puzvar2, 10);
    }

    #[test]
    fn test_to_json_map_empty() {
        let assignments: BTreeMap<PuzVar, i64> = BTreeMap::new();
        let result = PuzVar::to_json_map(&assignments);
        assert_eq!(result, json!({}));
    }

    #[test]
    fn test_to_json_map_multiple_vars() {
        let mut assignments: BTreeMap<PuzVar, i64> = BTreeMap::new();

        assignments.insert(PuzVar::new("x", vec![]), 42);
        assignments.insert(PuzVar::new("y", vec![1]), 10);
        assignments.insert(PuzVar::new("z", vec![1, 2]), 99);
        assignments.insert(PuzVar::new("grid", vec![2, 3]), 5);
        assignments.insert(PuzVar::new("grid", vec![2, 4]), 7);

        let result = PuzVar::to_json_map(&assignments);

        assert_eq!(
            result,
            json!({
                "x": 42,
                "y": {"1": 10},
                "z": {"1": {"2": 99}},
                "grid": {
                    "2": {
                        "3": 5,
                        "4": 7
                    }
                }
            })
        );
    }
}

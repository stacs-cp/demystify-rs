/// Module containing problem-related functionality.
pub mod parse;
pub mod planner;
pub mod solver;
pub mod util;

use std::fmt;

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

    #[must_use]
    pub fn indices(&self) -> &Vec<i64> {
        &self.indices
    }
}

impl fmt::Display for PuzVar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{:?}", self.name, self.indices)
    }
}

/// Represents a puzzle literal.
#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PuzLit {
    var: PuzVar,
    val: i64,
    equal: bool,
}

impl fmt::Display for PuzLit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.equal {
            write!(f, "{}={}", self.var, self.val)
        } else {
            write!(f, "{}!={}", self.var, self.val)
        }
    }
}

impl PuzLit {
    /// Creates a new `PuzLit` instance representing an equality constraint.
    #[must_use]
    pub fn new_eq_val(var: &PuzVar, val: i64) -> PuzLit {
        PuzLit {
            var: var.clone(),
            val,
            equal: true,
        }
    }

    /// Creates a new `PuzLit` instance representing an inequality constraint.
    #[must_use]
    pub fn new_neq_val(var: &PuzVar, val: i64) -> PuzLit {
        PuzLit {
            var: var.clone(),
            val,
            equal: false,
        }
    }

    /// Returns the variable associated with the literal.
    #[must_use]
    pub fn var(&self) -> PuzVar {
        self.var.clone()
    }

    /// Returns the value associated with the literal.
    #[must_use]
    pub fn val(&self) -> i64 {
        self.val
    }

    /// Returns the sign of the literal.
    #[must_use]
    pub fn sign(&self) -> bool {
        self.equal
    }

    /// Returns the negation of the literal.
    #[must_use]
    pub fn neg(&self) -> PuzLit {
        PuzLit {
            var: self.var.clone(),
            val: self.val,
            equal: !self.equal,
        }
    }

    /// Returns the 'equal' form of a literal
    pub fn normalise(&self) -> PuzLit {
        PuzLit {
            var: self.var.clone(),
            val: self.val,
            equal: true,
        }
    }

    pub fn equal_mod_sign(&self, p: &PuzLit) -> bool {
        self.var == p.var && self.val == p.val
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
    fn lit() {
        let v = Arc::new(PuzVar::new("v", vec![]));
        let w = Arc::new(PuzVar::new("w", vec![]));
        let l = PuzLit::new_eq_val(&v, 2);
        let nl = PuzLit::new_neq_val(&v, 2);
        let lw = PuzLit::new_eq_val(&w, 2);
        assert_eq!(l, l);
        assert_eq!(l, l.neg().neg());
        assert_eq!(l, nl.neg());
        assert_eq!(l.neg(), nl);
        assert!(l != lw);
        assert!(l.equal_mod_sign(&nl));
        assert!(nl.equal_mod_sign(&l));
        assert!(l.equal_mod_sign(&l));
        assert!(!l.equal_mod_sign(&lw));
        assert!(lw.equal_mod_sign(&lw));
        assert!(lw.equal_mod_sign(&lw.neg()));
    }
}

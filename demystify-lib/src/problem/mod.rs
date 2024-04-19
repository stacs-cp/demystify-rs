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
    #[must_use]
    pub fn new(var: &PuzVar, val: i64) -> VarValPair {
        VarValPair {
            var: var.clone(),
            val,
        }
    }

    pub fn var(&self) -> &PuzVar {
        &self.var
    }

    pub fn val(&self) -> i64 {
        self.val
    }

    pub fn is_lit(&self, puzlit: &PuzLit) -> bool {
        *self == puzlit.varval()
    }
}

/// Represents a puzzle literal.
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

    /// Returns the variable associated with the literal.
    #[must_use]
    pub fn varval(&self) -> VarValPair {
        self.varval.clone()
    }

    pub fn is_varval(&self, varval: &VarValPair) -> bool {
        self.varval == *varval
    }

    /// Returns the variable associated with the literal.
    #[must_use]
    pub fn var(&self) -> PuzVar {
        self.varval.var().clone()
    }

    /// Returns the value associated with the literal.
    #[must_use]
    pub fn val(&self) -> i64 {
        self.varval.val()
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
            varval: self.varval.clone(),
            equal: !self.equal,
        }
    }

    pub fn equal_mod_sign(&self, p: &PuzLit) -> bool {
        self.varval == p.varval
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
        assert!(l.equal_mod_sign(&nl));
        assert!(nl.equal_mod_sign(&l));
        assert!(l.equal_mod_sign(&l));
        assert!(!l.equal_mod_sign(&lw));
        assert!(lw.equal_mod_sign(&lw));
        assert!(lw.equal_mod_sign(&lw.neg()));
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
}

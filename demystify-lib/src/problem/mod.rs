pub mod parse;
pub mod planner;
pub mod solver;
pub mod util;

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PuzVar {
    name: String,
    indices: Vec<i64>,
}

impl PuzVar {
    #[must_use]
    pub fn new(name: &str, indices: Vec<i64>) -> PuzVar {
        PuzVar {
            name: name.to_string(),
            indices,
        }
    }

    #[must_use]
    pub fn name(&self) -> &String {
        &self.name
    }
}

impl fmt::Display for PuzVar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{:?}", self.name, self.indices)
    }
}

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
    #[must_use]
    pub fn new_eq_val(var: &PuzVar, val: i64) -> PuzLit {
        PuzLit {
            var: var.clone(),
            val,
            equal: true,
        }
    }

    #[must_use]
    pub fn new_neq_val(var: &PuzVar, val: i64) -> PuzLit {
        PuzLit {
            var: var.clone(),
            val,
            equal: false,
        }
    }

    #[must_use]
    pub fn var(&self) -> PuzVar {
        self.var.clone()
    }

    #[must_use]
    pub fn val(&self) -> i64 {
        self.val
    }

    #[must_use]
    pub fn sign(&self) -> bool {
        self.equal
    }

    #[must_use]
    pub fn neg(&self) -> PuzLit {
        PuzLit {
            var: self.var.clone(),
            val: self.val,
            equal: !self.equal,
        }
    }
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ConID {
    pub lit: PuzLit,
    pub name: String,
}

impl ConID {
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
    }
}

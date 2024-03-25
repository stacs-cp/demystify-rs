pub mod parse;
pub mod util;

use serde::{Deserialize, Serialize};

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PuzVar {
    name: String,
    indices: Vec<i64>,
}

impl PuzVar {
    pub fn new(name: &str, indices: Vec<i64>) -> PuzVar {
        PuzVar {
            name: name.to_string(),
            indices,
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PuzLit {
    var: PuzVar,
    val: i64,
    equal: bool,
}

impl PuzLit {
    pub fn new_eq_val(var: &PuzVar, val: i64) -> PuzLit {
        PuzLit {
            var: var.clone(),
            val,
            equal: true,
        }
    }

    pub fn new_neq_val(var: &PuzVar, val: i64) -> PuzLit {
        PuzLit {
            var: var.clone(),
            val,
            equal: false,
        }
    }

    pub fn var(&self) -> PuzVar {
        self.var.clone()
    }

    pub fn val(&self) -> i64 {
        self.val
    }

    pub fn sign(&self) -> bool {
        self.equal
    }

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

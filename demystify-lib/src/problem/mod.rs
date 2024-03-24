pub mod parse;
pub mod util;

use serde::{Deserialize, Serialize};

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct VarID {
    name: String,
    indices: Vec<i64>,
}

impl VarID {
    pub fn new(name: &str, indices: Vec<i64>) -> VarID {
        VarID {
            name: name.to_string(),
            indices,
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Lit {
    var: VarID,
    val: i64,
    equal: bool,
}

impl Lit {
    pub fn new_eq_val(var: &VarID, val: i64) -> Lit {
        Lit {
            var: var.clone(),
            val,
            equal: true,
        }
    }

    pub fn new_neq_val(var: &VarID, val: i64) -> Lit {
        Lit {
            var: var.clone(),
            val,
            equal: false,
        }
    }

    pub fn var(&self) -> VarID {
        self.var.clone()
    }

    pub fn val(&self) -> i64 {
        self.val
    }

    pub fn sign(&self) -> bool {
        self.equal
    }

    pub fn neg(&self) -> Lit {
        Lit {
            var: self.var.clone(),
            val: self.val,
            equal: !self.equal,
        }
    }
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ConID {
    pub lit: Lit,
    pub name: String,
}

impl ConID {
    fn new(lit: Lit, name: String) -> ConID {
        ConID { lit, name }
    }
}

#[cfg(test)]
mod tests {
    use super::{Lit, VarID};
    use std::sync::Arc;

    #[test]
    fn var() {
        let v = VarID::new("v", vec![]);
        let v2 = VarID::new("v", vec![2]);
        let w = VarID::new("w", vec![]);
        assert_eq!(v, v);
        assert!(v != w);
        assert!(v2 != w);
        assert!(v != v2);
    }

    #[test]
    fn lit() {
        let v = Arc::new(VarID::new("v", vec![]));
        let w = Arc::new(VarID::new("w", vec![]));
        let l = Lit::new_eq_val(&v, 2);
        let nl = Lit::new_neq_val(&v, 2);
        let lw = Lit::new_eq_val(&w, 2);
        assert_eq!(l, l);
        assert_eq!(l, l.neg().neg());
        assert_eq!(l, nl.neg());
        assert_eq!(l.neg(), nl);
        assert!(l != lw);
    }
}

pub mod parse;
pub mod util;

use serde::{Deserialize, Serialize};

#[derive(Clone, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct VarID {
    name: String,
    indices: Vec<i32>,
}

impl VarID {
    pub fn new(name: &str, indices: Vec<i32>) -> VarID {
        VarID {
            name: name.to_string(),
            indices,
        }
    }
}

#[derive(Clone, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Lit {
    var: VarID,
    val: i32,
    equal: bool,
}

impl Lit {
    pub fn new_eq_val(var: &VarID, val: i32) -> Lit {
        Lit {
            var: var.clone(),
            val,
            equal: true,
        }
    }

    pub fn new_neq_val(var: &VarID, val: i32) -> Lit {
        Lit {
            var: var.clone(),
            val,
            equal: false,
        }
    }

    pub fn neg(&self) -> Lit {
        Lit {
            var: self.var.clone(),
            val: self.val,
            equal: !self.equal,
        }
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

pub mod parse;
pub mod util;

use serde::{Deserialize, Serialize};

use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Var {
    name: String,
    dom: Vec<i32>,
    location: Vec<i32>,
}

impl Var {
    pub fn new(name: String, dom: Vec<i32>, location: Vec<i32>) -> Var {
        Var {
            name,
            dom,
            location,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Lit {
    var: Arc<Var>,
    val: i32,
    equal: bool,
}

impl Lit {
    pub fn new_eq_val(var: &Arc<Var>, val: i32) -> Lit {
        Lit {
            var: var.clone(),
            val,
            equal: true,
        }
    }

    pub fn new_neq_val(var: &Arc<Var>, val: i32) -> Lit {
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
    use super::{Lit, Var};
    use std::sync::Arc;

    #[test]
    fn var() {
        let v = Var::new("v".to_string(), vec![1, 2, 3], vec![1, 2]);
        let w = Var::new("w".to_string(), vec![1, 2, 3], vec![1, 2]);
        assert_eq!(v, v);
        assert!(v != w);
    }

    #[test]
    fn lit() {
        let v = Arc::new(Var::new("v".to_string(), vec![1, 2, 3], vec![1, 2]));
        let w = Arc::new(Var::new("w".to_string(), vec![1, 2, 3], vec![1, 2]));
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

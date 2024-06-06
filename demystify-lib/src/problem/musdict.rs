use std::collections::HashMap;

use rustsat::types::Lit;

/// A dictionary for storing muses (minimal unsatisfiable subsets) associated with literals.
pub struct MusDict {
    muses: HashMap<Lit, Vec<Vec<Lit>>>,
}

impl MusDict {
    /// Creates a new instance of `MusDict`.
    ///
    /// # Returns
    ///
    /// A new `MusDict` instance.
    pub fn new() -> Self {
        MusDict {
            muses: HashMap::new(),
        }
    }

    /// Adds a new mus to the dictionary.
    ///
    /// If the mus associated with the given literal already exists in the dictionary, the new mus
    /// will be added only if its length is smaller than the existing mus. If the lengths are equal,
    /// the new mus will be appended to the existing mus list.
    ///
    /// If the mus associated with the given literal does not exist in the dictionary, a new entry
    /// will be created with the given mus.
    ///
    /// # Arguments
    ///
    /// * `lit` - The literal associated with the mus.
    /// * `new_mus` - The new mus to be added.
    pub fn add_mus(&mut self, lit: Lit, new_mus: Vec<Lit>) {
        if let Some(mus_list) = self.muses.get_mut(&lit) {
            if new_mus.len() < mus_list[0].len() {
                mus_list.clear();
                mus_list.push(new_mus);
            } else if new_mus.len() == mus_list[0].len() {
                mus_list.push(new_mus);
            }
        } else {
            self.muses.insert(lit, vec![new_mus]);
        }
    }

    /// Returns a reference to the muses in the dictionary.
    pub fn muses(&self) -> &HashMap<Lit, Vec<Vec<Lit>>> {
        &self.muses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let mus_dict = MusDict::new();
        assert!(mus_dict.muses().is_empty());
    }

    #[test]
    fn test_add_mus_existing_literal_smaller_length() -> anyhow::Result<()> {
        let mut mus_dict = MusDict::new();
        let lit = Lit::from_ipasir(1)?;
        let mus1 = vec![Lit::from_ipasir(2)?, Lit::from_ipasir(3)?];
        let mus2 = vec![Lit::from_ipasir(4)?];
        mus_dict.add_mus(lit, mus1.clone());
        mus_dict.add_mus(lit, mus2.clone());
        assert_eq!(mus_dict.muses().get(&lit), Some(&vec![mus2]));
        Ok(())
    }

    #[test]
    fn test_add_mus_existing_literal_equal_length() -> anyhow::Result<()> {
        let mut mus_dict = MusDict::new();
        let lit = Lit::from_ipasir(1)?;
        let mus1 = vec![Lit::from_ipasir(2)?, Lit::from_ipasir(3)?];
        let mus2 = vec![Lit::from_ipasir(4)?, Lit::from_ipasir(5)?];
        mus_dict.add_mus(lit, mus1.clone());
        mus_dict.add_mus(lit, mus2.clone());
        assert_eq!(mus_dict.muses().get(&lit), Some(&vec![mus1, mus2]));
        Ok(())
    }

    #[test]
    fn test_add_mus_existing_literal_larger_length() -> anyhow::Result<()> {
        let mut mus_dict = MusDict::new();
        let lit = Lit::from_ipasir(1)?;
        let mus1 = vec![Lit::from_ipasir(2)?, Lit::from_ipasir(3)?];
        let mus2 = vec![Lit::from_ipasir(4)?];
        mus_dict.add_mus(lit, mus2.clone());
        mus_dict.add_mus(lit, mus1.clone());
        assert_eq!(mus_dict.muses().get(&lit), Some(&vec![mus2]));
        Ok(())
    }

    #[test]
    fn test_add_mus_new_literal() -> anyhow::Result<()> {
        let mut mus_dict = MusDict::new();
        let lit1 = Lit::from_ipasir(1)?;
        let lit2 = Lit::from_ipasir(2)?;
        let mus1 = vec![Lit::from_ipasir(3)?, Lit::from_ipasir(4)?];
        let mus2 = vec![Lit::from_ipasir(5)?, Lit::from_ipasir(6)?];
        mus_dict.add_mus(lit1, mus1.clone());
        mus_dict.add_mus(lit2, mus2.clone());
        assert_eq!(mus_dict.muses().get(&lit1), Some(&vec![mus1]));
        assert_eq!(mus_dict.muses().get(&lit2), Some(&vec![mus2]));
        Ok(())
    }
}

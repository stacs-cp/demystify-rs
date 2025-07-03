use std::collections::{BTreeSet, HashMap};

use rustsat::types::Lit;

/// A dictionary for storing muses (minimal unsatisfiable subsets) associated with literals.
#[derive(Clone)]
pub struct MusDict {
    muses: HashMap<Lit, BTreeSet<MusContext>>,
}

impl Default for MusDict {
    fn default() -> Self {
        Self::new()
    }
}

impl MusDict {
    /// Creates a new instance of `MusDict`.
    ///
    /// # Returns
    ///
    /// A new `MusDict` instance.
    #[must_use]
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
    pub fn add_mus(&mut self, lit: Lit, new_mus: BTreeSet<Lit>) {
        if let Some(mus_list) = self.muses.get_mut(&lit) {
            let len = if let Some(element) = mus_list.iter().next() {
                element.mus_len()
            } else {
                usize::MAX
            };

            if new_mus.len() < len {
                mus_list.clear();
                mus_list.insert(MusContext::new(lit, new_mus));
            } else if new_mus.len() == len {
                mus_list.insert(MusContext::new(lit, new_mus));
            }
        } else {
            let hs: BTreeSet<_> = std::iter::once(MusContext::new(lit, new_mus)).collect();
            self.muses.insert(lit, hs);
        }
    }

    #[must_use]
    pub fn min_lit(&self, lit: Lit) -> Option<usize> {
        if let Some(mus_list) = self.muses.get(&lit) {
            mus_list.iter().next().map(|mc| mc.mus_len())
        } else {
            None
        }
    }

    /// Returns a reference to the muses in the dictionary.
    #[must_use]
    pub fn muses(&self) -> &HashMap<Lit, BTreeSet<MusContext>> {
        &self.muses
    }

    /// Checks if the `MusDict` is empty.
    ///
    /// Returns true if the `MusDict` is empty, false otherwise.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.muses.is_empty()
    }

    #[must_use]
    pub fn min(&self) -> Option<usize> {
        self.muses
            .values()
            .flat_map(|sets| sets.iter().map(|x| x.mus_len()))
            .min()
    }
}

#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct MusContext {
    pub lits: BTreeSet<Lit>,
    pub mus: BTreeSet<Lit>,
}

impl MusContext {
    pub fn new(l: Lit, mus: BTreeSet<Lit>) -> Self {
        Self {
            lits: BTreeSet::from([l]),
            mus,
        }
    }

    pub fn new_multi_lit(lits: BTreeSet<Lit>, mus: BTreeSet<Lit>) -> Self {
        Self { lits, mus }
    }

    pub fn new_with_more_lits(mut lits: BTreeSet<Lit>, mc: &Self) -> Self {
        for l in &mc.lits {
            lits.insert(*l);
        }

        Self {
            lits,
            mus: mc.mus.clone(),
        }
    }

    pub fn mus_len(&self) -> usize {
        self.mus.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_new() {
        let mus_dict = MusDict::new();
        assert!(mus_dict.muses().is_empty());
        assert_eq!(mus_dict.min(), None);
        assert!(mus_dict.is_empty());
    }

    #[test]
    fn test_add_mus_existing_literal_smaller_length() -> anyhow::Result<()> {
        let mut mus_dict = MusDict::new();
        let lit = Lit::from_ipasir(1)?;
        let mus1 = BTreeSet::from([Lit::from_ipasir(2)?, Lit::from_ipasir(3)?]);
        let mus2 = BTreeSet::from([Lit::from_ipasir(4)?]);
        mus_dict.add_mus(lit, mus1.clone());
        mus_dict.add_mus(lit, mus2.clone());

        assert_eq!(mus_dict.min(), Some(1));
        assert!(!mus_dict.is_empty());
        Ok(())
    }

    #[test]
    fn test_add_mus_existing_literal_equal_length() -> anyhow::Result<()> {
        let mut mus_dict = MusDict::new();
        let lit = Lit::from_ipasir(1)?;
        let mus1 = BTreeSet::from([Lit::from_ipasir(2)?, Lit::from_ipasir(3)?]);
        let mus2 = BTreeSet::from([Lit::from_ipasir(4)?, Lit::from_ipasir(5)?]);
        mus_dict.add_mus(lit, mus1.clone());
        mus_dict.add_mus(lit, mus2.clone());
        let bts: BTreeSet<_> = vec![MusContext::new(lit, mus1), MusContext::new(lit, mus2)]
            .into_iter()
            .collect();
        assert_eq!(mus_dict.muses().get(&lit), Some(&bts));
        assert_eq!(mus_dict.min(), Some(2));
        assert!(!mus_dict.is_empty());
        Ok(())
    }

    #[test]
    fn test_min_lit_existing_literal() -> anyhow::Result<()> {
        let mut mus_dict = MusDict::new();
        let lit = Lit::from_ipasir(1)?;
        let lit2 = Lit::from_ipasir(2)?;
        let mus1 = BTreeSet::from([Lit::from_ipasir(2)?, Lit::from_ipasir(3)?]);
        let mus2 = BTreeSet::from([Lit::from_ipasir(4)?, Lit::from_ipasir(5)?]);
        mus_dict.add_mus(lit, mus1.clone());
        mus_dict.add_mus(lit, mus2.clone());
        assert_eq!(mus_dict.min_lit(lit), Some(2));
        assert_eq!(mus_dict.min_lit(lit2), None);
        Ok(())
    }

    #[test]
    fn test_min_lit_non_existing_literal() -> anyhow::Result<()> {
        let mus_dict = MusDict::new();
        let lit = Lit::from_ipasir(1)?;
        assert_eq!(mus_dict.min_lit(lit), None);
        Ok(())
    }

    #[test]
    fn test_add_mus_existing_literal_larger_length() -> anyhow::Result<()> {
        let mut mus_dict = MusDict::new();
        let lit = Lit::from_ipasir(1)?;
        let mus1 = BTreeSet::from([Lit::from_ipasir(2)?, Lit::from_ipasir(3)?]);
        let mus2 = BTreeSet::from([Lit::from_ipasir(4)?]);
        mus_dict.add_mus(lit, mus2.clone());
        mus_dict.add_mus(lit, mus1.clone());
        let bts: BTreeSet<_> = std::iter::once(MusContext::new(lit, mus2)).collect();
        assert_eq!(mus_dict.muses().get(&lit), Some(&bts));
        assert_eq!(mus_dict.min(), Some(1));
        assert!(!mus_dict.is_empty());
        Ok(())
    }

    #[test]
    fn test_add_mus_new_literal() -> anyhow::Result<()> {
        let mut mus_dict = MusDict::new();
        let lit1 = Lit::from_ipasir(1)?;
        let lit2 = Lit::from_ipasir(2)?;
        let mus1 = BTreeSet::from([Lit::from_ipasir(3)?, Lit::from_ipasir(4)?]);
        let mus2 = BTreeSet::from([Lit::from_ipasir(5)?, Lit::from_ipasir(6)?]);
        mus_dict.add_mus(lit1, mus1.clone());
        mus_dict.add_mus(lit2, mus2.clone());
        let bts1: BTreeSet<_> = std::iter::once(MusContext::new(lit1, mus1)).collect();
        let bts2: BTreeSet<_> = std::iter::once(MusContext::new(lit2, mus2)).collect();
        assert_eq!(mus_dict.muses().get(&lit1), Some(&bts1));
        assert_eq!(mus_dict.min(), Some(2));
        assert_eq!(mus_dict.muses().get(&lit2), Some(&bts2));
        assert_eq!(mus_dict.min(), Some(2));
        assert!(!mus_dict.is_empty());
        Ok(())
    }
}

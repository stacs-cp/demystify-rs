use std::collections::{BTreeMap, BTreeSet};

use rustsat::types::Lit;

/// A dictionary for storing muses (minimal unsatisfiable subsets) associated with literals.
///
/// Maintains two auxiliary indices so that size queries are O(1) amortised, not a scan
/// over all stored MUSes:
/// * `min_sizes[lit]` is the smallest MUS size recorded for `lit`.
/// * `size_index[size]` is the set of lits whose minimum MUS has that size. `size_index`
///   is a `BTreeMap`, so the global minimum is `.first_key_value()`.
///
/// Both indices are updated on every `add_mus` call; no other mutator touches the
/// stored MUSes, so the invariant `min_sizes[lit] == muses[lit].map(mus_len).min()`
/// and `size_index[size] == {lit | min_sizes[lit] == size}` holds at every entry and
/// exit from a `&mut self` method.
#[derive(Clone)]
pub struct MusDict {
    muses: BTreeMap<Lit, BTreeSet<MusContext>>,
    keep_all: bool,
    min_sizes: BTreeMap<Lit, usize>,
    size_index: BTreeMap<usize, BTreeSet<Lit>>,
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
            muses: BTreeMap::new(),
            keep_all: false,
            min_sizes: BTreeMap::new(),
            size_index: BTreeMap::new(),
        }
    }

    /// Creates a new `MusDict` configured to keep every MUS added, including those
    /// strictly larger than the current minimum for their literal. The flag can only
    /// be chosen at construction: flipping it mid-life would leave a dict that
    /// claims "smallest only" but still holds the larger MUSes accumulated before
    /// the flip.
    #[must_use]
    pub fn with_keep_all(keep_all: bool) -> Self {
        let mut d = Self::new();
        d.keep_all = keep_all;
        d
    }

    /// Returns whether this dict keeps all MUSes per literal.
    #[must_use]
    pub fn keep_all(&self) -> bool {
        self.keep_all
    }

    /// Records `candidate` as the minimum MUS size for `lit` if it is strictly smaller
    /// than the current minimum (or no minimum is recorded yet). Calling this with a
    /// value at least as large as the current minimum is a no-op, so it is always safe
    /// to invoke after a successful insert.
    fn update_min(&mut self, lit: Lit, candidate: usize) {
        let old = self.min_sizes.get(&lit).copied();
        if old.is_some_and(|m| m <= candidate) {
            return;
        }
        self.min_sizes.insert(lit, candidate);
        if let Some(old_size) = old
            && let Some(set) = self.size_index.get_mut(&old_size)
        {
            set.remove(&lit);
            if set.is_empty() {
                self.size_index.remove(&old_size);
            }
        }
        self.size_index.entry(candidate).or_default().insert(lit);
    }

    /// Adds a new mus to the dictionary.
    ///
    /// In the default mode, if the mus associated with the given literal already exists in the
    /// dictionary, the new mus will be added only if its length is smaller than the existing mus.
    /// If the lengths are equal, the new mus will be appended to the existing mus list.
    ///
    /// In `keep_all` mode, every MUS is retained regardless of size.
    ///
    /// If the mus associated with the given literal does not exist in the dictionary, a new entry
    /// will be created with the given mus.
    ///
    /// # Arguments
    ///
    /// * `lit` - The literal associated with the mus.
    /// * `new_mus` - The new mus to be added.
    pub fn add_mus(&mut self, lit: Lit, new_mus: BTreeSet<Lit>) {
        let new_size = new_mus.len();

        if self.keep_all {
            let inserted = self
                .muses
                .entry(lit)
                .or_default()
                .insert(MusContext::new(lit, new_mus));
            if inserted {
                self.update_min(lit, new_size);
            }
            return;
        }

        match self.muses.get_mut(&lit) {
            Some(mus_list) => {
                let current_min = *self
                    .min_sizes
                    .get(&lit)
                    .expect("min_sizes invariant: lit present in muses => lit in min_sizes");
                match new_size.cmp(&current_min) {
                    std::cmp::Ordering::Less => {
                        mus_list.clear();
                        mus_list.insert(MusContext::new(lit, new_mus));
                        self.update_min(lit, new_size);
                    }
                    std::cmp::Ordering::Equal => {
                        mus_list.insert(MusContext::new(lit, new_mus));
                    }
                    std::cmp::Ordering::Greater => {}
                }
            }
            None => {
                let hs: BTreeSet<_> = std::iter::once(MusContext::new(lit, new_mus)).collect();
                self.muses.insert(lit, hs);
                self.update_min(lit, new_size);
            }
        }
    }

    #[must_use]
    pub fn min_lit(&self, lit: Lit) -> Option<usize> {
        self.min_sizes.get(&lit).copied()
    }

    /// Returns a reference to the muses in the dictionary.
    #[must_use]
    pub fn muses(&self) -> &BTreeMap<Lit, BTreeSet<MusContext>> {
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
        self.size_index.keys().next().copied()
    }

    /// Like `min`, but only considers entries whose literal is still in `valid_lits`.
    /// Use this when the dict may contain stale entries from previous solving steps.
    #[must_use]
    pub fn min_filtered(&self, valid_lits: &BTreeSet<Lit>) -> Option<usize> {
        self.size_index
            .iter()
            .find(|(_, lits)| !lits.is_disjoint(valid_lits))
            .map(|(size, _)| *size)
    }

    /// Merges all entries from `other` into this dict (keeping smallest MUSes).
    pub fn merge(&mut self, other: &MusDict) {
        for (lit, mus_set) in &other.muses {
            for mc in mus_set {
                self.add_mus(*lit, mc.mus.clone());
            }
        }
    }
}

#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct MusContext {
    pub lits: BTreeSet<Lit>,
    pub mus: BTreeSet<Lit>,
}

impl MusContext {
    #[must_use]
    pub fn new(l: Lit, mus: BTreeSet<Lit>) -> Self {
        Self {
            lits: BTreeSet::from([l]),
            mus,
        }
    }

    #[must_use]
    pub fn new_multi_lit(lits: BTreeSet<Lit>, mus: BTreeSet<Lit>) -> Self {
        Self { lits, mus }
    }

    #[must_use]
    pub fn new_with_more_lits(mut lits: BTreeSet<Lit>, mc: &Self) -> Self {
        for l in &mc.lits {
            lits.insert(*l);
        }

        Self {
            lits,
            mus: mc.mus.clone(),
        }
    }

    #[must_use]
    pub fn mus_len(&self) -> usize {
        self.mus.len()
    }
}

/// Merges `MusContext` objects with identical `mus` values.
///
/// This function takes a list of `MusContext` objects and combines those that have
/// the same `mus` field by merging their `lits` fields together. The resulting list
/// contains `MusContext` objects where each unique `mus` appears exactly once, with
/// all associated literals consolidated.
///
/// # Arguments
///
/// * `v` - A slice of `MusContext` objects to be merged
///
/// # Returns
///
/// A new `Vec<MusContext>` where each `MusContext` has a unique `mus` field and
/// contains all literals from the original `MusContext` objects with the same `mus`.
///
#[must_use]
pub fn merge_muscontexts(v: &[MusContext]) -> Vec<MusContext> {
    let mut mus_map: BTreeMap<&BTreeSet<Lit>, BTreeSet<Lit>> = BTreeMap::new();

    // Group literals by their MUS
    for mc in v {
        mus_map.entry(&mc.mus).or_default().extend(&mc.lits);
    }

    // Create new MusContext objects with merged literals
    mus_map
        .into_iter()
        .map(|(mus, lits)| MusContext::new_multi_lit(lits, mus.clone()))
        .collect()
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
    fn test_min_and_min_filtered_track_index() -> anyhow::Result<()> {
        let mut mus_dict = MusDict::new();
        let lit_a = Lit::from_ipasir(1)?;
        let lit_b = Lit::from_ipasir(2)?;
        let lit_c = Lit::from_ipasir(3)?;
        let size1 = BTreeSet::from([Lit::from_ipasir(10)?]);
        let size2 = BTreeSet::from([Lit::from_ipasir(11)?, Lit::from_ipasir(12)?]);
        let size3 = BTreeSet::from([
            Lit::from_ipasir(13)?,
            Lit::from_ipasir(14)?,
            Lit::from_ipasir(15)?,
        ]);

        mus_dict.add_mus(lit_a, size3.clone());
        mus_dict.add_mus(lit_b, size2.clone());
        mus_dict.add_mus(lit_c, size1.clone());

        assert_eq!(mus_dict.min(), Some(1));
        assert_eq!(mus_dict.min_lit(lit_a), Some(3));
        assert_eq!(mus_dict.min_lit(lit_b), Some(2));
        assert_eq!(mus_dict.min_lit(lit_c), Some(1));

        // Filtering that excludes the cheapest lit should bump the answer up.
        let only_ab = BTreeSet::from([lit_a, lit_b]);
        assert_eq!(mus_dict.min_filtered(&only_ab), Some(2));
        let only_a = BTreeSet::from([lit_a]);
        assert_eq!(mus_dict.min_filtered(&only_a), Some(3));
        let empty = BTreeSet::new();
        assert_eq!(mus_dict.min_filtered(&empty), None);

        // Shrinking lit_a to size 1 should update both per-lit and global mins.
        mus_dict.add_mus(lit_a, size1.clone());
        assert_eq!(mus_dict.min_lit(lit_a), Some(1));
        assert_eq!(mus_dict.min_filtered(&only_a), Some(1));
        Ok(())
    }

    #[test]
    fn test_duplicate_mus_does_not_perturb_index() -> anyhow::Result<()> {
        let mut mus_dict = MusDict::new();
        let lit = Lit::from_ipasir(1)?;
        let mus = BTreeSet::from([Lit::from_ipasir(2)?, Lit::from_ipasir(3)?]);

        mus_dict.add_mus(lit, mus.clone());
        mus_dict.add_mus(lit, mus.clone());
        mus_dict.add_mus(lit, mus);

        assert_eq!(mus_dict.muses().get(&lit).map(BTreeSet::len), Some(1));
        assert_eq!(mus_dict.min_lit(lit), Some(2));
        assert_eq!(mus_dict.min(), Some(2));
        Ok(())
    }

    #[test]
    fn test_merge_updates_index() -> anyhow::Result<()> {
        let lit_a = Lit::from_ipasir(1)?;
        let lit_b = Lit::from_ipasir(2)?;
        let size1 = BTreeSet::from([Lit::from_ipasir(10)?]);
        let size2 = BTreeSet::from([Lit::from_ipasir(11)?, Lit::from_ipasir(12)?]);

        let mut dst = MusDict::new();
        dst.add_mus(lit_a, size2.clone());

        let mut src = MusDict::new();
        src.add_mus(lit_a, size1.clone());
        src.add_mus(lit_b, size2.clone());

        dst.merge(&src);
        assert_eq!(dst.min_lit(lit_a), Some(1));
        assert_eq!(dst.min_lit(lit_b), Some(2));
        assert_eq!(dst.min(), Some(1));
        Ok(())
    }

    #[test]
    fn test_keep_all_retains_larger_muses() -> anyhow::Result<()> {
        let mut mus_dict = MusDict::with_keep_all(true);
        let lit = Lit::from_ipasir(1)?;
        let mus_small = BTreeSet::from([Lit::from_ipasir(2)?]);
        let mus_medium = BTreeSet::from([Lit::from_ipasir(3)?, Lit::from_ipasir(4)?]);
        let mus_large = BTreeSet::from([
            Lit::from_ipasir(5)?,
            Lit::from_ipasir(6)?,
            Lit::from_ipasir(7)?,
        ]);

        mus_dict.add_mus(lit, mus_medium.clone());
        mus_dict.add_mus(lit, mus_small.clone());
        mus_dict.add_mus(lit, mus_large.clone());

        let stored = mus_dict.muses().get(&lit).expect("lit should be present");
        assert_eq!(stored.len(), 3);
        assert_eq!(mus_dict.min_lit(lit), Some(1));
        Ok(())
    }

    #[test]
    fn test_keep_all_ignores_duplicate_muses() -> anyhow::Result<()> {
        let mut mus_dict = MusDict::with_keep_all(true);
        let lit = Lit::from_ipasir(1)?;
        let mus = BTreeSet::from([Lit::from_ipasir(2)?, Lit::from_ipasir(3)?]);

        mus_dict.add_mus(lit, mus.clone());
        mus_dict.add_mus(lit, mus.clone());

        assert_eq!(mus_dict.muses().get(&lit).map(BTreeSet::len), Some(1));
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

    #[test]
    fn test_merge_muscontexts_empty() {
        let v: Vec<MusContext> = Vec::new();
        let result = merge_muscontexts(&v);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_merge_muscontexts_single_entry() -> anyhow::Result<()> {
        let lit = Lit::from_ipasir(1)?;
        let mus = BTreeSet::from([Lit::from_ipasir(2)?, Lit::from_ipasir(3)?]);
        let mc = MusContext::new(lit, mus);
        let v = vec![mc.clone()];

        let result = merge_muscontexts(&v);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], mc);
        Ok(())
    }

    #[test]
    fn test_merge_muscontexts_identical_mus() -> anyhow::Result<()> {
        let lit1 = Lit::from_ipasir(1)?;
        let lit2 = Lit::from_ipasir(2)?;
        let mus = BTreeSet::from([Lit::from_ipasir(3)?, Lit::from_ipasir(4)?]);

        let mc1 = MusContext::new(lit1, mus.clone());
        let mc2 = MusContext::new(lit2, mus);

        let v = vec![mc1, mc2];
        let result = merge_muscontexts(&v);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].mus_len(), 2);
        assert_eq!(result[0].lits.len(), 2);
        assert!(result[0].lits.contains(&lit1));
        assert!(result[0].lits.contains(&lit2));
        Ok(())
    }

    #[test]
    fn test_merge_muscontexts_different_mus() -> anyhow::Result<()> {
        let lit = Lit::from_ipasir(1)?;
        let mus1 = BTreeSet::from([Lit::from_ipasir(2)?, Lit::from_ipasir(3)?]);
        let mus2 = BTreeSet::from([Lit::from_ipasir(4)?, Lit::from_ipasir(5)?]);

        let mc1 = MusContext::new(lit, mus1);
        let mc2 = MusContext::new(lit, mus2);

        let v = vec![mc1, mc2];
        let result = merge_muscontexts(&v);

        assert_eq!(result.len(), 2);
        // Elements are sorted by mus content because of BTreeMap
        assert!(result[0].mus.contains(&Lit::from_ipasir(2)?));
        assert!(result[1].mus.contains(&Lit::from_ipasir(4)?));
        Ok(())
    }

    #[test]
    fn test_merge_muscontexts_complex_case() -> anyhow::Result<()> {
        let lit1 = Lit::from_ipasir(1)?;
        let lit2 = Lit::from_ipasir(2)?;
        let lit3 = Lit::from_ipasir(3)?;

        let mus1 = BTreeSet::from([Lit::from_ipasir(10)?, Lit::from_ipasir(11)?]);
        let mus2 = BTreeSet::from([Lit::from_ipasir(20)?, Lit::from_ipasir(21)?]);

        // Both lit1 and lit2 share mus1
        let mc1 = MusContext::new(lit1, mus1.clone());
        let mc2 = MusContext::new(lit2, mus1);
        // lit3 has a different mus
        let mc3 = MusContext::new(lit3, mus2);

        let v = vec![mc1, mc2, mc3];
        let result = merge_muscontexts(&v);

        assert_eq!(result.len(), 2);

        // Find the merged entry with lit1 and lit2
        let merged_entry = result.iter().find(|mc| mc.lits.contains(&lit1)).unwrap();
        assert_eq!(merged_entry.lits.len(), 2);
        assert!(merged_entry.lits.contains(&lit1));
        assert!(merged_entry.lits.contains(&lit2));

        // Find the entry with lit3
        let single_entry = result.iter().find(|mc| mc.lits.contains(&lit3)).unwrap();
        assert_eq!(single_entry.lits.len(), 1);
        assert!(single_entry.lits.contains(&lit3));

        Ok(())
    }
}

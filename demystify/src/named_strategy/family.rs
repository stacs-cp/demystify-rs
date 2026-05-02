//! Family-group resolution for fingerprinting.
//!
//! Given a raw constraint-family name (the part of a constraint description
//! before `[`, e.g. `"row_alldiff"`), a `FamilyMap` returns the canonical
//! symbol the fingerprinter should use. By default the raw name is returned
//! unchanged; passing a group definition collapses several raw families to a
//! shared group symbol (e.g. `row_alldiff` + `con_alldiff` + `box_alldiff`
//! → `unit_alldiff`).

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct FamilyMap {
    map: BTreeMap<String, String>,
}

impl FamilyMap {
    /// Identity remap: every constraint family symbol stays as-is.
    #[must_use]
    pub fn identity() -> Self {
        Self::default()
    }

    /// Build a remap that replaces every member of `members` with `group`.
    /// Later inserts override earlier ones for the same raw family.
    /// Accepts any iterator of strings (e.g. `family.members.keys()`).
    pub fn add_group<I, S>(&mut self, group: &str, members: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for m in members {
            self.map.insert(m.as_ref().to_string(), group.to_string());
        }
    }

    /// Return the canonical symbol for a raw family name.
    /// Falls back to the raw name if no group claims it.
    #[must_use]
    pub fn resolve<'a>(&'a self, raw: &'a str) -> &'a str {
        self.map.get(raw).map_or(raw, String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_passes_through() {
        let f = FamilyMap::identity();
        assert_eq!(f.resolve("row_alldiff"), "row_alldiff");
        assert_eq!(f.resolve("anything_else"), "anything_else");
    }

    #[test]
    fn group_remaps_members() {
        let mut f = FamilyMap::identity();
        f.add_group(
            "unit_alldiff",
            ["row_alldiff", "con_alldiff", "box_alldiff"],
        );
        assert_eq!(f.resolve("row_alldiff"), "unit_alldiff");
        assert_eq!(f.resolve("con_alldiff"), "unit_alldiff");
        assert_eq!(f.resolve("box_alldiff"), "unit_alldiff");
        assert_eq!(f.resolve("row_contains"), "row_contains");
    }

    #[test]
    fn later_group_overrides_earlier() {
        let mut f = FamilyMap::identity();
        f.add_group("unit_alldiff", ["row_alldiff"]);
        f.add_group("line_alldiff", ["row_alldiff"]);
        // Last write wins.
        assert_eq!(f.resolve("row_alldiff"), "line_alldiff");
    }

    #[test]
    fn add_group_accepts_string_keys() {
        // Ensure the API works with the BTreeMap key iterator from `Family.members`.
        let mut f = FamilyMap::identity();
        let members: std::collections::BTreeMap<String, String> = [
            ("row_contains".to_string(), "Row".to_string()),
            ("con_contains".to_string(), "Column".to_string()),
        ]
        .into_iter()
        .collect();
        f.add_group("unit_contains", members.keys());
        assert_eq!(f.resolve("row_contains"), "unit_contains");
        assert_eq!(f.resolve("con_contains"), "unit_contains");
    }
}

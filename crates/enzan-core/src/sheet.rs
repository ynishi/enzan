//! The modifiers of a character, kept by where they come from.
//!
//! A [`Sheet`] holds one [`ModifierSet`] for each source, such as `weapon` or
//! `level`, under the [`SourceId`] the game gave it. [`Catalog::evaluate_sheet`]
//! evaluates the stats of a sheet.
//!
//! # Order
//!
//! Modifiers are evaluated in ascending byte order of [`SourceId`], and within a
//! source in the order they were added. This is the fixed order that
//! [`ModifierSet`] asks of a caller that joins one set per source. Because it
//! is the sheet's and not the caller's, a sheet gives the same bits whatever
//! order the game equipped things in or loaded them in. Renaming a source can
//! change the order and so can change the bits.
//!
//! A sheet holds no cache: a value is computed on each call.
//!
//! [`Catalog::evaluate_sheet`]: crate::catalog::Catalog::evaluate_sheet

use std::collections::{BTreeMap, btree_map};

use crate::key::key_type;
use crate::stat::ModifierSet;

key_type! {
    /// The game's name for a source of modifiers, such as `weapon` or `level`.
    ///
    /// Its shape is the one of [`KeyError`](crate::KeyError). Sources are
    /// ordered by it, and [the module doc](self) says why that matters.
    SourceId
}

/// The modifier sets of a character, one for each source.
///
/// A plain value: it can be cloned, compared and rebuilt, and holds nothing but
/// the sets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sheet {
    sources: BTreeMap<SourceId, ModifierSet>,
}

impl Sheet {
    /// A sheet with no source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Puts `set` under `id`, and returns the set that was there, if any.
    pub fn set(&mut self, id: SourceId, set: ModifierSet) -> Option<ModifierSet> {
        self.sources.insert(id, set)
    }

    /// Takes the source `id` away, and returns its set, if it was there.
    pub fn remove(&mut self, id: &SourceId) -> Option<ModifierSet> {
        self.sources.remove(id)
    }

    /// The set of the source `id`.
    #[must_use]
    pub fn get(&self, id: &SourceId) -> Option<&ModifierSet> {
        self.sources.get(id)
    }

    /// The sources in ascending order of their ids, which is the order they are
    /// evaluated in.
    pub fn sources(&self) -> btree_map::Iter<'_, SourceId, ModifierSet> {
        self.sources.iter()
    }

    /// The sets of all sources joined in the order of the module doc.
    pub(crate) fn joined(&self) -> ModifierSet {
        self.sources
            .values()
            .flat_map(|set| set.modifiers().iter().cloned())
            .collect()
    }

    /// The source and the position within it of the modifier at `position` in
    /// [`Sheet::joined`].
    pub(crate) fn locate(&self, position: usize) -> Option<(&SourceId, usize)> {
        let mut start = 0;
        for (id, set) in &self.sources {
            let end = start + set.modifiers().len();
            if position < end {
                return Some((id, position - start));
            }
            start = end;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stat::{Modifier, ModifierKind, StatKey};

    fn id(s: &str) -> SourceId {
        SourceId::try_from(s).unwrap()
    }

    fn added(stat: &str, value: f64) -> Modifier {
        Modifier::new(StatKey::try_from(stat).unwrap(), ModifierKind::Added, value).unwrap()
    }

    fn set(items: &[(&str, f64)]) -> ModifierSet {
        items.iter().map(|&(s, v)| added(s, v)).collect()
    }

    #[test]
    fn source_id_has_the_shape_of_a_key() {
        assert!(SourceId::try_from("weapon").is_ok());
        assert_eq!(SourceId::try_from(""), Err(crate::KeyError::Empty));
        assert_eq!(
            SourceId::try_from("Weapon"),
            Err(crate::KeyError::InvalidStart)
        );
        assert_eq!(
            SourceId::try_from("a-b".to_owned()),
            Err(crate::KeyError::InvalidChar { index: 1 })
        );
        assert_eq!(id("weapon").as_str(), "weapon");
        assert_eq!(id("weapon").to_string(), "weapon");
    }

    #[test]
    fn set_replaces_and_returns_the_old_set() {
        let mut sheet = Sheet::new();
        assert_eq!(sheet.set(id("weapon"), set(&[("life", 1.0)])), None);
        assert_eq!(
            sheet.set(id("weapon"), set(&[("life", 2.0)])),
            Some(set(&[("life", 1.0)]))
        );
        assert_eq!(sheet.get(&id("weapon")), Some(&set(&[("life", 2.0)])));
        assert_eq!(sheet.get(&id("ring")), None);
    }

    #[test]
    fn remove_returns_the_set() {
        let mut sheet = Sheet::new();
        sheet.set(id("weapon"), set(&[("life", 1.0)]));
        assert_eq!(sheet.remove(&id("ring")), None);
        assert_eq!(sheet.remove(&id("weapon")), Some(set(&[("life", 1.0)])));
        assert_eq!(sheet.get(&id("weapon")), None);
    }

    #[test]
    fn sources_are_in_ascending_id_whatever_order_they_were_set() {
        let mut a = Sheet::new();
        let mut b = Sheet::new();
        for name in ["weapon", "level", "ring"] {
            a.set(id(name), set(&[("life", 1.0)]));
        }
        for name in ["ring", "weapon", "level"] {
            b.set(id(name), set(&[("life", 1.0)]));
        }
        let names = |s: &Sheet| s.sources().map(|(i, _)| i.to_string()).collect::<Vec<_>>();
        assert_eq!(names(&a), ["level", "ring", "weapon"]);
        assert_eq!(a, b);
    }

    #[test]
    fn joined_goes_by_source_and_then_by_the_order_added() {
        let mut sheet = Sheet::new();
        sheet.set(id("b"), set(&[("life", 3.0), ("life", 4.0)]));
        sheet.set(id("a"), set(&[("life", 1.0), ("life", 2.0)]));
        assert_eq!(
            sheet.joined(),
            set(&[("life", 1.0), ("life", 2.0), ("life", 3.0), ("life", 4.0)])
        );
    }

    #[test]
    fn locate_names_the_source_and_the_position_in_it() {
        let mut sheet = Sheet::new();
        sheet.set(id("b"), set(&[("life", 3.0), ("life", 4.0)]));
        sheet.set(id("a"), set(&[("life", 1.0)]));
        sheet.set(id("c"), ModifierSet::new());
        assert_eq!(sheet.locate(0), Some((&id("a"), 0)));
        assert_eq!(sheet.locate(1), Some((&id("b"), 0)));
        assert_eq!(sheet.locate(2), Some((&id("b"), 1)));
        assert_eq!(sheet.locate(3), None);
    }
}

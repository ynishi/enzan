//! The game's content as one checked whole: stats, bases, rarities, affixes,
//! drop tables and the progression rule.
//!
//! Each part is checked when it is built, in its own module. What no single part
//! can check is how they fit together, and that is what a [`Catalog`] adds: ids
//! that name one thing, and references that point at something that exists. A
//! caller that rolls drops or measures a balance does so against a `Catalog`, so
//! it cannot be handed a table that names a base nobody defined. The rules are
//! in the doc of [`Catalog`].

use std::collections::BTreeSet;

use crate::curve::Level;
use crate::loot::{AffixPool, DropTable, ItemRoll, Rarity, roll_item};
use crate::progression::Progression;
use crate::rng::{Rng, WeightedTable};
use crate::stat::{ModifierSet, StatDef, StatKey};

/// The number the game's data gives a base.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BaseId(u32);

impl BaseId {
    /// The raw number.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

impl From<u32> for BaseId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// The number the game's data gives a drop table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DropTableId(u32);

impl DropTableId {
    /// The raw number.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

impl From<u32> for DropTableId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// A kind of item that can drop, such as a sword.
///
/// The name is for display. No rule reads it, so two bases may share one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseDef {
    id: BaseId,
    name: String,
}

impl BaseDef {
    /// Joins the parts.
    #[must_use]
    pub fn new(id: BaseId, name: String) -> Self {
        Self { id, name }
    }

    /// The id.
    #[must_use]
    pub fn id(&self) -> BaseId {
        self.id
    }

    /// The name, for display.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// A drop table under the id the game refers to it by.
#[derive(Clone, Debug, PartialEq)]
pub struct DropTableDef {
    id: DropTableId,
    table: DropTable<BaseId>,
}

impl DropTableDef {
    /// Joins the parts.
    #[must_use]
    pub fn new(id: DropTableId, table: DropTable<BaseId>) -> Self {
        Self { id, table }
    }

    /// The id.
    #[must_use]
    pub fn id(&self) -> DropTableId {
        self.id
    }

    /// The table.
    #[must_use]
    pub fn table(&self) -> &DropTable<BaseId> {
        &self.table
    }
}

/// The parts of a [`Catalog`], before they are checked together.
///
/// Each part is already valid on its own. The parts have no invariant between
/// them until [`Catalog::new`] checks them, so the fields are public.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogParts {
    /// The stats, in the order they were given.
    pub stats: Vec<StatDef>,
    /// The bases that can drop.
    pub bases: Vec<BaseDef>,
    /// The rarities and their weights.
    pub rarities: WeightedTable<Rarity>,
    /// The affixes an item can carry.
    pub affixes: AffixPool,
    /// The drop tables.
    pub drop_tables: Vec<DropTableDef>,
    /// How levels are gained, if the game has levels.
    pub progression: Option<Progression>,
}

/// Why [`Catalog::check`] found the parts inconsistent.
///
/// An `index` is a position in the `Vec` of [`CatalogParts`] it names, and for
/// rarities a position in [`WeightedTable::entries`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    /// The stat at `index` has a key an earlier stat already has.
    #[error("the stat at index {index} repeats the key of an earlier stat")]
    DuplicateStat {
        /// The position of the repeat.
        index: usize,
    },
    /// The base at `index` has an id an earlier base already has.
    #[error("the base at index {index} repeats the id of an earlier base")]
    DuplicateBase {
        /// The position of the repeat.
        index: usize,
    },
    /// The rarity at `index` has an id an earlier rarity already has.
    #[error("the rarity at index {index} repeats the id of an earlier rarity")]
    DuplicateRarity {
        /// The position of the repeat.
        index: usize,
    },
    /// The drop table at `index` has an id an earlier drop table already has.
    #[error("the drop table at index {index} repeats the id of an earlier drop table")]
    DuplicateTable {
        /// The position of the repeat.
        index: usize,
    },
    /// The affix at `affix_index` is on a stat that is not in the stats.
    #[error("the affix at index {affix_index} is on a stat that is not defined")]
    UnknownAffixStat {
        /// The position of the affix in the pool.
        affix_index: usize,
    },
    /// A drop table lists a base that is not in the bases.
    #[error(
        "entry {entry_index} of the drop table at index {table_index} is a base that is not defined"
    )]
    UnknownTableBase {
        /// The position of the drop table.
        table_index: usize,
        /// The position of the entry in the table's bases.
        entry_index: usize,
    },
}

/// No drop table has the id asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("no drop table has this id")]
pub struct UnknownTable;

/// No base has the id asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("no base has this id")]
pub struct UnknownBase;

/// Why [`Catalog::evaluate_stats`] gave no values.
///
/// The errors are found in the order they are listed here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EvaluateStatsError {
    /// The key at `key_index` of the keys asked for is not a stat of the
    /// catalog.
    #[error("the key at index {key_index} is not a stat of the catalog")]
    UnknownStat {
        /// The position in the keys.
        key_index: usize,
    },
    /// The modifier at `modifier_index` is on a stat that is not in the
    /// catalog.
    #[error("the modifier at index {modifier_index} is on a stat that is not in the catalog")]
    UnknownModifierStat {
        /// The position in the modifiers.
        modifier_index: usize,
    },
    /// The value of the stat at `key_index` left the range of `f64`.
    #[error("the value of the stat at index {key_index} left the range of f64")]
    Overflow {
        /// The position in the keys.
        key_index: usize,
    },
}

/// The content of a game that has been checked as a whole.
///
/// A catalog holds only parts that fit together:
///
/// - the keys of the stats, and the ids of the bases, of the rarities and of
///   the drop tables, are each unique;
/// - the stat of every affix is one of the stats;
/// - every base in a drop table is one of the bases.
///
/// The id of an affix is unique too, but [`AffixPool`] checks that when it is
/// built, so a catalog does not. Nothing reads the name of a base.
///
/// A catalog does not change after it is built, so a lookup that succeeded once
/// succeeds again, and a roll against it differs only in the [`Rng`] it is
/// given.
#[derive(Clone, Debug, PartialEq)]
pub struct Catalog {
    parts: CatalogParts,
}

impl Catalog {
    /// Every way `parts` breaks the rules in the type's doc, in this order:
    /// repeated stats, repeated bases, repeated rarities, repeated drop tables,
    /// affixes on an undefined stat, drop table entries with an undefined base.
    ///
    /// Within a group the errors follow the order of the parts. For a repeat
    /// the index is where the key or id shows up again, so every occurrence
    /// after the first has its own error. An undefined reference is told by the
    /// position of the affix, or by the position of the drop table and of the
    /// entry in its [`DropTable::bases`].
    ///
    /// This is the one place the rules are checked; [`Catalog::new`] takes the
    /// first error from it, so a tool that wants to show them all and a caller
    /// that wants the first cannot disagree.
    #[must_use]
    pub fn check(parts: &CatalogParts) -> Vec<CatalogError> {
        let mut errors = Vec::new();
        errors.extend(
            repeats(parts.stats.iter().map(StatDef::key))
                .map(|index| CatalogError::DuplicateStat { index }),
        );
        errors.extend(
            repeats(parts.bases.iter().map(BaseDef::id))
                .map(|index| CatalogError::DuplicateBase { index }),
        );
        errors.extend(
            repeats(parts.rarities.entries().iter().map(|(r, _)| r.id()))
                .map(|index| CatalogError::DuplicateRarity { index }),
        );
        errors.extend(
            repeats(parts.drop_tables.iter().map(DropTableDef::id))
                .map(|index| CatalogError::DuplicateTable { index }),
        );

        let stats: BTreeSet<&StatKey> = parts.stats.iter().map(StatDef::key).collect();
        for (affix_index, affix) in parts.affixes.affixes().iter().enumerate() {
            // `AffixDef` keeps `low` and `high` on one stat.
            if !stats.contains(affix.low().stat()) {
                errors.push(CatalogError::UnknownAffixStat { affix_index });
            }
        }

        let bases: BTreeSet<BaseId> = parts.bases.iter().map(BaseDef::id).collect();
        for (table_index, def) in parts.drop_tables.iter().enumerate() {
            for (entry_index, (base, _)) in def.table.bases().entries().iter().enumerate() {
                if !bases.contains(base) {
                    errors.push(CatalogError::UnknownTableBase {
                        table_index,
                        entry_index,
                    });
                }
            }
        }
        errors
    }

    /// Builds the catalog from `parts`.
    ///
    /// # Errors
    ///
    /// The first error of [`Catalog::check`].
    pub fn new(parts: CatalogParts) -> Result<Self, CatalogError> {
        match Self::check(&parts).into_iter().next() {
            Some(error) => Err(error),
            None => Ok(Self { parts }),
        }
    }

    /// One defeat: rolls the drop table `table`, and then an item for each base
    /// that dropped.
    ///
    /// The order of draws is the one of [`DropTable::roll`], followed by
    /// [`roll_item`] for each base in the order they were drawn, all on `rng`.
    ///
    /// # Errors
    ///
    /// [`UnknownTable`] when no drop table has the id `table`; `rng` is left
    /// as it was.
    pub fn roll_drop(
        &self,
        table: DropTableId,
        item_level: Level,
        rng: &mut Rng,
    ) -> Result<Vec<ItemRoll<BaseId>>, UnknownTable> {
        let table = self.drop_table(table).ok_or(UnknownTable)?;
        Ok(self.loot_trial(table, item_level, rng))
    }

    /// Rolls the rarity and affixes of one item of `base`.
    ///
    /// # Errors
    ///
    /// [`UnknownBase`] when no base has the id `base`; `rng` is left as it was.
    pub fn roll_item(
        &self,
        base: BaseId,
        item_level: Level,
        rng: &mut Rng,
    ) -> Result<ItemRoll<BaseId>, UnknownBase> {
        if !self.parts.bases.iter().any(|b| b.id == base) {
            return Err(UnknownBase);
        }
        Ok(roll_item(
            base,
            &self.parts.rarities,
            &self.parts.affixes,
            item_level,
            rng,
        ))
    }

    /// The values of the stats `keys`, in the order of `keys`, under
    /// `modifiers`.
    ///
    /// Each stat is evaluated by [`ModifierSet::evaluate`], so modifiers count
    /// in the order they were pushed. A modifier on a stat that is not in the
    /// catalog is an error and not ignored, because a misspelt key would
    /// otherwise change nothing and show nowhere. A modifier on a stat of the
    /// catalog that is not in `keys` changes only the stats it is on.
    ///
    /// # Errors
    ///
    /// The first of these that applies: [`EvaluateStatsError::UnknownStat`] for
    /// the first key that is not a stat, [`EvaluateStatsError::UnknownModifierStat`]
    /// for the first modifier on a stat that is not in the catalog, then
    /// [`EvaluateStatsError::Overflow`] for the first key, in the order of
    /// `keys`, whose value leaves the range of `f64`.
    pub fn evaluate_stats(
        &self,
        modifiers: &ModifierSet,
        keys: &[StatKey],
    ) -> Result<Vec<f64>, EvaluateStatsError> {
        let mut defs = Vec::with_capacity(keys.len());
        for (key_index, key) in keys.iter().enumerate() {
            defs.push(
                self.stat(key)
                    .ok_or(EvaluateStatsError::UnknownStat { key_index })?,
            );
        }
        for (modifier_index, modifier) in modifiers.modifiers().iter().enumerate() {
            if self.stat(modifier.stat()).is_none() {
                return Err(EvaluateStatsError::UnknownModifierStat { modifier_index });
            }
        }
        defs.into_iter()
            .enumerate()
            .map(|(key_index, def)| {
                modifiers
                    .evaluate(def)
                    .map_err(|_| EvaluateStatsError::Overflow { key_index })
            })
            .collect()
    }

    /// The stat with the key `key`.
    #[must_use]
    pub fn stat(&self, key: &StatKey) -> Option<&StatDef> {
        self.parts.stats.iter().find(|s| s.key() == key)
    }

    /// The drop table with the id `id`.
    #[must_use]
    pub fn drop_table(&self, id: DropTableId) -> Option<&DropTable<BaseId>> {
        self.parts
            .drop_tables
            .iter()
            .find(|d| d.id == id)
            .map(DropTableDef::table)
    }

    /// The rule for levelling, if the game has one.
    #[must_use]
    pub fn progression(&self) -> Option<&Progression> {
        self.parts.progression.as_ref()
    }

    /// The stats in the order they were given.
    #[must_use]
    pub fn stats(&self) -> &[StatDef] {
        &self.parts.stats
    }

    /// The bases in the order they were given.
    #[must_use]
    pub fn bases(&self) -> &[BaseDef] {
        &self.parts.bases
    }

    /// The rarities and their weights.
    #[must_use]
    pub fn rarities(&self) -> &WeightedTable<Rarity> {
        &self.parts.rarities
    }

    /// The affixes an item can carry.
    #[must_use]
    pub fn affixes(&self) -> &AffixPool {
        &self.parts.affixes
    }

    /// The draws of one defeat, for a table that is known to be in the catalog.
    ///
    /// It is crate-private so that no caller outside the crate can reach it
    /// with a table the catalog does not hold: the bases of such a table are
    /// not checked against the bases. A caller in the crate passes only a table
    /// from [`Catalog::drop_table`] of the same catalog.
    pub(crate) fn loot_trial(
        &self,
        table: &DropTable<BaseId>,
        item_level: Level,
        rng: &mut Rng,
    ) -> Vec<ItemRoll<BaseId>> {
        table
            .roll(rng)
            .into_iter()
            .map(|base| {
                roll_item(
                    *base,
                    &self.parts.rarities,
                    &self.parts.affixes,
                    item_level,
                    rng,
                )
            })
            .collect()
    }
}

/// The positions in `keys` of a key that an earlier position already has.
fn repeats<K: Ord>(keys: impl Iterator<Item = K>) -> impl Iterator<Item = usize> {
    let mut seen = BTreeSet::new();
    keys.enumerate()
        .filter_map(move |(index, key)| (!seen.insert(key)).then_some(index))
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU32;

    use super::*;
    use crate::loot::{
        AffixDef, AffixGroup, AffixId, AffixKind, AffixSpec, Chance, CountRange, RarityId,
    };
    use crate::rng::Seed;
    use crate::stat::{Bounds, Modifier, ModifierKind};

    fn nz(n: u32) -> NonZeroU32 {
        NonZeroU32::new(n).unwrap()
    }

    fn lvl(n: u32) -> Level {
        Level::try_from(n).unwrap()
    }

    fn key(s: &str) -> StatKey {
        StatKey::try_from(s).unwrap()
    }

    fn stat_def(k: &str, base: f64) -> StatDef {
        StatDef::new(key(k), base, Bounds::UNBOUNDED).unwrap()
    }

    fn base(id: u32) -> BaseDef {
        BaseDef::new(BaseId::from(id), format!("base {id}"))
    }

    fn rarity(id: u32) -> (Rarity, NonZeroU32) {
        (
            Rarity::new(RarityId::from(id), CountRange::new(1, 1).unwrap(), 1, 1),
            nz(1),
        )
    }

    fn affix(id: u32, stat: &str) -> AffixDef {
        let at = |value| Modifier::new(key(stat), ModifierKind::Added, value).unwrap();
        AffixDef::new(AffixSpec {
            id: AffixId::from(id),
            kind: AffixKind::Prefix,
            tier: 1,
            required_level: lvl(1),
            weight: nz(1),
            group: AffixGroup::from(id),
            low: at(1.0),
            high: at(5.0),
        })
        .unwrap()
    }

    fn table(id: u32, chance: f64, count: (u32, u32), bases: &[u32]) -> DropTableDef {
        DropTableDef::new(
            DropTableId::from(id),
            DropTable::new(
                Chance::try_from(chance).unwrap(),
                CountRange::new(count.0, count.1).unwrap(),
                WeightedTable::new(bases.iter().map(|&b| (BaseId::from(b), nz(1))).collect())
                    .unwrap(),
            ),
        )
    }

    fn parts() -> CatalogParts {
        CatalogParts {
            stats: vec![stat_def("life", 10.0), stat_def("mana", 20.0)],
            bases: vec![base(1), base(2)],
            rarities: WeightedTable::new(vec![rarity(1), rarity(2)]).unwrap(),
            affixes: AffixPool::new(vec![affix(1, "life"), affix(2, "mana")]).unwrap(),
            drop_tables: vec![table(1, 1.0, (1, 1), &[1, 2])],
            progression: None,
        }
    }

    fn catalog() -> Catalog {
        Catalog::new(parts()).unwrap()
    }

    fn modifier(stat: &str, kind: ModifierKind, value: f64) -> Modifier {
        Modifier::new(key(stat), kind, value).unwrap()
    }

    #[test]
    fn consistent_parts_are_accepted() {
        assert!(Catalog::check(&parts()).is_empty());
        let catalog = catalog();
        assert_eq!(catalog.stats().len(), 2);
        assert_eq!(catalog.bases()[1].name(), "base 2");
        assert!(catalog.stat(&key("mana")).is_some());
        assert!(catalog.stat(&key("rage")).is_none());
        assert!(catalog.drop_table(DropTableId::from(1)).is_some());
        assert!(catalog.drop_table(DropTableId::from(2)).is_none());
        assert!(catalog.progression().is_none());
    }

    #[test]
    fn duplicate_stat() {
        let mut p = parts();
        p.stats.push(stat_def("life", 1.0));
        assert_eq!(
            Catalog::check(&p),
            [CatalogError::DuplicateStat { index: 2 }]
        );
    }

    #[test]
    fn duplicate_base() {
        let mut p = parts();
        p.bases.push(base(1));
        assert_eq!(
            Catalog::check(&p),
            [CatalogError::DuplicateBase { index: 2 }]
        );
    }

    #[test]
    fn duplicate_rarity() {
        let mut p = parts();
        p.rarities = WeightedTable::new(vec![rarity(1), rarity(2), rarity(1)]).unwrap();
        assert_eq!(
            Catalog::check(&p),
            [CatalogError::DuplicateRarity { index: 2 }]
        );
    }

    #[test]
    fn duplicate_table() {
        let mut p = parts();
        p.drop_tables.push(table(1, 0.5, (1, 1), &[1]));
        assert_eq!(
            Catalog::check(&p),
            [CatalogError::DuplicateTable { index: 1 }]
        );
    }

    #[test]
    fn every_repeat_after_the_first_is_reported() {
        let mut p = parts();
        p.stats.push(stat_def("life", 1.0));
        p.stats.push(stat_def("life", 2.0));
        assert_eq!(
            Catalog::check(&p),
            [
                CatalogError::DuplicateStat { index: 2 },
                CatalogError::DuplicateStat { index: 3 },
            ]
        );
    }

    #[test]
    fn unknown_affix_stat() {
        let mut p = parts();
        p.affixes = AffixPool::new(vec![
            affix(1, "life"),
            affix(2, "rage"),
            affix(3, "mana"),
            affix(4, "fury"),
        ])
        .unwrap();
        assert_eq!(
            Catalog::check(&p),
            [
                CatalogError::UnknownAffixStat { affix_index: 1 },
                CatalogError::UnknownAffixStat { affix_index: 3 },
            ]
        );
    }

    #[test]
    fn unknown_table_base() {
        let mut p = parts();
        p.drop_tables = vec![
            table(1, 1.0, (1, 1), &[1, 2]),
            table(2, 1.0, (1, 1), &[2, 9, 1, 8]),
        ];
        assert_eq!(
            Catalog::check(&p),
            [
                CatalogError::UnknownTableBase {
                    table_index: 1,
                    entry_index: 1
                },
                CatalogError::UnknownTableBase {
                    table_index: 1,
                    entry_index: 3
                },
            ]
        );
    }

    #[test]
    fn check_returns_all_in_order_and_new_the_first() {
        let mut p = parts();
        // Added out of order on purpose: the result follows the groups.
        p.drop_tables.push(table(1, 1.0, (1, 1), &[7]));
        p.affixes = AffixPool::new(vec![affix(1, "rage")]).unwrap();
        p.rarities = WeightedTable::new(vec![rarity(1), rarity(1)]).unwrap();
        p.bases.push(base(2));
        p.stats.push(stat_def("mana", 0.0));
        let want = [
            CatalogError::DuplicateStat { index: 2 },
            CatalogError::DuplicateBase { index: 2 },
            CatalogError::DuplicateRarity { index: 1 },
            CatalogError::DuplicateTable { index: 1 },
            CatalogError::UnknownAffixStat { affix_index: 0 },
            CatalogError::UnknownTableBase {
                table_index: 1,
                entry_index: 0,
            },
        ];
        assert_eq!(Catalog::check(&p), want);
        assert_eq!(Catalog::new(p).unwrap_err(), want[0]);
    }

    #[test]
    fn roll_drop_unknown_table() {
        let mut rng = Rng::new(Seed::from(1));
        assert_eq!(
            catalog().roll_drop(DropTableId::from(9), lvl(1), &mut rng),
            Err(UnknownTable)
        );
        assert_eq!(rng, Rng::new(Seed::from(1)));
    }

    #[test]
    fn roll_drop_is_a_function_of_the_rng_state() {
        let catalog = catalog();
        let roll = |seed: u64| {
            catalog
                .roll_drop(
                    DropTableId::from(1),
                    lvl(5),
                    &mut Rng::new(Seed::from(seed)),
                )
                .unwrap()
        };
        assert_eq!(roll(9), roll(9));
    }

    #[test]
    fn roll_drop_gives_count_items() {
        let mut p = parts();
        p.drop_tables = vec![table(1, 1.0, (2, 2), &[1, 2])];
        let catalog = Catalog::new(p).unwrap();
        let mut rng = Rng::new(Seed::from(3));
        let items = catalog
            .roll_drop(DropTableId::from(1), lvl(1), &mut rng)
            .unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn roll_item_unknown_base() {
        let catalog = catalog();
        let mut rng = Rng::new(Seed::from(1));
        assert_eq!(
            catalog.roll_item(BaseId::from(9), lvl(1), &mut rng),
            Err(UnknownBase)
        );
        assert_eq!(rng, Rng::new(Seed::from(1)));
        let item = catalog
            .roll_item(BaseId::from(2), lvl(1), &mut rng)
            .unwrap();
        assert_eq!(*item.base(), BaseId::from(2));
    }

    #[test]
    fn evaluate_stats_in_the_order_of_keys() {
        let catalog = catalog();
        let set: ModifierSet = [
            modifier("life", ModifierKind::Added, 5.0),
            modifier("mana", ModifierKind::Increased, 0.5),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            catalog.evaluate_stats(&set, &[key("mana"), key("life")]),
            Ok(vec![30.0, 15.0])
        );
        assert_eq!(catalog.evaluate_stats(&set, &[]), Ok(vec![]));
    }

    #[test]
    fn evaluate_stats_unknown_stat() {
        let set = ModifierSet::new();
        assert_eq!(
            catalog().evaluate_stats(&set, &[key("life"), key("rage"), key("fury")]),
            Err(EvaluateStatsError::UnknownStat { key_index: 1 })
        );
    }

    #[test]
    fn evaluate_stats_unknown_modifier_stat() {
        let set: ModifierSet = [
            modifier("life", ModifierKind::Added, 1.0),
            modifier("rage", ModifierKind::Added, 1.0),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            catalog().evaluate_stats(&set, &[key("life")]),
            Err(EvaluateStatsError::UnknownModifierStat { modifier_index: 1 })
        );
    }

    #[test]
    fn evaluate_stats_a_modifier_outside_the_keys_is_fine() {
        let set: ModifierSet = [modifier("mana", ModifierKind::Added, 1.0)]
            .into_iter()
            .collect();
        assert_eq!(
            catalog().evaluate_stats(&set, &[key("life")]),
            Ok(vec![10.0])
        );
    }

    #[test]
    fn evaluate_stats_unknown_key_comes_before_unknown_modifier() {
        let set: ModifierSet = [modifier("rage", ModifierKind::Added, 1.0)]
            .into_iter()
            .collect();
        assert_eq!(
            catalog().evaluate_stats(&set, &[key("fury")]),
            Err(EvaluateStatsError::UnknownStat { key_index: 0 })
        );
    }

    #[test]
    fn evaluate_stats_overflow_comes_last_and_names_the_first_key() {
        let mut p = parts();
        p.stats = vec![
            stat_def("life", 1e308),
            stat_def("mana", 1e308),
            stat_def("rage", 0.0),
        ];
        p.affixes = AffixPool::new(vec![]).unwrap();
        let catalog = Catalog::new(p).unwrap();
        let set: ModifierSet = [
            modifier("life", ModifierKind::Added, 1e308),
            modifier("mana", ModifierKind::Added, 1e308),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            catalog.evaluate_stats(&set, &[key("rage"), key("mana"), key("life")]),
            Err(EvaluateStatsError::Overflow { key_index: 1 })
        );
    }
}

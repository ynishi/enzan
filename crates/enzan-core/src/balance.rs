//! Measuring a game's balance: what a [`Catalog`] gives over many seeded
//! trials, and what its levelling rule asks for.
//!
//! The functions here run the rules of the core and count what comes out. They
//! add no rule of their own, so a number in a report is what the game itself
//! would have produced for the same seeds. They read no clock and no OS source
//! of randomness: the seed comes in as an argument, and the same catalog, the
//! same arguments and the same seed give the same report, bit for bit,
//! natively and in WebAssembly.

use std::collections::BTreeMap;

use crate::catalog::{Catalog, DropTableId};
use crate::curve::Level;
use crate::loot::{AffixId, RarityId};
use crate::progression::{Experience, GainError, Progress, Progression};
use crate::rng::{Rng, Seed};

/// Why a [`Trials`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TrialsError {
    /// The number is 0 or above the largest allowed.
    #[error("the number of trials must be from 1 to 1,000,000")]
    OutOfRange,
}

/// How many trials a measurement runs: from 1 to 1,000,000.
///
/// A [`Summary`] needs every value of an affix to find its percentiles, so a
/// measurement keeps them all. The upper bound is what that costs in memory:
/// the number of values is about the number of trials times the affixes per
/// trial, and 1,000,000 trials keep that within what a browser tab can hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Trials(u32);

impl Trials {
    /// The most trials a measurement runs. Front ends read the limit from
    /// here rather than writing the number again.
    pub const MAX: u32 = 1_000_000;

    /// The number of trials.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for Trials {
    type Error = TrialsError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if (1..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(TrialsError::OutOfRange)
        }
    }
}

/// Why [`measure_loot`] gave no report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MeasureError {
    /// The catalog has no drop table with the id asked for.
    #[error("the catalog has no drop table with this id")]
    UnknownTable,
    /// A count does not fit a `u32`, or a sum of values is not finite.
    #[error("a count or a sum left its range")]
    Overflow,
}

/// How many times each whole number came up.
///
/// A count is at most the number of items in the report it belongs to, and
/// [`measure_loot`] checks that number fits a `u32`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Histogram(BTreeMap<u32, u32>);

impl Histogram {
    /// The `(value, count)` pairs, from the lowest value to the highest. A
    /// value that never came up is not listed.
    pub fn iter(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.0.iter().map(|(&value, &count)| (value, count))
    }

    /// Counts one more `value`. The caller keeps the count within a `u32`.
    fn add(&mut self, value: u32) {
        *self.0.entry(value).or_insert(0) += 1;
    }
}

/// The spread of a list of numbers.
///
/// The numbers are put in order by [`f64::total_cmp`]. The percentile `k` is
/// the value at position `(n * k + 99) / 100 - 1` (counted from 0, in integer
/// arithmetic; the rounding up of `n * k / 100`) of the `n` numbers: the
/// nearest-rank rule, which always gives one
/// of the numbers and not a value between two. It is written in integers so that
/// a product that is off at the last bit of an `f64` cannot move the position.
/// The mean is the sum taken in the order the numbers came up, divided by `n`;
/// [`measure_loot`] fails when that sum is not finite, since an infinity would
/// otherwise be reported as a mean. There is no summary of no numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    min: f64,
    mean: f64,
    p50: f64,
    p90: f64,
    p99: f64,
    max: f64,
}

impl Summary {
    /// The summary of `values`, or `None` when there are none. `values` is left
    /// sorted.
    fn of(values: &mut [f64]) -> Result<Option<Self>, MeasureError> {
        if values.is_empty() {
            return Ok(None);
        }
        let n = values.len() as u64;
        // In the order the values came up, so before the sort.
        let mut sum = 0.0_f64;
        for &v in values.iter() {
            sum += v;
            if !sum.is_finite() {
                return Err(MeasureError::Overflow);
            }
        }
        values.sort_by(f64::total_cmp);
        let rank = |k: u64| {
            let position = (n * k).div_ceil(100) - 1;
            values[usize::try_from(position)
                .expect("the position is below n, which is the length of a slice")]
        };
        Ok(Some(Self {
            min: values[0],
            mean: sum / n as f64,
            p50: rank(50),
            p90: rank(90),
            p99: rank(99),
            max: values[values.len() - 1],
        }))
    }

    /// The smallest value.
    #[must_use]
    pub fn min(&self) -> f64 {
        self.min
    }

    /// The mean.
    #[must_use]
    pub fn mean(&self) -> f64 {
        self.mean
    }

    /// The 50th percentile.
    #[must_use]
    pub fn p50(&self) -> f64 {
        self.p50
    }

    /// The 90th percentile.
    #[must_use]
    pub fn p90(&self) -> f64 {
        self.p90
    }

    /// The 99th percentile.
    #[must_use]
    pub fn p99(&self) -> f64 {
        self.p99
    }

    /// The largest value.
    #[must_use]
    pub fn max(&self) -> f64 {
        self.max
    }
}

/// How often an affix rolled, and the spread of the values it rolled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AffixStats {
    count: u32,
    value: Option<Summary>,
}

impl AffixStats {
    /// How many items carried the affix.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.count
    }

    /// The spread of the values, or `None` when the affix never rolled.
    #[must_use]
    pub fn value(&self) -> Option<Summary> {
        self.value
    }
}

/// What [`measure_loot`] counted.
///
/// The rarities and the affixes of the catalog are all listed, with a count of 0
/// for one that never came up, so a report always has the same keys for the
/// same catalog and an absent entry cannot be mistaken for one that was not
/// measured.
#[derive(Clone, Debug, PartialEq)]
pub struct LootReport {
    trials: u32,
    items: u32,
    drops: Histogram,
    rarities: BTreeMap<RarityId, u32>,
    affix_count: Histogram,
    affixes: BTreeMap<AffixId, AffixStats>,
}

impl LootReport {
    /// How many trials ran.
    #[must_use]
    pub fn trials(&self) -> u32 {
        self.trials
    }

    /// How many items dropped over all trials.
    #[must_use]
    pub fn items(&self) -> u32 {
        self.items
    }

    /// How many items one trial dropped, and in how many trials.
    #[must_use]
    pub fn drops(&self) -> &Histogram {
        &self.drops
    }

    /// How many items were of each rarity.
    #[must_use]
    pub fn rarities(&self) -> &BTreeMap<RarityId, u32> {
        &self.rarities
    }

    /// How many affixes one item carried, and how many items did.
    #[must_use]
    pub fn affix_count(&self) -> &Histogram {
        &self.affix_count
    }

    /// For each affix, how many items carried it and the spread of its values.
    #[must_use]
    pub fn affixes(&self) -> &BTreeMap<AffixId, AffixStats> {
        &self.affixes
    }
}

/// Runs `trials` defeats against the drop table `table` and counts what dropped.
///
/// Trial `i` rolls [`Catalog::roll_drop`] on `Rng::stream(base_seed, i)`, so a
/// trial depends on `(base_seed, i)` and on nothing that came before it: it can
/// be rerun alone, and the trials can be counted in any grouping without
/// changing what they drew. The counts and sums are taken in the order of the
/// trials and every map is ordered by id, so the report does not depend on the
/// platform. The same catalog, arguments and seed give the same report, bit for
/// bit, natively and in WebAssembly.
///
/// Every count is at most the number of items, and that is checked to fit a
/// `u32` as the items are added; a count taken from it needs no check of its own.
///
/// # Errors
///
/// [`MeasureError::UnknownTable`] when the catalog has no table `table`, checked
/// before any trial runs. [`MeasureError::Overflow`] when the items do not fit a
/// `u32`, or the sum of the values of an affix is not finite.
pub fn measure_loot(
    catalog: &Catalog,
    table: DropTableId,
    item_level: Level,
    base_seed: Seed,
    trials: Trials,
) -> Result<LootReport, MeasureError> {
    let drop_table = catalog
        .drop_table(table)
        .ok_or(MeasureError::UnknownTable)?;
    let mut items = 0u32;
    let mut drops = Histogram::default();
    let mut rarities: BTreeMap<RarityId, u32> = catalog
        .rarities()
        .entries()
        .iter()
        .map(|(rarity, _)| (rarity.id(), 0))
        .collect();
    let mut affix_count = Histogram::default();
    let mut values: BTreeMap<AffixId, Vec<f64>> = catalog
        .affixes()
        .affixes()
        .iter()
        .map(|affix| (affix.id(), Vec::new()))
        .collect();

    for i in 0..trials.get() {
        let dropped = catalog.loot_trial(
            drop_table,
            item_level,
            &mut Rng::stream(base_seed, u64::from(i)),
        );
        let n = u32::try_from(dropped.len()).map_err(|_| MeasureError::Overflow)?;
        items = items.checked_add(n).ok_or(MeasureError::Overflow)?;
        drops.add(n);
        for item in &dropped {
            *rarities.entry(item.rarity()).or_insert(0) += 1;
            let carried =
                u32::try_from(item.affixes().len()).map_err(|_| MeasureError::Overflow)?;
            affix_count.add(carried);
            for affix in item.affixes() {
                values
                    .entry(affix.id())
                    .or_default()
                    .push(affix.modifier().value());
            }
        }
    }

    let mut affixes = BTreeMap::new();
    for (id, mut list) in values {
        let count = u32::try_from(list.len()).map_err(|_| MeasureError::Overflow)?;
        let value = Summary::of(&mut list)?;
        affixes.insert(id, AffixStats { count, value });
    }
    Ok(LootReport {
        trials: trials.get(),
        items,
        drops,
        rarities,
        affix_count,
        affixes,
    })
}

/// What [`measure_progression`] found, in the order of its inputs.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressionReport {
    at_experience: Vec<(Experience, Progress)>,
    to_level: Vec<(Level, Option<Experience>)>,
}

impl ProgressionReport {
    /// Each amount asked about, with the [`Progress`] of a character at level 1
    /// with no experience after gaining it once.
    #[must_use]
    pub fn at_experience(&self) -> &[(Experience, Progress)] {
        &self.at_experience
    }

    /// Each level asked about, with the experience a character at level 1 needs
    /// to reach it, or `None` when it is above the cap.
    #[must_use]
    pub fn to_level(&self) -> &[(Level, Option<Experience>)] {
        &self.to_level
    }
}

/// What `rule` does with the amounts in `experience` and asks for the `levels`.
///
/// Both come from the rule itself: [`Progression::gain`] from level 1 for each
/// amount, and [`Progression::experience_to_reach`] for each level, so the
/// report and the game cannot disagree.
///
/// # Errors
///
/// The errors of [`Progression::gain`] and
/// [`Progression::experience_to_reach`].
pub fn measure_progression(
    rule: &Progression,
    experience: &[Experience],
    levels: &[Level],
) -> Result<ProgressionReport, GainError> {
    let start = Progress::new(first_level(), Experience::ZERO);
    let at_experience = experience
        .iter()
        .map(|&amount| Ok((amount, rule.gain(start, amount)?.progress())))
        .collect::<Result<Vec<_>, GainError>>()?;
    let to_level = levels
        .iter()
        .map(|&level| Ok((level, rule.experience_to_reach(level)?)))
        .collect::<Result<Vec<_>, GainError>>()?;
    Ok(ProgressionReport {
        at_experience,
        to_level,
    })
}

fn first_level() -> Level {
    Level::try_from(1).expect("1 is a level")
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU32;

    use super::*;
    use crate::catalog::{BaseDef, BaseId, CatalogParts, DropTableDef};
    use crate::curve::Curve;
    use crate::loot::{
        AffixDef, AffixGroup, AffixKind, AffixPool, AffixSpec, Chance, CountRange, DropTable,
        Rarity,
    };
    use crate::progression::CapPolicy;
    use crate::rng::WeightedTable;
    use crate::stat::{Modifier, ModifierKind, StatDef, StatKey};

    fn nz(n: u32) -> NonZeroU32 {
        NonZeroU32::new(n).unwrap()
    }

    fn lvl(n: u32) -> Level {
        Level::try_from(n).unwrap()
    }

    fn trials(n: u32) -> Trials {
        Trials::try_from(n).unwrap()
    }

    fn xp(v: f64) -> Experience {
        Experience::try_from(v).unwrap()
    }

    fn key(s: &str) -> StatKey {
        StatKey::try_from(s).unwrap()
    }

    fn affix(id: u32, required_level: u32, low: f64, high: f64) -> AffixDef {
        let at = |value| Modifier::new(key("life"), ModifierKind::Added, value).unwrap();
        AffixDef::new(AffixSpec {
            id: AffixId::from(id),
            kind: AffixKind::Prefix,
            tier: 1,
            required_level: lvl(required_level),
            weight: nz(1),
            group: AffixGroup::from(id),
            low: at(low),
            high: at(high),
        })
        .unwrap()
    }

    fn rarity(id: u32, min: u32, max: u32) -> (Rarity, NonZeroU32) {
        (
            Rarity::new(RarityId::from(id), CountRange::new(min, max).unwrap(), 3, 3),
            nz(id),
        )
    }

    fn catalog(
        chance: f64,
        count: (u32, u32),
        rarities: Vec<(Rarity, NonZeroU32)>,
        affixes: Vec<AffixDef>,
    ) -> Catalog {
        Catalog::new(CatalogParts {
            stats: vec![StatDef::new(key("life"), 0.0, crate::stat::Bounds::UNBOUNDED).unwrap()],
            bases: vec![
                BaseDef::new(BaseId::from(1), "sword".to_string()),
                BaseDef::new(BaseId::from(2), "axe".to_string()),
            ],
            rarities: WeightedTable::new(rarities).unwrap(),
            affixes: AffixPool::new(affixes).unwrap(),
            drop_tables: vec![DropTableDef::new(
                DropTableId::from(1),
                DropTable::new(
                    Chance::try_from(chance).unwrap(),
                    CountRange::new(count.0, count.1).unwrap(),
                    WeightedTable::new(vec![(BaseId::from(1), nz(1)), (BaseId::from(2), nz(2))])
                        .unwrap(),
                ),
            )],
            progression: None,
        })
        .unwrap()
    }

    /// A catalog whose reports vary: a chance, a range of counts, rarities that
    /// carry different numbers of affixes and affixes with ranges.
    fn varied() -> Catalog {
        catalog(
            0.7,
            (0, 3),
            vec![rarity(1, 0, 0), rarity(2, 1, 2), rarity(3, 2, 3)],
            vec![
                affix(1, 1, 1.0, 10.0),
                affix(2, 1, 5.0, 6.0),
                affix(3, 1, 0.5, 2.5),
                affix(4, 1, 20.0, 40.0),
            ],
        )
    }

    /// The id of the one drop table `catalog` builds.
    fn table_id() -> DropTableId {
        DropTableId::from(1)
    }

    fn histogram(h: &Histogram) -> Vec<(u32, u32)> {
        h.iter().collect()
    }

    #[test]
    fn same_seed_same_report() {
        let c = varied();
        let run = |seed: u64| {
            measure_loot(&c, table_id(), lvl(10), Seed::from(seed), trials(500)).unwrap()
        };
        assert_eq!(run(7), run(7));
        assert_ne!(run(7), run(8));
    }

    #[test]
    fn trial_zero_is_rerun_alone() {
        let c = varied();
        let seed = Seed::from(99);
        let report = measure_loot(&c, table_id(), lvl(10), seed, trials(1)).unwrap();
        let items = c
            .roll_drop(table_id(), lvl(10), &mut Rng::stream(seed, 0))
            .unwrap();
        let n = u32::try_from(items.len()).unwrap();
        assert_eq!(report.trials(), 1);
        assert_eq!(report.items(), n);
        assert_eq!(histogram(report.drops()), [(n, 1)]);
        let rarity_total: u32 = report.rarities().values().sum();
        assert_eq!(rarity_total, n);
        let carried: u32 = items
            .iter()
            .map(|i| u32::try_from(i.affixes().len()).unwrap())
            .sum();
        let affix_total: u32 = report.affixes().values().map(AffixStats::count).sum();
        assert_eq!(affix_total, carried);
    }

    #[test]
    fn known_configuration() {
        let c = catalog(
            1.0,
            (1, 1),
            vec![rarity(1, 1, 1)],
            vec![affix(1, 1, 5.0, 5.0)],
        );
        let report = measure_loot(&c, table_id(), lvl(1), Seed::from(1), trials(50)).unwrap();
        assert_eq!(report.trials(), 50);
        assert_eq!(report.items(), 50);
        assert_eq!(histogram(report.drops()), [(1, 50)]);
        assert_eq!(histogram(report.affix_count()), [(1, 50)]);
        assert_eq!(
            report
                .rarities()
                .iter()
                .map(|(k, v)| (k.get(), *v))
                .collect::<Vec<_>>(),
            [(1, 50)]
        );
        let stats = report.affixes()[&AffixId::from(1)];
        assert_eq!(stats.count(), 50);
        let s = stats.value().unwrap();
        for v in [s.min(), s.mean(), s.p50(), s.p90(), s.p99(), s.max()] {
            assert_eq!(v, 5.0);
        }
    }

    #[test]
    fn what_never_came_up_is_listed_with_zero() {
        // The second affix needs level 100, so it cannot roll at level 1.
        let c = catalog(
            1.0,
            (1, 1),
            vec![rarity(1, 1, 1)],
            vec![affix(1, 1, 5.0, 5.0), affix(2, 100, 1.0, 2.0)],
        );
        let report = measure_loot(&c, table_id(), lvl(1), Seed::from(1), trials(20)).unwrap();
        let never = report.affixes()[&AffixId::from(2)];
        assert_eq!(never.count(), 0);
        assert_eq!(never.value(), None);

        // With no drop at all, the rarities are listed with 0 too.
        let c = catalog(
            0.0,
            (1, 1),
            vec![rarity(1, 1, 1), rarity(2, 1, 1)],
            vec![affix(1, 1, 5.0, 5.0)],
        );
        let report = measure_loot(&c, table_id(), lvl(1), Seed::from(1), trials(20)).unwrap();
        assert_eq!(report.items(), 0);
        assert_eq!(histogram(report.drops()), [(0, 20)]);
        assert!(histogram(report.affix_count()).is_empty());
        assert_eq!(
            report
                .rarities()
                .iter()
                .map(|(k, v)| (k.get(), *v))
                .collect::<Vec<_>>(),
            [(1, 0), (2, 0)]
        );
        assert_eq!(report.affixes()[&AffixId::from(1)].value(), None);
    }

    #[test]
    fn unknown_table() {
        assert_eq!(
            measure_loot(
                &varied(),
                DropTableId::from(9),
                lvl(1),
                Seed::from(1),
                trials(1)
            ),
            Err(MeasureError::UnknownTable)
        );
    }

    #[test]
    fn summary_nearest_rank() {
        // Out of order, so the sort is what puts them right.
        let mut values: Vec<f64> = (1..=100).rev().map(f64::from).collect();
        let s = Summary::of(&mut values).unwrap().unwrap();
        assert_eq!(s.min(), 1.0);
        assert_eq!(s.p50(), 50.0);
        assert_eq!(s.p90(), 90.0);
        assert_eq!(s.p99(), 99.0);
        assert_eq!(s.max(), 100.0);
        assert_eq!(s.mean(), 50.5);

        let s = Summary::of(&mut [4.5]).unwrap().unwrap();
        for v in [s.min(), s.mean(), s.p50(), s.p90(), s.p99(), s.max()] {
            assert_eq!(v, 4.5);
        }
        assert_eq!(Summary::of(&mut []), Ok(None));
    }

    #[test]
    fn summary_sum_must_be_finite() {
        assert_eq!(
            Summary::of(&mut [1e308, 1e308]),
            Err(MeasureError::Overflow)
        );
    }

    #[test]
    fn trials_range() {
        assert_eq!(Trials::try_from(0), Err(TrialsError::OutOfRange));
        assert_eq!(Trials::try_from(1_000_001), Err(TrialsError::OutOfRange));
        assert_eq!(Trials::try_from(1).unwrap().get(), 1);
        assert_eq!(Trials::try_from(1_000_000).unwrap().get(), 1_000_000);
    }

    #[test]
    fn progression_report() {
        let curve = Curve::table(vec![10.0, 20.0, 30.0]).unwrap();
        let rule = Progression::new(curve, lvl(4), CapPolicy::Discard).unwrap();
        let report = measure_progression(
            &rule,
            &[xp(35.0)],
            &[lvl(1), lvl(2), lvl(3), lvl(4), lvl(5)],
        )
        .unwrap();
        assert_eq!(
            report.at_experience(),
            [(xp(35.0), Progress::new(lvl(3), xp(5.0)))]
        );
        assert_eq!(
            report.to_level(),
            [
                (lvl(1), Some(xp(0.0))),
                (lvl(2), Some(xp(10.0))),
                (lvl(3), Some(xp(30.0))),
                (lvl(4), Some(xp(60.0))),
                (lvl(5), None),
            ]
        );
    }

    #[test]
    fn to_level_is_enough_for_gain() {
        // Adding these values rounds differently from subtracting them in
        // `gain`, so the plain sum falls short of level 8.
        let curve = Curve::table(vec![0.01, 1.1, 0.7, 0.7, 0.3, 1.1, 0.7]).unwrap();
        let rule = Progression::new(curve, lvl(8), CapPolicy::Keep).unwrap();
        let levels: Vec<Level> = (1..=8).map(lvl).collect();
        let report = measure_progression(&rule, &[], &levels).unwrap();
        let start = Progress::new(lvl(1), Experience::ZERO);
        for &(level, need) in report.to_level() {
            let reached = rule.gain(start, need.unwrap()).unwrap().progress().level();
            assert!(reached >= level, "{level:?} not reached with {need:?}");
        }
    }

    #[test]
    fn progression_sum_must_be_finite() {
        let curve = Curve::table(vec![f64::MAX, f64::MAX]).unwrap();
        let rule = Progression::new(curve, lvl(3), CapPolicy::Keep).unwrap();
        assert_eq!(
            measure_progression(&rule, &[], &[lvl(3)]),
            Err(GainError::Overflow)
        );
        assert!(measure_progression(&rule, &[], &[lvl(2)]).is_ok());
    }
}

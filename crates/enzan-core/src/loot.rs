//! Drops and affixes: what falls, and which modifiers an item carries.
//!
//! A [`DropTable`] decides whether something drops, how many bases, and which.
//! [`roll_item`] draws a [`Rarity`] and then the affixes of an [`AffixPool`]
//! that fit it, giving an [`ItemRoll`]. Both draw from a caller's [`Rng`], so a
//! seed replays a drop.
//!
//! # Differences from the issue and from core-design
//!
//! - There is no `LootError`. An inconsistency that can occur (an affix with a
//!   wrong range, a repeated id, weights that overflow) is returned when the
//!   data is built ([`AffixDefError`], [`AffixPoolError`]), so a roll does not
//!   fail.
//! - The item level is the [`Level`] of the `curve` module.
//! - The range of an affix value is kept as the two [`Modifier`]s at its ends,
//!   so the stat and the kind are not copied beside them.
//!
//! # Order of draws
//!
//! The order in which random numbers are consumed is part of the contract, as
//! the sequence is in [`rng`](crate::rng). Changing it is a breaking change:
//! raise the version and record it in the CHANGELOG.
//!
//! ```text
//! roll_item: rarity  = rarities.draw(rng)
//!            affixes = pool.roll(rarity, item_level, rng)
//! pool.roll: n = rarity.affix_count.draw(rng)  (min == max still consumes one
//!                                               below(1))
//!            n times: narrow the candidates
//!                     -> none left: stop (no further draw is made)
//!                     -> draw one with a WeightedTable
//!                     -> draw its value (one next_f64)
//! DropTable.roll: chance (next_f64() < p, one draw)
//!                 -> on a hit: count.draw(rng), then count times bases.draw(rng)
//! ```
//!
//! The caller decides in which order to call a [`DropTable`] and [`roll_item`]
//! on one [`Rng`], and the result depends on that order too.

use core::num::{NonZeroU32, NonZeroU64};
use std::collections::BTreeSet;

use crate::curve::Level;
use crate::rng::{Rng, WeightedTable, WeightedTableError, accumulate};
use crate::stat::Modifier;

/// Why a [`Chance`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ChanceError {
    /// The value is NaN, below 0 or above 1.
    #[error("a chance must be between 0 and 1")]
    OutOfRange,
}

/// A probability in `[0, 1]`.
///
/// `-0.0` becomes `+0.0`, so a chance of nothing has one form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chance(f64);

impl Chance {
    /// The probability.
    #[must_use]
    pub fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Chance {
    type Error = ChanceError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        // Written so that NaN fails both comparisons and lands in the error.
        if !(0.0..=1.0).contains(&value) {
            return Err(ChanceError::OutOfRange);
        }
        Ok(Self(if value == 0.0 { 0.0 } else { value }))
    }
}

/// Why a [`CountRange`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CountRangeError {
    /// `min` is greater than `max`.
    #[error("min must not be greater than max")]
    MinAboveMax,
}

/// A whole number drawn from `min` to `max`, both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CountRange {
    min: u32,
    max: u32,
}

impl CountRange {
    /// Builds the range `min..=max`.
    ///
    /// # Errors
    ///
    /// [`CountRangeError::MinAboveMax`] when `min > max`.
    pub fn new(min: u32, max: u32) -> Result<Self, CountRangeError> {
        if min > max {
            return Err(CountRangeError::MinAboveMax);
        }
        Ok(Self { min, max })
    }

    /// The smallest count.
    #[must_use]
    pub fn min(&self) -> u32 {
        self.min
    }

    /// The largest count.
    #[must_use]
    pub fn max(&self) -> u32 {
        self.max
    }

    /// Draws a count. It consumes a draw even when `min == max`, so the number
    /// of draws does not depend on the data.
    pub(crate) fn draw(&self, rng: &mut Rng) -> u32 {
        // The width is at most 2^32, which a u32 could not hold.
        let width = NonZeroU64::MIN.saturating_add(u64::from(self.max - self.min));
        let offset = u32::try_from(rng.below(width))
            .expect("below(width) is under max - min + 1, so it fits a u32");
        self.min + offset
    }
}

/// A chance to drop, and then bases drawn by weight.
///
/// See the module doc for the order of draws.
#[derive(Clone, Debug, PartialEq)]
pub struct DropTable<B> {
    chance: Chance,
    count: CountRange,
    bases: WeightedTable<B>,
}

impl<B> DropTable<B> {
    /// Joins parts that were each checked when they were built.
    #[must_use]
    pub fn new(chance: Chance, count: CountRange, bases: WeightedTable<B>) -> Self {
        Self {
            chance,
            count,
            bases,
        }
    }

    /// The chance that anything drops.
    #[must_use]
    pub fn chance(&self) -> Chance {
        self.chance
    }

    /// How many bases drop when something does.
    #[must_use]
    pub fn count(&self) -> CountRange {
        self.count
    }

    /// The bases and their weights.
    #[must_use]
    pub fn bases(&self) -> &WeightedTable<B> {
        &self.bases
    }

    /// Rolls the drop: nothing when the chance misses, otherwise `count` bases,
    /// each drawn on its own (the same base can repeat).
    ///
    /// The result does not reserve room for `count`: a count in the data is
    /// not trusted with a large allocation up front, and the vector grows as
    /// bases are drawn.
    pub fn roll(&self, rng: &mut Rng) -> Vec<&B> {
        let mut drops = Vec::new();
        if rng.next_f64() < self.chance.0 {
            for _ in 0..self.count.draw(rng) {
                drops.push(self.bases.draw(rng));
            }
        }
        drops
    }
}

/// Whether an affix is a prefix or a suffix. A [`Rarity`] limits each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AffixKind {
    /// Counts against [`Rarity::max_prefixes`].
    Prefix,
    /// Counts against [`Rarity::max_suffixes`].
    Suffix,
}

/// The number the game's data gives an affix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AffixId(u32);

impl AffixId {
    /// The raw number.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

impl From<u32> for AffixId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// The number of a group of affixes of which an item carries at most one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AffixGroup(u32);

impl AffixGroup {
    /// The raw number.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

impl From<u32> for AffixGroup {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// The number the game's data gives a rarity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RarityId(u32);

impl RarityId {
    /// The raw number.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

impl From<u32> for RarityId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// The parts of an [`AffixDef`], before they are checked.
///
/// It has no invariant of its own, so its fields are public; the rules are
/// checked by [`AffixDef::new`].
#[derive(Clone, Debug, PartialEq)]
pub struct AffixSpec {
    /// The id.
    pub id: AffixId,
    /// Prefix or suffix.
    pub kind: AffixKind,
    /// The tier, for display.
    pub tier: u32,
    /// The lowest item level the affix can appear on.
    pub required_level: Level,
    /// The weight in the draw among the candidates.
    pub weight: NonZeroU32,
    /// The group; an item carries at most one affix of a group.
    pub group: AffixGroup,
    /// The modifier at the low end of the value range.
    pub low: Modifier,
    /// The modifier at the high end of the value range.
    pub high: Modifier,
}

/// Why an [`AffixDef`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AffixDefError {
    /// `low` and `high` differ in stat or kind.
    #[error("low and high must have the same stat and kind")]
    ModifierMismatch,
    /// The value of `low` is above the value of `high`.
    #[error("the value of low must not be above the value of high")]
    MinAboveMax,
    /// `high - low` is not finite.
    #[error("the span between low and high must be finite")]
    SpanNotFinite,
}

/// An affix a pool can give: a range of one modifier, and the rules for when it
/// appears.
///
/// The tier is for display and takes no part in any rule.
///
/// A rolled value is `low + (high - low) * next_f64()`, in `[low, high]`. The
/// reasons it stays in the range, and in what R4 allows a modifier, are in
/// `Modifier::between`.
///
/// The checks run in this order and the first failure is returned: `low` and
/// `high` differ in stat or kind, the value of `low` is above that of `high`,
/// the span is not finite.
#[derive(Clone, Debug, PartialEq)]
pub struct AffixDef {
    id: AffixId,
    kind: AffixKind,
    tier: u32,
    required_level: Level,
    weight: NonZeroU32,
    group: AffixGroup,
    low: Modifier,
    high: Modifier,
}

impl AffixDef {
    /// Checks `spec` and builds the definition.
    ///
    /// # Errors
    ///
    /// Returns the first rule in the type's doc that `spec` breaks.
    pub fn new(spec: AffixSpec) -> Result<Self, AffixDefError> {
        let AffixSpec {
            id,
            kind,
            tier,
            required_level,
            weight,
            group,
            low,
            high,
        } = spec;
        if low.stat() != high.stat() || low.kind() != high.kind() {
            return Err(AffixDefError::ModifierMismatch);
        }
        if low.value() > high.value() {
            return Err(AffixDefError::MinAboveMax);
        }
        if !(high.value() - low.value()).is_finite() {
            return Err(AffixDefError::SpanNotFinite);
        }
        Ok(Self {
            id,
            kind,
            tier,
            required_level,
            weight,
            group,
            low,
            high,
        })
    }

    /// The id.
    #[must_use]
    pub fn id(&self) -> AffixId {
        self.id
    }

    /// Prefix or suffix.
    #[must_use]
    pub fn kind(&self) -> AffixKind {
        self.kind
    }

    /// The tier, for display.
    #[must_use]
    pub fn tier(&self) -> u32 {
        self.tier
    }

    /// The lowest item level the affix can appear on.
    #[must_use]
    pub fn required_level(&self) -> Level {
        self.required_level
    }

    /// The weight in the draw among the candidates.
    #[must_use]
    pub fn weight(&self) -> NonZeroU32 {
        self.weight
    }

    /// The group.
    #[must_use]
    pub fn group(&self) -> AffixGroup {
        self.group
    }

    /// The modifier at the low end of the value range.
    #[must_use]
    pub fn low(&self) -> &Modifier {
        &self.low
    }

    /// The modifier at the high end of the value range.
    #[must_use]
    pub fn high(&self) -> &Modifier {
        &self.high
    }
}

/// Why an [`AffixPool`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AffixPoolError {
    /// Two affixes have the same id.
    #[error("the affix id {} is used twice", id.get())]
    DuplicateId {
        /// The first id found twice.
        id: AffixId,
    },
    /// The weights of all affixes sum to more than `u64::MAX`.
    #[error("the affix weights sum to more than u64::MAX")]
    TotalOverflow,
}

/// The affixes an item can draw from, in the order they were given.
///
/// The order is part of the result for a given sequence, as in
/// [`WeightedTable`]. The pool may be empty.
///
/// The checks run in this order and the first failure is returned: from the
/// front, the first id found twice; then the sum of the weights. The sum is
/// checked here so that a roll, whose candidates are a part of the pool and
/// sum to no more, cannot overflow.
#[derive(Clone, Debug, PartialEq)]
pub struct AffixPool {
    affixes: Vec<AffixDef>,
}

impl AffixPool {
    /// Checks `affixes` and builds the pool.
    ///
    /// # Errors
    ///
    /// Returns the first rule in the type's doc that `affixes` breaks.
    pub fn new(affixes: Vec<AffixDef>) -> Result<Self, AffixPoolError> {
        let mut seen = BTreeSet::new();
        for a in &affixes {
            if !seen.insert(a.id) {
                return Err(AffixPoolError::DuplicateId { id: a.id });
            }
        }
        // The same sum a WeightedTable checks; an empty pool is allowed here.
        if let Err(WeightedTableError::TotalOverflow) =
            accumulate(affixes.iter().map(|a| NonZeroU64::from(a.weight)))
        {
            return Err(AffixPoolError::TotalOverflow);
        }
        Ok(Self { affixes })
    }

    /// The affixes in the order they were given.
    #[must_use]
    pub fn affixes(&self) -> &[AffixDef] {
        &self.affixes
    }

    /// Rolls the affixes of an item of `item_level` and `rarity`.
    ///
    /// R12: an affix is a candidate when, all at once, (a) `rarity` has room
    /// for its kind ([`Rarity`] counts the prefixes and suffixes chosen so
    /// far), (b) its required level is at most `item_level`, and (c) its group
    /// has not been chosen yet. Candidates are taken in the order of the pool.
    /// One is drawn by weight, then its value. The group of the chosen affix is
    /// then chosen too, so the same affix cannot come up twice. The chosen
    /// groups are kept in a `Vec` in the order they were chosen, as a hash set
    /// would read the OS for its seed.
    ///
    /// The number of affixes comes from [`Rarity`] (R13) and is fewer when
    /// candidates run out. The order of draws is in the module doc.
    pub fn roll(&self, rarity: &Rarity, item_level: Level, rng: &mut Rng) -> Vec<RolledAffix> {
        let n = rarity.affix_count.draw(rng);
        let mut rolled = Vec::new();
        let mut groups: Vec<AffixGroup> = Vec::new();
        let (mut prefixes, mut suffixes) = (0u32, 0u32);
        for _ in 0..n {
            let candidates: Vec<(usize, NonZeroU32)> = self
                .affixes
                .iter()
                .enumerate()
                .filter(|(_, a)| {
                    rarity.has_room(a.kind, prefixes, suffixes)
                        && a.required_level <= item_level
                        && !groups.contains(&a.group)
                })
                .map(|(i, a)| (i, a.weight))
                .collect();
            if candidates.is_empty() {
                break;
            }
            let table = WeightedTable::new(candidates).expect(
                "candidates are not empty (checked above) and are part of the pool, \
                 whose weights were checked to sum to at most u64::MAX",
            );
            let def = &self.affixes[*table.draw(rng)];
            let modifier = Modifier::between(&def.low, &def.high, rng.next_f64());
            match def.kind {
                AffixKind::Prefix => prefixes += 1,
                AffixKind::Suffix => suffixes += 1,
            }
            groups.push(def.group);
            rolled.push(RolledAffix {
                id: def.id,
                modifier,
            });
        }
        rolled
    }
}

/// A rarity: how many affixes an item gets, and how many of each kind it can
/// hold.
///
/// R13: the number of affixes `n` is drawn from `affix_count`. The number an
/// item gets can be fewer than `n`, because of the limits on prefixes and
/// suffixes and because the candidates run out. Running out is not an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rarity {
    id: RarityId,
    affix_count: CountRange,
    max_prefixes: u32,
    max_suffixes: u32,
}

impl Rarity {
    /// Joins the parts.
    #[must_use]
    pub fn new(
        id: RarityId,
        affix_count: CountRange,
        max_prefixes: u32,
        max_suffixes: u32,
    ) -> Self {
        Self {
            id,
            affix_count,
            max_prefixes,
            max_suffixes,
        }
    }

    /// The id.
    #[must_use]
    pub fn id(&self) -> RarityId {
        self.id
    }

    /// The range `n` is drawn from.
    #[must_use]
    pub fn affix_count(&self) -> CountRange {
        self.affix_count
    }

    /// The most prefixes an item can hold.
    #[must_use]
    pub fn max_prefixes(&self) -> u32 {
        self.max_prefixes
    }

    /// The most suffixes an item can hold.
    #[must_use]
    pub fn max_suffixes(&self) -> u32 {
        self.max_suffixes
    }

    /// Whether an item holding `prefixes` and `suffixes` can take one more of
    /// `kind`.
    pub(crate) fn has_room(&self, kind: AffixKind, prefixes: u32, suffixes: u32) -> bool {
        match kind {
            AffixKind::Prefix => prefixes < self.max_prefixes,
            AffixKind::Suffix => suffixes < self.max_suffixes,
        }
    }
}

/// An affix as rolled: which one, and the modifier with its value.
#[derive(Clone, Debug, PartialEq)]
pub struct RolledAffix {
    id: AffixId,
    modifier: Modifier,
}

impl RolledAffix {
    /// The id of the [`AffixDef`] it came from.
    #[must_use]
    pub fn id(&self) -> AffixId {
        self.id
    }

    /// The modifier, with the value rolled.
    #[must_use]
    pub fn modifier(&self) -> &Modifier {
        &self.modifier
    }
}

/// A base with the rarity and affixes rolled for it.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemRoll<B> {
    base: B,
    rarity: RarityId,
    affixes: Vec<RolledAffix>,
}

impl<B> ItemRoll<B> {
    /// The base.
    #[must_use]
    pub fn base(&self) -> &B {
        &self.base
    }

    /// The rarity drawn.
    #[must_use]
    pub fn rarity(&self) -> RarityId {
        self.rarity
    }

    /// The affixes in the order they were drawn.
    #[must_use]
    pub fn affixes(&self) -> &[RolledAffix] {
        &self.affixes
    }

    /// Gives back the base.
    #[must_use]
    pub fn into_base(self) -> B {
        self.base
    }
}

/// Rolls the rarity and affixes of `base`.
///
/// The order of draws is in the module doc.
pub fn roll_item<B>(
    base: B,
    rarities: &WeightedTable<Rarity>,
    pool: &AffixPool,
    item_level: Level,
    rng: &mut Rng,
) -> ItemRoll<B> {
    let rarity = rarities.draw(rng);
    let affixes = pool.roll(rarity, item_level, rng);
    ItemRoll {
        base,
        rarity: rarity.id,
        affixes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Seed;
    use crate::stat::{ModifierKind, StatKey};

    fn nz(n: u32) -> NonZeroU32 {
        NonZeroU32::new(n).unwrap()
    }

    fn lvl(n: u32) -> Level {
        Level::try_from(n).unwrap()
    }

    fn rng(seed: u64) -> Rng {
        Rng::new(Seed::from(seed))
    }

    fn md(stat: &str, kind: ModifierKind, value: f64) -> Modifier {
        Modifier::new(StatKey::try_from(stat).unwrap(), kind, value).unwrap()
    }

    fn spec(id: u32, kind: AffixKind, group: u32, low: f64, high: f64) -> AffixSpec {
        AffixSpec {
            id: AffixId::from(id),
            kind,
            tier: 1,
            required_level: lvl(1),
            weight: nz(10),
            group: AffixGroup::from(group),
            low: md("life", ModifierKind::Added, low),
            high: md("life", ModifierKind::Added, high),
        }
    }

    fn def(s: AffixSpec) -> AffixDef {
        AffixDef::new(s).unwrap()
    }

    fn pool(specs: Vec<AffixSpec>) -> AffixPool {
        AffixPool::new(specs.into_iter().map(def).collect()).unwrap()
    }

    fn rarity(min: u32, max: u32, prefixes: u32, suffixes: u32) -> Rarity {
        Rarity::new(
            RarityId::from(1),
            CountRange::new(min, max).unwrap(),
            prefixes,
            suffixes,
        )
    }

    fn ids(rolled: &[RolledAffix]) -> Vec<u32> {
        rolled.iter().map(|r| r.id().get()).collect()
    }

    use AffixKind::{Prefix, Suffix};

    #[test]
    fn chance_range() {
        for bad in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
            assert_eq!(Chance::try_from(bad), Err(ChanceError::OutOfRange));
        }
        assert!(Chance::try_from(-0.0).unwrap().get().is_sign_positive());
        assert_eq!(Chance::try_from(1.0).unwrap().get(), 1.0);
    }

    fn drop_table(chance: f64, min: u32, max: u32) -> DropTable<&'static str> {
        DropTable::new(
            Chance::try_from(chance).unwrap(),
            CountRange::new(min, max).unwrap(),
            WeightedTable::new(vec![("sword", nz(1)), ("axe", nz(3))]).unwrap(),
        )
    }

    #[test]
    fn chance_zero_never_hits_and_one_always_does() {
        let mut r = rng(1);
        let never = drop_table(0.0, 1, 1);
        let always = drop_table(1.0, 1, 1);
        for _ in 0..1000 {
            assert!(never.roll(&mut r).is_empty());
            assert_eq!(always.roll(&mut r).len(), 1);
        }
    }

    #[test]
    fn drop_table_count() {
        let t = drop_table(1.0, 2, 2);
        assert_eq!(t.roll(&mut rng(5)).len(), 2);
    }

    #[test]
    fn count_range_rejects_min_above_max() {
        assert_eq!(CountRange::new(3, 2), Err(CountRangeError::MinAboveMax));
    }

    #[test]
    fn count_range_draws_every_value_and_no_other() {
        let c = CountRange::new(2, 4).unwrap();
        let mut r = rng(9);
        let mut seen = [false; 3];
        for _ in 0..1000 {
            let x = c.draw(&mut r);
            assert!((2..=4).contains(&x));
            seen[(x - 2) as usize] = true;
        }
        assert_eq!(seen, [true; 3]);
    }

    #[test]
    fn count_range_single_value() {
        let c = CountRange::new(5, 5).unwrap();
        let mut r = rng(9);
        for _ in 0..100 {
            assert_eq!(c.draw(&mut r), 5);
        }
    }

    #[test]
    fn count_range_full_width_does_not_overflow() {
        let c = CountRange::new(0, u32::MAX).unwrap();
        let mut r = rng(9);
        for _ in 0..100 {
            let _ = c.draw(&mut r);
        }
    }

    #[test]
    fn affix_def_checks() {
        let mut s = spec(1, Prefix, 1, 1.0, 2.0);
        s.high = md("mana", ModifierKind::Added, 2.0);
        assert_eq!(AffixDef::new(s), Err(AffixDefError::ModifierMismatch));

        let mut s = spec(1, Prefix, 1, 1.0, 2.0);
        s.high = md("life", ModifierKind::Increased, 2.0);
        assert_eq!(AffixDef::new(s), Err(AffixDefError::ModifierMismatch));

        assert_eq!(
            AffixDef::new(spec(1, Prefix, 1, 3.0, 2.0)),
            Err(AffixDefError::MinAboveMax)
        );
        assert_eq!(
            AffixDef::new(spec(1, Prefix, 1, -f64::MAX, f64::MAX)),
            Err(AffixDefError::SpanNotFinite)
        );
    }

    #[test]
    fn pool_rejects_duplicate_id() {
        let a = def(spec(7, Prefix, 1, 1.0, 2.0));
        let b = def(spec(8, Prefix, 2, 1.0, 2.0));
        assert_eq!(
            AffixPool::new(vec![a.clone(), b, a]),
            Err(AffixPoolError::DuplicateId {
                id: AffixId::from(7)
            })
        );
    }

    #[test]
    fn pool_accepts_empty() {
        assert!(AffixPool::new(vec![]).unwrap().affixes().is_empty());
    }

    #[test]
    fn value_stays_in_range() {
        let p = pool(vec![spec(1, Prefix, 1, 10.0, 20.0)]);
        let ra = rarity(1, 1, 1, 0);
        let mut r = rng(3);
        for _ in 0..1000 {
            let v = p.roll(&ra, lvl(1), &mut r)[0].modifier().value();
            assert!((10.0..=20.0).contains(&v), "{v}");
        }
        let p = pool(vec![spec(1, Prefix, 1, 5.0, 5.0)]);
        for _ in 0..100 {
            assert_eq!(p.roll(&ra, lvl(1), &mut r)[0].modifier().value(), 5.0);
        }
    }

    #[test]
    fn required_level_is_respected() {
        let mut high = spec(2, Prefix, 2, 1.0, 2.0);
        high.required_level = lvl(10);
        let p = pool(vec![spec(1, Prefix, 1, 1.0, 2.0), high]);
        let ra = rarity(2, 2, 2, 0);
        for seed in 0..100 {
            assert_eq!(ids(&p.roll(&ra, lvl(9), &mut rng(seed))), [1]);
        }
        assert_eq!(p.roll(&ra, lvl(10), &mut rng(0)).len(), 2);
    }

    #[test]
    fn one_affix_per_group() {
        let p = pool(vec![
            spec(1, Prefix, 5, 1.0, 2.0),
            spec(2, Prefix, 5, 1.0, 2.0),
        ]);
        let ra = rarity(2, 2, 2, 0);
        for seed in 0..100 {
            assert_eq!(p.roll(&ra, lvl(1), &mut rng(seed)).len(), 1);
        }
    }

    #[test]
    fn same_affix_is_not_drawn_twice() {
        let p = pool(vec![spec(1, Prefix, 1, 1.0, 2.0)]);
        let ra = rarity(3, 3, 3, 0);
        assert_eq!(p.roll(&ra, lvl(1), &mut rng(1)).len(), 1);
    }

    #[test]
    fn kind_limits() {
        let p = pool(vec![
            spec(1, Prefix, 1, 1.0, 2.0),
            spec(2, Suffix, 2, 1.0, 2.0),
        ]);
        for seed in 0..50 {
            let no_prefix = rarity(2, 2, 0, 2);
            assert_eq!(ids(&p.roll(&no_prefix, lvl(1), &mut rng(seed))), [2]);
            let no_suffix = rarity(2, 2, 2, 0);
            assert_eq!(ids(&p.roll(&no_suffix, lvl(1), &mut rng(seed))), [1]);
        }
    }

    #[test]
    fn running_out_ends_early() {
        let p = pool(vec![
            spec(1, Prefix, 1, 1.0, 2.0),
            spec(2, Prefix, 2, 1.0, 2.0),
            spec(3, Prefix, 3, 1.0, 2.0),
        ]);
        let ra = rarity(4, 4, 2, 0);
        assert_eq!(p.roll(&ra, lvl(1), &mut rng(1)).len(), 2);
        let empty = pool(vec![]);
        assert!(empty.roll(&ra, lvl(1), &mut rng(1)).is_empty());
    }

    #[test]
    fn no_draw_after_running_out() {
        let s = spec(1, Prefix, 1, 1.0, 2.0);
        let weight = u64::from(s.weight.get());
        let p = pool(vec![s]);
        let ra = rarity(3, 3, 3, 0);
        let mut got = rng(77);
        let _ = p.roll(&ra, lvl(1), &mut got);
        // The count, then one affix drawn and valued; nothing for the two that
        // could not be filled.
        let mut want = rng(77);
        want.below(NonZeroU64::MIN);
        want.below(NonZeroU64::new(weight).unwrap());
        want.next_f64();
        assert_eq!(got, want);
    }

    #[test]
    fn pool_order_is_part_of_the_result() {
        let a = spec(1, Prefix, 1, 1.0, 2.0);
        let b = spec(2, Prefix, 2, 1.0, 2.0);
        let ra = rarity(1, 1, 1, 0);
        let fwd = pool(vec![a.clone(), b.clone()]);
        let rev = pool(vec![b, a]);
        assert_ne!(
            ids(&fwd.roll(&ra, lvl(1), &mut rng(1))),
            ids(&rev.roll(&ra, lvl(1), &mut rng(1)))
        );
    }

    fn fixture() -> (WeightedTable<Rarity>, AffixPool) {
        let rarities = WeightedTable::new(vec![
            (
                Rarity::new(RarityId::from(1), CountRange::new(1, 2).unwrap(), 1, 1),
                nz(3),
            ),
            (
                Rarity::new(RarityId::from(2), CountRange::new(3, 4).unwrap(), 2, 2),
                nz(1),
            ),
        ])
        .unwrap();
        let mut w = spec(4, Suffix, 3, 0.1, 0.3);
        w.low = md("speed", ModifierKind::Increased, 0.1);
        w.high = md("speed", ModifierKind::Increased, 0.3);
        w.weight = nz(5);
        let mut x = spec(3, Suffix, 2, 4.0, 8.0);
        x.required_level = lvl(5);
        let p = pool(vec![
            spec(1, Prefix, 1, 10.0, 20.0),
            spec(2, Prefix, 1, 30.0, 40.0),
            x,
            w,
        ]);
        (rarities, p)
    }

    #[test]
    fn roll_item_is_deterministic() {
        let (rarities, p) = fixture();
        let go = |seed| roll_item("sword", &rarities, &p, lvl(10), &mut rng(seed));
        assert_eq!(go(42), go(42));
        assert!((0..10).any(|s| go(s) != go(42)));
    }

    #[test]
    fn roll_item_reference() {
        // The output of this implementation, pinned to notice a change in the
        // order of draws or in the arithmetic. #5 compares native and WASM.
        let (rarities, p) = fixture();
        let item = roll_item("sword", &rarities, &p, lvl(10), &mut rng(1234567));
        let got: Vec<(u32, f64)> = item
            .affixes()
            .iter()
            .map(|a| (a.id().get(), a.modifier().value()))
            .collect();
        assert_eq!(*item.base(), "sword");
        assert_eq!(item.rarity().get(), 1);
        assert_eq!(got, [(2, 32.49007657382291)]);
    }

    #[test]
    fn pool_roll_reference_with_several_affixes() {
        // The output of this implementation, pinned like roll_item_reference,
        // with a rarity that takes up to three affixes so that the order of
        // the candidate draws is pinned too. #5 compares native and WASM.
        let (rarities, p) = fixture();
        let rare = &rarities.entries()[1].0;
        let got: Vec<(u32, f64)> = p
            .roll(rare, lvl(10), &mut rng(1234567))
            .iter()
            .map(|a| (a.id().get(), a.modifier().value()))
            .collect();
        assert_eq!(
            got,
            [
                (1, 15.322073040624193),
                (3, 7.558117962474332),
                (4, 0.21812952566240068)
            ]
        );
    }
}

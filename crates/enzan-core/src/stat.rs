//! Stats and the modifiers that change them.
//!
//! A [`StatDef`] says what a stat is: its key, its base value and the bounds of
//! its result. A [`ModifierSet`] holds [`Modifier`]s in the order they were
//! added, and [`ModifierSet::evaluate`] turns the two into a value. The formula
//! and its reasons are stated there.
//!
//! Values are `f64`. A rate is a decimal fraction (100% is `1.0`), and a
//! reduction (reduced, less) is a negative value.

use core::fmt;

/// Why a [`StatKey`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StatKeyError {
    /// The key is empty.
    #[error("a stat key must not be empty")]
    Empty,
    /// The key is longer than 64 bytes.
    #[error("a stat key must be at most 64 bytes")]
    TooLong,
    /// The first character is not `a-z`.
    #[error("a stat key must start with a-z")]
    InvalidStart,
    /// A character after the first is not `a-z`, `0-9` or `_`.
    #[error("a stat key may contain only a-z, 0-9 and _ (byte {index})")]
    InvalidChar {
        /// The byte position of the first invalid character.
        index: usize,
    },
}

/// The name of a stat, such as `life` or `fire_damage`.
///
/// A key is 1 to 64 bytes. The first character is `a-z`, and the rest are
/// `a-z`, `0-9` or `_`. Only ASCII is allowed, so a byte is a character. The
/// shape is fixed so a key reads the same in data, logs and code.
///
/// The checks run in this order and the first failure is returned: empty,
/// too long, invalid start, invalid character (the byte position of the first
/// one).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatKey(String);

impl StatKey {
    const MAX_LEN: usize = 64;

    /// The key as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(s: &str) -> Result<(), StatKeyError> {
        let bytes = s.as_bytes();
        if bytes.is_empty() {
            return Err(StatKeyError::Empty);
        }
        if bytes.len() > Self::MAX_LEN {
            return Err(StatKeyError::TooLong);
        }
        if !bytes[0].is_ascii_lowercase() {
            return Err(StatKeyError::InvalidStart);
        }
        match bytes
            .iter()
            .position(|b| !(b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_'))
        {
            Some(index) => Err(StatKeyError::InvalidChar { index }),
            None => Ok(()),
        }
    }
}

impl TryFrom<&str> for StatKey {
    type Error = StatKeyError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::validate(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for StatKey {
    type Error = StatKeyError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::validate(&value)?;
        Ok(Self(value))
    }
}

impl fmt::Display for StatKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for StatKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Why a [`Bounds`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BoundsError {
    /// `min` or `max` is NaN.
    #[error("a bound must not be NaN")]
    NotANumber,
    /// `min` is positive infinity.
    #[error("min must not be positive infinity")]
    MinIsPositiveInfinity,
    /// `max` is negative infinity.
    #[error("max must not be negative infinity")]
    MaxIsNegativeInfinity,
    /// `min` is greater than `max`.
    #[error("min must not be greater than max")]
    MinAboveMax,
}

/// The closed range a stat's result is held in.
///
/// Neither end is NaN, `min` is not positive infinity, `max` is not negative
/// infinity, and `min <= max`. Negative infinity as `min` means no lower
/// bound, and positive infinity as `max` means no upper bound. Those limits keep
/// the clamp from ever producing a non-finite result out of a finite value.
///
/// The checks run in this order and the first failure is returned: NaN,
/// `min` is positive infinity, `max` is negative infinity, `min` above `max`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    min: f64,
    max: f64,
}

impl Bounds {
    /// No lower bound and no upper bound.
    pub const UNBOUNDED: Self = Self {
        min: f64::NEG_INFINITY,
        max: f64::INFINITY,
    };

    /// Builds the range `min..=max`.
    ///
    /// # Errors
    ///
    /// Returns the first rule in the type's doc that the arguments break.
    pub fn new(min: f64, max: f64) -> Result<Self, BoundsError> {
        if min.is_nan() || max.is_nan() {
            return Err(BoundsError::NotANumber);
        }
        if min == f64::INFINITY {
            return Err(BoundsError::MinIsPositiveInfinity);
        }
        if max == f64::NEG_INFINITY {
            return Err(BoundsError::MaxIsNegativeInfinity);
        }
        if min > max {
            return Err(BoundsError::MinAboveMax);
        }
        Ok(Self { min, max })
    }

    /// The lower end; negative infinity when there is none.
    #[must_use]
    pub fn min(&self) -> f64 {
        self.min
    }

    /// The upper end; positive infinity when there is none.
    #[must_use]
    pub fn max(&self) -> f64 {
        self.max
    }
}

/// Why a [`StatDef`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StatDefError {
    /// The base value is NaN or infinite.
    #[error("the base value must be finite")]
    BaseNotFinite,
}

/// What a stat is: its key, its base value and the bounds of its result.
///
/// The result of [`ModifierSet::evaluate`] is clamped to `bounds`. The base may
/// lie outside the bounds; only the result is clamped.
///
/// The clamp does not make a stat stop at zero. When `base + added` is negative
/// and the Increased total is positive, the negative value grows more negative.
/// A stat that must not go below a floor says so in its bounds.
#[derive(Clone, Debug, PartialEq)]
pub struct StatDef {
    key: StatKey,
    base: f64,
    bounds: Bounds,
}

impl StatDef {
    /// Builds a stat definition.
    ///
    /// `base` must be finite: a NaN or infinite base would make every result
    /// a failure of arithmetic and not a value the game chose.
    ///
    /// # Errors
    ///
    /// [`StatDefError::BaseNotFinite`] when `base` is NaN or infinite.
    pub fn new(key: StatKey, base: f64, bounds: Bounds) -> Result<Self, StatDefError> {
        if !base.is_finite() {
            return Err(StatDefError::BaseNotFinite);
        }
        Ok(Self { key, base, bounds })
    }

    /// The key of the stat.
    #[must_use]
    pub fn key(&self) -> &StatKey {
        &self.key
    }

    /// The value before any modifier.
    #[must_use]
    pub fn base(&self) -> f64 {
        self.base
    }

    /// The range the result is held in.
    #[must_use]
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }
}

/// How a [`Modifier`] combines into a stat. See [`ModifierSet::evaluate`] for
/// the formula.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModifierKind {
    /// A flat amount added to the base.
    Added,
    /// A rate that adds up with the other Increased rates before it scales.
    Increased,
    /// A rate that multiplies in on its own.
    More,
}

/// Why a [`Modifier`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ModifierError {
    /// The value is NaN or infinite.
    #[error("a modifier value must be finite")]
    NotFinite,
    /// A More value is below -1.
    #[error("a More value must be at least -1")]
    MoreBelowMinusOne,
}

/// One change to one stat.
///
/// The value of a rate is a decimal fraction (100% is `1.0`), and a reduction
/// is negative.
///
/// The value must be finite, since a NaN or infinite value is not a number the
/// game chose. A More value must also be at least `-1`.
///
/// The two rates are not held to the same limit on purpose. Increased values
/// add up and the sum is cut at zero in [`ModifierSet::evaluate`], because
/// reductions that add up to more than 100% happen in ordinary builds and
/// should stop at nothing left. A More value multiplies by `1 + value` on its
/// own, and a single less above 100% has nothing it could mean; allowing it
/// would flip the sign of the result.
#[derive(Clone, Debug, PartialEq)]
pub struct Modifier {
    stat: StatKey,
    kind: ModifierKind,
    value: f64,
}

impl Modifier {
    /// Builds a modifier of `kind` on `stat`.
    ///
    /// # Errors
    ///
    /// [`ModifierError::NotFinite`] when `value` is NaN or infinite, and
    /// [`ModifierError::MoreBelowMinusOne`] when `kind` is
    /// [`ModifierKind::More`] and `value` is below `-1`.
    pub fn new(stat: StatKey, kind: ModifierKind, value: f64) -> Result<Self, ModifierError> {
        if !value.is_finite() {
            return Err(ModifierError::NotFinite);
        }
        if kind == ModifierKind::More && value < -1.0 {
            return Err(ModifierError::MoreBelowMinusOne);
        }
        Ok(Self { stat, kind, value })
    }

    /// The stat this modifier changes.
    #[must_use]
    pub fn stat(&self) -> &StatKey {
        &self.stat
    }

    /// How the value combines into the stat.
    #[must_use]
    pub fn kind(&self) -> ModifierKind {
        self.kind
    }

    /// The amount, or the rate as a decimal fraction.
    #[must_use]
    pub fn value(&self) -> f64 {
        self.value
    }

    /// The modifier a fraction `u` of the way from `low` to `high`, with the
    /// stat and kind of `low`.
    ///
    /// The value is `low + (high - low) * u`, held to `[low, high]` by two
    /// comparisons (not `f64::max`, `min` or `clamp`, which leave the sign of a
    /// zero unspecified).
    ///
    /// It builds the struct without [`Modifier::new`] because the result is
    /// already known to satisfy the rules of `new`. Both ends satisfy them (a
    /// value that is finite, and for More at least `-1`), and those rules are
    /// closed over an interval: every number between two numbers that pass also
    /// passes. When a rule that is not convex is added to `new`, this function
    /// must be reviewed.
    ///
    /// The interval is `[low, high]`: `high` itself can be returned, because
    /// `low + p` can round up to it (for example `low = 1e16`, `high = 1e16 + 2`
    /// and `u` near 1), but nothing beyond it: `u` is at most `1 - 2^-53`, so
    /// `fl((high - low) * u)` is at most the float just below `high - low`,
    /// rounding being monotonic. The comparisons are a second defence. `-0.0` is not returned: `x + y` is
    /// `-0.0` only when both are `-0.0`, and here either `low` or
    /// `p = (high - low) * u` is `+0.0`.
    ///
    /// The caller guarantees what the `debug_assert`s check: the same stat and
    /// kind, `low <= high`, a finite span, and `0 <= u < 1`.
    pub(crate) fn between(low: &Modifier, high: &Modifier, u: f64) -> Modifier {
        debug_assert!(low.stat == high.stat && low.kind == high.kind);
        debug_assert!(low.value <= high.value);
        debug_assert!((high.value - low.value).is_finite());
        debug_assert!((0.0..1.0).contains(&u));
        let v = low.value + (high.value - low.value) * u;
        let value = if v < low.value {
            low.value
        } else if v > high.value {
            high.value
        } else {
            v
        };
        Modifier {
            stat: low.stat.clone(),
            kind: low.kind,
            value,
        }
    }
}

/// Why [`ModifierSet::evaluate`] gave no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EvaluateError {
    /// A step of the calculation left the range `f64` can represent.
    #[error("the calculation left the range of f64")]
    Overflow,
}

/// Modifiers in the order they were added.
///
/// The sums and the product in [`ModifierSet::evaluate`] are taken in that
/// order. Order can change an `f64` result, so a caller that joins one set per
/// source joins them in a fixed order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModifierSet {
    modifiers: Vec<Modifier>,
}

impl ModifierSet {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a modifier after the ones already in the set.
    pub fn push(&mut self, modifier: Modifier) {
        self.modifiers.push(modifier);
    }

    /// The modifiers in the order they were added.
    #[must_use]
    pub fn modifiers(&self) -> &[Modifier] {
        &self.modifiers
    }

    /// The value of the stat `def` under the modifiers on its key.
    ///
    /// Modifiers on other stats are ignored. The ones on the key are taken in
    /// the order they were added:
    ///
    /// ```text
    /// added    = 0.0 + each Added value
    /// increase = 0.0 + each Increased value
    /// more     = 1.0 * (1.0 + value) for each More value
    /// flat     = base + added
    /// factor   = 1.0 + increase, or 0.0 if that is below 0.0
    /// raw      = flat * factor * more        (multiplied in this order)
    /// result   = raw clamped to the bounds of def
    /// ```
    ///
    /// `added`, `increase`, `more`, `flat` and `raw` must each be finite. A
    /// result of zero, including `-0.0` (from `raw` or from a bound of `-0.0`),
    /// becomes `+0.0`, so a result of nothing has one form.
    ///
    /// Increased is cut at zero on its own, before the product. An Increased
    /// total of -150% leaves a factor of 0 and not -0.5, which would turn a
    /// positive value negative.
    ///
    /// # Errors
    ///
    /// [`EvaluateError::Overflow`] when any of the finite checks above fails.
    /// Infinity and NaN are what a calculation produces after it leaves the range
    /// of `f64`; they are not values the game chose. The bounds are a rule
    /// the game chose, so they apply silently to a finite value. That is why
    /// an infinite `raw` is an error even when the upper bound is finite: it is
    /// not clamped. The finite check on each total comes before the cut and the
    /// clamp, because those would turn a total of negative infinity into zero
    /// and hide the failure.
    #[must_use = "the result may be an error"]
    pub fn evaluate(&self, def: &StatDef) -> Result<f64, EvaluateError> {
        let mut added = 0.0_f64;
        let mut increase = 0.0_f64;
        let mut more = 1.0_f64;
        for m in self.modifiers.iter().filter(|m| m.stat == def.key) {
            match m.kind {
                ModifierKind::Added => added += m.value,
                ModifierKind::Increased => increase += m.value,
                ModifierKind::More => more *= 1.0 + m.value,
            }
        }
        finite(added)?;
        finite(increase)?;
        finite(more)?;
        let flat = finite(def.base + added)?;
        // A comparison, not `f64::max`: `max` leaves the sign of a zero result
        // unspecified, and the core promises the same bits on every target.
        let factor = if 1.0 + increase < 0.0 {
            0.0
        } else {
            1.0 + increase
        };
        let raw = finite(flat * factor * more)?;
        let result = raw.clamp(def.bounds.min, def.bounds.max);
        // A bound of -0.0 can bring a negative zero back after the clamp.
        Ok(if result == 0.0 { 0.0 } else { result })
    }
}

/// `x` when it is finite, and [`EvaluateError::Overflow`] when it is not.
fn finite(x: f64) -> Result<f64, EvaluateError> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(EvaluateError::Overflow)
    }
}

impl Extend<Modifier> for ModifierSet {
    fn extend<T: IntoIterator<Item = Modifier>>(&mut self, iter: T) {
        self.modifiers.extend(iter);
    }
}

impl FromIterator<Modifier> for ModifierSet {
    fn from_iter<T: IntoIterator<Item = Modifier>>(iter: T) -> Self {
        Self {
            modifiers: Vec::from_iter(iter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &str) -> StatKey {
        StatKey::try_from(s).unwrap()
    }

    fn def(base: f64, bounds: Bounds) -> StatDef {
        StatDef::new(key("life"), base, bounds).unwrap()
    }

    fn m(stat: &str, kind: ModifierKind, value: f64) -> Modifier {
        Modifier::new(key(stat), kind, value).unwrap()
    }

    fn set(items: &[(ModifierKind, f64)]) -> ModifierSet {
        items.iter().map(|&(k, v)| m("life", k, v)).collect()
    }

    use ModifierKind::{Added, Increased, More};

    #[test]
    fn issue_example() {
        let d = def(1000.0, Bounds::UNBOUNDED);
        let s = set(&[(Increased, 2.0), (Increased, 1.0)]);
        assert_eq!(s.evaluate(&d), Ok(4000.0));
        let s = set(&[(Increased, 2.0), (Increased, 1.0), (More, 1.0)]);
        assert_eq!(s.evaluate(&d), Ok(8000.0));
    }

    #[test]
    fn added_goes_before_increased() {
        let d = def(10.0, Bounds::UNBOUNDED);
        assert_eq!(set(&[(Added, 5.0)]).evaluate(&d), Ok(15.0));
        assert_eq!(
            set(&[(Added, 5.0), (Increased, 0.5)]).evaluate(&d),
            Ok(22.5)
        );
    }

    #[test]
    fn more_multiplies() {
        let d = def(100.0, Bounds::UNBOUNDED);
        assert_eq!(set(&[(More, 0.5), (More, 0.5)]).evaluate(&d), Ok(225.0));
    }

    #[test]
    fn reductions() {
        let d = def(100.0, Bounds::UNBOUNDED);
        assert_eq!(set(&[(Increased, -0.5)]).evaluate(&d), Ok(50.0));
        assert_eq!(
            set(&[(Increased, -1.0), (Increased, -0.5)]).evaluate(&d),
            Ok(0.0)
        );
    }

    #[test]
    fn more_minus_one_is_positive_zero() {
        let d = def(100.0, Bounds::UNBOUNDED);
        let v = set(&[(More, -1.0)]).evaluate(&d).unwrap();
        assert_eq!(v, 0.0);
        assert!(v.is_sign_positive());
        assert_eq!(
            Modifier::new(key("life"), More, -1.5),
            Err(ModifierError::MoreBelowMinusOne)
        );
    }

    #[test]
    fn negative_flat_with_zero_factor_is_positive_zero() {
        let d = def(-10.0, Bounds::UNBOUNDED);
        let v = set(&[(Increased, -1.0), (Increased, -1.0)])
            .evaluate(&d)
            .unwrap();
        assert_eq!(v, 0.0);
        assert!(v.is_sign_positive());
    }

    #[test]
    fn negative_zero_bound_gives_positive_zero() {
        let d = def(-5.0, Bounds::new(-0.0, 100.0).unwrap());
        let v = ModifierSet::new().evaluate(&d).unwrap();
        assert_eq!(v, 0.0);
        assert!(v.is_sign_positive());
        let d = def(5.0, Bounds::new(-10.0, -0.0).unwrap());
        let v = ModifierSet::new().evaluate(&d).unwrap();
        assert!(v.is_sign_positive());
    }

    #[test]
    fn clamps_the_result() {
        let b = Bounds::new(0.0, 100.0).unwrap();
        let d = def(50.0, b);
        assert_eq!(set(&[(Added, 100.0)]).evaluate(&d), Ok(100.0));
        assert_eq!(set(&[(Added, -100.0)]).evaluate(&d), Ok(0.0));
        assert_eq!(set(&[(Added, 10.0)]).evaluate(&d), Ok(60.0));
        let d = def(1e9, Bounds::UNBOUNDED);
        assert_eq!(set(&[(Added, 1.0)]).evaluate(&d), Ok(1e9 + 1.0));
    }

    #[test]
    fn base_outside_bounds_is_clamped_in_the_result() {
        let d = def(500.0, Bounds::new(0.0, 100.0).unwrap());
        assert_eq!(ModifierSet::new().evaluate(&d), Ok(100.0));
        assert_eq!(d.base(), 500.0);
    }

    #[test]
    fn other_stats_are_ignored() {
        let d = def(10.0, Bounds::UNBOUNDED);
        let mut s = ModifierSet::new();
        s.push(m("mana", Added, 99.0));
        s.push(m("life", Added, 1.0));
        s.extend([m("mana", More, 5.0)]);
        assert_eq!(s.modifiers().len(), 3);
        assert_eq!(s.evaluate(&d), Ok(11.0));
    }

    #[test]
    fn order_of_addition_is_kept() {
        let d = def(10.0, Bounds::UNBOUNDED);
        let a = set(&[(Added, 1e16), (Added, 1.0), (Added, -1e16)]);
        assert_eq!(a.evaluate(&d), Ok(10.0));
        let b = set(&[(Added, 1e16), (Added, -1e16), (Added, 1.0)]);
        assert_eq!(b.evaluate(&d), Ok(11.0));
    }

    #[test]
    fn overflow() {
        let d = def(0.0, Bounds::UNBOUNDED);
        let d1 = def(1.0, Bounds::UNBOUNDED);
        let cases = [
            (&d1, set(&[(Added, f64::MAX), (Added, f64::MAX)])),
            (&d1, set(&[(Increased, -f64::MAX), (Increased, -f64::MAX)])),
            (&d, set(&[(More, f64::MAX), (More, f64::MAX)])),
            (&d1, set(&[(More, f64::MAX), (More, f64::MAX)])),
            (
                &def(1e308, Bounds::new(0.0, 100.0).unwrap()),
                set(&[(Increased, 1e308)]),
            ),
        ];
        for (d, s) in &cases {
            assert_eq!(s.evaluate(d), Err(EvaluateError::Overflow));
        }
    }

    #[test]
    fn stat_key() {
        assert_eq!(StatKey::try_from(""), Err(StatKeyError::Empty));
        let long = "a".repeat(65);
        assert_eq!(StatKey::try_from(long.as_str()), Err(StatKeyError::TooLong));
        assert_eq!(StatKey::try_from("1abc"), Err(StatKeyError::InvalidStart));
        assert_eq!(StatKey::try_from("Abc"), Err(StatKeyError::InvalidStart));
        assert_eq!(
            StatKey::try_from("ab-c"),
            Err(StatKeyError::InvalidChar { index: 2 })
        );
        let long_bad = format!("a-{}", "a".repeat(63));
        assert_eq!(long_bad.len(), 65);
        assert_eq!(StatKey::try_from(long_bad), Err(StatKeyError::TooLong));
        assert!(StatKey::try_from("life").is_ok());
        assert!(StatKey::try_from("fire_damage").is_ok());
        assert!(StatKey::try_from("a").is_ok());
        let max = "a".repeat(64);
        let k = StatKey::try_from(max.clone()).unwrap();
        assert_eq!(k.as_ref(), max);
        assert_eq!(k.to_string(), max);
    }

    #[test]
    fn modifier_not_finite() {
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                Modifier::new(key("life"), Added, v),
                Err(ModifierError::NotFinite)
            );
        }
    }

    #[test]
    fn stat_def_base_not_finite() {
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                StatDef::new(key("life"), v, Bounds::UNBOUNDED),
                Err(StatDefError::BaseNotFinite)
            );
        }
    }

    #[test]
    fn bounds_rules() {
        let inf = f64::INFINITY;
        assert_eq!(Bounds::new(f64::NAN, 1.0), Err(BoundsError::NotANumber));
        assert_eq!(
            Bounds::new(inf, inf),
            Err(BoundsError::MinIsPositiveInfinity)
        );
        assert_eq!(
            Bounds::new(-inf, -inf),
            Err(BoundsError::MaxIsNegativeInfinity)
        );
        assert_eq!(Bounds::new(2.0, 1.0), Err(BoundsError::MinAboveMax));
        assert_eq!(
            Bounds::new(inf, -inf),
            Err(BoundsError::MinIsPositiveInfinity)
        );
        let b = Bounds::new(-inf, inf).unwrap();
        assert_eq!(b, Bounds::UNBOUNDED);
        assert_eq!((b.min(), b.max()), (-inf, inf));
    }

    #[test]
    fn between_ends_and_inside() {
        let top = 1.0 - f64::EPSILON / 2.0;
        let low = m("life", Added, 10.0);
        let high = m("life", Added, 20.0);
        assert_eq!(Modifier::between(&low, &high, 0.0).value(), 10.0);
        let v = Modifier::between(&low, &high, top).value();
        assert!((10.0..=20.0).contains(&v));
        let same = m("life", Increased, 3.5);
        let b = Modifier::between(&same, &same, 0.7);
        assert_eq!(b.value(), 3.5);
        assert_eq!((b.stat(), b.kind()), (same.stat(), Increased));
    }

    #[test]
    fn between_keeps_more_at_minus_one() {
        let low = m("life", More, -1.0);
        let high = m("life", More, 0.5);
        assert_eq!(Modifier::between(&low, &high, 0.0).value(), -1.0);
    }

    #[test]
    fn between_zero_is_positive() {
        let low = m("life", Added, 0.0);
        let high = m("life", Added, -0.0);
        assert!(
            Modifier::between(&low, &high, 0.5)
                .value()
                .is_sign_positive()
        );
    }
}

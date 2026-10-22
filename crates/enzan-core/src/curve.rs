//! Curves: a value for each [`Level`].
//!
//! A [`Curve`] maps a level to a number, so one shape serves both the
//! experience a level asks for and the strength of an enemy at that level.
//! There are three shapes: polynomial, exponential and a table.
//!
//! # Differences from the issue and from core-design
//!
//! [`Level`] lives here and not in `progression`. [`Curve::value`] takes a
//! `Level`, so putting it in `progression` would make `curve` depend on
//! `progression`, against the direction `progression` depends on `curve`.

use core::num::NonZeroU32;

/// Why a [`Level`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LevelError {
    /// The level is zero; levels start at 1.
    #[error("a level must be at least 1")]
    Zero,
}

/// A level, counted from 1.
///
/// Zero is not a level, so the type cannot hold it and no function that takes
/// a `Level` has to check for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Level(NonZeroU32);

impl Level {
    /// The level as a number, at least 1.
    #[must_use]
    pub fn get(&self) -> u32 {
        self.0.get()
    }

    /// The level after this one. At `u32::MAX` it stays at `u32::MAX`.
    pub(crate) fn saturating_next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl TryFrom<u32> for Level {
    type Error = LevelError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        NonZeroU32::new(value).map(Self).ok_or(LevelError::Zero)
    }
}

impl From<NonZeroU32> for Level {
    fn from(value: NonZeroU32) -> Self {
        Self(value)
    }
}

/// Why a [`Curve`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CurveError {
    /// There are no coefficients or no table values.
    #[error("a curve needs at least one value")]
    Empty,
    /// A coefficient or table value is NaN or infinite.
    #[error("the value at index {index} must be finite")]
    NotFinite {
        /// The position of the first such value. For
        /// [`Curve::exponential`], 0 is the base and 1 is the growth.
        index: usize,
    },
    /// A coefficient or table value is below zero.
    #[error("the value at index {index} must not be negative")]
    Negative {
        /// The position of the first such value.
        index: usize,
    },
    /// The base of an exponential curve is zero or below.
    #[error("the base must be greater than zero")]
    NonPositiveBase,
    /// The growth of an exponential curve is zero or below.
    #[error("the growth must be greater than zero")]
    NonPositiveGrowth,
}

/// Why [`Curve::value`] gave no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CurveValueError {
    /// A step of the calculation left the range `f64` can represent.
    #[error("the calculation left the range of f64")]
    Overflow,
    /// The level is beyond the end of the table.
    #[error("the level is beyond the end of the table")]
    LevelOutOfTable,
}

#[derive(Clone, Debug, PartialEq)]
enum Shape {
    Polynomial(Vec<f64>),
    Exponential { base: f64, growth: f64 },
    Table(Vec<f64>),
}

/// A value for each level, as a polynomial, an exponential or a table.
///
/// The value is finite and at least zero, or [`Curve::value`] returns an
/// error. Construction leaves no path to a negative value or to NaN: the
/// coefficients and table values are at least zero, and the base and growth
/// are above zero. [`CurveValueError::Overflow`] is not the true value being
/// large; it means a step on the way (the power `growth^(L - 1)` or a product)
/// cannot be held in an `f64`.
///
/// The power is [`libm::pow`]. `f64::powf` calls the platform's libm, so it can
/// differ between native and WebAssembly. A polynomial uses only addition and
/// multiplication. The output of `libm` is part of the compatibility of this
/// crate: its version is given with a caret, since pinning an exact version in
/// a library can break the dependency resolution of the crates downstream. A
/// test that pins the bits of a `pow` result detects a version that changes
/// it. A change of the values is a breaking change.
///
/// A `-0.0` coefficient or table value is stored as `+0.0`, so a curve has one
/// form for nothing. `-0.0 >= 0.0` is true and passes the checks, but the bits differ.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve(Shape);

/// `v` with a `-0.0` turned into `+0.0`.
pub(crate) fn normalize_zero(v: f64) -> f64 {
    if v == 0.0 { 0.0 } else { v }
}

fn validate_values(values: Vec<f64>) -> Result<Vec<f64>, CurveError> {
    if values.is_empty() {
        return Err(CurveError::Empty);
    }
    let mut out = Vec::with_capacity(values.len());
    for (index, v) in values.into_iter().enumerate() {
        if !v.is_finite() {
            return Err(CurveError::NotFinite { index });
        }
        if v < 0.0 {
            return Err(CurveError::Negative { index });
        }
        out.push(normalize_zero(v));
    }
    Ok(out)
}

impl Curve {
    /// Builds the polynomial `c_0 + c_1 L + c_2 L^2 + ...`, with the
    /// coefficients lowest power first.
    ///
    /// It is evaluated by Horner's method from the last coefficient, which
    /// keeps it to additions and multiplications.
    ///
    /// # Errors
    ///
    /// The first failure in this order: [`CurveError::Empty`], then for each
    /// coefficient from the first, [`CurveError::NotFinite`] and then
    /// [`CurveError::Negative`].
    pub fn polynomial(coefficients: Vec<f64>) -> Result<Self, CurveError> {
        Ok(Self(Shape::Polynomial(validate_values(coefficients)?)))
    }

    /// Builds `base * growth^(L - 1)`, so level 1 has the value `base`.
    ///
    /// A curve that is zero at every level is `polynomial(vec![0.0])`: a base
    /// of zero would turn `0 * inf` into NaN where the power overflows.
    ///
    /// # Errors
    ///
    /// The first failure in this order: for the base, [`CurveError::NotFinite`]
    /// then [`CurveError::NonPositiveBase`]; then for the growth,
    /// [`CurveError::NotFinite`] then [`CurveError::NonPositiveGrowth`]. A NaN
    /// or infinite base is reported as `NotFinite { index: 0 }` and growth as
    /// `NotFinite { index: 1 }`.
    pub fn exponential(base: f64, growth: f64) -> Result<Self, CurveError> {
        if !base.is_finite() {
            return Err(CurveError::NotFinite { index: 0 });
        }
        if base <= 0.0 {
            return Err(CurveError::NonPositiveBase);
        }
        if !growth.is_finite() {
            return Err(CurveError::NotFinite { index: 1 });
        }
        if growth <= 0.0 {
            return Err(CurveError::NonPositiveGrowth);
        }
        Ok(Self(Shape::Exponential { base, growth }))
    }

    /// Builds a table where level `L` has `values[L - 1]`.
    ///
    /// # Errors
    ///
    /// The first failure in this order: [`CurveError::Empty`], then for each
    /// value from the first, [`CurveError::NotFinite`] and then
    /// [`CurveError::Negative`].
    pub fn table(values: Vec<f64>) -> Result<Self, CurveError> {
        Ok(Self(Shape::Table(validate_values(values)?)))
    }

    /// The value at `level`. See the type's doc for what the result promises.
    ///
    /// # Errors
    ///
    /// [`CurveValueError::Overflow`] when the result is not finite, and
    /// [`CurveValueError::LevelOutOfTable`] when a table has no value for
    /// `level`.
    #[must_use = "the result may be an error"]
    pub fn value(&self, level: Level) -> Result<f64, CurveValueError> {
        let l = f64::from(level.get());
        let v = match &self.0 {
            Shape::Polynomial(coefficients) => {
                let mut acc = 0.0_f64;
                for c in coefficients.iter().rev() {
                    acc = acc * l + c;
                }
                acc
            }
            Shape::Exponential { base, growth } => {
                // `level` is at least 1, so the subtraction cannot underflow.
                base * libm::pow(*growth, f64::from(level.get() - 1))
            }
            Shape::Table(values) => {
                let index = usize::try_from(level.get() - 1)
                    .map_err(|_| CurveValueError::LevelOutOfTable)?;
                *values.get(index).ok_or(CurveValueError::LevelOutOfTable)?
            }
        };
        if v.is_finite() {
            Ok(v)
        } else {
            Err(CurveValueError::Overflow)
        }
    }

    /// Whether the curve gives a value at every level up to `last`, given that
    /// it gives one at `last`.
    ///
    /// A polynomial has coefficients of at least zero, so its Horner form does
    /// not decrease as `L` grows, and a table has a value at `last` only if it
    /// has one at every earlier level. An exponential does not decrease when
    /// its growth is at least 1. With growth below 1 the largest value is at
    /// level 1, which is `base` and finite, so nothing needs checking.
    pub(crate) fn check_through(&self, last: Level) -> Result<(), CurveValueError> {
        if let Shape::Exponential { growth, .. } = &self.0
            && *growth < 1.0
        {
            return Ok(());
        }
        self.value(last).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lv(n: u32) -> Level {
        Level::try_from(n).unwrap()
    }

    #[test]
    fn level_rejects_zero() {
        assert_eq!(Level::try_from(0), Err(LevelError::Zero));
        assert_eq!(Level::try_from(1).unwrap().get(), 1);
        assert_eq!(Level::try_from(u32::MAX).unwrap().get(), u32::MAX);
    }

    #[test]
    fn polynomial_values() {
        let c = Curve::polynomial(vec![100.0, 0.0, 10.0]).unwrap();
        assert_eq!(c.value(lv(1)), Ok(110.0));
        assert_eq!(c.value(lv(3)), Ok(190.0));
        let c = Curve::polynomial(vec![5.0]).unwrap();
        assert_eq!(c.value(lv(7)), Ok(5.0));
    }

    #[test]
    fn polynomial_errors() {
        assert_eq!(Curve::polynomial(vec![]), Err(CurveError::Empty));
        assert_eq!(
            Curve::polynomial(vec![1.0, -1.0]),
            Err(CurveError::Negative { index: 1 })
        );
        assert_eq!(
            Curve::polynomial(vec![f64::NAN]),
            Err(CurveError::NotFinite { index: 0 })
        );
    }

    #[test]
    fn polynomial_overflow() {
        let c = Curve::polynomial(vec![0.0, 0.0, 1e300]).unwrap();
        assert_eq!(c.value(lv(u32::MAX)), Err(CurveValueError::Overflow));
    }

    #[test]
    fn exponential_values() {
        let c = Curve::exponential(100.0, 1.5).unwrap();
        assert_eq!(c.value(lv(1)), Ok(100.0));
        assert_eq!(c.value(lv(3)), Ok(225.0));
        let c = Curve::exponential(100.0, 1.1).unwrap();
        assert_eq!(c.value(lv(11)), Ok(259.374_246_010_000_2));
    }

    #[test]
    fn libm_pow_bits_detect_a_version_that_changes_the_output() {
        assert_eq!(libm::pow(1.1, 10.0).to_bits(), 0x4004_bffc_0c03_023e);
    }

    #[test]
    fn exponential_errors() {
        assert_eq!(
            Curve::exponential(0.0, 1.5),
            Err(CurveError::NonPositiveBase)
        );
        assert_eq!(
            Curve::exponential(-0.0, 1.5),
            Err(CurveError::NonPositiveBase)
        );
        assert_eq!(
            Curve::exponential(1.0, 0.0),
            Err(CurveError::NonPositiveGrowth)
        );
        assert_eq!(
            Curve::exponential(f64::NAN, 1.5),
            Err(CurveError::NotFinite { index: 0 })
        );
        assert_eq!(
            Curve::exponential(1.0, f64::INFINITY),
            Err(CurveError::NotFinite { index: 1 })
        );
    }

    #[test]
    fn exponential_overflow() {
        let c = Curve::exponential(1.0, 10.0).unwrap();
        assert_eq!(c.value(lv(400)), Err(CurveValueError::Overflow));
    }

    #[test]
    fn table_values() {
        let c = Curve::table(vec![10.0, 20.0, 30.0]).unwrap();
        assert_eq!(c.value(lv(2)), Ok(20.0));
        assert_eq!(c.value(lv(4)), Err(CurveValueError::LevelOutOfTable));
        assert_eq!(Curve::table(vec![]), Err(CurveError::Empty));
    }

    #[test]
    fn negative_zero_becomes_positive_zero() {
        let c = Curve::polynomial(vec![-0.0]).unwrap();
        assert!(c.value(lv(1)).unwrap().is_sign_positive());
        let c = Curve::table(vec![-0.0]).unwrap();
        assert!(c.value(lv(1)).unwrap().is_sign_positive());
    }
}

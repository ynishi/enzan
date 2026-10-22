//! Experience, levels and the rule that turns one into the other.
//!
//! A [`Progress`] is a state: a level and the experience inside it. A
//! [`Progression`] is the rule: a [`Curve`] of the experience each level asks
//! for, a cap, and what happens to experience at the cap. [`Progression::gain`]
//! takes a `Progress` and an amount and returns a new `Progress` with what
//! happened, as a value.
//!
//! # Differences from the issue and from core-design
//!
//! The rule of how levels rise and what happens at the cap (R7) belongs to
//! [`Progression`], the type that holds the rules, and not to [`Progress`],
//! which holds only the values of a state.

use core::num::NonZeroU32;

use crate::curve::{Curve, CurveValueError, Level, normalize_zero};

/// Why an [`Experience`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ExperienceError {
    /// The amount is NaN or infinite.
    #[error("experience must be finite")]
    NotFinite,
    /// The amount is below zero.
    #[error("experience must not be negative")]
    Negative,
}

/// An amount of experience: finite and at least zero.
///
/// A `-0.0` becomes `+0.0`, so nothing has one form.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Experience(f64);

impl Experience {
    /// No experience.
    pub const ZERO: Self = Self(0.0);

    /// The amount.
    #[must_use]
    pub fn get(&self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Experience {
    type Error = ExperienceError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if !value.is_finite() {
            return Err(ExperienceError::NotFinite);
        }
        if value < 0.0 {
            return Err(ExperienceError::Negative);
        }
        Ok(Self(normalize_zero(value)))
    }
}

/// What happens to experience that is at the cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CapPolicy {
    /// The experience is set to zero.
    Discard,
    /// The experience is kept and keeps adding up.
    Keep,
}

/// A level and the experience inside it.
///
/// A `Progress` is accepted even when its experience is already more than the
/// level asks for. The next [`Progression::gain`] settles it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Progress {
    level: Level,
    experience: Experience,
}

impl Progress {
    /// A state at `level` with `experience` inside it.
    #[must_use]
    pub fn new(level: Level, experience: Experience) -> Self {
        Self { level, experience }
    }

    /// The level.
    #[must_use]
    pub fn level(&self) -> Level {
        self.level
    }

    /// The experience inside the level.
    #[must_use]
    pub fn experience(&self) -> Experience {
        self.experience
    }
}

/// Why a [`Progression`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProgressionError {
    /// A table has no value for some level below the cap.
    #[error("the curve has no value for a level below the cap")]
    CurveTooShort,
    /// The curve overflows at a level below the cap.
    #[error("the curve overflows at a level below the cap")]
    CurveOverflow,
}

/// Why [`Progression::gain`] gave no result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GainError {
    /// The starting level is above the cap.
    #[error("the level is above the cap")]
    LevelAboveCap,
    /// The experience left the range `f64` can represent.
    #[error("the experience left the range of f64")]
    Overflow,
    /// The curve gave no value for a level.
    #[error(transparent)]
    Curve(#[from] CurveValueError),
}

/// The rule for levelling: the experience each level asks for, the cap and the
/// policy at the cap.
///
/// The curve's value at level `L` is the experience needed to go from `L` to
/// `L + 1`.
#[derive(Clone, Debug, PartialEq)]
pub struct Progression {
    curve: Curve,
    cap: Level,
    at_cap: CapPolicy,
}

impl Progression {
    /// Builds a rule, after checking that `curve` has a finite value at every
    /// level from 1 to `cap - 1`, the levels [`Progression::gain`] asks for.
    /// Finding it here means a mismatch is not left for a player to reach.
    ///
    /// Only the last level is evaluated, since a table has a value there only
    /// if it has one at every earlier level, and the other shapes do not
    /// decrease with the level in the way that matters. The reasons are on
    /// [`Curve`]. A cap of 1 asks for no level.
    ///
    /// # Errors
    ///
    /// [`ProgressionError::CurveTooShort`] when a table is shorter than
    /// `cap - 1`, and [`ProgressionError::CurveOverflow`] when the value at
    /// `cap - 1` is not finite.
    pub fn new(curve: Curve, cap: Level, at_cap: CapPolicy) -> Result<Self, ProgressionError> {
        if let Some(last) = NonZeroU32::new(cap.get() - 1) {
            curve
                .check_through(Level::from(last))
                .map_err(|e| match e {
                    CurveValueError::LevelOutOfTable => ProgressionError::CurveTooShort,
                    CurveValueError::Overflow => ProgressionError::CurveOverflow,
                })?;
        }
        Ok(Self { curve, cap, at_cap })
    }

    /// The curve of the experience each level asks for.
    #[must_use]
    pub fn curve(&self) -> &Curve {
        &self.curve
    }

    /// The highest level.
    #[must_use]
    pub fn cap(&self) -> Level {
        self.cap
    }

    /// What happens to experience at the cap.
    #[must_use]
    pub fn at_cap(&self) -> CapPolicy {
        self.at_cap
    }

    /// Adds `amount` to `from` and raises the level for as long as the
    /// experience covers what the level asks for.
    ///
    /// ```text
    /// from.level > cap  -> Err(LevelAboveCap)
    /// xp = from.experience + amount           (must be finite, else Overflow)
    /// level = from.level
    /// while level < cap:
    ///     need = curve.value(level)
    ///     if xp < need: break
    ///     xp = xp - need                      (xp >= need >= 0, so not negative)
    ///     level = level + 1
    /// at the cap: Discard sets xp to 0, Keep leaves it
    /// ```
    ///
    /// Any number of levels can be gained at once. The time is proportional to
    /// `cap - from.level`, since a run of levels that ask for nothing is walked
    /// one by one. At the cap under [`CapPolicy::Keep`] the experience keeps
    /// adding up and can end in [`GainError::Overflow`].
    ///
    /// What happened is told by the return value: see [`Gain`].
    ///
    /// # Errors
    ///
    /// [`GainError::LevelAboveCap`] and [`GainError::Overflow`] as above. A
    /// curve error can occur only as a second line of defence: after the
    /// checks in [`Progression::new`] it needs a growth of at least 1 whose
    /// `pow` is not monotonic at the edge of 1 ulp.
    #[must_use = "the result holds the new state and may be an error"]
    pub fn gain(&self, from: Progress, amount: Experience) -> Result<Gain, GainError> {
        if from.level > self.cap {
            return Err(GainError::LevelAboveCap);
        }
        let mut xp = from.experience.get() + amount.get();
        if !xp.is_finite() {
            return Err(GainError::Overflow);
        }
        let mut level = from.level;
        while level < self.cap {
            let need = self.curve.value(level)?;
            if xp < need {
                break;
            }
            xp -= need;
            // `level < cap <= u32::MAX`, so the sum cannot saturate.
            level = level.saturating_next();
        }
        if level == self.cap && self.at_cap == CapPolicy::Discard {
            xp = 0.0;
        }
        Ok(Gain {
            progress: Progress {
                level,
                experience: Experience(xp),
            },
            leveled_up: (level > from.level).then_some(LeveledUp {
                from: from.level,
                to: level,
            }),
            reached_cap: from.level < self.cap && level == self.cap,
        })
    }
}

/// A rise in level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeveledUp {
    from: Level,
    to: Level,
}

impl LeveledUp {
    /// The level before.
    #[must_use]
    pub fn from(&self) -> Level {
        self.from
    }

    /// The level after, which is above [`LeveledUp::from`].
    #[must_use]
    pub fn to(&self) -> Level {
        self.to
    }
}

/// The result of [`Progression::gain`]: the new state and what happened.
///
/// The fields are private so that the link between them cannot be broken from
/// outside: when [`Gain::reached_cap`] is true, [`Gain::leveled_up`] is `Some`
/// and its `to` is the cap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gain {
    progress: Progress,
    leveled_up: Option<LeveledUp>,
    reached_cap: bool,
}

impl Gain {
    /// The state after the gain.
    #[must_use]
    pub fn progress(&self) -> Progress {
        self.progress
    }

    /// The rise in level, or `None` when the level did not change.
    #[must_use]
    pub fn leveled_up(&self) -> Option<LeveledUp> {
        self.leveled_up
    }

    /// Whether this gain took the level from below the cap to the cap.
    #[must_use]
    pub fn reached_cap(&self) -> bool {
        self.reached_cap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lv(n: u32) -> Level {
        Level::try_from(n).unwrap()
    }

    fn xp(v: f64) -> Experience {
        Experience::try_from(v).unwrap()
    }

    fn table_rule(at_cap: CapPolicy) -> Progression {
        let curve = Curve::table(vec![10.0, 20.0, 30.0]).unwrap();
        Progression::new(curve, lv(4), at_cap).unwrap()
    }

    fn up(from: u32, to: u32) -> Option<LeveledUp> {
        Some(LeveledUp {
            from: lv(from),
            to: lv(to),
        })
    }

    #[test]
    fn experience_checks() {
        assert_eq!(
            Experience::try_from(f64::NAN),
            Err(ExperienceError::NotFinite)
        );
        assert_eq!(Experience::try_from(-1.0), Err(ExperienceError::Negative));
        assert_eq!(Experience::try_from(0.0).unwrap().get(), 0.0);
        assert!(Experience::try_from(-0.0).unwrap().get().is_sign_positive());
    }

    #[test]
    fn new_checks_the_curve_up_to_the_cap() {
        let table = || Curve::table(vec![10.0, 20.0, 30.0]).unwrap();
        assert!(Progression::new(table(), lv(4), CapPolicy::Keep).is_ok());
        assert_eq!(
            Progression::new(table(), lv(5), CapPolicy::Keep),
            Err(ProgressionError::CurveTooShort)
        );
        let poly = Curve::polynomial(vec![0.0, 0.0, 1e300]).unwrap();
        assert_eq!(
            Progression::new(poly, lv(u32::MAX), CapPolicy::Keep),
            Err(ProgressionError::CurveOverflow)
        );
        let one = Curve::table(vec![10.0]).unwrap();
        assert!(Progression::new(one, lv(1), CapPolicy::Keep).is_ok());
    }

    #[test]
    fn gain_within_a_level() {
        let g = table_rule(CapPolicy::Discard)
            .gain(Progress::new(lv(1), xp(0.0)), xp(5.0))
            .unwrap();
        assert_eq!(g.progress(), Progress::new(lv(1), xp(5.0)));
        assert_eq!(g.leveled_up(), None);
        assert!(!g.reached_cap());
    }

    #[test]
    fn gain_several_levels() {
        let g = table_rule(CapPolicy::Discard)
            .gain(Progress::new(lv(1), xp(0.0)), xp(35.0))
            .unwrap();
        assert_eq!(g.progress(), Progress::new(lv(3), xp(5.0)));
        assert_eq!(g.leveled_up(), up(1, 3));
        assert!(!g.reached_cap());
    }

    #[test]
    fn gain_to_the_cap_discards_or_keeps() {
        let start = Progress::new(lv(1), xp(0.0));
        let g = table_rule(CapPolicy::Discard)
            .gain(start, xp(1000.0))
            .unwrap();
        assert_eq!(g.progress(), Progress::new(lv(4), xp(0.0)));
        assert_eq!(g.leveled_up(), up(1, 4));
        assert!(g.reached_cap());
        let g = table_rule(CapPolicy::Keep).gain(start, xp(1000.0)).unwrap();
        assert_eq!(g.progress(), Progress::new(lv(4), xp(940.0)));
        assert!(g.reached_cap());
    }

    #[test]
    fn gain_at_the_cap() {
        let start = Progress::new(lv(4), xp(940.0));
        let g = table_rule(CapPolicy::Keep).gain(start, xp(10.0)).unwrap();
        assert_eq!(g.progress(), Progress::new(lv(4), xp(950.0)));
        assert_eq!(g.leveled_up(), None);
        assert!(!g.reached_cap());
        let g = table_rule(CapPolicy::Discard)
            .gain(start, xp(10.0))
            .unwrap();
        assert_eq!(g.progress(), Progress::new(lv(4), xp(0.0)));
    }

    #[test]
    fn gain_exactly_at_the_threshold() {
        let g = table_rule(CapPolicy::Discard)
            .gain(Progress::new(lv(1), xp(0.0)), xp(10.0))
            .unwrap();
        assert_eq!(g.progress(), Progress::new(lv(2), xp(0.0)));
    }

    #[test]
    fn gain_of_nothing_changes_nothing() {
        let start = Progress::new(lv(2), xp(3.0));
        let g = table_rule(CapPolicy::Discard).gain(start, xp(0.0)).unwrap();
        assert_eq!(g.progress(), start);
        assert_eq!(g.leveled_up(), None);
        assert!(!g.reached_cap());
    }

    #[test]
    fn gain_through_levels_that_ask_for_nothing() {
        let rule = Progression::new(
            Curve::polynomial(vec![0.0]).unwrap(),
            lv(5),
            CapPolicy::Discard,
        )
        .unwrap();
        let g = rule.gain(Progress::new(lv(1), xp(0.0)), xp(0.0)).unwrap();
        assert_eq!(g.progress().level(), lv(5));
        assert_eq!(g.leveled_up(), up(1, 5));
        assert!(g.reached_cap());
    }

    #[test]
    fn gain_errors() {
        let rule = table_rule(CapPolicy::Discard);
        assert_eq!(
            rule.gain(Progress::new(lv(5), xp(0.0)), xp(1.0)),
            Err(GainError::LevelAboveCap)
        );
        assert_eq!(
            rule.gain(Progress::new(lv(1), xp(f64::MAX)), xp(f64::MAX)),
            Err(GainError::Overflow)
        );
    }

    #[test]
    fn gain_at_the_cap_passes_the_same_finite_check() {
        let rule = table_rule(CapPolicy::Keep);
        assert_eq!(
            rule.gain(Progress::new(lv(4), xp(f64::MAX)), xp(f64::MAX)),
            Err(GainError::Overflow)
        );
    }
}

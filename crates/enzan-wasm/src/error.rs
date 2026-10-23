//! The errors of the binding: the closed set of codes, where a code is shown,
//! and the one place that maps the errors of `enzan-core` to a code.

use core::fmt;

use enzan_core::balance::{MeasureError, TrialsError};
use enzan_core::catalog::{
    CatalogError, EvaluateSheetError, EvaluateStatsError, ItemError, UnknownBase, UnknownTable,
};
use enzan_core::curve::{CurveError, CurveValueError, LevelError};
use enzan_core::loot::{AffixDefError, AffixPoolError, ChanceError, CountRangeError};
use enzan_core::progression::{ExperienceError, GainError, ProgressionError};
use enzan_core::rng::WeightedTableError;
use enzan_core::stat::{BoundsError, ModifierError, StatDefError, StatKeyError};
use serde::Serialize;
use tsify::Tsify;
use wasm_bindgen::JsValue;

closed_set! {
/// Why a call or a content spec was refused: a closed set, the same for every
/// method.
///
/// The first group is about a content spec (`Content.load`, `Content.validate`)
/// and the second about a call on loaded content. A code can come up in the
/// other group too, where the same rule applies: `invalid-level` is a level of
/// an affix in a spec and an item level in a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Tsify)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    /// The value is not the shape asked for: a missing, unknown or mistyped
    /// field, or a number that is not a whole number in range.
    InvalidShape,
    /// A stat key does not have the shape of a key (see the `invalid-key`
    /// code).
    InvalidStatKey,
    /// Two stats have the same key.
    DuplicateStat,
    /// The bounds of a stat are not a range.
    InvalidBounds,
    /// The base value of a stat is not a finite number.
    InvalidBaseValue,
    /// Two bases have the same id.
    DuplicateBase,
    /// There is no rarity.
    NoRarity,
    /// Two rarities have the same id.
    DuplicateRarity,
    /// A weight is 0.
    InvalidWeight,
    /// A range is not `min <= max`, or it is above the limit this binding sets.
    InvalidRange,
    /// A modifier value is not allowed for its kind.
    InvalidModifier,
    /// The low and high of an affix are not a finite range with low at most
    /// high.
    AffixRange,
    /// Two affixes have the same id.
    DuplicateAffix,
    /// An affix, a stat key or a modifier names a stat that is not defined.
    UnknownStat,
    /// The weights of a table add up to more than a `u64` holds.
    WeightOverflow,
    /// A chance is not between 0 and 1.
    InvalidChance,
    /// Two drop tables have the same id.
    DuplicateTable,
    /// A drop table or a call names a base that is not defined.
    UnknownBase,
    /// A drop table lists no base.
    NoBase,
    /// A curve is not a valid curve.
    InvalidCurve,
    /// A table curve has no value for a level below the cap.
    CurveTooShort,
    /// A curve is not finite at a level below the cap.
    CurveOverflow,
    /// A call names a drop table that is not defined.
    UnknownTable,
    /// A level is below 1.
    InvalidLevel,
    /// The number of trials is below 1 or above the limit.
    TrialsOutOfRange,
    /// An amount of experience is not finite or is negative.
    InvalidExperience,
    /// A level is above the cap.
    LevelAboveCap,
    /// The content has no progression and the call needs one.
    NoProgression,
    /// A count or a sum left its range.
    Overflow,
    /// The id of a source or the class of a base is not a key: 1 to 64
    /// characters of `a-z`, `0-9` and `_` starting with a letter. This is the
    /// one statement of the shape of a key for the interface.
    /// `invalid-stat-key` is the same rule for the key of a stat and stays for
    /// contract version 1; it is to be folded into this code at the next
    /// contract version.
    InvalidKey,
    /// Two sources have the same id.
    DuplicateSource,
    /// An item names a rarity that is not defined.
    UnknownRarity,
    /// An item names an affix that is not defined.
    UnknownAffix,
    /// An item is not one the content can roll: an affix its rarity, its level
    /// or the groups of its other affixes do not admit, a value outside the
    /// range of its affix, or more affixes than its rarity allows.
    InvalidItem,
}
}

/// The name of a unit variant as JavaScript sees it. It is taken from the serde
/// form, which the generated type is also made from, so the names are written
/// once: by `rename_all` on the enum.
pub(crate) fn serde_name<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        _ => unreachable!("a unit variant of a rename_all enum serializes to a string"),
    }
}

impl ErrorCode {
    /// The code as JavaScript sees it (kebab-case).
    #[must_use]
    pub fn name(self) -> String {
        serde_name(&self)
    }
}

/// One refusal: a code, where in the input it is, and a sentence for a person.
///
/// `Content.validate` returns a list of these and every other method throws the
/// first as an `EnzanError`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Tsify)]
pub struct Issue {
    /// What is wrong.
    pub code: ErrorCode,
    /// Where, as a path in the input such as `affixes[2].low` or
    /// `dropTables[0].bases[1].base`; empty when it is the input as a whole.
    pub at: String,
    /// What is wrong, for a person. Not for a program: branch on `code`.
    pub message: String,
}

impl Issue {
    /// Joins the parts.
    pub(crate) fn new(code: ErrorCode, at: impl Into<String>, message: impl fmt::Display) -> Self {
        Self {
            code,
            at: at.into(),
            message: message.to_string(),
        }
    }

    /// A key that is the id of a source or the class of a base and is not of
    /// the shape of a key. It is `invalid-key` and not the code its core error
    /// maps to, which is the one for a stat.
    pub(crate) fn invalid_key(at: impl Into<String>, error: &enzan_core::KeyError) -> Self {
        Self::new(ErrorCode::InvalidKey, at, error)
    }

    /// An error of `enzan-core`, with the code it maps to.
    pub(crate) fn of<E: Coded + fmt::Display>(at: impl Into<String>, error: &E) -> Self {
        Self::new(error.code(), at, error)
    }
}

/// The code of an error of `enzan-core`.
///
/// Every mapping is here, and every `match` is exhaustive, so a variant added
/// to the core stops this crate from compiling until it has a code.
pub(crate) trait Coded {
    /// The code.
    fn code(&self) -> ErrorCode;
}

/// The code of a key read as the key of a stat. A source id or a class is the
/// same rule under `invalid-key`, which the place that reads one says.
impl Coded for StatKeyError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::Empty | Self::TooLong | Self::InvalidStart | Self::InvalidChar { .. } => {
                ErrorCode::InvalidStatKey
            }
        }
    }
}

impl Coded for BoundsError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::NotANumber
            | Self::MinIsPositiveInfinity
            | Self::MaxIsNegativeInfinity
            | Self::MinAboveMax => ErrorCode::InvalidBounds,
        }
    }
}

impl Coded for StatDefError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::BaseNotFinite => ErrorCode::InvalidBaseValue,
        }
    }
}

impl Coded for ModifierError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::NotFinite | Self::MoreBelowMinusOne => ErrorCode::InvalidModifier,
        }
    }
}

impl Coded for AffixDefError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::ModifierMismatch | Self::MinAboveMax | Self::SpanNotFinite => {
                ErrorCode::AffixRange
            }
        }
    }
}

impl Coded for AffixPoolError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::DuplicateId { .. } => ErrorCode::DuplicateAffix,
            Self::TotalOverflow => ErrorCode::WeightOverflow,
        }
    }
}

impl Coded for ChanceError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::OutOfRange => ErrorCode::InvalidChance,
        }
    }
}

impl Coded for CountRangeError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::MinAboveMax => ErrorCode::InvalidRange,
        }
    }
}

impl Coded for CurveError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::Empty
            | Self::NotFinite { .. }
            | Self::Negative { .. }
            | Self::NonPositiveBase
            | Self::NonPositiveGrowth => ErrorCode::InvalidCurve,
        }
    }
}

impl Coded for CurveValueError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::Overflow => ErrorCode::CurveOverflow,
            Self::LevelOutOfTable => ErrorCode::CurveTooShort,
        }
    }
}

impl Coded for ProgressionError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::CurveTooShort => ErrorCode::CurveTooShort,
            Self::CurveOverflow => ErrorCode::CurveOverflow,
        }
    }
}

impl Coded for LevelError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::Zero => ErrorCode::InvalidLevel,
        }
    }
}

impl Coded for ExperienceError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::NotFinite | Self::Negative => ErrorCode::InvalidExperience,
        }
    }
}

impl Coded for GainError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::LevelAboveCap => ErrorCode::LevelAboveCap,
            Self::Overflow => ErrorCode::Overflow,
            Self::Curve(inner) => inner.code(),
        }
    }
}

impl Coded for TrialsError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::OutOfRange => ErrorCode::TrialsOutOfRange,
        }
    }
}

impl Coded for MeasureError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::UnknownTable => ErrorCode::UnknownTable,
            Self::Overflow => ErrorCode::Overflow,
        }
    }
}

impl Coded for UnknownTable {
    fn code(&self) -> ErrorCode {
        ErrorCode::UnknownTable
    }
}

impl Coded for UnknownBase {
    fn code(&self) -> ErrorCode {
        ErrorCode::UnknownBase
    }
}

impl Coded for EvaluateStatsError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::UnknownStat { .. } | Self::UnknownModifierStat { .. } => ErrorCode::UnknownStat,
            Self::Overflow { .. } => ErrorCode::Overflow,
        }
    }
}

impl Coded for EvaluateSheetError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::UnknownStat { .. } | Self::UnknownModifierStat { .. } => ErrorCode::UnknownStat,
            Self::Overflow { .. } => ErrorCode::Overflow,
        }
    }
}

impl Coded for ItemError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::UnknownBase => ErrorCode::UnknownBase,
            Self::UnknownRarity => ErrorCode::UnknownRarity,
            Self::UnknownAffix { .. } => ErrorCode::UnknownAffix,
            Self::NotAdmitted { .. } | Self::OutOfRange { .. } | Self::TooManyAffixes => {
                ErrorCode::InvalidItem
            }
        }
    }
}

impl Coded for CatalogError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::DuplicateStat { .. } => ErrorCode::DuplicateStat,
            Self::DuplicateBase { .. } => ErrorCode::DuplicateBase,
            Self::DuplicateRarity { .. } => ErrorCode::DuplicateRarity,
            Self::DuplicateTable { .. } => ErrorCode::DuplicateTable,
            Self::UnknownAffixStat { .. } => ErrorCode::UnknownStat,
            Self::UnknownTableBase { .. } => ErrorCode::UnknownBase,
        }
    }
}

/// The code of an error of a weighted table, whose `Empty` is a different
/// refusal for each table of the content: `empty` says which.
pub(crate) fn weighted_table_code(error: &WeightedTableError, empty: ErrorCode) -> ErrorCode {
    match error {
        WeightedTableError::Empty => empty,
        WeightedTableError::TotalOverflow => ErrorCode::WeightOverflow,
    }
}

/// The JavaScript `Error` a method throws for `issue`: its `name` is
/// `"EnzanError"` and it carries `code` and `at`, so it keeps its stack and
/// passes `instanceof Error`.
pub(crate) fn to_js_error(issue: &Issue) -> JsValue {
    let error = js_sys::Error::new(&issue.message);
    error.set_name("EnzanError");
    // Setting a property on an object that was just made cannot be refused, and
    // an error that lacks `code` still says what it says in its message.
    let _ = js_sys::Reflect::set(&error, &"code".into(), &issue.code.name().into());
    let _ = js_sys::Reflect::set(&error, &"at".into(), &issue.at.as_str().into());
    error.into()
}

/// The error thrown when a value the binding built cannot be turned into a
/// JavaScript value. It is a plain `Error`, not an `EnzanError`: the values are
/// plain data, so this is not something a caller's input can cause.
pub(crate) fn internal_error(error: &tsify::Error) -> JsValue {
    js_sys::Error::new(&error.to_string()).into()
}

#[wasm_bindgen::prelude::wasm_bindgen(typescript_custom_section)]
const ENZAN_ERROR_TS: &str = r#"
/** What a method throws: an `Error` whose `name` is "EnzanError". */
export interface EnzanError extends Error {
    name: "EnzanError";
    /** Why, as a code that stays the same between releases of the same contract version. */
    code: ErrorCode;
    /** Where in the input, as in `Issue.at`. */
    at: string;
}

/** Whether `e` is an `EnzanError`, as thrown by a method of this package. */
export function isEnzanError(e: unknown): e is EnzanError;

/** The kinds of modifier, in the order they combine. */
export function modifierKinds(): ModifierKind[];

/** Every error code the package throws or returns, in ascending order. */
export function errorCodes(): ErrorCode[];
"#;

//! WebAssembly bindings of enzan for TypeScript.
//!
//! An adapter over `enzan-core`: it converts between JavaScript values and the
//! core's types and calls the core. It holds no mechanics of its own, and the
//! rules it enforces (ids, references, ranges) are the core's. The error codes
//! are listed in [`ErrorCode`], which is the source of truth for them.
//!
//! # The contract
//!
//! This section is the source of truth for the interface. [`CONTRACT_VERSION`]
//! says which version of it the package speaks.
//!
//! 1. **Load once.** Content is read into a [`Content`] once; after that it is
//!    an immutable handle and every method is a call on it. To reload, load a
//!    new one and `free()` the old one (it is also freed when collected).
//! 2. **The game owns the random state.** A state is a `bigint`. A method that
//!    draws takes a state and returns the new one. Save it with `toString()`
//!    and restore it with `BigInt(s)`; a `bigint` makes `JSON.stringify` throw.
//!    `rngStream(seed, n)` gives the state of the `n`th independent stream of a
//!    seed, and a seed is also a valid state.
//! 3. **Plain values.** Arguments and results are plain JavaScript values.
//!    `Content` is the only class.
//! 4. **The types come from Rust.** The DTOs of this crate are the contract and
//!    the TypeScript types are generated from them, not written by hand.
//! 5. **Serde checks the numbers.** A method takes one object, so that a
//!    fractional, negative or out-of-range number in an integer field is
//!    `invalid-shape` and not silently rounded (wasm-bindgen turns `1.5` into
//!    `1` and `-1` into `4294967295` in a plain `u32` argument). A field that is
//!    64 bits wide (a random state, a seed, a stream) is a `bigint`; every other
//!    number is a `u32` or an `f64`, and no type here holds a `usize`.
//! 6. **Lists are ascending.** A list keyed by an id or a value is in
//!    ascending order of it, so the same input gives the same list.
//! 7. **Errors.** A method throws an `Error` whose `name` is `"EnzanError"` and
//!    that carries `code` and `at` (`isEnzanError` tells it). `Content.validate`
//!    does not throw; it returns every [`Issue`]. The generated `.d.ts` does not
//!    show what a method throws.
//! 8. **Determinism.** The same content, input and random state give, bit for
//!    bit, the result the native core gives. A number crosses as the bits of an
//!    `f64`. In a spec, `NaN` and infinities are refused (`invalid-shape`) and
//!    `-0` is read as `+0`; in the arguments of a call, a number that is not
//!    finite is refused by the rule of its field (`invalid-experience`,
//!    `invalid-modifier`).
//! 9. **Versioned.** [`CONTRACT_VERSION`] changes when a change breaks what a
//!    caller relies on. A field added to a result does not break it. A spec
//!    refuses fields it does not know.
//! 10. **Call per event.** A call crosses the boundary once, so make it per game
//!     event (a defeat, a level gained) and not per number. A small object
//!     costs about 3 microseconds to cross.
//!
//! Two more rules follow from these:
//!
//! - One random state is not shared between uses. Drops, crits and spawns each
//!   take their own stream (`rngStream(seed, n)`), because sharing one mixes the
//!   order of the game's calls into the order the drops draw in, which is part
//!   of the contract, and a replay would then need the game's calls in the same
//!   order too. `rngNextF64` and `rngBelow` draw on the stream of their use.
//! - Nothing a method does after a `load` panics. A panic in WebAssembly is a
//!   trap, which JavaScript sees as a `RuntimeError` that does not say why. An
//!   `expect` on an invariant is the one exception, and its message gives the
//!   reason.

#![warn(missing_docs)]

mod content;
mod dto;
mod error;
mod spec;

#[cfg(test)]
mod tests;

pub use content::Content;
pub use dto::{
    AffixReport, AtExperience, Bin, EvaluateStatsArgs, GainExperienceArgs, GainResult, ItemRoll,
    LeveledUp, LootReport, MeasureLootArgs, MeasureProgressionArgs, ModifierInput, ModifierKind,
    Progress, ProgressionReport, RarityCount, RngBelowArgs, RngBelowResult, RngNextF64Args,
    RngNextF64Result, RngStreamArgs, RollDropArgs, RollDropResult, RollItemArgs, RollItemResult,
    RolledAffix, StatValue, Summary, ToLevel,
};
pub use error::{ErrorCode, Issue};
pub use spec::{
    AffixKindSpec, AffixSpec, AtCapSpec, BaseSpec, ContentSpec, CurveSpec, DropBaseSpec,
    DropTableSpec, ProgressionSpec, RaritySpec, StatSpec,
};

use tsify::Ts;
use wasm_bindgen::prelude::*;

use crate::content::{out, read, thrown};

/// The version of the contract this package speaks.
///
/// It is raised by a change that breaks a caller: a rename, removal or new
/// meaning of a field of a spec, an argument or a result; a spec that was
/// accepted and is now refused; a code removed or given another meaning; a
/// change to the order in which the random state is drawn from (the same state
/// then gives other items). A field added to a result, and a code added for a
/// case that used to be another code's, do not raise it.
pub const CONTRACT_VERSION: u32 = 1;

/// The version of the contract this package speaks.
#[wasm_bindgen(js_name = contractVersion)]
#[must_use]
pub fn contract_version() -> u32 {
    CONTRACT_VERSION
}

/// The most trials `measureLoot` takes, so a page can bound its input without
/// writing the number again. The core's `Trials` holds the limit.
#[wasm_bindgen(js_name = maxTrials)]
#[must_use]
pub fn max_trials() -> u32 {
    enzan_core::balance::Trials::MAX
}

/// Whether `e` is an `EnzanError`, as thrown by a method of this package.
///
/// It looks at the `name` of an `Error` and not at its class, so it holds for
/// an error that crossed a realm or a worker.
#[wasm_bindgen(js_name = isEnzanError, skip_typescript)]
#[must_use]
pub fn is_enzan_error(e: &JsValue) -> bool {
    // `instanceof Error` is false for an error made in another realm, so the
    // `name` is read from any object.
    e.is_object()
        && js_sys::Reflect::get(e, &"name".into())
            .is_ok_and(|name| name.as_string().as_deref() == Some("EnzanError"))
}

/// The random state that starts stream `stream` of `seed`.
///
/// Streams of one seed are independent. Use one for each use of randomness in
/// the game.
#[wasm_bindgen(js_name = rngStream)]
pub fn rng_stream(args: Ts<RngStreamArgs>) -> Result<u64, JsValue> {
    Ok(read(&args)?.run())
}

/// A number in `[0, 1)` and the state after it.
#[wasm_bindgen(js_name = rngNextF64)]
pub fn rng_next_f64(args: Ts<RngNextF64Args>) -> Result<Ts<RngNextF64Result>, JsValue> {
    out(&read(&args)?.run())
}

/// A whole number from 0 up to but not including `n` without bias, and the
/// state after it. `n` is at least 1 (`invalid-range`).
#[wasm_bindgen(js_name = rngBelow)]
pub fn rng_below(args: Ts<RngBelowArgs>) -> Result<Ts<RngBelowResult>, JsValue> {
    out(&read(&args)?.run().map_err(thrown)?)
}

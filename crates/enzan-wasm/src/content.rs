//! The handle JavaScript holds: loaded content, and the methods that call the
//! core against it.
//!
//! Nothing here decides anything. A method reads its argument object, runs the
//! pure function of the same name in [`crate::dto`], and writes the result; the
//! rules and the errors are in those functions, which are tested natively.

use enzan_core::catalog::Catalog;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tsify::{Ts, Tsify};
use wasm_bindgen::prelude::*;

use crate::dto::{
    self, BaseInfo, CheckItemArgs, EvaluateSheetArgs, EvaluateStatsArgs, GainExperienceArgs,
    GainResult, ItemRoll, LootReport, MeasureLootArgs, MeasureProgressionArgs, ProgressionReport,
    RollDropArgs, RollDropResult, RollItemArgs, RollItemResult, StatValue,
};
use crate::error::{ErrorCode, Issue, internal_error, to_js_error};
use crate::spec::{self, ContentSpec};

/// The error to throw for `issue`.
pub(crate) fn thrown(issue: Issue) -> JsValue {
    to_js_error(&issue)
}

/// The arguments of a call, read from the object JavaScript passed.
///
/// A failure is `invalid-shape`, and `at` is the field when the reader can say
/// which one.
pub(crate) fn read<A>(args: &Ts<A>) -> Result<A, JsValue>
where
    A: Tsify + DeserializeOwned,
    A::JsType: Clone,
{
    args.to_rust().map_err(|error| {
        // `to_rust` does not say where; a second read of the same value does.
        let reader = serde_wasm_bindgen::Deserializer::from(args.js_value());
        let issue = match serde_path_to_error::deserialize::<_, A>(reader) {
            Err(e) => Issue::new(
                ErrorCode::InvalidShape,
                spec::at_of(e.path()),
                message_of(e.inner()),
            ),
            Ok(_) => Issue::new(ErrorCode::InvalidShape, "", message_of(&error)),
        };
        thrown(issue)
    })
}

/// The message of a reader's error, which prints as a JavaScript error with
/// its `Error: ` prefix.
fn message_of(error: &impl core::fmt::Display) -> String {
    let text = error.to_string();
    text.strip_prefix("Error: ")
        .map_or_else(|| text.clone(), str::to_owned)
}

/// The result of a call, as the object JavaScript receives.
pub(crate) fn out<R: Tsify + Serialize>(value: &R) -> Result<Ts<R>, JsValue> {
    Ts::from_rust(value).map_err(|e| internal_error(&e))
}

/// The catalog of a spec as JavaScript passed it, or every refusal.
fn catalog_of(spec: &Ts<ContentSpec>) -> Result<Catalog, Vec<Issue>> {
    let value = serde_wasm_bindgen::from_value::<Value>(spec.js_value())
        .map_err(|e| vec![Issue::new(ErrorCode::InvalidShape, "", message_of(&e))])?;
    spec::build(value)
}

/// Game content that has been read and checked, and the calls against it.
///
/// It does not change once loaded. Free it with `free()` when a new one takes
/// its place, or leave it to be collected; a call after `free()` throws the
/// plain `Error` wasm-bindgen throws for a freed object, not an `EnzanError`.
#[wasm_bindgen]
pub struct Content {
    pub(crate) catalog: Catalog,
}

#[wasm_bindgen]
impl Content {
    /// Checks a spec and returns every refusal, none when it is good. It does
    /// not throw.
    ///
    /// The check has two stages, and the second runs only when the first found
    /// nothing. The first reads the shape and builds each part with the rules of
    /// the part (a stat key, a range, a weight, a curve, and so on). The second
    /// checks the parts together: ids that are used twice and references to
    /// something that is not defined. So while any part is wrong, a refusal
    /// about how the parts fit together is not given. An error of shape is
    /// alone, since nothing can be built from it.
    pub fn validate(spec: Ts<ContentSpec>) -> Result<Vec<Ts<Issue>>, JsValue> {
        match catalog_of(&spec) {
            Ok(_) => Ok(Vec::new()),
            Err(issues) => issues.iter().map(out).collect(),
        }
    }

    /// Checks a spec and loads it. It throws the first refusal of
    /// `Content.validate` as an `EnzanError`.
    pub fn load(spec: Ts<ContentSpec>) -> Result<Content, JsValue> {
        catalog_of(&spec)
            .map(|catalog| Content { catalog })
            .map_err(|issues| thrown(spec::first(issues)))
    }

    /// One defeat: the items a drop table drops at an item level.
    #[wasm_bindgen(js_name = rollDrop)]
    pub fn roll_drop(&self, args: Ts<RollDropArgs>) -> Result<Ts<RollDropResult>, JsValue> {
        out(&read(&args)?.run(&self.catalog).map_err(thrown)?)
    }

    /// One item of a base at an item level.
    #[wasm_bindgen(js_name = rollItem)]
    pub fn roll_item(&self, args: Ts<RollItemArgs>) -> Result<Ts<RollItemResult>, JsValue> {
        out(&read(&args)?.run(&self.catalog).map_err(thrown)?)
    }

    /// The values of stats under modifiers, in the order of `stats`.
    #[wasm_bindgen(js_name = evaluateStats)]
    pub fn evaluate_stats(
        &self,
        args: Ts<EvaluateStatsArgs>,
    ) -> Result<Vec<Ts<StatValue>>, JsValue> {
        read(&args)?
            .run(&self.catalog)
            .map_err(thrown)?
            .iter()
            .map(out)
            .collect()
    }

    /// The values of stats under sources of modifiers, in the order of `stats`.
    /// The sources count in ascending order of id, whatever order they are
    /// given in.
    #[wasm_bindgen(js_name = evaluateSheet)]
    pub fn evaluate_sheet(
        &self,
        args: Ts<EvaluateSheetArgs>,
    ) -> Result<Vec<Ts<StatValue>>, JsValue> {
        read(&args)?
            .run(&self.catalog)
            .map_err(thrown)?
            .iter()
            .map(out)
            .collect()
    }

    /// An item checked against the content, such as one read back from a save:
    /// the item in its canonical form if the content can roll it, and an
    /// `EnzanError` if not.
    #[wasm_bindgen(js_name = checkItem)]
    pub fn check_item(&self, args: Ts<CheckItemArgs>) -> Result<Ts<ItemRoll>, JsValue> {
        out(&read(&args)?.run(&self.catalog).map_err(thrown)?)
    }

    /// The bases defined in the content, in ascending order of id.
    pub fn bases(&self) -> Result<Vec<Ts<BaseInfo>>, JsValue> {
        dto::bases(&self.catalog).iter().map(out).collect()
    }

    /// Adds experience to a character's progress.
    #[wasm_bindgen(js_name = gainExperience)]
    pub fn gain_experience(&self, args: Ts<GainExperienceArgs>) -> Result<Ts<GainResult>, JsValue> {
        out(&read(&args)?.run(&self.catalog).map_err(thrown)?)
    }

    /// Runs many defeats of a drop table and counts what dropped.
    #[wasm_bindgen(js_name = measureLoot)]
    pub fn measure_loot(&self, args: Ts<MeasureLootArgs>) -> Result<Ts<LootReport>, JsValue> {
        out(&read(&args)?.run(&self.catalog).map_err(thrown)?)
    }

    /// What the levelling rule does with amounts of experience, and what it
    /// asks for levels.
    #[wasm_bindgen(js_name = measureProgression)]
    pub fn measure_progression(
        &self,
        args: Ts<MeasureProgressionArgs>,
    ) -> Result<Ts<ProgressionReport>, JsValue> {
        out(&read(&args)?.run(&self.catalog).map_err(thrown)?)
    }

    /// The keys of the stats defined in the content, in ascending order.
    #[wasm_bindgen(js_name = statKeys)]
    #[must_use]
    pub fn stat_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .catalog
            .stats()
            .iter()
            .map(|s| s.key().as_str().to_owned())
            .collect();
        keys.sort();
        keys
    }
}

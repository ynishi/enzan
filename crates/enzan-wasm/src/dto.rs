//! The arguments and results of the methods, and how they map to and from the
//! types of `enzan-core`.
//!
//! These types are the contract: the TypeScript types are generated from them.
//! Every `run` is a pure function of its arguments, so it is tested natively;
//! the `Content` methods only carry JavaScript values to and from it.
//!
//! Two things about the generated types are chosen by the type that is
//! serialized at the top and not by the types inside it, so they are repeated on
//! each type that has the field: `large_number_types_as_bigints` on a type with
//! a `u64` (a random state, a seed or a stream) and `missing_as_null` on a type
//! that holds an `Option`.

use core::num::NonZeroU64;

use enzan_core::balance::{self, Trials};
use enzan_core::catalog::{BaseId, Catalog, DropTableId, EvaluateStatsError};
use enzan_core::curve::Level;
use enzan_core::loot;
use enzan_core::progression::{self, Experience, GainError};
use enzan_core::rng::{Rng, Seed};
use enzan_core::stat::{self, Modifier, ModifierSet, StatKey};
use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::CONTRACT_VERSION;
use crate::error::{ErrorCode, Issue};

/// How a modifier combines into a stat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum ModifierKind {
    /// Added to the base value.
    Added,
    /// A percentage, as a fraction, added to the others of its kind.
    Increased,
    /// A multiplier, as a fraction, applied on its own.
    More,
}

impl From<ModifierKind> for stat::ModifierKind {
    fn from(kind: ModifierKind) -> Self {
        match kind {
            ModifierKind::Added => Self::Added,
            ModifierKind::Increased => Self::Increased,
            ModifierKind::More => Self::More,
        }
    }
}

impl From<stat::ModifierKind> for ModifierKind {
    fn from(kind: stat::ModifierKind) -> Self {
        match kind {
            stat::ModifierKind::Added => Self::Added,
            stat::ModifierKind::Increased => Self::Increased,
            stat::ModifierKind::More => Self::More,
        }
    }
}

fn level(at: &str, value: u32) -> Result<Level, Issue> {
    Level::try_from(value).map_err(|e| Issue::of(at, &e))
}

fn experience(at: &str, value: f64) -> Result<Experience, Issue> {
    Experience::try_from(value).map_err(|e| Issue::of(at, &e))
}

fn stat_key(at: &str, value: &str) -> Result<StatKey, Issue> {
    StatKey::try_from(value).map_err(|e| Issue::of(at, &e))
}

fn rule(catalog: &Catalog) -> Result<&progression::Progression, Issue> {
    catalog.progression().ok_or_else(|| {
        Issue::new(
            ErrorCode::NoProgression,
            "",
            "the content has no progression",
        )
    })
}

// ---- random numbers -------------------------------------------------------

/// The arguments of `rngStream`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(large_number_types_as_bigints)]
pub struct RngStreamArgs {
    /// The seed the streams come from.
    pub seed: u64,
    /// Which independent stream of the seed.
    pub stream: u64,
}

impl RngStreamArgs {
    /// The state that starts stream `stream` of `seed`.
    pub(crate) fn run(&self) -> u64 {
        Rng::stream(Seed::from(self.seed), self.stream).state()
    }
}

/// The arguments of `rngNextF64`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(large_number_types_as_bigints)]
pub struct RngNextF64Args {
    /// The state to draw from.
    pub rng: u64,
}

/// The result of `rngNextF64`.
#[derive(Clone, Debug, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(large_number_types_as_bigints)]
pub struct RngNextF64Result {
    /// A number in `[0, 1)`.
    pub value: f64,
    /// The state to draw the next number from.
    pub rng: u64,
}

impl RngNextF64Args {
    /// One number in `[0, 1)` and the state after it.
    pub(crate) fn run(&self) -> RngNextF64Result {
        let mut rng = Rng::new(Seed::from(self.rng));
        let value = rng.next_f64();
        RngNextF64Result {
            value,
            rng: rng.state(),
        }
    }
}

/// The arguments of `rngBelow`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(large_number_types_as_bigints)]
pub struct RngBelowArgs {
    /// The state to draw from.
    pub rng: u64,
    /// The number of values to choose from, at least 1.
    pub n: u32,
}

/// The result of `rngBelow`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(large_number_types_as_bigints)]
pub struct RngBelowResult {
    /// A whole number from 0 up to but not including `n`.
    pub value: u32,
    /// The state to draw the next number from.
    pub rng: u64,
}

impl RngBelowArgs {
    /// One whole number below `n` without bias, and the state after it.
    pub(crate) fn run(&self) -> Result<RngBelowResult, Issue> {
        let n = NonZeroU64::new(u64::from(self.n))
            .ok_or_else(|| Issue::new(ErrorCode::InvalidRange, "n", "n must be at least 1"))?;
        let mut rng = Rng::new(Seed::from(self.rng));
        let value = u32::try_from(rng.below(n)).expect("a value below n, which is a u32, is a u32");
        Ok(RngBelowResult {
            value,
            rng: rng.state(),
        })
    }
}

// ---- items ----------------------------------------------------------------

/// An affix an item carries, with the value that was rolled.
#[derive(Clone, Debug, PartialEq, Serialize, Tsify)]
pub struct RolledAffix {
    /// The id of the affix in the content.
    pub id: u32,
    /// The stat it changes.
    pub stat: String,
    /// How it combines into the stat.
    pub modifier: ModifierKind,
    /// The value that was rolled, between the low and the high of the affix.
    pub value: f64,
}

/// An item that dropped.
#[derive(Clone, Debug, PartialEq, Serialize, Tsify)]
pub struct ItemRoll {
    /// The id of the base.
    pub base: u32,
    /// The id of the rarity.
    pub rarity: u32,
    /// The affixes, in the order they were rolled.
    pub affixes: Vec<RolledAffix>,
}

impl ItemRoll {
    fn from_core(item: &loot::ItemRoll<BaseId>) -> Self {
        Self {
            base: item.base().get(),
            rarity: item.rarity().get(),
            affixes: item
                .affixes()
                .iter()
                .map(|affix| {
                    let modifier = affix.modifier();
                    RolledAffix {
                        id: affix.id().get(),
                        stat: modifier.stat().to_string(),
                        modifier: modifier.kind().into(),
                        value: modifier.value(),
                    }
                })
                .collect(),
        }
    }
}

/// The arguments of `rollDrop`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(large_number_types_as_bigints)]
pub struct RollDropArgs {
    /// The id of the drop table.
    pub table: u32,
    /// The level of the item, at least 1.
    pub item_level: u32,
    /// The state to draw from.
    pub rng: u64,
}

/// The result of `rollDrop`.
#[derive(Clone, Debug, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(large_number_types_as_bigints)]
pub struct RollDropResult {
    /// The items that dropped, possibly none.
    pub items: Vec<ItemRoll>,
    /// The state to draw the next number from.
    pub rng: u64,
}

impl RollDropArgs {
    /// One defeat: the items of the table, rolled on the state.
    pub(crate) fn run(&self, catalog: &Catalog) -> Result<RollDropResult, Issue> {
        let item_level = level("itemLevel", self.item_level)?;
        let mut rng = Rng::new(Seed::from(self.rng));
        let items = catalog
            .roll_drop(DropTableId::from(self.table), item_level, &mut rng)
            .map_err(|e| Issue::of("table", &e))?;
        Ok(RollDropResult {
            items: items.iter().map(ItemRoll::from_core).collect(),
            rng: rng.state(),
        })
    }
}

/// The arguments of `rollItem`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(large_number_types_as_bigints)]
pub struct RollItemArgs {
    /// The id of the base.
    pub base: u32,
    /// The level of the item, at least 1.
    pub item_level: u32,
    /// The state to draw from.
    pub rng: u64,
}

/// The result of `rollItem`.
#[derive(Clone, Debug, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(large_number_types_as_bigints)]
pub struct RollItemResult {
    /// The item.
    pub item: ItemRoll,
    /// The state to draw the next number from.
    pub rng: u64,
}

impl RollItemArgs {
    /// One item of a base, rolled on the state.
    pub(crate) fn run(&self, catalog: &Catalog) -> Result<RollItemResult, Issue> {
        let item_level = level("itemLevel", self.item_level)?;
        let mut rng = Rng::new(Seed::from(self.rng));
        let item = catalog
            .roll_item(BaseId::from(self.base), item_level, &mut rng)
            .map_err(|e| Issue::of("base", &e))?;
        Ok(RollItemResult {
            item: ItemRoll::from_core(&item),
            rng: rng.state(),
        })
    }
}

// ---- stats ----------------------------------------------------------------

/// A modifier to count into a stat.
#[derive(Clone, Debug, PartialEq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct ModifierInput {
    /// The key of the stat.
    pub stat: String,
    /// How it combines into the stat.
    pub modifier: ModifierKind,
    /// The value, a finite number; for `more` it is at least -1.
    pub value: f64,
}

/// The arguments of `evaluateStats`.
#[derive(Clone, Debug, PartialEq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateStatsArgs {
    /// The modifiers, counted in this order.
    pub modifiers: Vec<ModifierInput>,
    /// The keys of the stats to evaluate.
    pub stats: Vec<String>,
}

/// The value of a stat.
#[derive(Clone, Debug, PartialEq, Serialize, Tsify)]
pub struct StatValue {
    /// The key of the stat.
    pub stat: String,
    /// Its value under the modifiers, within the bounds of the stat.
    pub value: f64,
}

impl EvaluateStatsArgs {
    /// The value of each stat asked for, in the order asked.
    pub(crate) fn run(&self, catalog: &Catalog) -> Result<Vec<StatValue>, Issue> {
        let mut modifiers = ModifierSet::new();
        for (i, input) in self.modifiers.iter().enumerate() {
            let key = stat_key(&format!("modifiers[{i}].stat"), &input.stat)?;
            let modifier = Modifier::new(key, input.modifier.into(), input.value)
                .map_err(|e| Issue::of(format!("modifiers[{i}].value"), &e))?;
            modifiers.push(modifier);
        }
        let keys = self
            .stats
            .iter()
            .enumerate()
            .map(|(i, key)| stat_key(&format!("stats[{i}]"), key))
            .collect::<Result<Vec<_>, _>>()?;
        let values = catalog
            .evaluate_stats(&modifiers, &keys)
            .map_err(|e| Issue::of(evaluate_at(&e), &e))?;
        Ok(self
            .stats
            .iter()
            .zip(values)
            .map(|(stat, value)| StatValue {
                stat: stat.clone(),
                value,
            })
            .collect())
    }
}

fn evaluate_at(error: &EvaluateStatsError) -> String {
    match error {
        EvaluateStatsError::UnknownStat { key_index }
        | EvaluateStatsError::Overflow { key_index } => format!("stats[{key_index}]"),
        EvaluateStatsError::UnknownModifierStat { modifier_index } => {
            format!("modifiers[{modifier_index}].stat")
        }
    }
}

// ---- progression ----------------------------------------------------------

/// Where a character stands: a level and the experience gained toward the next.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// The level, at least 1.
    pub level: u32,
    /// The experience toward the next level: finite and at least 0.
    pub experience: f64,
}

impl Progress {
    fn from_core(progress: progression::Progress) -> Self {
        Self {
            level: progress.level().get(),
            experience: progress.experience().get(),
        }
    }

    fn into_core(self, at: &str) -> Result<progression::Progress, Issue> {
        Ok(progression::Progress::new(
            level(&format!("{at}.level"), self.level)?,
            experience(&format!("{at}.experience"), self.experience)?,
        ))
    }
}

/// The arguments of `gainExperience`.
#[derive(Clone, Debug, PartialEq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct GainExperienceArgs {
    /// Where the character stands.
    pub progress: Progress,
    /// The experience gained: finite and at least 0.
    pub amount: f64,
}

/// A rise in level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Tsify)]
pub struct LeveledUp {
    /// The level before.
    pub from: u32,
    /// The level after, above `from`.
    pub to: u32,
}

/// The result of `gainExperience`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(missing_as_null)]
pub struct GainResult {
    /// Where the character stands after the gain.
    pub progress: Progress,
    /// The rise in level, or null when the level did not change.
    pub leveled_up: Option<LeveledUp>,
    /// Whether this gain took the level from below the cap to the cap.
    pub reached_cap: bool,
}

impl GainExperienceArgs {
    /// The experience added to the progress, and the levels it raised.
    pub(crate) fn run(&self, catalog: &Catalog) -> Result<GainResult, Issue> {
        let from = self.progress.into_core("progress")?;
        let amount = experience("amount", self.amount)?;
        let gain = rule(catalog)?.gain(from, amount).map_err(|e| {
            let at = match e {
                GainError::LevelAboveCap => "progress.level",
                GainError::Overflow | GainError::Curve(_) => "",
            };
            Issue::of(at, &e)
        })?;
        Ok(GainResult {
            progress: Progress::from_core(gain.progress()),
            leveled_up: gain.leveled_up().map(|up| LeveledUp {
                from: up.from().get(),
                to: up.to().get(),
            }),
            reached_cap: gain.reached_cap(),
        })
    }
}

/// The arguments of `measureProgression`.
#[derive(Clone, Debug, PartialEq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct MeasureProgressionArgs {
    /// Amounts of experience to ask about, each finite and at least 0.
    pub experience: Vec<f64>,
    /// Levels to ask about, each at least 1.
    pub levels: Vec<u32>,
}

/// What a character at level 1 with no experience has after gaining an amount.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Tsify)]
pub struct AtExperience {
    /// The amount gained.
    pub experience: f64,
    /// The level it gives.
    pub level: u32,
    /// The experience left toward the next level.
    pub remainder: f64,
}

/// The experience a character at level 1 with no experience needs to reach a
/// level.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct ToLevel {
    /// The level.
    pub level: u32,
    /// The experience needed, or null when the level is above the cap.
    pub experience: Option<f64>,
}

/// The result of `measureProgression`, in the order of the arguments.
#[derive(Clone, Debug, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(missing_as_null)]
pub struct ProgressionReport {
    /// The version of the contract that made the report.
    pub contract_version: u32,
    /// One entry for each amount in `experience`.
    pub at_experience: Vec<AtExperience>,
    /// One entry for each level in `levels`.
    pub to_level: Vec<ToLevel>,
}

impl MeasureProgressionArgs {
    /// What the levelling rule does with the amounts and what it asks for the
    /// levels.
    pub(crate) fn run(&self, catalog: &Catalog) -> Result<ProgressionReport, Issue> {
        let amounts = self
            .experience
            .iter()
            .enumerate()
            .map(|(i, &amount)| experience(&format!("experience[{i}]"), amount))
            .collect::<Result<Vec<_>, _>>()?;
        let levels = self
            .levels
            .iter()
            .enumerate()
            .map(|(i, &value)| level(&format!("levels[{i}]"), value))
            .collect::<Result<Vec<_>, _>>()?;
        let report = balance::measure_progression(rule(catalog)?, &amounts, &levels)
            .map_err(|e| Issue::of("", &e))?;
        Ok(ProgressionReport {
            contract_version: CONTRACT_VERSION,
            at_experience: report
                .at_experience()
                .iter()
                .map(|(amount, progress)| AtExperience {
                    experience: amount.get(),
                    level: progress.level().get(),
                    remainder: progress.experience().get(),
                })
                .collect(),
            to_level: report
                .to_level()
                .iter()
                .map(|(level, needed)| ToLevel {
                    level: level.get(),
                    experience: needed.map(|needed| needed.get()),
                })
                .collect(),
        })
    }
}

// ---- loot balance ---------------------------------------------------------

/// The arguments of `measureLoot`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(large_number_types_as_bigints)]
pub struct MeasureLootArgs {
    /// The id of the drop table.
    pub table: u32,
    /// The level of the items, at least 1.
    pub item_level: u32,
    /// The seed; trial `i` draws from stream `i` of it.
    pub seed: u64,
    /// How many defeats to run, from 1 to 1,000,000, and at most
    /// 10,000,000 items at the most the table can drop each time.
    pub trials: u32,
}

/// How many times a whole number came up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Tsify)]
pub struct Bin {
    /// The number.
    pub value: u32,
    /// How many times it came up.
    pub count: u32,
}

/// The spread of a list of numbers: percentiles are values of the list
/// (nearest rank).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Tsify)]
pub struct Summary {
    /// The smallest.
    pub min: f64,
    /// The mean.
    pub mean: f64,
    /// The median.
    pub p50: f64,
    /// The 90th percentile.
    pub p90: f64,
    /// The 99th percentile.
    pub p99: f64,
    /// The largest.
    pub max: f64,
}

impl From<balance::Summary> for Summary {
    fn from(summary: balance::Summary) -> Self {
        Self {
            min: summary.min(),
            mean: summary.mean(),
            p50: summary.p50(),
            p90: summary.p90(),
            p99: summary.p99(),
            max: summary.max(),
        }
    }
}

/// How many items were of a rarity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Tsify)]
pub struct RarityCount {
    /// The id of the rarity.
    pub id: u32,
    /// The number of items.
    pub count: u32,
}

/// How many items carried an affix, and the spread of its values.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Tsify)]
#[tsify(missing_as_null)]
pub struct AffixReport {
    /// The id of the affix.
    pub id: u32,
    /// The number of items that carried it.
    pub count: u32,
    /// The spread of its values, or null when no item carried it.
    pub value: Option<Summary>,
}

/// The result of `measureLoot`: every rarity and every affix of the content is
/// listed, with a count of 0 when it never came up. Lists are in ascending
/// order of the number they are keyed by.
#[derive(Clone, Debug, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(missing_as_null)]
pub struct LootReport {
    /// The version of the contract that made the report.
    pub contract_version: u32,
    /// How many defeats ran.
    pub trials: u32,
    /// How many items dropped over all of them.
    pub items: u32,
    /// How many items one defeat dropped, and in how many defeats.
    pub drops: Vec<Bin>,
    /// How many items were of each rarity.
    pub rarities: Vec<RarityCount>,
    /// How many affixes one item carried, and how many items did.
    pub affix_count: Vec<Bin>,
    /// For each affix, how many items carried it and the spread of its values.
    pub affixes: Vec<AffixReport>,
}

impl LootReport {
    fn from_core(report: &balance::LootReport) -> Self {
        let bins = |histogram: &balance::Histogram| {
            histogram
                .iter()
                .map(|(value, count)| Bin { value, count })
                .collect()
        };
        Self {
            contract_version: CONTRACT_VERSION,
            trials: report.trials(),
            items: report.items(),
            drops: bins(report.drops()),
            rarities: report
                .rarities()
                .iter()
                .map(|(id, &count)| RarityCount {
                    id: id.get(),
                    count,
                })
                .collect(),
            affix_count: bins(report.affix_count()),
            affixes: report
                .affixes()
                .iter()
                .map(|(id, stats)| AffixReport {
                    id: id.get(),
                    count: stats.count(),
                    value: stats.value().map(Summary::from),
                })
                .collect(),
        }
    }
}

/// The most items `measureLoot` may make: `trials` times the most items one
/// defeat of the table drops.
///
/// The core keeps every affix value until the end to take percentiles, so a
/// million trials of a table that drops a thousand items would run out of
/// WebAssembly memory, which traps. The limit is this binding's, not the core's.
pub const MAX_LOOT_ITEMS: u64 = 10_000_000;

impl MeasureLootArgs {
    /// The report of `trials` defeats of the table, each on its own stream of
    /// the seed.
    pub(crate) fn run(&self, catalog: &Catalog) -> Result<LootReport, Issue> {
        let item_level = level("itemLevel", self.item_level)?;
        let trials = Trials::try_from(self.trials).map_err(|e| Issue::of("trials", &e))?;
        if let Some(table) = catalog.drop_table(DropTableId::from(self.table)) {
            let most = u64::from(self.trials) * u64::from(table.count().max());
            if most > MAX_LOOT_ITEMS {
                return Err(Issue::new(
                    ErrorCode::TrialsOutOfRange,
                    "trials",
                    format_args!(
                        "trials times the most items the table drops is {most}, above {MAX_LOOT_ITEMS}"
                    ),
                ));
            }
        }
        let report = balance::measure_loot(
            catalog,
            DropTableId::from(self.table),
            item_level,
            Seed::from(self.seed),
            trials,
        )
        .map_err(|e| match e {
            balance::MeasureError::UnknownTable => Issue::of("table", &e),
            balance::MeasureError::Overflow => Issue::of("", &e),
        })?;
        Ok(LootReport::from_core(&report))
    }
}

//! The content spec: the shape the game's data is written in, and its
//! translation to a [`Catalog`].
//!
//! # Reading
//!
//! A spec is read in two steps: JavaScript value to [`serde_json::Value`] (with
//! `serde-wasm-bindgen`), then `Value` to [`ContentSpec`] (with
//! `serde_path_to_error`). The detour exists because `deny_unknown_fields` does
//! not take effect on a struct that `serde-wasm-bindgen` reads directly, so a
//! typo in a field name would be dropped without a word (measured with
//! `serde-wasm-bindgen` 0.6.5). Read from a `Value` it does, and
//! `serde_path_to_error` says where. Any error of this step is `invalid-shape`:
//! an unknown field, a missing one, a wrong type, and a number that is not a
//! whole number in range.
//!
//! # Translating
//!
//! A [`ContentSpec`] becomes a [`Catalog`] in two stages, and the second runs
//! only when the first found nothing:
//!
//! 1. each part is built with the constructor of the core that owns its rules
//!    (`StatKey`, `Bounds`, `StatDef`, `Modifier`, `AffixDef`, `AffixPool`,
//!    `Rarity`, `CountRange`, `Chance`, `WeightedTable`, `DropTable`, `Curve`,
//!    `Progression`), and every refusal is collected;
//! 2. the parts are checked together by [`Catalog::check`]: ids that name one
//!    thing, references that point at something.
//!
//! The rules are in the core; this module only says where in the spec a
//! refusal is (`at`).

use core::num::NonZeroU32;

use enzan_core::catalog::{
    BaseDef, BaseId, Catalog, CatalogError, CatalogParts, DropTableDef, DropTableId,
};
use enzan_core::curve::{Curve, Level};
use enzan_core::loot::{
    self, AffixDef, AffixGroup, AffixId, AffixKind, AffixPool, Chance, CountRange, DropTable,
    Rarity, RarityId,
};
use enzan_core::progression::{CapPolicy, Progression};
use enzan_core::rng::WeightedTable;
use enzan_core::stat::{Bounds, Modifier, StatDef, StatKey};
use serde::Deserialize;
use serde_json::Value;
use tsify::Tsify;

use crate::dto::ModifierKind;
use crate::error::{Coded, ErrorCode, Issue, weighted_table_code};

/// The most items one defeat of a drop table may drop.
///
/// The core makes every item of a defeat before it counts them, so a count in
/// the millions by mistake in the data would stop the browser tab. The limit is
/// this binding's, not the core's.
pub const MAX_DROP_COUNT: u32 = 1_000;

/// The game's content, as written in the data.
///
/// Unknown fields are refused. The numbers that are ids, counts, levels and
/// weights are whole numbers; a weight and a level are at least 1.
#[derive(Clone, Debug, PartialEq, Deserialize, Tsify)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContentSpec {
    /// The stats. A key is unique.
    pub stats: Vec<StatSpec>,
    /// The kinds of item that can drop. An id is unique.
    pub bases: Vec<BaseSpec>,
    /// The rarities, at least one. An id is unique.
    pub rarities: Vec<RaritySpec>,
    /// The affixes an item can carry. An id is unique, and the stat of each is
    /// one of `stats`.
    pub affixes: Vec<AffixSpec>,
    /// The drop tables. An id is unique, and the bases of each are in `bases`.
    pub drop_tables: Vec<DropTableSpec>,
    /// How levels are gained, if the game has levels.
    #[serde(default)]
    #[tsify(optional)]
    pub progression: Option<ProgressionSpec>,
}

/// A stat: a key, the value it starts from, and the range it stays in.
#[derive(Clone, Debug, PartialEq, Deserialize, Tsify)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StatSpec {
    /// 1 to 64 characters of `a-z`, `0-9` and `_`, starting with a letter.
    pub key: String,
    /// The value before any modifier: a finite number.
    pub base: f64,
    /// The lowest the stat can be; no lower limit when left out.
    #[serde(default, deserialize_with = "present")]
    #[tsify(optional)]
    pub min: Option<f64>,
    /// The highest the stat can be, at least `min`; no upper limit when left
    /// out.
    #[serde(default, deserialize_with = "present")]
    #[tsify(optional)]
    pub max: Option<f64>,
}

/// An optional number that, when the field is there, is a number.
///
/// A `NaN` or an infinity crosses a `Value` as `null`, which a plain
/// `Option<f64>` reads as left out: `min: NaN` would then be no lower limit. So
/// a field that is there must hold a number, and a missing one is `None` (by
/// `serde(default)`).
fn present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
    f64::deserialize(deserializer).map(Some)
}

/// A kind of item that can drop.
#[derive(Clone, Debug, PartialEq, Deserialize, Tsify)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BaseSpec {
    /// The id the drop tables and the calls refer to it by.
    pub id: u32,
    /// The name, for display. No rule reads it.
    pub name: String,
}

/// A rarity: its weight, and how many affixes an item of it can get.
#[derive(Clone, Debug, PartialEq, Deserialize, Tsify)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RaritySpec {
    /// The id items report their rarity by.
    pub id: u32,
    /// How likely it is against the other rarities, at least 1.
    pub weight: u32,
    /// The fewest and the most affixes drawn, `[min, max]` with `min <= max`.
    pub affix_count: (u32, u32),
    /// The most prefixes an item of it can hold.
    pub max_prefixes: u32,
    /// The most suffixes an item of it can hold.
    pub max_suffixes: u32,
}

/// Whether an affix is a prefix or a suffix, which are limited separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum AffixKindSpec {
    /// Counts against the prefixes of the rarity.
    Prefix,
    /// Counts against the suffixes of the rarity.
    Suffix,
}

/// An affix: a modifier on a stat whose value is drawn between `low` and
/// `high`.
#[derive(Clone, Debug, PartialEq, Deserialize, Tsify)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AffixSpec {
    /// The id items report the affix by.
    pub id: u32,
    /// Prefix or suffix.
    pub kind: AffixKindSpec,
    /// The tier, for display. No rule reads it.
    pub tier: u32,
    /// The lowest item level it can come up on, at least 1.
    pub required_level: u32,
    /// How likely it is against the others that can come up, at least 1.
    pub weight: u32,
    /// Affixes of one group do not come up together on one item.
    pub group: u32,
    /// The key of the stat it changes.
    pub stat: String,
    /// How it combines into the stat.
    pub modifier: ModifierKind,
    /// The lowest value drawn.
    pub low: f64,
    /// The highest value drawn, at least `low`.
    pub high: f64,
}

/// A base a drop table can drop, and how likely.
#[derive(Clone, Debug, PartialEq, Deserialize, Tsify)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DropBaseSpec {
    /// The id of a base.
    pub base: u32,
    /// How likely it is against the others of the table, at least 1.
    pub weight: u32,
}

/// What a defeat drops: a chance, then a count of bases drawn by weight.
#[derive(Clone, Debug, PartialEq, Deserialize, Tsify)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DropTableSpec {
    /// The id the calls refer to it by.
    pub id: u32,
    /// The chance that anything drops, from 0 to 1.
    pub chance: f64,
    /// The fewest and the most items when something drops, `[min, max]` with
    /// `min <= max` and `max` at most 1,000.
    pub count: (u32, u32),
    /// The bases that can drop, at least one.
    pub bases: Vec<DropBaseSpec>,
}

/// What happens to experience gained at the cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum AtCapSpec {
    /// It is thrown away.
    Discard,
    /// It is kept and keeps adding up.
    Keep,
}

/// The experience each level asks for.
#[derive(Clone, Debug, PartialEq, Deserialize, Tsify)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum CurveSpec {
    /// `c0 + c1 L + c2 L^2 + ...`, with the coefficients lowest power first.
    Polynomial {
        /// Finite and at least 0, and at least one.
        coefficients: Vec<f64>,
    },
    /// `base * growth^(L - 1)`.
    Exponential {
        /// The value at level 1: finite and above 0.
        base: f64,
        /// The factor for each level: finite and above 0.
        growth: f64,
    },
    /// The value of level `L` is `values[L - 1]`.
    Table {
        /// Finite and at least 0, and at least one.
        values: Vec<f64>,
    },
}

/// How levels are gained.
#[derive(Clone, Debug, PartialEq, Deserialize, Tsify)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProgressionSpec {
    /// The experience that level `L` asks for to reach level `L + 1`.
    pub curve: CurveSpec,
    /// The highest level, at least 1.
    pub cap: u32,
    /// What happens to experience gained at the cap.
    pub at_cap: AtCapSpec,
}

/// Where in the spec a path of `serde_path_to_error` is, in the form of `at`.
pub(crate) fn at_of(path: &serde_path_to_error::Path) -> String {
    let text = path.to_string();
    // The path of the whole input is written ".".
    if text == "." { String::new() } else { text }
}

/// Reads a spec from a value, refusing what is not the shape of a
/// [`ContentSpec`].
pub(crate) fn parse(value: Value) -> Result<ContentSpec, Issue> {
    serde_path_to_error::deserialize::<_, ContentSpec>(value)
        .map_err(|e| Issue::new(ErrorCode::InvalidShape, at_of(e.path()), e.inner()))
}

/// The refusals found so far.
#[derive(Default)]
struct Issues(Vec<Issue>);

impl Issues {
    /// The value of a built part, or `None` after noting why it could not be
    /// built.
    fn built<T, E: Coded + core::fmt::Display>(
        &mut self,
        at: impl Into<String>,
        result: Result<T, E>,
    ) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.0.push(Issue::of(at, &error));
                None
            }
        }
    }

    fn weight(&mut self, at: impl Into<String>, weight: u32) -> Option<NonZeroU32> {
        let weight = NonZeroU32::new(weight);
        if weight.is_none() {
            self.0.push(Issue::new(
                ErrorCode::InvalidWeight,
                at,
                "a weight must be at least 1",
            ));
        }
        weight
    }

    fn push(&mut self, code: ErrorCode, at: impl Into<String>, message: impl core::fmt::Display) {
        self.0.push(Issue::new(code, at, message));
    }
}

/// Builds every item with `build`, which notes a refusal and gives `None`; the
/// list is complete only when all of them were built.
fn each<S, T>(items: &[S], mut build: impl FnMut(usize, &S) -> Option<T>) -> Option<Vec<T>> {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| build(i, item))
        .collect::<Vec<_>>()
        .into_iter()
        .collect()
}

fn stat(i: usize, spec: &StatSpec, issues: &mut Issues) -> Option<StatDef> {
    let key = issues.built(
        format!("stats[{i}].key"),
        StatKey::try_from(spec.key.as_str()),
    );
    let bounds = match (spec.min, spec.max) {
        (None, None) => Some(Bounds::UNBOUNDED),
        (min, max) => {
            let result = Bounds::new(
                min.unwrap_or(f64::NEG_INFINITY),
                max.unwrap_or(f64::INFINITY),
            );
            // The bounds of a range with a missing end are refused where the
            // end that is there is.
            let at = if max.is_some() { "max" } else { "min" };
            issues.built(format!("stats[{i}].{at}"), result)
        }
    };
    let (key, bounds) = (key?, bounds?);
    issues.built(
        format!("stats[{i}].base"),
        StatDef::new(key, spec.base, bounds),
    )
}

fn rarity(i: usize, spec: &RaritySpec, issues: &mut Issues) -> Option<(Rarity, NonZeroU32)> {
    let weight = issues.weight(format!("rarities[{i}].weight"), spec.weight);
    let count = issues.built(
        format!("rarities[{i}].affixCount"),
        CountRange::new(spec.affix_count.0, spec.affix_count.1),
    );
    Some((
        Rarity::new(
            RarityId::from(spec.id),
            count?,
            spec.max_prefixes,
            spec.max_suffixes,
        ),
        weight?,
    ))
}

fn affix(i: usize, spec: &AffixSpec, issues: &mut Issues) -> Option<AffixDef> {
    let level = issues.built(
        format!("affixes[{i}].requiredLevel"),
        Level::try_from(spec.required_level),
    );
    let weight = issues.weight(format!("affixes[{i}].weight"), spec.weight);
    let key = issues.built(
        format!("affixes[{i}].stat"),
        StatKey::try_from(spec.stat.as_str()),
    );
    let kind = enzan_core::stat::ModifierKind::from(spec.modifier);
    let (low, high) = match key {
        Some(key) => (
            issues.built(
                format!("affixes[{i}].low"),
                Modifier::new(key.clone(), kind, spec.low),
            ),
            issues.built(
                format!("affixes[{i}].high"),
                Modifier::new(key, kind, spec.high),
            ),
        ),
        None => (None, None),
    };
    let (level, weight, low, high) = (level?, weight?, low?, high?);
    issues.built(
        format!("affixes[{i}].high"),
        AffixDef::new(loot::AffixSpec {
            id: AffixId::from(spec.id),
            kind: match spec.kind {
                AffixKindSpec::Prefix => AffixKind::Prefix,
                AffixKindSpec::Suffix => AffixKind::Suffix,
            },
            tier: spec.tier,
            required_level: level,
            weight,
            group: AffixGroup::from(spec.group),
            low,
            high,
        }),
    )
}

/// The pool of `defs`, with a repeated id placed at its second appearance in
/// `specs`.
fn affix_pool(specs: &[AffixSpec], defs: Vec<AffixDef>, issues: &mut Issues) -> Option<AffixPool> {
    match AffixPool::new(defs) {
        Ok(pool) => Some(pool),
        Err(error) => {
            let at = match &error {
                loot::AffixPoolError::DuplicateId { id } => specs
                    .iter()
                    .enumerate()
                    .filter(|(_, spec)| spec.id == id.get())
                    .nth(1)
                    .map_or_else(|| "affixes".to_owned(), |(i, _)| format!("affixes[{i}].id")),
                loot::AffixPoolError::TotalOverflow => "affixes".to_owned(),
            };
            issues.0.push(Issue::of(at, &error));
            None
        }
    }
}

fn drop_table(i: usize, spec: &DropTableSpec, issues: &mut Issues) -> Option<DropTableDef> {
    let p = format!("dropTables[{i}]");
    let chance = issues.built(format!("{p}.chance"), Chance::try_from(spec.chance));
    let count = issues.built(
        format!("{p}.count"),
        CountRange::new(spec.count.0, spec.count.1),
    );
    let count = count.filter(|count| {
        let within = count.max() <= MAX_DROP_COUNT;
        if !within {
            issues.push(
                ErrorCode::InvalidRange,
                format!("{p}.count"),
                format_args!("the most items one defeat may drop is {MAX_DROP_COUNT}"),
            );
        }
        within
    });
    let entries = each(&spec.bases, |j, entry| {
        let weight = issues.weight(format!("{p}.bases[{j}].weight"), entry.weight)?;
        Some((BaseId::from(entry.base), weight))
    });
    let bases = entries.and_then(|entries| match WeightedTable::new(entries) {
        Ok(table) => Some(table),
        Err(error) => {
            let code = weighted_table_code(&error, ErrorCode::NoBase);
            issues.push(code, format!("{p}.bases"), error);
            None
        }
    });
    Some(DropTableDef::new(
        DropTableId::from(spec.id),
        DropTable::new(chance?, count?, bases?),
    ))
}

fn progression(spec: &ProgressionSpec, issues: &mut Issues) -> Option<Progression> {
    let curve = match &spec.curve {
        CurveSpec::Polynomial { coefficients } => Curve::polynomial(coefficients.clone()),
        CurveSpec::Exponential { base, growth } => Curve::exponential(*base, *growth),
        CurveSpec::Table { values } => Curve::table(values.clone()),
    };
    let curve = issues.built("progression.curve", curve);
    let cap = issues.built("progression.cap", Level::try_from(spec.cap));
    let at_cap = match spec.at_cap {
        AtCapSpec::Discard => CapPolicy::Discard,
        AtCapSpec::Keep => CapPolicy::Keep,
    };
    issues.built("progression.curve", Progression::new(curve?, cap?, at_cap))
}

/// Stage 1: the parts of the catalog, or every refusal of a part.
pub(crate) fn parts(spec: &ContentSpec) -> Result<CatalogParts, Vec<Issue>> {
    let mut issues = Issues::default();
    let stats = each(&spec.stats, |i, s| stat(i, s, &mut issues));
    let bases = Some(
        spec.bases
            .iter()
            .map(|b| BaseDef::new(BaseId::from(b.id), b.name.clone()))
            .collect::<Vec<_>>(),
    );
    let rarities = each(&spec.rarities, |i, r| rarity(i, r, &mut issues)).and_then(|entries| {
        match WeightedTable::new(entries) {
            Ok(table) => Some(table),
            Err(error) => {
                let code = weighted_table_code(&error, ErrorCode::NoRarity);
                issues.push(code, "rarities", error);
                None
            }
        }
    });
    let affixes = each(&spec.affixes, |i, a| affix(i, a, &mut issues))
        .and_then(|defs| affix_pool(&spec.affixes, defs, &mut issues));
    let drop_tables = each(&spec.drop_tables, |i, t| drop_table(i, t, &mut issues));
    let progression = match &spec.progression {
        None => Some(None),
        Some(p) => progression(p, &mut issues).map(Some),
    };
    match (stats, bases, rarities, affixes, drop_tables, progression) {
        (
            Some(stats),
            Some(bases),
            Some(rarities),
            Some(affixes),
            Some(drop_tables),
            Some(progression),
        ) if issues.0.is_empty() => Ok(CatalogParts {
            stats,
            bases,
            rarities,
            affixes,
            drop_tables,
            progression,
        }),
        // Every builder above notes why before it gives `None`, so a failed
        // build always carries at least one issue; `first` relies on that.
        _ => Err(issues.0),
    }
}

/// The first of `issues`, which a failed build never leaves empty.
pub(crate) fn first(issues: Vec<Issue>) -> Issue {
    issues
        .into_iter()
        .next()
        .expect("every builder that gives None notes an issue first")
}

/// Where in the spec an inconsistency found by [`Catalog::check`] is.
fn catalog_issue(error: &CatalogError) -> Issue {
    let at = match *error {
        CatalogError::DuplicateStat { index } => format!("stats[{index}].key"),
        CatalogError::DuplicateBase { index } => format!("bases[{index}].id"),
        CatalogError::DuplicateRarity { index } => format!("rarities[{index}].id"),
        CatalogError::DuplicateTable { index } => format!("dropTables[{index}].id"),
        CatalogError::UnknownAffixStat { affix_index } => format!("affixes[{affix_index}].stat"),
        CatalogError::UnknownTableBase {
            table_index,
            entry_index,
        } => format!("dropTables[{table_index}].bases[{entry_index}].base"),
    };
    Issue::of(at, error)
}

/// Stage 2: the catalog of `parts`, or every inconsistency among them.
fn catalog(parts: CatalogParts) -> Result<Catalog, Vec<Issue>> {
    let errors = Catalog::check(&parts);
    if errors.is_empty() {
        Catalog::new(parts).map_err(|e| vec![catalog_issue(&e)])
    } else {
        Err(errors.iter().map(catalog_issue).collect())
    }
}

/// The catalog of a spec given as a value, or every refusal.
///
/// A refusal of shape ends it at once. After that the parts are built (stage 1)
/// and only when all of them are is the catalog checked as a whole (stage 2),
/// so while a part is wrong no refusal about how the parts fit together is
/// given: it would be about a spec that does not yet say what it means. The
/// list is never empty when this is an `Err`.
pub(crate) fn build(value: Value) -> Result<Catalog, Vec<Issue>> {
    let spec = parse(value).map_err(|issue| vec![issue])?;
    catalog(parts(&spec)?)
}

//! Native tests of the pure part: a spec to a catalog, a catalog to the results
//! of a call, and the codes.
//!
//! The boundary (a `JsValue` to a `Value`, the throw) is not here; it needs a
//! JavaScript engine and is checked by the smoke test that runs the package.

use enzan_core::catalog::{Catalog, DropTableId};
use enzan_core::curve::Level;
use enzan_core::loot::AffixPoolError;
use enzan_core::progression::GainError;
use enzan_core::rng::{Rng, Seed, WeightedTableError};
use enzan_core::stat::StatDefError;
use serde_json::{Value, json};

use crate::dto::{
    EvaluateStatsArgs, GainExperienceArgs, MAX_LOOT_ITEMS, MeasureLootArgs, MeasureProgressionArgs,
    ModifierInput, ModifierKind, Progress, RngBelowArgs, RngNextF64Args, RngStreamArgs,
    RollDropArgs, RollItemArgs,
};
use crate::error::{Coded, ErrorCode, Issue, weighted_table_code};
use crate::spec::{self, ContentSpec};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../tests/fixtures/content.json"))
        .expect("the fixture is JSON")
}

fn catalog() -> Catalog {
    spec::build(fixture()).expect("the fixture is good content")
}

/// The fixture with `edit` applied.
fn edited(edit: impl FnOnce(&mut Value)) -> Value {
    let mut value = fixture();
    edit(&mut value);
    value
}

/// Replaces the part of `value` at the JSON pointer `pointer`, or adds the
/// field when the parent is an object that lacks it.
fn set(value: &mut Value, pointer: &str, new: Value) {
    let (parent, name) = pointer
        .rsplit_once('/')
        .expect("a pointer has a parent and a name");
    let parent = value
        .pointer_mut(parent)
        .expect("the pointer names a part of the fixture");
    match parent {
        Value::Object(fields) => {
            fields.insert(name.to_owned(), new);
        }
        other => {
            *other
                .get_mut(name.parse::<usize>().expect("an index"))
                .expect("the index is in the array") = new;
        }
    }
}

/// Every refusal of the fixture with `edit` applied.
fn issues(edit: impl FnOnce(&mut Value)) -> Vec<Issue> {
    spec::build(edited(edit)).expect_err("the edit makes the content wrong")
}

/// The first refusal of the fixture with `edit` applied.
fn first_issue(edit: impl FnOnce(&mut Value)) -> Issue {
    issues(edit).remove(0)
}

fn pair(issue: &Issue) -> (ErrorCode, &str) {
    (issue.code, issue.at.as_str())
}

// ---- the spec -------------------------------------------------------------

#[test]
fn the_fixture_is_a_catalog() {
    let catalog = catalog();
    assert_eq!(catalog.stats().len(), 3);
    assert_eq!(catalog.bases().len(), 3);
    assert_eq!(catalog.rarities().entries().len(), 3);
    assert_eq!(catalog.affixes().affixes().len(), 6);
    assert!(catalog.drop_table(DropTableId::from(1)).is_some());
    assert!(catalog.drop_table(DropTableId::from(2)).is_some());
    assert!(catalog.progression().is_some());
}

#[test]
fn a_spec_without_a_progression_is_a_catalog_without_one() {
    let value = edited(|v| {
        v.as_object_mut().expect("an object").remove("progression");
    });
    assert!(
        spec::build(value)
            .expect("good content")
            .progression()
            .is_none()
    );
}

#[test]
fn the_other_curves_and_policies_are_read() {
    for progression in [
        json!({ "curve": { "kind": "exponential", "base": 100, "growth": 1.5 },
                "cap": 5, "atCap": "keep" }),
        json!({ "curve": { "kind": "table", "values": [10, 20, 30] },
                "cap": 4, "atCap": "discard" }),
    ] {
        let value = edited(|v| set(v, "/progression", progression));
        assert!(spec::build(value).is_ok());
    }
}

/// Where in the spec each code comes from, and the `at` it gives. One row for
/// each code: `Spec` edits the fixture and takes the first refusal, `Call` runs
/// a call on the fixture's catalog, and `Core` is a code that no JavaScript
/// value can cause (see its comment) and is checked at the mapping instead.
enum Case {
    Spec(fn() -> Issue),
    Call(fn(&Catalog) -> Result<(), Issue>),
    Core(fn() -> Issue),
}

fn ignore<T>(result: Result<T, Issue>) -> Result<(), Issue> {
    result.map(|_| ())
}

fn cases() -> Vec<(ErrorCode, &'static str, Case)> {
    use ErrorCode as C;
    vec![
        (
            C::InvalidShape,
            "typo",
            Case::Spec(|| {
                first_issue(|v| {
                    v.as_object_mut()
                        .expect("an object")
                        .insert("typo".to_owned(), json!(1));
                })
            }),
        ),
        (
            C::InvalidStatKey,
            "stats[1].key",
            Case::Spec(|| first_issue(|v| set(v, "/stats/1/key", json!("Not A Key")))),
        ),
        (
            C::DuplicateStat,
            "stats[2].key",
            Case::Spec(|| first_issue(|v| set(v, "/stats/2/key", json!("life")))),
        ),
        (
            C::InvalidBounds,
            "stats[0].max",
            Case::Spec(|| {
                first_issue(|v| {
                    set(v, "/stats/0/min", json!(5));
                    set(v, "/stats/0/max", json!(1));
                })
            }),
        ),
        // A value that is not finite reaches a spec as `null` (that is how a
        // `NaN` crosses a `Value`), which is `invalid-shape`. The code is for a
        // spec built in Rust.
        (
            C::InvalidBaseValue,
            "stats[0].base",
            Case::Core(|| {
                let mut spec = spec::parse(fixture()).expect("good shape");
                spec.stats[0].base = f64::NAN;
                spec::parts(&spec).expect_err("not finite").remove(0)
            }),
        ),
        (
            C::DuplicateBase,
            "bases[1].id",
            Case::Spec(|| first_issue(|v| set(v, "/bases/1/id", json!(1)))),
        ),
        (
            C::NoRarity,
            "rarities",
            Case::Spec(|| first_issue(|v| set(v, "/rarities", json!([])))),
        ),
        (
            C::DuplicateRarity,
            "rarities[1].id",
            Case::Spec(|| first_issue(|v| set(v, "/rarities/1/id", json!(1)))),
        ),
        (
            C::InvalidWeight,
            "rarities[0].weight",
            Case::Spec(|| first_issue(|v| set(v, "/rarities/0/weight", json!(0)))),
        ),
        (
            C::InvalidRange,
            "rarities[1].affixCount",
            Case::Spec(|| first_issue(|v| set(v, "/rarities/1/affixCount", json!([3, 1])))),
        ),
        (
            C::InvalidModifier,
            "affixes[2].low",
            Case::Spec(|| {
                first_issue(|v| {
                    set(v, "/affixes/2/modifier", json!("more"));
                    set(v, "/affixes/2/low", json!(-2));
                })
            }),
        ),
        (
            C::AffixRange,
            "affixes[0].high",
            Case::Spec(|| {
                first_issue(|v| {
                    set(v, "/affixes/0/low", json!(9));
                    set(v, "/affixes/0/high", json!(1));
                })
            }),
        ),
        (
            C::DuplicateAffix,
            "affixes[3].id",
            Case::Spec(|| first_issue(|v| set(v, "/affixes/3/id", json!(1)))),
        ),
        (
            C::UnknownStat,
            "affixes[1].stat",
            Case::Spec(|| first_issue(|v| set(v, "/affixes/1/stat", json!("mana")))),
        ),
        // The weights of a table add up to more than a `u64` only with more than
        // 2^32 entries, which no spec has room for. The code is for the core's
        // refusal.
        (
            C::WeightOverflow,
            "rarities",
            Case::Core(|| {
                let code =
                    weighted_table_code(&WeightedTableError::TotalOverflow, ErrorCode::NoBase);
                assert_eq!(code, AffixPoolError::TotalOverflow.code());
                Issue::new(code, "rarities", "the weights overflow")
            }),
        ),
        (
            C::InvalidChance,
            "dropTables[0].chance",
            Case::Spec(|| first_issue(|v| set(v, "/dropTables/0/chance", json!(1.5)))),
        ),
        (
            C::DuplicateTable,
            "dropTables[1].id",
            Case::Spec(|| first_issue(|v| set(v, "/dropTables/1/id", json!(1)))),
        ),
        (
            C::UnknownBase,
            "dropTables[0].bases[1].base",
            Case::Spec(|| first_issue(|v| set(v, "/dropTables/0/bases/1/base", json!(99)))),
        ),
        (
            C::NoBase,
            "dropTables[1].bases",
            Case::Spec(|| first_issue(|v| set(v, "/dropTables/1/bases", json!([])))),
        ),
        (
            C::InvalidCurve,
            "progression.curve",
            Case::Spec(|| {
                first_issue(|v| {
                    set(
                        v,
                        "/progression/curve",
                        json!({ "kind": "table", "values": [] }),
                    )
                })
            }),
        ),
        (
            C::CurveTooShort,
            "progression.curve",
            Case::Spec(|| {
                first_issue(|v| {
                    set(
                        v,
                        "/progression/curve",
                        json!({ "kind": "table", "values": [100] }),
                    );
                })
            }),
        ),
        (
            C::CurveOverflow,
            "progression.curve",
            Case::Spec(|| {
                first_issue(|v| {
                    set(
                        v,
                        "/progression/curve",
                        json!({ "kind": "polynomial", "coefficients": [1e308, 1e308] }),
                    );
                })
            }),
        ),
        (
            C::UnknownTable,
            "table",
            Case::Call(|c| {
                ignore(
                    RollDropArgs {
                        table: 99,
                        item_level: 1,
                        rng: 1,
                    }
                    .run(c),
                )
            }),
        ),
        (
            C::InvalidLevel,
            "itemLevel",
            Case::Call(|c| {
                ignore(
                    RollDropArgs {
                        table: 1,
                        item_level: 0,
                        rng: 1,
                    }
                    .run(c),
                )
            }),
        ),
        (
            C::TrialsOutOfRange,
            "trials",
            Case::Call(|c| {
                ignore(
                    MeasureLootArgs {
                        table: 1,
                        item_level: 1,
                        seed: 1,
                        trials: 0,
                    }
                    .run(c),
                )
            }),
        ),
        (
            C::InvalidExperience,
            "amount",
            Case::Call(|c| {
                ignore(
                    GainExperienceArgs {
                        progress: Progress {
                            level: 1,
                            experience: 0.0,
                        },
                        amount: -1.0,
                    }
                    .run(c),
                )
            }),
        ),
        (
            C::LevelAboveCap,
            "progress.level",
            Case::Call(|c| {
                ignore(
                    GainExperienceArgs {
                        progress: Progress {
                            level: 11,
                            experience: 0.0,
                        },
                        amount: 1.0,
                    }
                    .run(c),
                )
            }),
        ),
        (
            C::NoProgression,
            "",
            Case::Call(|_| {
                let value = edited(|v| {
                    v.as_object_mut().expect("an object").remove("progression");
                });
                let without = spec::build(value).expect("good content");
                ignore(
                    MeasureProgressionArgs {
                        experience: vec![],
                        levels: vec![],
                    }
                    .run(&without),
                )
            }),
        ),
        (
            C::Overflow,
            "stats[0]",
            Case::Call(|c| {
                let big = ModifierInput {
                    stat: "damage".to_owned(),
                    modifier: ModifierKind::Added,
                    value: 1e308,
                };
                ignore(
                    EvaluateStatsArgs {
                        modifiers: vec![big.clone(), big],
                        stats: vec!["damage".to_owned()],
                    }
                    .run(c),
                )
            }),
        ),
    ]
}

/// A position for each code, in the order of the enum. The match is exhaustive
/// so that a new code stops this test from compiling until it is listed here and
/// in [`ALL`].
fn index(code: ErrorCode) -> usize {
    use ErrorCode as C;
    match code {
        C::InvalidShape => 0,
        C::InvalidStatKey => 1,
        C::DuplicateStat => 2,
        C::InvalidBounds => 3,
        C::InvalidBaseValue => 4,
        C::DuplicateBase => 5,
        C::NoRarity => 6,
        C::DuplicateRarity => 7,
        C::InvalidWeight => 8,
        C::InvalidRange => 9,
        C::InvalidModifier => 10,
        C::AffixRange => 11,
        C::DuplicateAffix => 12,
        C::UnknownStat => 13,
        C::WeightOverflow => 14,
        C::InvalidChance => 15,
        C::DuplicateTable => 16,
        C::UnknownBase => 17,
        C::NoBase => 18,
        C::InvalidCurve => 19,
        C::CurveTooShort => 20,
        C::CurveOverflow => 21,
        C::UnknownTable => 22,
        C::InvalidLevel => 23,
        C::TrialsOutOfRange => 24,
        C::InvalidExperience => 25,
        C::LevelAboveCap => 26,
        C::NoProgression => 27,
        C::Overflow => 28,
    }
}

const ALL: [ErrorCode; 29] = {
    use ErrorCode as C;
    [
        C::InvalidShape,
        C::InvalidStatKey,
        C::DuplicateStat,
        C::InvalidBounds,
        C::InvalidBaseValue,
        C::DuplicateBase,
        C::NoRarity,
        C::DuplicateRarity,
        C::InvalidWeight,
        C::InvalidRange,
        C::InvalidModifier,
        C::AffixRange,
        C::DuplicateAffix,
        C::UnknownStat,
        C::WeightOverflow,
        C::InvalidChance,
        C::DuplicateTable,
        C::UnknownBase,
        C::NoBase,
        C::InvalidCurve,
        C::CurveTooShort,
        C::CurveOverflow,
        C::UnknownTable,
        C::InvalidLevel,
        C::TrialsOutOfRange,
        C::InvalidExperience,
        C::LevelAboveCap,
        C::NoProgression,
        C::Overflow,
    ]
};

#[test]
fn every_code_has_a_case_that_gives_it() {
    let catalog = catalog();
    let cases = cases();
    for (code, at, case) in &cases {
        let issue = match case {
            Case::Spec(make) | Case::Core(make) => make(),
            Case::Call(call) => call(&catalog).expect_err("the call is refused"),
        };
        assert_eq!(pair(&issue), (*code, *at), "{issue:?}");
        assert!(!issue.message.is_empty(), "{code:?} has a message");
    }
    // One row for each code, in the order of the enum.
    let listed: Vec<ErrorCode> = cases.iter().map(|(code, ..)| *code).collect();
    assert_eq!(listed, ALL);
    assert!(ALL.iter().enumerate().all(|(i, &code)| index(code) == i));
}

#[test]
fn the_name_of_a_code_is_its_kebab_case_form() {
    assert_eq!(ErrorCode::InvalidShape.name(), "invalid-shape");
    assert_eq!(ErrorCode::TrialsOutOfRange.name(), "trials-out-of-range");
    for code in ALL {
        let name = code.name();
        assert!(
            name.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
            "{code:?} is {name}"
        );
    }
}

#[test]
fn the_core_errors_that_no_spec_reaches_still_map() {
    assert_eq!(
        StatDefError::BaseNotFinite.code(),
        ErrorCode::InvalidBaseValue
    );
    assert_eq!(
        weighted_table_code(&WeightedTableError::Empty, ErrorCode::NoBase),
        ErrorCode::NoBase
    );
    assert_eq!(
        weighted_table_code(&WeightedTableError::Empty, ErrorCode::NoRarity),
        ErrorCode::NoRarity
    );
    assert_eq!(
        GainError::Curve(enzan_core::curve::CurveValueError::LevelOutOfTable).code(),
        ErrorCode::CurveTooShort
    );
}

#[test]
fn where_a_refusal_is() {
    let found = first_issue(|v| {
        set(v, "/affixes/2/modifier", json!("more"));
        set(v, "/affixes/2/low", json!(-2));
    });
    assert_eq!(pair(&found), (ErrorCode::InvalidModifier, "affixes[2].low"));

    let found = first_issue(|v| set(v, "/dropTables/0/bases/1/base", json!(99)));
    assert_eq!(
        pair(&found),
        (ErrorCode::UnknownBase, "dropTables[0].bases[1].base")
    );

    let found = first_issue(|v| set(v, "/dropTables/0/bases/1/weight", json!(0)));
    assert_eq!(
        pair(&found),
        (ErrorCode::InvalidWeight, "dropTables[0].bases[1].weight")
    );
}

#[test]
fn a_repeated_affix_id_is_placed_at_its_second_appearance() {
    // 1 is the id of affixes 0, 1 and 4: the repeat is at index 1.
    let found = issues(|v| {
        set(v, "/affixes/1/id", json!(1));
        set(v, "/affixes/4/id", json!(1));
    });
    assert_eq!(found.len(), 1);
    assert_eq!(
        pair(&found[0]),
        (ErrorCode::DuplicateAffix, "affixes[1].id")
    );
}

#[test]
fn a_field_that_is_not_known_is_a_shape_error_at_any_depth() {
    for (pointer, name) in [
        ("", "typo"),
        ("/stats/0", "bse"),
        ("/rarities/1", "wieght"),
        ("/affixes/3", "stat_key"),
        ("/dropTables/0/bases/1", "bse"),
        ("/progression", "cp"),
        ("/progression/curve", "base"),
    ] {
        let found = first_issue(|v| {
            let target = if pointer.is_empty() {
                &mut *v
            } else {
                v.pointer_mut(pointer).expect("a part of the fixture")
            };
            target
                .as_object_mut()
                .expect("an object")
                .insert(name.to_owned(), json!(1));
        });
        assert_eq!(found.code, ErrorCode::InvalidShape, "{pointer} {name}");
        assert!(
            found.message.contains(name),
            "the message names the field: {found:?}"
        );
    }
}

#[test]
fn a_missing_field_and_a_wrong_type_are_shape_errors() {
    let found = first_issue(|v| {
        v["stats"][0]
            .as_object_mut()
            .expect("an object")
            .remove("key");
    });
    assert_eq!(found.code, ErrorCode::InvalidShape);
    assert_eq!(found.at, "stats[0]");

    let found = first_issue(|v| set(v, "/bases/1/name", json!(7)));
    assert_eq!(
        (found.code, found.at.as_str()),
        (ErrorCode::InvalidShape, "bases[1].name")
    );

    let found = first_issue(|v| set(v, "/affixes/0/kind", json!("middle")));
    assert_eq!(
        (found.code, found.at.as_str()),
        (ErrorCode::InvalidShape, "affixes[0].kind")
    );

    let found = first_issue(|v| set(v, "/rarities/1/affixCount", json!([1, 2, 3])));
    assert_eq!(found.code, ErrorCode::InvalidShape);
}

#[test]
fn a_whole_number_field_refuses_a_fraction_or_a_negative() {
    for (pointer, value, at) in [
        ("/bases/0/id", json!(1.5), "bases[0].id"),
        ("/bases/0/id", json!(-1), "bases[0].id"),
        ("/rarities/0/weight", json!(2.5), "rarities[0].weight"),
        (
            "/rarities/1/affixCount",
            json!([0.5, 2]),
            "rarities[1].affixCount[0]",
        ),
        (
            "/rarities/1/maxPrefixes",
            json!(-1),
            "rarities[1].maxPrefixes",
        ),
        (
            "/affixes/0/requiredLevel",
            json!(1.5),
            "affixes[0].requiredLevel",
        ),
        (
            "/dropTables/0/count",
            json!([1, -2]),
            "dropTables[0].count[1]",
        ),
        ("/progression/cap", json!(10.5), "progression.cap"),
        ("/bases/0/id", json!(4_294_967_296_u64), "bases[0].id"),
    ] {
        let found = first_issue(|v| set(v, pointer, value.clone()));
        assert_eq!(
            pair(&found),
            (ErrorCode::InvalidShape, at),
            "{pointer} {value}"
        );
    }
}

#[test]
fn a_number_that_is_not_finite_is_a_shape_error() {
    // `NaN` and the infinities cross a `Value` as `null`.
    let found = first_issue(|v| set(v, "/stats/0/base", Value::Null));
    assert_eq!(
        (found.code, found.at.as_str()),
        (ErrorCode::InvalidShape, "stats[0].base")
    );
}

#[test]
fn a_bound_that_is_not_finite_is_a_shape_error_and_not_left_out() {
    // `min: NaN` crosses as `min: null`, which must not read as no lower limit.
    for (pointer, at) in [
        ("/stats/0/min", "stats[0].min"),
        ("/stats/2/max", "stats[2].max"),
    ] {
        let found = first_issue(|v| set(v, pointer, Value::Null));
        assert_eq!(
            (found.code, found.at.as_str()),
            (ErrorCode::InvalidShape, at)
        );
    }
}

#[test]
fn an_input_that_is_not_an_object_is_a_shape_error_for_the_whole_input() {
    let found = spec::build(json!([])).expect_err("not a spec").remove(0);
    assert_eq!(
        (found.code, found.at.as_str()),
        (ErrorCode::InvalidShape, "")
    );
}

#[test]
fn a_part_that_is_wrong_hides_how_the_parts_fit_together() {
    // A stat key that is not a key (stage 1) together with a repeated base id
    // and an affix on a stat that is not there (stage 2).
    let both = issues(|v| {
        set(v, "/stats/1/key", json!("Bad Key"));
        set(v, "/bases/1/id", json!(1));
        set(v, "/affixes/1/stat", json!("mana"));
    });
    assert_eq!(
        both.iter().map(pair).collect::<Vec<_>>(),
        [(ErrorCode::InvalidStatKey, "stats[1].key")]
    );

    // With the stage 1 error mended, stage 2 gives all of its own, in the order
    // the core checks them.
    let rest = issues(|v| {
        set(v, "/bases/1/id", json!(1));
        set(v, "/affixes/1/stat", json!("mana"));
        set(v, "/dropTables/1/bases/0/base", json!(77));
    });
    assert_eq!(
        rest.iter().map(pair).collect::<Vec<_>>(),
        [
            (ErrorCode::DuplicateBase, "bases[1].id"),
            (ErrorCode::UnknownStat, "affixes[1].stat"),
            // Base 2 is gone with the id that was repeated over it.
            (ErrorCode::UnknownBase, "dropTables[0].bases[1].base"),
            (ErrorCode::UnknownBase, "dropTables[1].bases[0].base"),
        ]
    );
}

#[test]
fn every_wrong_part_is_reported_not_only_the_first() {
    let found = issues(|v| {
        set(v, "/stats/0/key", json!("Bad"));
        set(v, "/rarities/1/weight", json!(0));
        set(v, "/affixes/2/modifier", json!("more"));
        set(v, "/affixes/2/low", json!(-2));
        set(v, "/dropTables/0/chance", json!(2));
        set(v, "/progression/cap", json!(0));
    });
    assert_eq!(
        found.iter().map(pair).collect::<Vec<_>>(),
        [
            (ErrorCode::InvalidStatKey, "stats[0].key"),
            (ErrorCode::InvalidWeight, "rarities[1].weight"),
            (ErrorCode::InvalidModifier, "affixes[2].low"),
            (ErrorCode::InvalidChance, "dropTables[0].chance"),
            (ErrorCode::InvalidLevel, "progression.cap"),
        ]
    );
}

#[test]
fn a_drop_table_may_drop_at_most_a_thousand_items() {
    let found = first_issue(|v| set(v, "/dropTables/0/count", json!([1, 1001])));
    assert_eq!(
        pair(&found),
        (ErrorCode::InvalidRange, "dropTables[0].count")
    );
    assert!(found.message.contains("1000"), "{found:?}");

    let value = edited(|v| set(v, "/dropTables/0/count", json!([1, 1000])));
    assert!(spec::build(value).is_ok());
}

#[test]
fn a_loot_measure_may_make_at_most_ten_million_items() {
    let value = edited(|v| set(v, "/dropTables/0/count", json!([1, 1000])));
    let catalog = spec::build(value).expect("good content");
    let args = |trials| MeasureLootArgs {
        table: 1,
        item_level: 10,
        seed: 1,
        trials,
    };
    let refused = args(10_001).run(&catalog).expect_err("too many items");
    assert_eq!(pair(&refused), (ErrorCode::TrialsOutOfRange, "trials"));
    assert!(
        refused.message.contains(&MAX_LOOT_ITEMS.to_string()),
        "{refused:?}"
    );
    assert!(args(10).run(&catalog).is_ok());
}

#[test]
fn a_spec_in_rust_is_what_the_fixture_says() {
    let spec: ContentSpec = spec::parse(fixture()).expect("good shape");
    assert_eq!(spec.drop_tables[0].bases[1].base, 2);
    assert_eq!(spec.rarities[2].affix_count, (3, 4));
}

// ---- calls ----------------------------------------------------------------

fn roll_drop(rng: u64) -> crate::dto::RollDropResult {
    RollDropArgs {
        table: 1,
        item_level: 10,
        rng,
    }
    .run(&catalog())
    .expect("the table is there")
}

#[test]
fn a_drop_from_the_same_state_is_the_same_drop() {
    assert_eq!(roll_drop(42), roll_drop(42));
    assert_ne!(roll_drop(42), roll_drop(43));
}

#[test]
fn the_state_that_comes_back_continues_the_sequence() {
    let first = roll_drop(42);
    let second = roll_drop(first.rng);
    // The same two defeats from the same start, as the game would replay them.
    let replay_first = roll_drop(42);
    let replay_second = roll_drop(replay_first.rng);
    assert_eq!(first, replay_first);
    assert_eq!(second, replay_second);

    // And as the core gives them on one generator.
    let catalog = catalog();
    let mut rng = Rng::new(Seed::from(42));
    let table = DropTableId::from(1);
    let level = Level::try_from(10).expect("a level");
    let core_first = catalog.roll_drop(table, level, &mut rng).expect("a table");
    assert_eq!(rng.state(), first.rng);
    let core_second = catalog.roll_drop(table, level, &mut rng).expect("a table");
    assert_eq!(rng.state(), second.rng);
    assert_eq!(first.items.len(), core_first.len());
    assert_eq!(second.items.len(), core_second.len());
}

#[test]
fn an_item_has_the_affixes_the_rarity_allows() {
    let catalog = catalog();
    let mut rng = 7;
    for _ in 0..200 {
        let rolled = RollItemArgs {
            base: 1,
            item_level: 10,
            rng,
        }
        .run(&catalog)
        .expect("the base is there");
        rng = rolled.rng;
        let allowed = match rolled.item.rarity {
            1 => 0,
            2 => 2,
            _ => 4,
        };
        assert!(rolled.item.affixes.len() <= allowed, "{rolled:?}");
        assert_eq!(rolled.item.base, 1);
    }
}

#[test]
fn a_base_that_is_not_there_is_refused_at_the_base() {
    let found = RollItemArgs {
        base: 99,
        item_level: 1,
        rng: 1,
    }
    .run(&catalog())
    .expect_err("no such base");
    assert_eq!(pair(&found), (ErrorCode::UnknownBase, "base"));
}

#[test]
fn the_stats_come_back_in_the_order_asked() {
    let catalog = catalog();
    let args = |modifiers: Vec<ModifierInput>, stats: &[&str]| EvaluateStatsArgs {
        modifiers,
        stats: stats.iter().map(|s| (*s).to_owned()).collect(),
    };
    let bare = args(vec![], &["damage", "life"])
        .run(&catalog)
        .expect("stats");
    assert_eq!(
        bare.iter()
            .map(|s| (s.stat.as_str(), s.value))
            .collect::<Vec<_>>(),
        [("damage", 10.0), ("life", 100.0)]
    );

    let added = ModifierInput {
        stat: "damage".to_owned(),
        modifier: ModifierKind::Added,
        value: 5.0,
    };
    let more = args(vec![added.clone()], &["life", "damage"])
        .run(&catalog)
        .expect("stats");
    assert_eq!(more[0].value, 100.0);
    assert_eq!(more[1].value, 15.0);

    let unknown = args(vec![], &["mana"]).run(&catalog).expect_err("no stat");
    assert_eq!(pair(&unknown), (ErrorCode::UnknownStat, "stats[0]"));

    let typo = ModifierInput {
        stat: "dmage".to_owned(),
        ..added.clone()
    };
    let unknown = args(vec![added, typo], &["damage"])
        .run(&catalog)
        .expect_err("no stat");
    assert_eq!(
        pair(&unknown),
        (ErrorCode::UnknownStat, "modifiers[1].stat")
    );

    let bad = ModifierInput {
        stat: "Damage".to_owned(),
        modifier: ModifierKind::Added,
        value: 1.0,
    };
    let refused = args(vec![bad], &["damage"])
        .run(&catalog)
        .expect_err("no key");
    assert_eq!(
        pair(&refused),
        (ErrorCode::InvalidStatKey, "modifiers[0].stat")
    );
}

#[test]
fn experience_raises_the_level_and_tells_how() {
    let catalog = catalog();
    let gain = |level, experience, amount| {
        GainExperienceArgs {
            progress: Progress { level, experience },
            amount,
        }
        .run(&catalog)
    };
    // The fixture asks 100 + 50 L for level L: 150 at level 1, 200 at level 2.
    let none = gain(1, 0.0, 10.0).expect("a gain");
    assert_eq!(
        none.progress,
        Progress {
            level: 1,
            experience: 10.0
        }
    );
    assert_eq!(none.leveled_up, None);
    assert!(!none.reached_cap);

    let up = gain(1, 0.0, 350.0).expect("a gain");
    assert_eq!(
        up.progress,
        Progress {
            level: 3,
            experience: 0.0
        }
    );
    let rise = up.leveled_up.expect("leveled up");
    assert_eq!((rise.from, rise.to), (1, 3));

    let capped = gain(9, 0.0, 1_000_000.0).expect("a gain");
    assert!(capped.reached_cap);
    assert_eq!(capped.progress.level, 10);
}

#[test]
fn the_progression_report_lists_what_was_asked_in_order() {
    let report = MeasureProgressionArgs {
        experience: vec![0.0, 150.0, 1e9],
        levels: vec![1, 2, 10, 11],
    }
    .run(&catalog())
    .expect("a report");
    assert_eq!(report.contract_version, crate::CONTRACT_VERSION);
    assert_eq!(
        report
            .at_experience
            .iter()
            .map(|a| a.level)
            .collect::<Vec<_>>(),
        [1, 2, 10]
    );
    assert_eq!(
        report
            .to_level
            .iter()
            .map(|t| (t.level, t.experience))
            .collect::<Vec<_>>(),
        [
            (1, Some(0.0)),
            (2, Some(150.0)),
            (10, Some(3_150.0)),
            (11, None)
        ]
    );
}

#[test]
fn a_loot_report_is_the_same_for_the_same_seed_and_lists_everything() {
    let catalog = catalog();
    let args = MeasureLootArgs {
        table: 1,
        item_level: 10,
        seed: 2u64.pow(60) + 1,
        trials: 300,
    };
    let report = args.run(&catalog).expect("a report");
    assert_eq!(report, args.run(&catalog).expect("a report"));
    assert_eq!(report.trials, 300);
    assert_eq!(report.drops.iter().map(|b| b.count).sum::<u32>(), 300);
    assert_eq!(
        report.items,
        report.drops.iter().map(|b| b.value * b.count).sum::<u32>()
    );
    // Every rarity and every affix of the content, by id.
    assert_eq!(
        report.rarities.iter().map(|r| r.id).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(
        report.affixes.iter().map(|a| a.id).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );
    assert_eq!(
        report.rarities.iter().map(|r| r.count).sum::<u32>(),
        report.items
    );
    for affix in &report.affixes {
        assert_eq!(affix.value.is_none(), affix.count == 0);
    }

    let unknown = MeasureLootArgs {
        table: 9,
        ..args.clone()
    }
    .run(&catalog)
    .expect_err("no table");
    assert_eq!(pair(&unknown), (ErrorCode::UnknownTable, "table"));
}

// ---- random numbers -------------------------------------------------------

#[test]
fn a_stream_is_the_state_the_core_gives() {
    let args = RngStreamArgs {
        seed: u64::MAX - 5,
        stream: 3,
    };
    assert_eq!(args.run(), Rng::stream(Seed::from(u64::MAX - 5), 3).state());
    assert_ne!(
        args.run(),
        RngStreamArgs {
            stream: 4,
            ..args.clone()
        }
        .run()
    );
}

#[test]
fn a_draw_returns_the_value_and_the_next_state() {
    let first = RngNextF64Args { rng: 99 }.run();
    let mut rng = Rng::new(Seed::from(99));
    assert_eq!(first.value, rng.next_f64());
    assert_eq!(first.rng, rng.state());
    assert!((0.0..1.0).contains(&first.value));

    let below = RngBelowArgs {
        rng: first.rng,
        n: 6,
    }
    .run()
    .expect("a draw");
    assert!(below.value < 6);
    assert_eq!(
        RngBelowArgs {
            rng: first.rng,
            n: 6
        }
        .run()
        .expect("a draw"),
        below
    );

    let none = RngBelowArgs { rng: 1, n: 0 }
        .run()
        .expect_err("n is at least 1");
    assert_eq!(pair(&none), (ErrorCode::InvalidRange, "n"));
    assert_eq!(
        RngBelowArgs { rng: 1, n: 1 }.run().expect("a draw").value,
        0
    );
}

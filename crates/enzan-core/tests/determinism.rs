//! Checks that the core gives the same bits natively and in WebAssembly.
//!
//! Each test drives one module through a fixed input, folds every output into a
//! FNV-1a 64 bit digest and compares it with a pinned value. The pins are the
//! output of the implementation, taken natively; the same test run on
//! `wasm32-wasip1` must reach the same values. A mismatch names the module and
//! prints both digests in hexadecimal.
//!
//! Changing a pin is a breaking change, for the same reason as the rules in
//! the [`enzan_core::rng`] module doc: it means the outputs of the core moved.
//!
//! The WebAssembly run is `just wasm-test`.

use core::num::{NonZeroU32, NonZeroU64};

use enzan_core::balance::{Trials, measure_loot};
use enzan_core::catalog::{BaseDef, BaseId, Catalog, CatalogParts, DropTableDef, DropTableId};
use enzan_core::curve::{Curve, Level};
use enzan_core::loot::{
    AffixDef, AffixGroup, AffixId, AffixKind, AffixPool, AffixSpec, Chance, CountRange, DropTable,
    Rarity, RarityId, roll_item,
};
use enzan_core::progression::{CapPolicy, Experience, Progress, Progression};
use enzan_core::rng::{Rng, Seed, WeightedTable};
use enzan_core::stat::{Bounds, Modifier, ModifierKind, ModifierSet, StatDef, StatKey};

/// FNV-1a, 64 bit.
struct Digest(u64);

impl Digest {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn u64(&mut self, x: u64) {
        for byte in x.to_le_bytes() {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn f64(&mut self, x: f64) {
        // A NaN payload is not the same on every target.
        assert!(x.is_finite(), "a non-finite value reached the digest: {x}");
        self.u64(x.to_bits());
    }
}

fn pin(module: &str, pinned: u64, actual: u64) {
    assert_eq!(
        pinned, actual,
        "{module}: pinned {pinned:#018x}, got {actual:#018x}"
    );
}

fn lvl(n: u32) -> Level {
    Level::try_from(n).unwrap()
}

fn nz32(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}

fn key(s: &str) -> StatKey {
    StatKey::try_from(s).unwrap()
}

#[test]
fn rng() {
    let seed = Seed::from(0x0123_4567_89ab_cdef);
    let mut d = Digest::new();
    let mut r = Rng::new(seed);
    for _ in 0..2000 {
        d.u64(r.next_u64());
    }
    for _ in 0..2000 {
        d.f64(r.next_f64());
    }
    let n = NonZeroU64::new(1000).unwrap();
    for _ in 0..2000 {
        d.u64(r.below(n));
    }
    for stream in 1..=3 {
        let mut s = Rng::stream(seed, stream);
        for _ in 0..100 {
            d.u64(s.next_u64());
        }
    }
    pin("rng", 0x0a70_c3c7_37bd_9cb6, d.0);
}

#[test]
fn stat() {
    let kinds = [
        ModifierKind::Added,
        ModifierKind::Increased,
        ModifierKind::More,
    ];
    let mut r = Rng::new(Seed::from(11));
    let mut d = Digest::new();
    for i in 0..48_u32 {
        let mut set = ModifierSet::new();
        for _ in 0..=r.below(NonZeroU64::new(8).unwrap()) {
            let kind = kinds[r.below(NonZeroU64::new(3).unwrap()) as usize];
            let stat = if r.below(NonZeroU64::new(5).unwrap()) == 0 {
                "other"
            } else {
                "life"
            };
            // Hundredths from -1.00 to 1.99; More stays at or above -1.
            let value = (r.below(NonZeroU64::new(300).unwrap()) as f64 - 100.0) / 100.0;
            let value = if kind == ModifierKind::Added {
                value * 50.0
            } else {
                value
            };
            set.push(Modifier::new(key(stat), kind, value).unwrap());
        }
        // Every third stat is clamped tight so the clamp is exercised.
        let bounds = if i % 3 == 0 {
            Bounds::new(-20.0, 90.0).unwrap()
        } else {
            Bounds::UNBOUNDED
        };
        let def = StatDef::new(key("life"), f64::from(i) * 7.5 - 40.0, bounds).unwrap();
        d.f64(set.evaluate(&def).unwrap());
    }
    pin("stat", 0x0638_59db_b088_82eb, d.0);
}

#[test]
fn curve() {
    let polynomial = Curve::polynomial(vec![12.5, 3.25, 0.125, 0.001]).unwrap();
    let exponential = Curve::exponential(10.0, 1.0037).unwrap();
    let table = Curve::table((1..=2000).map(|i| f64::from(i) * 1.5 + 0.25).collect()).unwrap();
    let mut d = Digest::new();
    for l in 1..=2000 {
        let level = lvl(l);
        d.f64(polynomial.value(level).unwrap());
        d.f64(exponential.value(level).unwrap());
        d.f64(table.value(level).unwrap());
    }
    pin("curve", 0x0775_ca0e_e9f4_c432, d.0);
}

#[test]
fn progression() {
    let rule = Progression::new(
        Curve::polynomial(vec![100.0, 25.0, 3.5]).unwrap(),
        lvl(40),
        CapPolicy::Keep,
    )
    .unwrap();
    let mut r = Rng::new(Seed::from(21));
    let mut progress = Progress::new(lvl(1), Experience::ZERO);
    let mut d = Digest::new();
    for _ in 0..60 {
        let amount = r.below(NonZeroU64::new(6000).unwrap()) as f64 / 3.0;
        let gain = rule
            .gain(progress, Experience::try_from(amount).unwrap())
            .unwrap();
        progress = gain.progress();
        d.u64(u64::from(progress.level().get()));
        d.f64(progress.experience().get());
    }
    pin("progression", 0xf427_be5d_79f9_586d, d.0);
}

fn modifier(kind: ModifierKind, value: f64) -> Modifier {
    Modifier::new(key("life"), kind, value).unwrap()
}

fn affix(id: u32, kind: AffixKind, group: u32, required: u32, low: f64, high: f64) -> AffixDef {
    AffixDef::new(AffixSpec {
        id: AffixId::from(id),
        kind,
        tier: 1,
        required_level: lvl(required),
        weight: nz32(10 + id),
        group: AffixGroup::from(group),
        low: modifier(ModifierKind::Added, low),
        high: modifier(ModifierKind::Added, high),
    })
    .unwrap()
}

#[test]
fn loot() {
    let pool = AffixPool::new(vec![
        affix(1, AffixKind::Prefix, 1, 1, 1.0, 10.0),
        affix(2, AffixKind::Prefix, 1, 10, 8.0, 25.5),
        affix(3, AffixKind::Prefix, 2, 1, 0.5, 3.75),
        affix(4, AffixKind::Prefix, 3, 20, 30.0, 60.0),
        affix(5, AffixKind::Suffix, 4, 1, 2.0, 9.0),
        affix(6, AffixKind::Suffix, 4, 15, 9.0, 18.0),
        affix(7, AffixKind::Suffix, 5, 1, 0.1, 0.9),
        affix(8, AffixKind::Suffix, 6, 25, 40.0, 41.5),
    ])
    .unwrap();
    let rarity = |id: u32, min: u32, max: u32, prefixes: u32, suffixes: u32| {
        (
            Rarity::new(
                RarityId::from(id),
                CountRange::new(min, max).unwrap(),
                prefixes,
                suffixes,
            ),
            nz32(10 * (4 - id)),
        )
    };
    let rarities = WeightedTable::new(vec![
        rarity(1, 0, 0, 0, 0),
        rarity(2, 1, 2, 1, 1),
        rarity(3, 3, 5, 3, 3),
    ])
    .unwrap();

    let mut r = Rng::new(Seed::from(31));
    let mut d = Digest::new();
    for i in 0..300_u32 {
        let item = roll_item(i, &rarities, &pool, lvl(1 + i % 40), &mut r);
        d.u64(u64::from(item.rarity().get()));
        for a in item.affixes() {
            d.u64(u64::from(a.id().get()));
            d.f64(a.modifier().value());
        }
    }
    pin("loot", 0xd68b_9a7e_310a_dd61, d.0);
}

#[test]
fn balance() {
    let affixes = AffixPool::new(vec![
        affix(1, AffixKind::Prefix, 1, 1, 1.0, 10.0),
        affix(2, AffixKind::Prefix, 1, 10, 8.0, 25.5),
        affix(3, AffixKind::Prefix, 2, 1, 0.5, 3.75),
        affix(4, AffixKind::Prefix, 3, 20, 30.0, 60.0),
        affix(5, AffixKind::Suffix, 4, 1, 2.0, 9.0),
        affix(6, AffixKind::Suffix, 4, 15, 9.0, 18.0),
        affix(7, AffixKind::Suffix, 5, 1, 0.1, 0.9),
        affix(8, AffixKind::Suffix, 6, 25, 40.0, 41.5),
    ])
    .unwrap();
    let rarity = |id: u32, min: u32, max: u32, prefixes: u32, suffixes: u32| {
        (
            Rarity::new(
                RarityId::from(id),
                CountRange::new(min, max).unwrap(),
                prefixes,
                suffixes,
            ),
            nz32(10 * (4 - id)),
        )
    };
    let table = |id: u32, chance: f64, min: u32, max: u32, bases: &[u32]| {
        DropTableDef::new(
            DropTableId::from(id),
            DropTable::new(
                Chance::try_from(chance).unwrap(),
                CountRange::new(min, max).unwrap(),
                WeightedTable::new(
                    bases
                        .iter()
                        .map(|&b| (BaseId::from(b), nz32(b + 1)))
                        .collect(),
                )
                .unwrap(),
            ),
        )
    };
    let catalog = Catalog::new(CatalogParts {
        stats: vec![
            StatDef::new(key("life"), 100.0, Bounds::UNBOUNDED).unwrap(),
            StatDef::new(key("other"), 0.0, Bounds::UNBOUNDED).unwrap(),
        ],
        bases: (1..=3)
            .map(|id| BaseDef::new(BaseId::from(id), format!("base {id}")))
            .collect(),
        rarities: WeightedTable::new(vec![
            rarity(1, 0, 0, 0, 0),
            rarity(2, 1, 2, 1, 1),
            rarity(3, 3, 5, 3, 3),
        ])
        .unwrap(),
        affixes,
        drop_tables: vec![
            table(1, 0.8, 1, 3, &[1, 2, 3]),
            table(2, 0.25, 2, 2, &[2, 3]),
        ],
        progression: None,
    })
    .unwrap();

    let mut d = Digest::new();
    for (table, level, seed) in [(1, 30, 41), (2, 12, 42)] {
        let report = measure_loot(
            &catalog,
            DropTableId::from(table),
            lvl(level),
            Seed::from(seed),
            Trials::try_from(2_000).unwrap(),
        )
        .unwrap();
        d.u64(u64::from(report.trials()));
        d.u64(u64::from(report.items()));
        for (value, count) in report.drops().iter() {
            d.u64(u64::from(value));
            d.u64(u64::from(count));
        }
        for (id, count) in report.rarities() {
            d.u64(u64::from(id.get()));
            d.u64(u64::from(*count));
        }
        for (value, count) in report.affix_count().iter() {
            d.u64(u64::from(value));
            d.u64(u64::from(count));
        }
        for (id, stats) in report.affixes() {
            d.u64(u64::from(id.get()));
            d.u64(u64::from(stats.count()));
            if let Some(s) = stats.value() {
                for v in [s.min(), s.mean(), s.p50(), s.p90(), s.p99(), s.max()] {
                    d.f64(v);
                }
            }
        }
    }
    pin("balance", 0xf635_3cea_9433_fe8d, d.0);
}

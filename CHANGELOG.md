# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Runtime lists of the interface's closed sets in `enzan-wasm`: `modifierKinds()`,
  `errorCodes()`, and `Content::statKeys` on loaded content.
- `StatKey::as_str` in `enzan-core`.
- `sheet` in `enzan-core`: `Sheet`, the modifier sets of a character kept by
  `SourceId`, and `Catalog::evaluate_sheet`, which evaluates them in ascending
  id and, within a source, in the order added. `Catalog::check_item` says
  whether the catalog can roll an item at an item level (`ItemError` tells why
  not); `ItemRoll` now holds the `item_level` it was rolled at and gives its
  `modifiers()`; `BaseDef` takes an optional `ItemClass`, which nothing in the
  core reads. A source id, a class and a stat key share one rule, `KeyError`.
- `enzan-wasm`: `evaluateSheet`, `checkItem` and `bases()`; `class` on a base in
  the spec; an item result carries `itemLevel`; and the codes `invalid-key`,
  `duplicate-source`, `unknown-rarity`, `unknown-affix` and `invalid-item`.
  Arguments skip fields they do not know, so an item `rollDrop` gave can be
  passed to `checkItem` as it is; contract version stays 1.
- `rng` in `enzan-core`: `Seed`, `Rng` (SplitMix64, same sequence natively and in
  WebAssembly) and `WeightedTable` for seeded, integer-weighted draws.
- `stat` in `enzan-core`: `StatKey`, `Bounds`, `StatDef`, `Modifier` and
  `ModifierSet` for stats built from Added, Increased and More modifiers,
  evaluated in the order they were added and clamped to bounds.
- `curve` in `enzan-core`: `Level` and `Curve` (polynomial, exponential and
  table shapes; the power is `libm::pow`, so values match natively and in
  WebAssembly).
- `progression` in `enzan-core`: `Experience`, `Progress`, `Progression` and
  `Progression::gain`, which returns a `Gain`: the new state, the
  `LeveledUp` if any, and `reached_cap`; and a `CapPolicy` of `Discard` or
  `Keep` at the cap.
- `loot` in `enzan-core`: `DropTable` (chance, count, weighted bases) and
  `roll_item`, which draws a `Rarity` and then the affixes of an `AffixPool`
  by level, kind limit and exclusive group; the order of draws is part of the
  contract and a roll does not fail.
- `catalog` in `enzan-core`: `Catalog`, the stats, bases, rarities, affixes and
  drop tables of a game checked as a whole (unique ids, references that
  resolve), with `roll_drop`, `roll_item` and `evaluate_stats` against it.
- `balance` in `enzan-core`: `measure_loot` runs seeded trials of a drop table
  and returns a `LootReport` (counts and `Summary` percentiles, the same bits
  natively and in WebAssembly); `measure_progression` reports what a levelling
  rule asks for.
- `Rng::state`, so a game can save a sequence and resume it with
  `Rng::new(Seed::from(rng.state()))`.
- `enzan-wasm`: the WebAssembly interface for TypeScript (contract version 1).
  A `Content` handle is loaded once from a spec, then `rollDrop`, `rollItem`,
  `evaluateStats`, `gainExperience`, `measureLoot` and `measureProgression` are
  calls on it, with `rngStream`, `rngNextF64` and `rngBelow` for the random
  state, which the game owns as a `bigint`.
- `enzan-wasm` types are generated from the Rust ones (tsify); an error is
  thrown as an `EnzanError` with a closed set of `code`s and an `at`, and
  `Content.validate` returns every one found.
- `lab/`, the Balance Lab: a Vite and TypeScript page that edits the content,
  validates it with places for the errors, and runs `measureLoot` to show a
  loot report (`just lab-dev`).
- `just wasm-smoke` calls the generated `enzan-wasm` package from node, and
  `just lab-build` builds the lab; both are part of `just check`, which now
  needs `wasm-bindgen-cli` at the version in `Cargo.lock`.
- `arena/`, a minimal game that uses the WebAssembly interface to find where it
  is awkward: a Vite and TypeScript page with a pure, deterministic simulation
  (`arena/src/sim`), a Canvas 2D renderer and a saved game (`just arena-dev`).
  What the game has to compute because the core does not is in
  `arena/src/sim/glue`, each marked `FINDING`.
- `just arena-build`, `just arena-test` and `just arena-smoke` type-check and
  build the arena, run its `node --test` tests, and run 10,000 headless ticks
  with the real package (the same digest twice, and after a save and a load);
  all three are part of `just check`. `just wasm-pkg` now writes the package to
  `arena/src/pkg` as well as `lab/src/pkg`.
- Workspace dependency `thiserror`.
- Workspace dependency `libm`.
- Workspace dependencies `wasm-bindgen`, `js-sys`, `serde`, `serde_json`,
  `serde-wasm-bindgen`, `serde_path_to_error` and `tsify`.

### Changed

- Evaluating through a `Sheet` orders sources by id. A caller that joined one
  set per source in another order (the arena did, by level and then slot) gets
  different bits for the same modifiers.
- `StatKeyError` is now an alias of `KeyError`, whose messages say "key" and not
  "stat key". `BaseDef::new` takes a third argument, the optional class.
- `just check` builds and lints for `wasm32-unknown-unknown` and runs the `enzan-core`
  tests as WebAssembly (`wasm32-wasip1`, under node 22), including a
  `determinism` test that pins one digest per module.
- `clippy.toml` forbids std's transcendental functions and `powi`; use `libm`.

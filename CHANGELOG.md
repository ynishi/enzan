# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
- Workspace dependency `thiserror`.
- Workspace dependency `libm`.

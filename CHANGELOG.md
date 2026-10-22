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
- Workspace dependency `thiserror`.
- Workspace dependency `libm`.

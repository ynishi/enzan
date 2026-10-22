# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `rng` in `enzan-core`: `Seed`, `Rng` (SplitMix64, same sequence natively and in
  WebAssembly) and `WeightedTable` for seeded, integer-weighted draws.
- Workspace dependency `thiserror`.

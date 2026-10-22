//! enzan (演算, "computation"): pure game mechanics and math (progression,
//! modifiers, loot, director) for hack-and-slash games, shared by WASM and
//! native simulation.
//!
//! The crate to depend on. It re-exports `enzan-core` (from the core's first
//! module on); the WASM bindings (`enzan-wasm`) and the headless simulation
//! (`enzan-sim`) sit beside it and also depend only on the core.

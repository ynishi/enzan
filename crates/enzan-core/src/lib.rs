//! Core of enzan (演算, "computation"): pure game mechanics and math for
//! hack-and-slash games.
//!
//! Every other crate depends on this one; this one depends on none of them.
//!
//! # Purity
//!
//! The core reads no clock, does no I/O and renders nothing. Time, seeds and
//! measurements come in as arguments. Nothing it returns may depend on the
//! platform or on iteration order the language leaves unspecified, so the same
//! inputs give the same result natively and in WebAssembly.
//!
//! Because the same result is promised natively and in WebAssembly, the
//! transcendental functions are called through the `libm` crate. std's math
//! functions return different bits on each target (native, `wasm32-wasip1` and
//! `wasm32-unknown-unknown`), and running the tests on `wasm32-wasip1` stands in
//! for the browser only under this condition. `clippy.toml` forbids them with
//! `disallowed-methods`, which enforces it.

#![warn(missing_docs)]

pub mod balance;
pub mod catalog;
pub mod curve;
mod key;
pub mod loot;
pub mod progression;
pub mod rng;
pub mod sheet;
pub mod stat;

pub use key::KeyError;

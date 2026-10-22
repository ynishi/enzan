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

#![warn(missing_docs)]

pub mod rng;

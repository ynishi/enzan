# enzan

enzan (演算, "computation") is the math and mechanics of hack-and-slash games
without the rendering: progression and level curves, stat modifiers, loot and
affix generation, and the director that paces enemy waves.

It is built for games whose fun is in the numbers — levels and weapons that
inflate, maps that stay fixed while what appears on them is random. The same
core runs in two places:

- **In the browser**, compiled to WebAssembly and called from TypeScript, inside
  the game itself and in a balance lab that redraws curves and distributions as
  coefficients move.
- **Natively, without a screen**, running many seeded trials to measure balance
  before anyone plays.

## Promises

- **Deterministic.** The same inputs and seed give the same result, natively
  and in WebAssembly.
- **Pure.** The core reads no clock, does no I/O and renders nothing. Time,
  seeds and measurements come in as arguments.
- **One formula, one place.** Each mechanic is defined once, so a change of
  coefficient reaches the game, the lab and the simulation together.

## Crates

| Crate        | Role                                                                    |
| ------------ | ----------------------------------------------------------------------- |
| `enzan`      | What you depend on. Re-exports the core.                                |
| `enzan-core` | The mechanics and math. Pure; depends on no other crate here.           |
| `enzan-wasm` | WebAssembly bindings for TypeScript. Converts values and calls the core. |
| `enzan-sim`  | Headless native simulation. Supplies seeds and calls the core.          |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md), and
[PUBLIC_DEVELOPMENT.md](PUBLIC_DEVELOPMENT.md) for what may be published.
Security reports: [SECURITY.md](SECURITY.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

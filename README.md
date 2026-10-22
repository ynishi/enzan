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

## In the browser

`enzan-wasm` is read once and then called once per game event. Load the content
into a `Content`, then call a method of it for each defeat or level gained:

```ts
import init, { Content, rngStream } from "./pkg/enzan_wasm.js";

await init();
const content = Content.load(spec); // throws an EnzanError if the spec is wrong

// One stream of the seed for drops. Use another stream for anything else.
let rng = rngStream({ seed: 12345n, stream: 1n });

const drop = content.rollDrop({ table: 1, itemLevel: 20, rng });
rng = drop.rng; // pass the returned state to the next call
console.log(drop.items);
```

The package is made by `just wasm-pkg` (`wasm-bindgen --target web`); the types
come from the Rust ones. The rules of the interface, with the error codes, are
in the crate docs of `enzan-wasm` (`cargo doc`).

- **The game owns the random state.** It is a `bigint`; save it with
  `rng.toString()` and restore it with `BigInt(s)`, since `JSON.stringify`
  throws on a `bigint`. A `number` below 2^53 is accepted for a seed or state
  too, but use a `bigint`. Do not share one state between a drop and anything
  else (crits, spawns): take another stream with `rngStream` for each use, or a
  replay needs the game's calls in the same order too.
- **Errors.** A method throws an `Error` named `EnzanError`; tell it with
  `isEnzanError(e)` and read `e.code` and `e.at` (where in the input).
  `Content.validate(spec)` returns every problem and does not throw. The
  generated `.d.ts` does not show what a method throws.
- **Freeing.** `content.free()` (or `using content = ...`) releases it; if you
  do not call it, it is released when collected. To load new content, load a new
  `Content` and free the old one. A call on a freed `Content` throws a plain
  `Error`, not an `EnzanError`.
- **Cost of a call.** Crossing the boundary with a small object is about 3
  microseconds (`rollDrop`, measured by `just wasm-smoke`), so call per game
  event and not per number.

The Balance Lab in [`lab/`](lab) is a page that edits the content, validates it
with places, and runs `measureLoot` to show what drops. `just lab-dev` serves it.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md), and
[PUBLIC_DEVELOPMENT.md](PUBLIC_DEVELOPMENT.md) for what may be published.
Security reports: [SECURITY.md](SECURITY.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

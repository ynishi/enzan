// Runs the arena headless with the real generated package: 10,000 ticks of a
// scripted player, twice for the same digest, and once more with a save and a
// load in the middle. `just arena-smoke` runs it after `just arena-test`, which
// leaves the emitted sim in build-sim/.
//
// It loads the `--target web` package by handing `initSync` the bytes of the
// .wasm, as lab/scripts/smoke.mjs does.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const root = new URL("../", import.meta.url);
const pkgDir = new URL("src/pkg/", root);
const simDir = new URL("build-sim/src/sim/", root);
const pkg = await import(new URL("enzan_wasm.js", pkgDir));
const { makeCore, advance } = await import(new URL("session.js", simDir));
const { newWorld } = await import(new URL("world.js", simDir));
const { stampOf, digest, saveText, loadText } = await import(new URL("save.js", simDir));

pkg.initSync({ module: readFileSync(new URL("enzan_wasm_bg.wasm", pkgDir)) });

const readJson = (name) => JSON.parse(readFileSync(new URL(name, root), "utf8"));
const spec = readJson("content.json");
const tuning = readJson("tuning.json");
assert.deepEqual(pkg.Content.validate(spec), []);
assert.deepEqual(pkg.modifierKinds(), ["added", "increased", "more"]);

const content = pkg.Content.load(spec);
assert.deepEqual(content.statKeys(), spec.stats.map((s) => s.key).sort());
const core = makeCore(pkg, content);
const stamp = stampOf(spec, tuning, pkg.contractVersion());

const TICKS = 10_000;
const SEED = 2n ** 60n + 7n;

/**
 * What the player asks for at a tick: round a square, 120 ticks to a side,
 * then a pause. Piecewise by tick, so there is no trigonometry to differ.
 */
function scripted(tick) {
  const phase = Math.floor(tick / 120) % 5;
  if (phase === 0) return { dx: 1, dy: 0 };
  if (phase === 1) return { dx: 0, dy: 1 };
  if (phase === 2) return { dx: -1, dy: 0 };
  if (phase === 3) return { dx: 0, dy: -1 };
  return { dx: 0, dy: 0 };
}

const results = [];
function check(name, fn) {
  try {
    fn();
  } catch (e) {
    console.log(`FAIL ${name}`);
    throw e;
  }
  results.push(name);
  console.log(`ok   ${name}`);
}

function run(until) {
  return advance(newWorld(SEED, core, tuning), core, tuning, scripted, until);
}

// A throw out of the interface goes up through `advance` and fails this check
// by name; the count is checked as well, in case anything ever catches one.
let first;
check("the interface throws nothing in 10,000 ticks", () => {
  first = run(TICKS);
  assert.equal(first.findings.thrown, 0);
});
const firstDigest = digest(first);
const { findings } = first;

console.log(
  `tick ${first.tick}, over ${first.over}, level ${first.player.progress.level}, ` +
    `${findings.defeats} defeats, equipment ${Object.keys(first.equipment).join(",") || "none"}`,
);

check("a defeat, a level gained and an item equipped", () => {
  assert.ok(findings.defeats >= 1, "no defeat");
  assert.ok(first.player.progress.level > 1, "no level gained");
  assert.ok(Object.keys(first.equipment).length >= 1, "no item equipped");
});

check("the same seed and inputs give the same digest", () => {
  assert.equal(digest(run(TICKS)), firstDigest);
});

check("a save at tick 5,000, loaded from its text, continues to the same digest", () => {
  const half = run(TICKS / 2);
  assert.equal(half.tick, TICKS / 2, "the player died before the save");
  const text = saveText(half, stamp);
  const loaded = loadText(text, stamp, core, tuning);
  assert.ok(loaded.ok, loaded.ok ? "" : loaded.reason);
  assert.equal(digest(loaded.world), digest(half));
  const end = advance(loaded.world, core, tuning, scripted, TICKS - half.tick);
  assert.equal(digest(end), firstDigest);
});

console.log(`calls ${JSON.stringify(findings.calls)}`);
console.log(`glue  ${JSON.stringify(findings.glue)}`);
content.free();
console.log(`smoke: ${results.length} checks passed, digest ${firstDigest}`);

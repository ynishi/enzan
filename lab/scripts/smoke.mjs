// Calls the generated package from node, where the boundary is checked: what
// the native tests of `enzan-wasm` cannot reach (JsValue in, a thrown Error
// out, bigint, free). `just wasm-smoke` runs it after `just wasm-pkg`.
//
// It loads the `--target web` package that the lab imports, by handing
// `initSync` the bytes of the .wasm, so no second build is made for node.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const pkgDir = new URL("../src/pkg/", import.meta.url);
const pkg = await import(new URL("enzan_wasm.js", pkgDir));
const fixture = new URL(
  "../../crates/enzan-wasm/tests/fixtures/content.json",
  import.meta.url,
);

pkg.initSync({ module: readFileSync(new URL("enzan_wasm_bg.wasm", pkgDir)) });
const {
  Content,
  contractVersion,
  errorCodes,
  isEnzanError,
  modifierKinds,
  rngStream,
  rngNextF64,
} = pkg;

const spec = JSON.parse(readFileSync(fixture, "utf8"));
const table = spec.dropTables[0].id;
const clone = (value) => JSON.parse(JSON.stringify(value));

/** The error `fn` throws; fails when it does not throw. */
function thrown(fn) {
  try {
    fn();
  } catch (e) {
    return e;
  }
  assert.fail("expected a throw");
}

const results = [];
function check(name, fn) {
  fn();
  results.push(name);
  console.log(`ok   ${name}`);
}

check("contractVersion is 1", () => {
  assert.equal(contractVersion(), 1);
});

check("modifierKinds returns the closed set in order", () => {
  assert.deepEqual(modifierKinds(), ["added", "increased", "more"]);
});

check("errorCodes returns the closed set in ascending order", () => {
  const codes = errorCodes();
  assert.deepEqual(codes, [...new Set(codes)].sort());
  assert.ok(codes.includes("invalid-shape"));
  assert.ok(codes.includes("overflow"));
});

check("the sample content has no issues", () => {
  assert.deepEqual(Content.validate(spec), []);
});

check("a misspelled field is invalid-shape, at its place", () => {
  const top = clone(spec);
  top.typo = 1;
  const nested = clone(spec);
  nested.stats[0].typo = 1;
  const [topIssue] = Content.validate(top);
  assert.equal(topIssue.code, "invalid-shape");
  assert.equal(topIssue.at, "typo");
  const [nestedIssue] = Content.validate(nested);
  assert.equal(nestedIssue.code, "invalid-shape");
  assert.equal(nestedIssue.at, "stats[0].typo");
  assert.equal(thrown(() => Content.load(nested)).at, "stats[0].typo");
});

const content = Content.load(spec);
const rng0 = rngStream({ seed: 2n ** 60n + 7n, stream: 1n });

check("content.statKeys returns the keys in ascending order", () => {
  assert.deepEqual(
    content.statKeys(),
    spec.stats.map((s) => s.key).sort(),
  );
});

/** An item that dropped, with the fields `rollDrop` gives it. */
function droppedItem() {
  let rng = rng0;
  for (let i = 0; i < 200; i++) {
    const drop = content.rollDrop({ table, itemLevel: 20, rng });
    rng = drop.rng;
    const item = drop.items.find((it) => it.affixes.length > 0);
    if (item) return item;
  }
  assert.fail("no item with affixes dropped");
}

check("checkItem on a rollDrop item returns an equal item", () => {
  const item = droppedItem();
  assert.equal(item.itemLevel, 20);
  assert.deepEqual(content.checkItem({ item }), item);
  const e = thrown(() =>
    content.checkItem({ item: { ...item, affixes: [{ id: 9999, value: 1 }] } }),
  );
  assert.equal(e.code, "unknown-affix");
  assert.equal(e.at, "item.affixes[0].id");
});

check("evaluateSheet gives the same values whatever order the sources come in", () => {
  const item = droppedItem();
  const weapon = { id: "weapon", from: { item } };
  const level = {
    id: "level",
    from: { modifiers: [{ stat: "life", modifier: "added", value: 10 }] },
  };
  const stats = ["life", "damage"];
  const one = content.evaluateSheet({ sources: [weapon, level], stats });
  const other = content.evaluateSheet({ sources: [level, weapon], stats });
  assert.deepEqual(one, other);
  assert.deepEqual(
    one.map((s) => s.stat),
    stats,
  );
  const dup = thrown(() =>
    content.evaluateSheet({ sources: [level, weapon, level], stats }),
  );
  assert.equal(dup.code, "duplicate-source");
  assert.equal(dup.at, "sources[2].id");
});

check("bases() lists the bases in ascending id", () => {
  const ids = content.bases().map((b) => b.id);
  assert.deepEqual(
    ids,
    spec.bases.map((b) => b.id).sort((a, b) => a - b),
  );
  assert.ok(content.bases().every((b) => b.class === null));
});

check("an item level that is negative or fractional is invalid-shape", () => {
  for (const itemLevel of [-1, 1.5]) {
    const e = thrown(() => content.rollDrop({ table, itemLevel, rng: rng0 }));
    assert.equal(e.code, "invalid-shape", `itemLevel ${itemLevel}`);
  }
});

check("a seed above 2^53 replays, and the returned state continues", () => {
  assert.equal(typeof rng0, "bigint");
  const a = content.rollDrop({ table, itemLevel: 20, rng: rng0 });
  const b = content.rollDrop({ table, itemLevel: 20, rng: rng0 });
  assert.deepEqual(a, b);
  assert.notEqual(a.rng, rng0);
  const c = content.rollDrop({ table, itemLevel: 20, rng: a.rng });
  assert.notEqual(c.rng, a.rng);
  // The same two draws from the same start give the same pair.
  const again = content.rollDrop({ table, itemLevel: 20, rng: a.rng });
  assert.deepEqual(c, again);
  // A saved state restores through its text.
  const restored = BigInt(a.rng.toString());
  assert.deepEqual(content.rollDrop({ table, itemLevel: 20, rng: restored }), c);
  assert.deepEqual(rngNextF64({ rng: rng0 }), rngNextF64({ rng: rng0 }));
});

check("a number below 2^53 is read as the same bigint", () => {
  assert.deepEqual(
    content.rollDrop({ table, itemLevel: 20, rng: 12345 }),
    content.rollDrop({ table, itemLevel: 20, rng: 12345n }),
  );
});

check("a state that does not fit 64 bits is invalid-shape", () => {
  const e = thrown(() =>
    content.rollDrop({ table, itemLevel: 20, rng: 2n ** 64n + 5n }),
  );
  assert.equal(e.code, "invalid-shape");
  assert.equal(e.at, "rng");
  assert.equal(thrown(() => rngNextF64({ rng: 2n ** 64n + 5n })).code, "invalid-shape");
});

check("a loot report survives a JSON round trip", () => {
  const report = content.measureLoot({
    table,
    itemLevel: 20,
    seed: 2n ** 60n + 7n,
    trials: 2000,
  });
  assert.equal(report.trials, 2000);
  assert.deepEqual(JSON.parse(JSON.stringify(report)), report);
  const again = content.measureLoot({
    table,
    itemLevel: 20,
    seed: 2n ** 60n + 7n,
    trials: 2000,
  });
  assert.deepEqual(again, report);
});

check("a thrown error is an EnzanError with a code and an at", () => {
  const e = thrown(() => content.rollDrop({ table: 9999, itemLevel: 1, rng: rng0 }));
  assert.ok(e instanceof Error);
  assert.equal(e.name, "EnzanError");
  assert.equal(isEnzanError(e), true);
  assert.equal(e.code, "unknown-table");
  assert.equal(e.at, "table");
  assert.equal(typeof e.message, "string");
  assert.equal(isEnzanError(new Error("x")), false);
  assert.equal(isEnzanError(null), false);
  assert.equal(isEnzanError("EnzanError"), false);
});

// The cost of crossing the boundary, for the README: the content is loaded, so
// this is one small object in and one out per call.
{
  const calls = 10_000;
  let rng = rng0;
  const start = performance.now();
  for (let i = 0; i < calls; i++) {
    rng = content.rollDrop({ table, itemLevel: 20, rng }).rng;
  }
  const ms = performance.now() - start;
  console.log(
    `time rollDrop x${calls}: ${ms.toFixed(1)} ms, ${((ms * 1000) / calls).toFixed(2)} us per call`,
  );
}

check("a call after free() throws, and not an EnzanError", () => {
  content.free();
  const e = thrown(() => content.rollDrop({ table, itemLevel: 20, rng: rng0 }));
  assert.ok(e instanceof Error);
  assert.equal(isEnzanError(e), false);
});

console.log(`smoke: ${results.length} checks passed`);

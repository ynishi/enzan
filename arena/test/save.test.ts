import assert from "node:assert/strict";
import { test } from "node:test";

import { advance } from "../src/sim/session.ts";
import { canonical, fromSave, loadText, saveText, stampOf, toSave } from "../src/sim/save.ts";
import { newWorld } from "../src/sim/world.ts";
import { fakeCore, readTuning, scripted } from "./fake.ts";

const tuning = readTuning();
const stamp = stampOf({ content: 1 }, tuning, 1);

/** A world that has enemies, a level and equipment to save. */
function played() {
  const { core } = fakeCore();
  const world = advance(newWorld(2n ** 60n + 7n, core, tuning), core, tuning, scripted, 1500);
  assert.ok(world.enemies.length > 0, "no enemies");
  assert.ok(Object.keys(world.equipment).length > 0, "nothing equipped");
  return { core, world };
}

test("a save, loaded and saved again, is the same text", () => {
  const { core, world } = played();
  const text = saveText(world, stamp);
  const loaded = loadText(text, stamp, core, tuning);
  assert.ok(loaded.ok);
  assert.equal(saveText(loaded.world, stamp), text);
});

test("a loaded world equals the original apart from its findings", () => {
  const { core, world } = played();
  const loaded = loadText(saveText(world, stamp), stamp, core, tuning);
  assert.ok(loaded.ok);
  assert.deepEqual({ ...loaded.world, findings: null }, { ...world, findings: null });
});

test("the save is plain JSON: the random states are texts", () => {
  const { world } = played();
  const save = toSave(world, stamp);
  assert.equal(typeof save.seed, "string");
  assert.equal(typeof save.rng.drops, "string");
  assert.deepEqual(JSON.parse(JSON.stringify(save)), save);
});

test("canonical sorts keys, refuses what is not finite and writes -0 as 0", () => {
  assert.equal(canonical({ b: 1, a: { d: [2, { z: 1, y: 2 }], c: "x" } }), '{"a":{"c":"x","d":[2,{"y":2,"z":1}]},"b":1}');
  assert.equal(canonical({ a: undefined, b: 1 }), '{"b":1}');
  assert.equal(canonical(-0), "0");
  assert.equal(canonical({ x: -0 }), '{"x":0}');
  assert.throws(() => canonical(Number.NaN));
  assert.throws(() => canonical({ a: [Number.POSITIVE_INFINITY] }));
  assert.throws(() => canonical(1n));
});

test("a wrong format, stamp or shape is refused, with a reason", () => {
  const { core, world } = played();
  const save = toSave(world, stamp);
  const refused = (value: unknown, expected: string) => {
    const result = fromSave(value, stamp, core, tuning);
    assert.equal(result.ok, false);
    assert.ok(!result.ok && result.reason.includes(expected), !result.ok ? result.reason : "");
  };
  refused({ ...save, format: 2 }, "format");
  refused({ ...save, contentHash: "00000000" }, "content or tuning");
  refused({ ...save, contractVersion: 2 }, "contract 2");
  refused({ format: 1, contentHash: save.contentHash }, "contract undefined");
  refused({ ...save, seed: "abc" }, "seed");
  refused({ ...save, seed: "18446744073709551616" }, "seed");
  refused({ ...save, player: null }, "player");
  refused({ ...save, equipment: { head: save.equipment.weapon } }, "slot");
  refused({ ...save, player: { ...save.player, progress: { level: 2, experience: -1 } } }, "experience");
  refused({ ...save, director: { ...save.director, wave: tuning.waves.length } }, "director.wave");
  refused({ ...save, enemies: save.enemies.map((enemy) => ({ ...enemy, level: 99 })) }, "enemies[0].level");
  refused({ ...save, nextId: 1 }, "id");
  const [slot, worn] = Object.entries(save.equipment)[0] ?? [];
  assert.ok(slot !== undefined && worn !== undefined);
  const other = Object.keys(tuning.slots).find((base) => tuning.slots[base] !== slot);
  refused({ ...save, equipment: { [slot]: { ...worn, base: Number(other) } } }, `equipment.${slot}.base`);
  refused({ ...save, equipment: { [slot]: { ...worn, base: 99 } } }, `equipment.${slot}.base`);
  assert.deepEqual(loadText("not json", stamp, core, tuning), { ok: false, reason: "save is not JSON" });
});

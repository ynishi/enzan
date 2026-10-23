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

test("a saved item is an item input: ids and values, no stat or modifier", () => {
  const { world } = played();
  const save = toSave(world, stamp);
  const items = [...Object.values(save.equipment), ...save.floor.map((found) => found.item)];
  assert.ok(items.some((saved) => saved.affixes.length > 0));
  for (const saved of items) {
    assert.deepEqual(Object.keys(saved).sort(), ["affixes", "base", "itemLevel", "rarity"]);
    for (const affix of saved.affixes) {
      assert.deepEqual(Object.keys(affix).sort(), ["id", "value"]);
    }
  }
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
  refused({ ...save, format: 3 }, "format 3 is not 2");
  refused({ ...save, contentHash: "00000000" }, "content or tuning");
  refused({ ...save, contractVersion: 2 }, "contract 2");
  refused({ format: 2, contentHash: save.contentHash }, "contract undefined");
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
  const other = core
    .bases()
    .find((base) => base.class !== null && tuning.slotOfClass[base.class] !== slot);
  assert.ok(other !== undefined);
  refused({ ...save, equipment: { [slot]: { ...worn, base: other.id } } }, `equipment.${slot}.base`);
  refused({ ...save, equipment: { [slot]: { ...worn, base: 99 } } }, `equipment.${slot}.base`);
  const { itemLevel: _itemLevel, ...withoutLevel } = worn;
  refused({ ...save, equipment: { [slot]: withoutLevel } }, `equipment.${slot}.itemLevel`);
  refused({ ...save, equipment: { [slot]: { ...worn, itemLevel: 0 } } }, `equipment.${slot}.itemLevel`);
  refused({ ...save, equipment: { [slot]: { ...worn, itemLevel: 1.5 } } }, `equipment.${slot}.itemLevel`);
  assert.deepEqual(loadText("not json", stamp, core, tuning), { ok: false, reason: "save is not JSON" });
});

test("a save of format 1 is refused, because its items have no item level", () => {
  const { core, world } = played();
  const save = toSave(world, stamp);
  const result = fromSave({ ...save, format: 1 }, stamp, core, tuning);
  assert.equal(result.ok, false);
  assert.ok(!result.ok && result.reason.includes("no item level"), !result.ok ? result.reason : "");
});

test("an item the content cannot roll is refused on load, wherever it is", () => {
  const { core, world } = played();
  const save = toSave(world, stamp);
  const [slot, worn] = Object.entries(save.equipment)[0] ?? [];
  assert.ok(slot !== undefined && worn !== undefined);
  // A value outside the range of its affix: the shape is right, the content cannot roll it.
  const outOfRange = { ...worn, affixes: worn.affixes.map((affix) => ({ ...affix, value: 999 })) };
  assert.ok(outOfRange.affixes.length > 0, "the worn item has no affix to break");

  const onBody = fromSave({ ...save, equipment: { [slot]: outOfRange } }, stamp, core, tuning);
  assert.equal(onBody.ok, false);
  assert.ok(
    !onBody.ok && onBody.reason.startsWith(`equipment.${slot}.affixes[0].value: `) && onBody.reason.includes("999"),
    !onBody.ok ? onBody.reason : "",
  );

  const lying = { id: save.nextId, x: 0, y: 0, item: outOfRange };
  const floor = fromSave(
    { ...save, floor: [...save.floor, lying], nextId: save.nextId + 1 },
    stamp,
    core,
    tuning,
  );
  assert.equal(floor.ok, false);
  assert.ok(
    !floor.ok &&
      floor.reason.startsWith(`floor[${save.floor.length}].item.affixes[0].value: `) &&
      floor.reason.includes("999"),
    !floor.ok ? floor.reason : "",
  );
});

test("a load has every item checked by the core and keeps the canonical one", () => {
  const { world } = played();
  const { core, calls } = fakeCore();
  const loaded = fromSave(toSave(world, stamp), stamp, core, tuning);
  assert.ok(loaded.ok);
  assert.equal(calls.checkItem, Object.keys(world.equipment).length + world.floor.length);
  assert.equal(calls.bases, 1);
  assert.equal(calls.evaluateSheet, 1);
  assert.deepEqual(loaded.world.equipment, world.equipment);
});

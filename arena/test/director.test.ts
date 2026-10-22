import assert from "node:assert/strict";
import { test } from "node:test";

import { direct } from "../src/sim/director.ts";
import { newWorld, type World } from "../src/sim/world.ts";
import { fakeCore, readTuning } from "./fake.ts";

const tuning = readTuning();

/** Ticks of the director alone, with nothing ever defeated. */
function spawnFor(seed: bigint, ticks: number, each?: (world: World) => void): World {
  const { core } = fakeCore();
  const world = newWorld(seed, core, tuning);
  for (let i = 0; i < ticks; i++) {
    direct(world, core, tuning);
    each?.(world);
    world.tick += 1;
  }
  return world;
}

test("the same seed gives the same spawns, another seed other ones", () => {
  const a = spawnFor(7n, 3000);
  const b = spawnFor(7n, 3000);
  assert.ok(a.enemies.length > 0);
  assert.deepEqual(a.enemies, b.enemies);
  assert.deepEqual(a.director, b.director);
  assert.notDeepEqual(spawnFor(8n, 3000).enemies, a.enemies);
});

test("live enemies never exceed the cap of the wave", () => {
  let reached = false;
  const last = tuning.waves.length - 1;
  const duration = tuning.waves.reduce((sum, wave) => sum + wave.duration, 0);
  const world = spawnFor(1n, duration, (w) => {
    const cap = tuning.waves[w.director.wave]?.cap ?? 0;
    assert.ok(w.enemies.length <= cap, `${w.enemies.length} live above the cap ${cap}`);
    reached ||= w.enemies.length === cap;
  });
  assert.ok(reached, "the cap was never reached, so the check proved nothing");
  assert.equal(world.director.wave, last);
});

test("enemies spawn on the border of the arena, at the level of the wave", () => {
  const world = spawnFor(3n, 600);
  const { width, height } = tuning.arena;
  for (const enemy of world.enemies) {
    const onBorder = enemy.x === 0 || enemy.x === width || enemy.y === 0 || enemy.y === height;
    assert.ok(onBorder, `enemy ${enemy.id} at ${enemy.x}, ${enemy.y}`);
    assert.equal(enemy.level, tuning.waves[0]?.level);
  }
});

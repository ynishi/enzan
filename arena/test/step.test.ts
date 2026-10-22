import assert from "node:assert/strict";
import { test } from "node:test";

import { advance } from "../src/sim/session.ts";
import { step } from "../src/sim/step.ts";
import { newWorld } from "../src/sim/world.ts";
import { fakeCore, readTuning, scripted } from "./fake.ts";

const tuning = readTuning();

test("the same seed and inputs give the same world", () => {
  const run = (seed: bigint) => {
    const { core } = fakeCore();
    return advance(newWorld(seed, core, tuning), core, tuning, scripted, 2000);
  };
  const a = run(5n);
  assert.ok(a.player.progress.level > 1 || a.floor.length > 0, "nothing happened");
  assert.deepEqual(a, run(5n));
  assert.notDeepEqual(a.rng, run(6n).rng);
});

test("a throw partway through a tick leaves the world given to advance whole", () => {
  const { core } = fakeCore();
  const world = advance(newWorld(3n, core, tuning), core, tuning, scripted, 600);
  const { findings: _findings, ...before } = structuredClone(world);
  const thrownBefore = world.findings.thrown;
  // A core whose first rollDrop throws, after the defeat has already been counted.
  const failing = {
    ...core,
    rollDrop: () => {
      throw new Error("rollDrop failed");
    },
  };

  assert.throws(() => advance(world, failing, tuning, scripted, 5000), /rollDrop failed/);

  const { findings: _after, ...after } = world;
  assert.deepEqual(after, before);
  assert.equal(world.findings.thrown, thrownBefore + 1);
});

test("a defeat calls gainExperience and rollDrop once each", () => {
  const { core, calls } = fakeCore();
  const world = newWorld(9n, core, tuning);
  // One enemy about to fall next to the player, and no spawn in this tick.
  world.director.spawnCooldown = 1000;
  world.enemies.push({
    id: world.nextId,
    level: 1,
    x: world.player.x + 5,
    y: world.player.y,
    life: 1,
    contactCooldown: 0,
  });
  world.nextId += 1;
  const before = { ...calls };

  step(world, { dx: 0, dy: 0 }, core, tuning);

  assert.equal(world.findings.defeats, 1);
  assert.equal(world.enemies.length, 0);
  assert.equal(calls.gainExperience - before.gainExperience, 1);
  assert.equal(calls.rollDrop - before.rollDrop, 1);
  assert.equal(world.floor.length + Object.keys(world.equipment).length, 1);
});

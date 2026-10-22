// Which enemies come, and when: the provisional director.
//
// FINDING: enzan promises a director that paces enemy waves and the core has
// none. This is a wave table with a cap on live enemies, kept in tuning.json
// and read here, so that a game has something to run while the shape of a
// core director is learned from using it.

import type { Core } from "./core.ts";
import { enemyStats } from "./glue/enemy.ts";
import type { Tuning, Wave, World } from "./world.ts";

/** The first tick after the wave at `index`. */
function waveEnd(waves: Wave[], index: number): number {
  let end = 0;
  for (let i = 0; i <= index; i++) {
    const wave = waves[i];
    if (wave !== undefined) {
      end += wave.duration;
    }
  }
  return end;
}

/** A place on the border of the arena, counted round from its top left. */
function borderPoint(arena: Tuning["arena"], position: number): { x: number; y: number } {
  const { width, height } = arena;
  if (position < width) {
    return { x: position, y: 0 };
  }
  if (position < width + height) {
    return { x: width, y: position - width };
  }
  if (position < 2 * width + height) {
    return { x: 2 * width + height - position, y: height };
  }
  return { x: 0, y: 2 * (width + height) - position };
}

/**
 * One tick of the director: moves on to the next wave when the time of this one
 * is up, and spawns an enemy when the wait is over and there are fewer live
 * enemies than the cap. Where it spawns is drawn on the spawn stream.
 */
export function direct(world: World, core: Core, tuning: Tuning): void {
  const { waves, arena } = tuning;
  const director = world.director;
  while (director.wave < waves.length - 1 && world.tick >= waveEnd(waves, director.wave)) {
    director.wave += 1;
  }
  const wave = waves[director.wave];
  if (wave === undefined) {
    throw new Error("tuning.json has no waves");
  }
  if (director.spawnCooldown > 0) {
    director.spawnCooldown -= 1;
  }
  if (director.spawnCooldown > 0 || world.enemies.length >= wave.cap) {
    return;
  }
  const drawn = core.rngBelow({ rng: world.rng.spawns, n: 2 * (arena.width + arena.height) });
  world.rng.spawns = drawn.rng;
  const at = borderPoint(arena, drawn.value);
  world.enemies.push({
    id: world.nextId,
    level: wave.level,
    x: at.x,
    y: at.y,
    life: enemyStats(tuning, wave.level, world.findings).life,
    contactCooldown: 0,
  });
  world.nextId += 1;
  director.spawnCooldown = wave.period;
}

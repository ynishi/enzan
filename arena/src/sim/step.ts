// One tick of the game, at 60 to the second. Time is counted in ticks.
//
// `step` changes `world` and nothing else. Every order is fixed: enemies and
// floor items are arrays in creation order (ids increase), removing one keeps
// the order, and the nearest enemy ties break on the lower id. An error out of
// the interface is not caught here; it goes up to the caller.

import type { Core } from "./core.ts";
import { direct } from "./director.ts";
import { hitDamage } from "./glue/combat.ts";
import { enemyStats } from "./glue/enemy.ts";
import { playerStats, slotOf } from "./glue/equip.ts";
import type { Enemy, FloorItem, Tuning, World } from "./world.ts";

/** Where the player asks to go: each of the two is -1, 0 or 1. */
export interface Input {
  dx: number;
  dy: number;
}

function squaredDistance(ax: number, ay: number, bx: number, by: number): number {
  const dx = ax - bx;
  const dy = ay - by;
  return dx * dx + dy * dy;
}

function movePlayer(world: World, input: Input, tuning: Tuning): void {
  const length = Math.sqrt(input.dx * input.dx + input.dy * input.dy);
  if (length === 0) {
    return;
  }
  const { radius, speed } = tuning.player;
  const player = world.player;
  const { width, height } = tuning.arena;
  player.x = Math.min(Math.max(player.x + (input.dx / length) * speed, radius), width - radius);
  player.y = Math.min(Math.max(player.y + (input.dy / length) * speed, radius), height - radius);
}

function moveEnemies(world: World, tuning: Tuning): void {
  const player = world.player;
  for (const enemy of world.enemies) {
    const dx = player.x - enemy.x;
    const dy = player.y - enemy.y;
    const distance = Math.sqrt(dx * dx + dy * dy);
    if (distance > 0) {
      const speed = Math.min(enemyStats(tuning, enemy.level, world.findings).speed, distance);
      enemy.x += (dx / distance) * speed;
      enemy.y += (dy / distance) * speed;
    }
  }
}

/** Enemies in contact hurt the player, each once in a while. */
function contactDamage(world: World, tuning: Tuning): void {
  const player = world.player;
  const reach = tuning.player.radius + tuning.enemy.radius;
  for (const enemy of world.enemies) {
    if (enemy.contactCooldown > 0) {
      enemy.contactCooldown -= 1;
    }
    if (
      enemy.contactCooldown <= 0 &&
      squaredDistance(enemy.x, enemy.y, player.x, player.y) <= reach * reach
    ) {
      player.life -= enemyStats(tuning, enemy.level, world.findings).damage;
      enemy.contactCooldown = tuning.enemy.contactPeriod;
    }
  }
  if (player.life <= 0) {
    player.life = 0;
    world.over = true;
  }
}

/** The nearest enemy within `range`; of two at one distance, the lower id. */
function nearest(world: World, range: number): Enemy | undefined {
  const player = world.player;
  let best: Enemy | undefined;
  let bestDistance = range * range;
  for (const enemy of world.enemies) {
    const distance = squaredDistance(enemy.x, enemy.y, player.x, player.y);
    if (distance <= bestDistance && (best === undefined || distance < bestDistance)) {
      best = enemy;
      bestDistance = distance;
    }
  }
  return best;
}

function autoAttack(world: World, core: Core, tuning: Tuning): void {
  const player = world.player;
  if (player.attackCooldown > 0) {
    player.attackCooldown -= 1;
  }
  if (player.attackCooldown > 0) {
    return;
  }
  const target = nearest(world, tuning.player.attackRange);
  if (target === undefined) {
    return;
  }
  const hit = hitDamage(core, world.rng.crits, world.stats, tuning, world.findings);
  world.rng.crits = hit.rng;
  target.life -= hit.damage;
  player.attackCooldown = tuning.player.attackPeriod;
}

/** The stats again, after the level or the equipment changed. */
function refreshStats(world: World, core: Core, tuning: Tuning): void {
  world.stats = playerStats(
    core,
    tuning,
    world.player.progress.level,
    world.equipment,
    world.findings,
  );
}

/** A defeat: experience for the player, and what the table drops on the floor. */
function defeat(world: World, enemy: Enemy, core: Core, tuning: Tuning): void {
  world.findings.defeats += 1;
  const player = world.player;
  const gain = core.gainExperience({
    progress: player.progress,
    amount: enemyStats(tuning, enemy.level, world.findings).xp,
  });
  player.progress = gain.progress;
  if (gain.leveledUp !== null) {
    refreshStats(world, core, tuning);
    player.life = world.stats.life;
  }
  const drop = core.rollDrop({
    table: tuning.dropTable,
    itemLevel: enemy.level,
    rng: world.rng.drops,
  });
  world.rng.drops = drop.rng;
  drop.items.forEach((item, i) => {
    world.floor.push({ id: world.nextId, x: enemy.x + i * 10, y: enemy.y, item });
    world.nextId += 1;
  });
}

function resolveDefeats(world: World, core: Core, tuning: Tuning): void {
  const defeated = world.enemies.filter((enemy) => enemy.life <= 0);
  if (defeated.length === 0) {
    return;
  }
  for (const enemy of defeated) {
    defeat(world, enemy, core, tuning);
  }
  world.enemies = world.enemies.filter((enemy) => enemy.life > 0);
}

/** An item within reach is put on, in the slot of its base, in place of what was there. */
function pickUp(world: World, core: Core, tuning: Tuning): void {
  const player = world.player;
  const reach = tuning.player.pickupRadius;
  const left: FloorItem[] = [];
  for (const found of world.floor) {
    if (squaredDistance(found.x, found.y, player.x, player.y) > reach * reach) {
      left.push(found);
      continue;
    }
    world.equipment[slotOf(world.classes, tuning, found.item.base, world.findings)] = found.item;
    refreshStats(world, core, tuning);
    player.life = Math.min(player.life, world.stats.life);
  }
  world.floor = left;
}

/** Runs one tick. Nothing happens once the player is dead. */
export function step(world: World, input: Input, core: Core, tuning: Tuning): void {
  if (world.over) {
    return;
  }
  movePlayer(world, input, tuning);
  direct(world, core, tuning);
  moveEnemies(world, tuning);
  contactDamage(world, tuning);
  if (!world.over) {
    autoAttack(world, core, tuning);
    resolveDefeats(world, core, tuning);
    pickUp(world, core, tuning);
  }
  world.tick += 1;
}

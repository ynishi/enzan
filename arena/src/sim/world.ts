// The state of a game, and the numbers it is run with.
//
// `World` is plain data: numbers, strings, arrays, and the bigint random
// states. It is what `save.ts` writes, apart from `classes`, `stats` and
// `findings`.

import type { Core, ItemRoll, Progress } from "./core.ts";
import { type Findings, meter, newFindings } from "./findings.ts";
import { type BaseClasses, baseClasses, playerStats, type Slot, type StatKey } from "./glue/equip.ts";

/** The numbers the content spec cannot hold, read from tuning.json. */
export interface Tuning {
  arena: { width: number; height: number };
  player: {
    radius: number;
    speed: number;
    attackRange: number;
    attackPeriod: number;
    pickupRadius: number;
    critMultiplier: number;
    /** Added to life and damage for each level above 1. */
    levelBonus: { life: number; damage: number };
  };
  /** The drop table every defeat rolls. */
  dropTable: number;
  /**
   * The slot each class of base is worn in. The class is the content's (the
   * `class` of a base); which slot takes a class is the game's rule.
   */
  slotOfClass: Record<string, string>;
  enemy: {
    radius: number;
    /** Ticks between two hits of one enemy that stays in contact. */
    contactPeriod: number;
    byLevel: EnemyRow[];
  };
  waves: Wave[];
}

export interface EnemyRow {
  level: number;
  life: number;
  /** The experience a defeat gives. */
  xp: number;
  /** Taken from the player's life by each hit of contact. */
  damage: number;
  /** Distance moved in a tick. */
  speed: number;
}

/** A wave lasts `duration` ticks; the last one lasts for ever. */
export interface Wave {
  /** The level of the enemies it spawns. */
  level: number;
  /** The most live enemies. */
  cap: number;
  /** Ticks between two spawns while under the cap. */
  period: number;
  duration: number;
}

export type Stats = Record<StatKey, number>;

export interface Player {
  x: number;
  y: number;
  life: number;
  progress: Progress;
  attackCooldown: number;
}

export interface Enemy {
  id: number;
  level: number;
  x: number;
  y: number;
  life: number;
  contactCooldown: number;
}

export interface FloorItem {
  id: number;
  x: number;
  y: number;
  item: ItemRoll;
}

export type Equipment = { [S in Slot]?: ItemRoll };

export interface World {
  tick: number;
  seed: bigint;
  /** One random state for each use: a use never shares a stream. */
  rng: { drops: bigint; spawns: bigint; crits: bigint };
  player: Player;
  equipment: Equipment;
  /** In creation order, so in increasing id. */
  enemies: Enemy[];
  /** In creation order, so in increasing id. */
  floor: FloorItem[];
  director: { wave: number; spawnCooldown: number };
  nextId: number;
  /** The player is dead and nothing moves any more. */
  over: boolean;
  /**
   * The class of each base, asked of the content once when the world is made
   * or loaded; not saved, since the content gives it again.
   */
  classes: BaseClasses;
  /** Derived from the level and the equipment; not saved. */
  stats: Stats;
  /** Not saved. */
  findings: Findings;
}

/** The streams of a seed, by use. */
const STREAM = { drops: 1n, spawns: 2n, crits: 3n } as const;

/** What the player holds and how many levels it has at the start. */
export function newWorld(seed: bigint, core: Core, tuning: Tuning): World {
  const findings = newFindings();
  const counted = meter(core, findings);
  const progress: Progress = { level: 1, experience: 0 };
  const equipment: Equipment = {};
  const stats = playerStats(counted, tuning, progress.level, equipment, findings);
  return {
    tick: 0,
    seed,
    rng: {
      drops: counted.rngStream({ seed, stream: STREAM.drops }),
      spawns: counted.rngStream({ seed, stream: STREAM.spawns }),
      crits: counted.rngStream({ seed, stream: STREAM.crits }),
    },
    player: {
      x: tuning.arena.width / 2,
      y: tuning.arena.height / 2,
      life: stats.life,
      progress,
      attackCooldown: 0,
    },
    equipment,
    enemies: [],
    floor: [],
    director: { wave: 0, spawnCooldown: 0 },
    nextId: 1,
    over: false,
    classes: baseClasses(counted),
    stats,
    findings,
  };
}

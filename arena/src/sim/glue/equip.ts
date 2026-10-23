// What the game computes about equipment and the player's strength because the
// core does not. Each piece is marked FINDING with what is missing.

import type { Core, ModifierInput, SourceInput } from "../core.ts";
import { type Findings, used } from "../findings.ts";
import type { Equipment, Stats, Tuning } from "../world.ts";

/** The stats the game asks the core for, in the order it asks. */
export const STAT_KEYS = ["life", "damage", "crit"] as const;

export type StatKey = (typeof STAT_KEYS)[number];

export const SLOTS = ["weapon", "offhand", "ring"] as const;

export type Slot = (typeof SLOTS)[number];

function isSlot(name: string): name is Slot {
  return (SLOTS as readonly string[]).includes(name);
}

/** The class the content gives each base, by the id of the base; null for a base with none. */
export type BaseClasses = Record<number, string | null>;

/**
 * The class of every base of the content, asked of the core once. `World` keeps
 * the answer (outside its saved fields), so `slotOf` does not ask again for
 * each item.
 */
export function baseClasses(core: Core): BaseClasses {
  const classes: BaseClasses = {};
  for (const base of core.bases()) {
    classes[base.id] = base.class;
  }
  return classes;
}

/**
 * The slot an item of `base` is worn in: the content's class of the base,
 * through the tuning's slot for that class.
 *
 * That a class means a slot is the game's rule by design (the core reads a
 * class and does nothing with it).
 */
export function slotOf(classes: BaseClasses, tuning: Tuning, base: number, findings: Findings): Slot {
  used(findings, "slotOf");
  const found = classes[base];
  if (found === undefined) {
    throw new Error(`the content has no base ${base}`);
  }
  if (found === null) {
    throw new Error(`base ${base} has no class in the content`);
  }
  const slot = tuning.slotOfClass[found];
  if (slot === undefined || !isSlot(slot)) {
    throw new Error(`tuning.json has no slot for the class ${found}`);
  }
  return slot;
}

/**
 * What the levels above 1 are worth.
 *
 * FINDING: the progression of the content says how much experience a level
 * asks for and nothing about what a level gives. The bonus for each level is a
 * number in tuning.json, turned into added modifiers here.
 */
export function levelBonus(tuning: Tuning, level: number, findings: Findings): ModifierInput[] {
  used(findings, "levelBonus");
  const gained = level - 1;
  const { life, damage } = tuning.player.levelBonus;
  return [
    { stat: "life", modifier: "added", value: life * gained },
    { stat: "damage", modifier: "added", value: damage * gained },
  ];
}

/**
 * The stats of the player at a level with the equipment worn.
 *
 * The core evaluates the sources it is handed and keeps no character, so the
 * game asks again after every level gained and every item put on: one source
 * for the level and one for each worn slot, named by the slot. The sources
 * count in ascending order of id, which is the core's rule, so the order given
 * here does not matter.
 */
export function playerStats(
  core: Core,
  tuning: Tuning,
  level: number,
  equipment: Equipment,
  findings: Findings,
): Stats {
  used(findings, "playerStats");
  const sources: SourceInput[] = [
    { id: "level", from: { modifiers: levelBonus(tuning, level, findings) } },
  ];
  for (const slot of SLOTS) {
    const item = equipment[slot];
    if (item !== undefined) {
      sources.push({ id: slot, from: { item } });
    }
  }
  const values = core.evaluateSheet({ sources, stats: [...STAT_KEYS] });
  const stats = {} as Stats;
  for (const key of STAT_KEYS) {
    const found = values.find((entry) => entry.stat === key);
    if (found === undefined) {
      throw new Error(`evaluateSheet gave no value for ${key}`);
    }
    stats[key] = found.value;
  }
  return stats;
}

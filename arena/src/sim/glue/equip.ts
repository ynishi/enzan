// What the game computes about equipment and the player's strength because the
// core does not. Each piece is marked FINDING with what is missing.

import type { Core, ItemRoll, ModifierInput } from "../core.ts";
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

/**
 * The slot an item of `base` is worn in.
 *
 * FINDING: a base in the content spec has an id and a name and nothing else, so
 * the core has no equipment slot. The game keeps a table of its own
 * (tuning.json, `slots`) and reads it here.
 */
export function slotOf(tuning: Tuning, base: number, findings: Findings): Slot {
  used(findings, "slotOf");
  const slot = tuning.slots[String(base)];
  if (slot === undefined || !isSlot(slot)) {
    throw new Error(`tuning.json has no slot for base ${base}`);
  }
  return slot;
}

/**
 * The modifiers an item adds to the player's stats.
 *
 * FINDING: a `RolledAffix` already has the shape of a `ModifierInput` (stat,
 * modifier, value) plus the id of the affix, but the two are different types
 * and the game copies the three fields. `evaluateStats` could take items, or
 * the core could say which modifiers an item stands for.
 */
export function itemModifiers(item: ItemRoll, findings: Findings): ModifierInput[] {
  used(findings, "itemModifiers");
  return item.affixes.map((affix) => ({
    stat: affix.stat,
    modifier: affix.modifier,
    value: affix.value,
  }));
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
 * FINDING: the core evaluates the modifiers it is handed and keeps no
 * character, so the game collects them in a fixed order (level, then the slots
 * in turn: the same order after a load), asks again after every level gained
 * and every item put on, and turns the list that comes back, in the order
 * asked, into a record.
 */
export function playerStats(
  core: Core,
  tuning: Tuning,
  level: number,
  equipment: Equipment,
  findings: Findings,
): Stats {
  used(findings, "playerStats");
  const modifiers = levelBonus(tuning, level, findings);
  for (const slot of SLOTS) {
    const item = equipment[slot];
    if (item !== undefined) {
      modifiers.push(...itemModifiers(item, findings));
    }
  }
  const values = core.evaluateStats({ modifiers, stats: [...STAT_KEYS] });
  const stats = {} as Stats;
  for (const key of STAT_KEYS) {
    const found = values.find((entry) => entry.stat === key);
    if (found === undefined) {
      throw new Error(`evaluateStats gave no value for ${key}`);
    }
    stats[key] = found.value;
  }
  return stats;
}

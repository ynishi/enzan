// A stand-in for the interface, so the tests of the sim do not load
// WebAssembly. It is deterministic and counts its calls; it is not the
// formulas of the core, only enough of them for the sim to run.

import { readFileSync } from "node:fs";

import type { BaseInfo, Core } from "../src/sim/core.ts";
import type { Input } from "../src/sim/step.ts";
import type { Tuning } from "../src/sim/world.ts";

export type Calls = { [Name in keyof Core]: number };

function next(rng: bigint): bigint {
  return BigInt.asUintN(64, rng * 6364136223846793005n + 1442695040888963407n);
}

const BASE = { life: 100, damage: 10, crit: 0.05 } as const;

/** The bases of the content the game ships with, as `bases()` gives them. */
const BASES: BaseInfo[] = [
  { id: 1, name: "Short Sword", class: "sword" },
  { id: 2, name: "Round Shield", class: "shield" },
  { id: 3, name: "Iron Ring", class: "ring" },
];

/** The range of the one affix the fake rolls (affix 1 of the content). */
const AFFIX_RANGE = { id: 1, low: 1, high: 5 } as const;

/** A refusal shaped as the package throws it: an `Error` with the `at` of the input. */
function refusal(message: string, at: string): Error {
  return Object.assign(new Error(message), { name: "EnzanError", at });
}

export function fakeCore(): { core: Core; calls: Calls } {
  const calls: Calls = {
    rollDrop: 0,
    evaluateSheet: 0,
    checkItem: 0,
    bases: 0,
    gainExperience: 0,
    rngStream: 0,
    rngNextF64: 0,
    rngBelow: 0,
  };
  const core: Core = {
    rngStream: ({ seed, stream }) => {
      calls.rngStream += 1;
      return BigInt.asUintN(64, seed * 0x9e3779b97f4a7c15n + stream * 0xbf58476d1ce4e5b9n + 1n);
    },
    rngNextF64: ({ rng }) => {
      calls.rngNextF64 += 1;
      const state = next(rng);
      return { value: Number(state >> 11n) / 9007199254740992, rng: state };
    },
    rngBelow: ({ rng, n }) => {
      calls.rngBelow += 1;
      const state = next(rng);
      return { value: Number(state >> 33n) % n, rng: state };
    },
    rollDrop: ({ rng, itemLevel }) => {
      calls.rollDrop += 1;
      const state = next(rng);
      const base = 1 + Number((state >> 40n) % 3n);
      const value = 1 + Number((state >> 20n) % 5n);
      return {
        items: [
          {
            base,
            rarity: 1,
            itemLevel,
            affixes: [{ id: 1, stat: "damage", modifier: "added", value }],
          },
        ],
        rng: state,
      };
    },
    bases: () => {
      calls.bases += 1;
      return BASES.map((base) => ({ ...base }));
    },
    checkItem: ({ item }) => {
      calls.checkItem += 1;
      if (!BASES.some((base) => base.id === item.base)) {
        throw refusal(`there is no base ${item.base}`, "item.base");
      }
      if (item.rarity !== 1) {
        throw refusal(`there is no rarity ${item.rarity}`, "item.rarity");
      }
      if (!Number.isInteger(item.itemLevel) || item.itemLevel < 1) {
        throw refusal(`item level ${item.itemLevel} is below 1`, "item.itemLevel");
      }
      return {
        base: item.base,
        rarity: item.rarity,
        itemLevel: item.itemLevel,
        affixes: item.affixes.map((affix, j) => {
          if (affix.id !== AFFIX_RANGE.id) {
            throw refusal(`affix ${affix.id} is not in the content`, `item.affixes[${j}].id`);
          }
          if (!(affix.value >= AFFIX_RANGE.low && affix.value <= AFFIX_RANGE.high)) {
            throw refusal(
              `the value ${affix.value} of affix ${affix.id} is outside ${AFFIX_RANGE.low} to ${AFFIX_RANGE.high}`,
              `item.affixes[${j}].value`,
            );
          }
          return { id: affix.id, stat: "damage", modifier: "added" as const, value: affix.value };
        }),
      };
    },
    evaluateSheet: ({ sources, stats }) => {
      calls.evaluateSheet += 1;
      // The core's rule: sources count in ascending order of id.
      const ordered = [...sources].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
      const modifiers = ordered.flatMap((source) =>
        "modifiers" in source.from
          ? source.from.modifiers
          : source.from.item.affixes.map((affix) => ({
              stat: "damage",
              modifier: "added" as const,
              value: affix.value,
            })),
      );
      return stats.map((stat) => {
        const key = stat as keyof typeof BASE;
        let added = 0;
        let increased = 0;
        let more = 1;
        for (const modifier of modifiers) {
          if (modifier.stat !== stat) continue;
          if (modifier.modifier === "added") added += modifier.value;
          else if (modifier.modifier === "increased") increased += modifier.value;
          else more *= 1 + modifier.value;
        }
        return { stat, value: (BASE[key] + added) * (1 + increased) * more };
      });
    },
    gainExperience: ({ progress, amount }) => {
      calls.gainExperience += 1;
      let experience = progress.experience + amount;
      let level = progress.level;
      while (experience >= 100) {
        experience -= 100;
        level += 1;
      }
      return {
        progress: { level, experience },
        leveledUp: level > progress.level ? { from: progress.level, to: level } : null,
        reachedCap: false,
      };
    },
  };
  return { core, calls };
}

/** The tuning the game ships with. */
export function readTuning(): Tuning {
  return JSON.parse(readFileSync(new URL("../../tuning.json", import.meta.url), "utf8")) as Tuning;
}

/** A square, 120 ticks to a side, then a pause: the same script the smoke runs. */
export function scripted(tick: number): Input {
  const phase = Math.floor(tick / 120) % 5;
  if (phase === 0) return { dx: 1, dy: 0 };
  if (phase === 1) return { dx: 0, dy: 1 };
  if (phase === 2) return { dx: -1, dy: 0 };
  if (phase === 3) return { dx: 0, dy: -1 };
  return { dx: 0, dy: 0 };
}

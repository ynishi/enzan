// A stand-in for the interface, so the tests of the sim do not load
// WebAssembly. It is deterministic and counts its calls; it is not the
// formulas of the core, only enough of them for the sim to run.

import { readFileSync } from "node:fs";

import type { Core } from "../src/sim/core.ts";
import type { Input } from "../src/sim/step.ts";
import type { Tuning } from "../src/sim/world.ts";

export type Calls = { [Name in keyof Core]: number };

function next(rng: bigint): bigint {
  return BigInt.asUintN(64, rng * 6364136223846793005n + 1442695040888963407n);
}

const BASE = { life: 100, damage: 10, crit: 0.05 } as const;

export function fakeCore(): { core: Core; calls: Calls } {
  const calls: Calls = {
    rollDrop: 0,
    evaluateStats: 0,
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
    rollDrop: ({ rng }) => {
      calls.rollDrop += 1;
      const state = next(rng);
      const base = 1 + Number((state >> 40n) % 3n);
      const value = 1 + Number((state >> 20n) % 5n);
      return {
        items: [
          {
            base,
            rarity: 1,
            affixes: [{ id: 1, stat: "damage", modifier: "added", value }],
          },
        ],
        rng: state,
      };
    },
    evaluateStats: ({ modifiers, stats }) => {
      calls.evaluateStats += 1;
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

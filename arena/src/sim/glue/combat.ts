// How a hit lands, which the core leaves to the game.

import type { Core } from "../core.ts";
import { type Findings, used } from "../findings.ts";
import type { Stats, Tuning } from "../world.ts";

/**
 * The damage of one hit and the random state after it.
 *
 * FINDING: the content has a `crit` stat, a number between 0 and 1, and the
 * core says nothing of what it means. The roll against it, the multiplier of a
 * critical hit and the stream the roll draws on (`rngNextF64` of the crit
 * stream) are the game's.
 */
export function hitDamage(
  core: Core,
  rng: bigint,
  stats: Stats,
  tuning: Tuning,
  findings: Findings,
): { damage: number; rng: bigint } {
  used(findings, "hitDamage");
  const roll = core.rngNextF64({ rng });
  const damage = roll.value < stats.crit ? stats.damage * tuning.player.critMultiplier : stats.damage;
  return { damage, rng: roll.rng };
}

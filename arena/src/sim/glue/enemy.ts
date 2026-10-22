// What the game says about its enemies because the content spec does not.

import { type Findings, used } from "../findings.ts";
import type { EnemyRow, Tuning } from "../world.ts";

/**
 * The life, experience reward, contact damage and speed of an enemy of a
 * level.
 *
 * FINDING: the content spec has no enemy: no life, no experience per defeat,
 * no strength that grows with a level. The game keeps a row for each level in
 * tuning.json and reads it here. A level with no row is an error in the
 * game's data that `Content.validate` cannot see.
 */
export function enemyStats(tuning: Tuning, level: number, findings: Findings): EnemyRow {
  used(findings, "enemyStats");
  const row = tuning.enemy.byLevel.find((candidate) => candidate.level === level);
  if (row === undefined) {
    throw new Error(`tuning.json has no enemy of level ${level}`);
  }
  return row;
}

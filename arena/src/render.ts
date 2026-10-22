// The world on a canvas. The only module that draws; it reads the world and
// changes nothing in it.

import type { Slot } from "./sim/glue/equip.ts";
import type { Tuning, World } from "./sim/world.ts";

/** One color for each slot, for the items on the floor. */
const SLOT_COLOR: Record<Slot, string> = {
  weapon: "#e8c547",
  offhand: "#5fb0ff",
  ring: "#c78bff",
};

/** Enemies are redder for each level. */
function enemyColor(level: number): string {
  const green = Math.max(0, 150 - level * 25);
  return `rgb(220, ${green}, ${green})`;
}

export function render(
  ctx: CanvasRenderingContext2D,
  world: World,
  tuning: Tuning,
  slotOf: (base: number) => Slot,
): void {
  const { width, height } = tuning.arena;
  const player = world.player;

  ctx.fillStyle = "#16181d";
  ctx.fillRect(0, 0, width, height);

  // How far the automatic attack reaches.
  ctx.strokeStyle = "#ffffff22";
  ctx.beginPath();
  ctx.arc(player.x, player.y, tuning.player.attackRange, 0, 2 * Math.PI);
  ctx.stroke();

  for (const found of world.floor) {
    ctx.fillStyle = SLOT_COLOR[slotOf(found.item.base)];
    ctx.fillRect(found.x - 4, found.y - 4, 8, 8);
  }

  for (const enemy of world.enemies) {
    ctx.fillStyle = enemyColor(enemy.level);
    ctx.beginPath();
    ctx.arc(enemy.x, enemy.y, tuning.enemy.radius, 0, 2 * Math.PI);
    ctx.fill();
  }

  ctx.fillStyle = world.over ? "#777" : "#7fe08a";
  ctx.beginPath();
  ctx.arc(player.x, player.y, tuning.player.radius, 0, 2 * Math.PI);
  ctx.fill();

  // Life across the top of the arena.
  ctx.fillStyle = "#00000088";
  ctx.fillRect(8, 8, 204, 12);
  ctx.fillStyle = "#d94a4a";
  ctx.fillRect(10, 10, 200 * (player.life / world.stats.life), 8);

  if (world.over) {
    ctx.fillStyle = "#ffffffcc";
    ctx.font = "28px system-ui, sans-serif";
    ctx.textAlign = "center";
    ctx.fillText("Defeated. Press R to start again.", width / 2, height / 2);
    ctx.textAlign = "start";
  }
}

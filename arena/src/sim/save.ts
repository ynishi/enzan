// A world to a plain JSON object and text, and back.
//
// What crosses a save is JSON: the random states and the seed, which are
// bigints in the world, are decimal strings in the save (contract 2 of
// `enzan-wasm`). What can be derived is not saved: `load` asks the core for the
// stats again. The mapping is written field by field and the compiler checks it
// names every field of `World` except the two that are left out.

import { type Core, isModifierKind, type ItemRoll, type Progress } from "./core.ts";
import { meter, newFindings } from "./findings.ts";
import { playerStats, SLOTS, slotOf } from "./glue/equip.ts";
import type { Enemy, Equipment, FloorItem, Player, Tuning, World } from "./world.ts";

/** The fields of a world that are not saved. */
type Derived = "stats" | "findings";

export interface Save {
  format: 1;
  /** `contentHash` of the content and tuning the world was made with. */
  contentHash: string;
  /** The contract version of the interface the world was run on. */
  contractVersion: number;
  tick: number;
  seed: string;
  rng: { drops: string; spawns: string; crits: string };
  player: Player;
  equipment: Equipment;
  enemies: Enemy[];
  floor: FloorItem[];
  director: { wave: number; spawnCooldown: number };
  nextId: number;
  over: boolean;
}

export type Loaded = { ok: true; world: World } | { ok: false; reason: string };

// ---- text ------------------------------------------------------------------

/**
 * JSON with the keys of every object in order, so the same value is always the
 * same text. It throws on a number that is not finite, and on anything JSON
 * has no place for. `-0` is written as 0, as JSON does: a position can become
 * -0 and a save must not fail on it.
 */
export function canonical(value: unknown): string {
  switch (typeof value) {
    case "number":
      if (!Number.isFinite(value)) {
        throw new Error(`cannot write the number ${value}`);
      }
      return Object.is(value, -0) ? "0" : JSON.stringify(value);
    case "string":
    case "boolean":
      return JSON.stringify(value);
    case "object": {
      if (value === null) {
        return "null";
      }
      if (Array.isArray(value)) {
        return `[${value.map(canonical).join(",")}]`;
      }
      const record = value as Record<string, unknown>;
      const parts: string[] = [];
      for (const key of Object.keys(record).sort()) {
        const member = record[key];
        if (member !== undefined) {
          parts.push(`${JSON.stringify(key)}:${canonical(member)}`);
        }
      }
      return `{${parts.join(",")}}`;
    }
    default:
      throw new Error(`cannot write a ${typeof value}`);
  }
}

/** The UTF-8 bytes of a text. */
function utf8(text: string): number[] {
  const bytes: number[] = [];
  for (const char of text) {
    const code = char.codePointAt(0) ?? 0;
    if (code < 0x80) {
      bytes.push(code);
    } else if (code < 0x800) {
      bytes.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
    } else if (code < 0x10000) {
      bytes.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
    } else {
      bytes.push(
        0xf0 | (code >> 18),
        0x80 | ((code >> 12) & 0x3f),
        0x80 | ((code >> 6) & 0x3f),
        0x80 | (code & 0x3f),
      );
    }
  }
  return bytes;
}

/** FNV-1a, 32 bits, over the UTF-8 bytes of a text, as 8 hex digits. */
function fnv1a(text: string): string {
  let hash = 0x811c9dc5;
  for (const byte of utf8(text)) {
    hash = (hash ^ byte) >>> 0;
    // hash * 16777619 modulo 2^32, with 16777619 = 2^24 + 403, in exact steps.
    hash = (((hash << 24) >>> 0) + hash * 403) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

/** Which content and tuning a save belongs to; the parsed objects, not the files. */
export function contentHash(content: unknown, tuning: unknown): string {
  return fnv1a(canonical(content) + canonical(tuning));
}

/**
 * What a save is checked against before it is read. The same world replays
 * only on the same content and tuning and on the same contract of the
 * interface, so a save made under another one is refused rather than read.
 */
export interface Stamp {
  contentHash: string;
  contractVersion: number;
}

export function stampOf(content: unknown, tuning: unknown, contractVersion: number): Stamp {
  return { contentHash: contentHash(content, tuning), contractVersion };
}

// ---- world to save ---------------------------------------------------------

function itemOut(source: ItemRoll): ItemRoll {
  return {
    base: source.base,
    rarity: source.rarity,
    affixes: source.affixes.map((affix) => ({
      id: affix.id,
      stat: affix.stat,
      modifier: affix.modifier,
      value: affix.value,
    })),
  };
}

/** The world as a save; nothing in it is shared with the world. */
export function toSave(world: World, stamp: Stamp): Save {
  const worn: Equipment = {};
  for (const slot of SLOTS) {
    const wornItem = world.equipment[slot];
    if (wornItem !== undefined) {
      worn[slot] = itemOut(wornItem);
    }
  }
  const fields = {
    tick: world.tick,
    seed: world.seed.toString(),
    rng: {
      drops: world.rng.drops.toString(),
      spawns: world.rng.spawns.toString(),
      crits: world.rng.crits.toString(),
    },
    player: {
      x: world.player.x,
      y: world.player.y,
      life: world.player.life,
      progress: {
        level: world.player.progress.level,
        experience: world.player.progress.experience,
      },
      attackCooldown: world.player.attackCooldown,
    },
    equipment: worn,
    enemies: world.enemies.map((enemy) => ({
      id: enemy.id,
      level: enemy.level,
      x: enemy.x,
      y: enemy.y,
      life: enemy.life,
      contactCooldown: enemy.contactCooldown,
    })),
    floor: world.floor.map((found) => ({
      id: found.id,
      x: found.x,
      y: found.y,
      item: itemOut(found.item),
    })),
    director: {
      wave: world.director.wave,
      spawnCooldown: world.director.spawnCooldown,
    },
    nextId: world.nextId,
    over: world.over,
  } satisfies { [K in Exclude<keyof World, Derived>]: unknown };
  return {
    format: 1,
    contentHash: stamp.contentHash,
    contractVersion: stamp.contractVersion,
    ...fields,
  };
}

/** The text of a save: the same world always gives the same text. */
export function saveText(world: World, stamp: Stamp): string {
  return canonical(toSave(world, stamp));
}

/** No content or contract: a digest is of the world alone. */
const UNSTAMPED: Stamp = { contentHash: "", contractVersion: 0 };

/** A short number for a world: the same for worlds with the same save. */
export function digest(world: World): string {
  return fnv1a(canonical(toSave(world, UNSTAMPED)));
}

// ---- save to world ---------------------------------------------------------

/** What is wrong with a save, thrown inside `fromSave` and turned into a reason. */
class Refused extends Error {}

function refuse(at: string, want: string): never {
  throw new Refused(`${at} is not ${want}`);
}

function record(value: unknown, at: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    refuse(at, "an object");
  }
  return value as Record<string, unknown>;
}

function list(value: unknown, at: string): unknown[] {
  if (!Array.isArray(value)) {
    refuse(at, "a list");
  }
  return value as unknown[];
}

function number(value: unknown, at: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    refuse(at, "a number");
  }
  return value as number;
}

function atLeastZero(value: unknown, at: string): number {
  const n = number(value, at);
  if (n < 0) {
    refuse(at, "at least 0");
  }
  return n;
}

function whole(value: unknown, at: string): number {
  const n = atLeastZero(value, at);
  if (!Number.isInteger(n)) {
    refuse(at, "a whole number");
  }
  return n;
}

function text(value: unknown, at: string): string {
  if (typeof value !== "string") {
    refuse(at, "a text");
  }
  return value as string;
}

function flag(value: unknown, at: string): boolean {
  if (typeof value !== "boolean") {
    refuse(at, "true or false");
  }
  return value as boolean;
}

/** A random state: a decimal text that fits 64 bits. */
function state(value: unknown, at: string): bigint {
  const digits = text(value, at);
  if (!/^[0-9]{1,20}$/.test(digits)) {
    refuse(at, "a decimal state");
  }
  const parsed = BigInt(digits);
  if (parsed >= 1n << 64n) {
    refuse(at, "a state of 64 bits");
  }
  return parsed;
}

function item(value: unknown, at: string): ItemRoll {
  const source = record(value, at);
  return {
    base: whole(source.base, `${at}.base`),
    rarity: whole(source.rarity, `${at}.rarity`),
    affixes: list(source.affixes, `${at}.affixes`).map((entry, i) => {
      const affix = record(entry, `${at}.affixes[${i}]`);
      const modifier = text(affix.modifier, `${at}.affixes[${i}].modifier`);
      if (!isModifierKind(modifier)) {
        refuse(`${at}.affixes[${i}].modifier`, "a modifier");
      }
      return {
        id: whole(affix.id, `${at}.affixes[${i}].id`),
        stat: text(affix.stat, `${at}.affixes[${i}].stat`),
        modifier,
        value: number(affix.value, `${at}.affixes[${i}].value`),
      };
    }),
  };
}

function progress(value: unknown, at: string): Progress {
  const source = record(value, at);
  return {
    level: whole(source.level, `${at}.level`),
    // The core asks for experience that is finite and at least 0.
    experience: atLeastZero(source.experience, `${at}.experience`),
  };
}

function equipment(value: unknown, at: string): Equipment {
  const source = record(value, at);
  const worn: Equipment = {};
  for (const key of Object.keys(source)) {
    const slot = SLOTS.find((candidate) => candidate === key);
    if (slot === undefined) {
      refuse(`${at}.${key}`, "a slot");
    }
    worn[slot] = item(source[key], `${at}.${key}`);
  }
  return worn;
}

/**
 * Refuses what has the right shape but that the sim could not run with
 * `tuning`: an enemy level with no row, a wave past the last, a base with no
 * slot or worn in another slot, and ids that are not increasing below `nextId`
 * (the order `step` breaks ties by).
 */
function fitsTuning(
  loaded: Pick<World, "enemies" | "floor" | "equipment" | "director" | "nextId">,
  tuning: Tuning,
): void {
  const scratch = newFindings();
  const slotAt = (base: number, at: string) => {
    try {
      return slotOf(tuning, base, scratch);
    } catch {
      return refuse(at, "a base with a slot");
    }
  };
  if (loaded.director.wave >= tuning.waves.length) {
    refuse("director.wave", "a wave of the tuning");
  }
  loaded.enemies.forEach((enemy, i) => {
    if (!tuning.enemy.byLevel.some((row) => row.level === enemy.level)) {
      refuse(`enemies[${i}].level`, "a level of the tuning");
    }
  });
  for (const slot of SLOTS) {
    const worn = loaded.equipment[slot];
    if (worn !== undefined && slotAt(worn.base, `equipment.${slot}.base`) !== slot) {
      refuse(`equipment.${slot}.base`, `a base worn as ${slot}`);
    }
  }
  loaded.floor.forEach((found, i) => {
    slotAt(found.item.base, `floor[${i}].item.base`);
  });
  for (const [name, things] of [
    ["enemies", loaded.enemies],
    ["floor", loaded.floor],
  ] as const) {
    let below = 0;
    things.forEach((thing, i) => {
      if (thing.id <= below || thing.id >= loaded.nextId) {
        refuse(`${name}[${i}].id`, "increasing and below nextId");
      }
      below = thing.id;
    });
  }
}

/**
 * A world from a parsed save. A wrong format, a stamp that is not `stamp` (the
 * content, the tuning or the interface's contract changed) or a shape that does
 * not fit is `{ ok: false }` with the reason. The stats are asked of the core
 * again.
 */
export function fromSave(value: unknown, stamp: Stamp, core: Core, tuning: Tuning): Loaded {
  try {
    const source = record(value, "save");
    if (source.format !== 1) {
      return { ok: false, reason: `save format ${String(source.format)} is not 1` };
    }
    if (source.contractVersion !== stamp.contractVersion) {
      return {
        ok: false,
        reason: `save was made with contract ${String(source.contractVersion)} of the interface, not ${stamp.contractVersion}`,
      };
    }
    if (source.contentHash !== stamp.contentHash) {
      return { ok: false, reason: "save was made with other content or tuning" };
    }
    const rng = record(source.rng, "rng");
    const player = record(source.player, "player");
    const director = record(source.director, "director");
    const loaded = {
      tick: whole(source.tick, "tick"),
      seed: state(source.seed, "seed"),
      rng: {
        drops: state(rng.drops, "rng.drops"),
        spawns: state(rng.spawns, "rng.spawns"),
        crits: state(rng.crits, "rng.crits"),
      },
      player: {
        x: number(player.x, "player.x"),
        y: number(player.y, "player.y"),
        life: number(player.life, "player.life"),
        progress: progress(player.progress, "player.progress"),
        attackCooldown: whole(player.attackCooldown, "player.attackCooldown"),
      },
      equipment: equipment(source.equipment, "equipment"),
      enemies: list(source.enemies, "enemies").map((entry, i): Enemy => {
        const enemy = record(entry, `enemies[${i}]`);
        return {
          id: whole(enemy.id, `enemies[${i}].id`),
          level: whole(enemy.level, `enemies[${i}].level`),
          x: number(enemy.x, `enemies[${i}].x`),
          y: number(enemy.y, `enemies[${i}].y`),
          life: number(enemy.life, `enemies[${i}].life`),
          contactCooldown: whole(enemy.contactCooldown, `enemies[${i}].contactCooldown`),
        };
      }),
      floor: list(source.floor, "floor").map((entry, i): FloorItem => {
        const found = record(entry, `floor[${i}]`);
        return {
          id: whole(found.id, `floor[${i}].id`),
          x: number(found.x, `floor[${i}].x`),
          y: number(found.y, `floor[${i}].y`),
          item: item(found.item, `floor[${i}].item`),
        };
      }),
      director: {
        wave: whole(director.wave, "director.wave"),
        spawnCooldown: whole(director.spawnCooldown, "director.spawnCooldown"),
      },
      nextId: whole(source.nextId, "nextId"),
      over: flag(source.over, "over"),
    } satisfies { [K in Exclude<keyof World, Derived>]: unknown };
    if (loaded.player.progress.level < 1) {
      refuse("player.progress.level", "at least 1");
    }
    fitsTuning(loaded, tuning);
    const findings = newFindings();
    const stats = playerStats(
      meter(core, findings),
      tuning,
      loaded.player.progress.level,
      loaded.equipment,
      findings,
    );
    return { ok: true, world: { ...loaded, stats, findings } };
  } catch (e) {
    return { ok: false, reason: e instanceof Error ? e.message : String(e) };
  }
}

/** A world from the text of a save; see `fromSave`. */
export function loadText(textOfSave: string, stamp: Stamp, core: Core, tuning: Tuning): Loaded {
  let parsed: unknown;
  try {
    parsed = JSON.parse(textOfSave);
  } catch {
    return { ok: false, reason: "save is not JSON" };
  }
  return fromSave(parsed, stamp, core, tuning);
}

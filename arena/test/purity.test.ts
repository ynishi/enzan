// Holds the sim to its rules by reading the JavaScript tsc emitted from it
// (build-sim/src/sim). Comments are not in the syntax tree, so the FINDING
// comments do not count.
//
// The type-level half of the rules is in tsconfig.sim.json: no DOM and no node
// globals. What it cannot say, this does: which `Math` the sim may use, that
// the sim imports nothing from outside itself, and that no clock is read.

import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";

import { parse } from "acorn";

/** The `Math` members whose result is correctly rounded, so the same everywhere. */
const MATH_ALLOWED = new Set(["sqrt", "abs", "min", "max", "floor", "ceil", "round", "trunc", "sign"]);

const BANNED_NAMES = new Set([
  "Date",
  "performance",
  "globalThis",
  "window",
  "document",
  "self",
  "crypto",
  "setTimeout",
  "requestAnimationFrame",
]);

type Ast = { type: string; [key: string]: unknown };

function isAst(value: unknown): value is Ast {
  return typeof value === "object" && value !== null && typeof (value as Ast).type === "string";
}

function walk(node: Ast, parent: Ast | null, visit: (node: Ast, parent: Ast | null) => void): void {
  visit(node, parent);
  for (const key of Object.keys(node)) {
    const child = node[key];
    if (Array.isArray(child)) {
      for (const entry of child) {
        if (isAst(entry)) walk(entry, node, visit);
      }
    } else if (isAst(child)) {
      walk(child, node, visit);
    }
  }
}

const simDir = new URL("../src/sim/", import.meta.url);

/** What in `source`, the text of the module at `file`, breaks the rules. */
function violations(source: string, file: URL): string[] {
  const found: string[] = [];
  const program = parse(source, { ecmaVersion: "latest", sourceType: "module" }) as unknown as Ast;
  const checkImport = (from: unknown) => {
    const ok =
      typeof from === "string" &&
      from.startsWith(".") &&
      new URL(from, file).href.startsWith(simDir.href);
    if (!ok) found.push(`import of ${String(from)}`);
  };
  walk(program, null, (node, parent) => {
    if (node.type === "BinaryExpression" && node.operator === "**") found.push("**");
    if (node.type === "AssignmentExpression" && node.operator === "**=") found.push("**=");
    if (node.type === "ImportDeclaration" || node.type === "ExportAllDeclaration") {
      checkImport((node.source as Ast).value);
    }
    if (node.type === "ExportNamedDeclaration" && node.source !== null) {
      checkImport((node.source as Ast).value);
    }
    if (node.type === "ImportExpression") found.push("dynamic import");
    if (node.type !== "Identifier") return;
    const name = node.name as string;
    if (BANNED_NAMES.has(name)) found.push(name);
    if (name === "Math") {
      const member = parent !== null && parent.type === "MemberExpression" && parent.object === node;
      const property = member ? (parent.property as Ast) : null;
      const allowed =
        member &&
        parent.computed === false &&
        property !== null &&
        property.type === "Identifier" &&
        MATH_ALLOWED.has(property.name as string);
      if (!allowed) found.push("Math other than a member of the allowed list");
    }
  });
  return found;
}

function emitted(dir: URL): URL[] {
  const files: URL[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      files.push(...emitted(new URL(`${entry.name}/`, dir)));
    } else if (entry.name.endsWith(".js")) {
      files.push(new URL(entry.name, dir));
    }
  }
  return files;
}

test("the emitted sim breaks none of the rules", () => {
  const files = emitted(simDir);
  assert.ok(files.length >= 10, `only ${files.length} files under ${simDir.href}`);
  for (const file of files) {
    assert.deepEqual(violations(readFileSync(file, "utf8"), file), [], file.href);
  }
});

test("the check itself sees what it is there to see", () => {
  const file = new URL("probe.js", simDir);
  const bad = [
    "const x = Math.random();",
    "const M = Math; M.sqrt(2);",
    "const y = Math.sin(1);",
    "const z = Math[`sqrt`](4);",
    "let w = 2 ** 3;",
    "let v = 2; v **= 2;",
    "const t = Date.now();",
    "const p = performance.now();",
    "const g = globalThis;",
    'import fs from "node:fs";',
    'import pkg from "../pkg/enzan_wasm.js";',
    'import("./world.js");',
    'export * from "../../outside.js";',
  ];
  for (const source of bad) {
    assert.notDeepEqual(violations(source, file), [], source);
  }
  const good = ['import { step } from "./step.js"; Math.sqrt(Math.max(1, 2));'];
  for (const source of good) {
    assert.deepEqual(violations(source, file), [], source);
  }
});

#!/usr/bin/env -S node --no-warnings=ExperimentalWarning
// Cargo runner for wasm32-wasip1 (see .cargo/config.toml): runs a test binary
// under `node:wasi` and exits with its status. Needs node >= 22.
import { readFile } from 'node:fs/promises';
import { WASI } from 'node:wasi';
import process from 'node:process';

if (+process.versions.node.split('.')[0] < 22) {
  console.error(`wasi-runner needs node >= 22, found ${process.version}`);
  process.exit(1);
}

const [, , wasmPath, ...rest] = process.argv;
if (!wasmPath) {
  console.error('usage: wasi-runner.mjs <file.wasm> [args...]');
  process.exit(1);
}

const wasi = new WASI({
  version: 'preview1',
  args: [wasmPath, ...rest],
  env: process.env,
  returnOnExit: true,
});

const module = await WebAssembly.compile(await readFile(wasmPath));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
const code = wasi.start(instance);
process.exit(code);

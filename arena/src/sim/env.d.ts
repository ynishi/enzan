// The generated package's .d.ts names a few types of the web. The sim has no
// DOM, so they are declared here as opaque: enough for the `import type` of the
// package to type-check, and nothing a sim module could call. Only
// tsconfig.sim.json includes this file; the page and the tests have the real
// declarations.

type RequestInfo = unknown;
type URL = unknown;
type Response = unknown;
type BufferSource = unknown;

declare namespace WebAssembly {
  interface Module {}
  interface Memory {}
  interface Table {}
}

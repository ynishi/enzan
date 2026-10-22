import { defineConfig } from "vite";

export default defineConfig({
  server: {
    // The page imports the sample content from the Rust crate's fixtures, which
    // lie outside this directory.
    fs: { allow: [".", "../crates/enzan-wasm/tests/fixtures"] },
  },
});

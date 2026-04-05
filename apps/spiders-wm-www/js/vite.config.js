import { defineConfig } from "vite";
import { resolve } from "node:path";

export default defineConfig({
  base: "./",
  build: {
    lib: {
      entry: resolve(import.meta.dirname, "src/monaco-host.ts"),
      formats: ["es"],
      fileName: () => "monaco-host.js",
    },
    outDir: resolve(import.meta.dirname, "dist"),
    emptyOutDir: false,
    codeSplitting: false,
  },
});

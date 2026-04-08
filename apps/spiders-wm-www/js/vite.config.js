import { defineConfig } from "vite";
import { resolve } from "node:path";

export default defineConfig({
  base: "./",
  build: {
    lib: {
      entry: {
        "monaco-host": resolve(import.meta.dirname, "src/monaco-host.ts"),
        "xterm-host": resolve(import.meta.dirname, "src/xterm-host.ts"),
      },
      formats: ["es"],
      fileName: (_format, entryName) => `${entryName}.js`,
    },
    outDir: resolve(import.meta.dirname, "dist"),
    emptyOutDir: false,
    codeSplitting: false,
  },
});

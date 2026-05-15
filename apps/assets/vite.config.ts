import { defineConfig } from "vite";

export default defineConfig({
  publicDir: false,
  build: {
    manifest: "manifest.json",
    outDir: "dist",
    emptyOutDir: true,
    assetsDir: "assets",
    rollupOptions: {
      input: {
        docs: "src/docs.ts",
        status: "src/status.ts",
      },
    },
  },
});

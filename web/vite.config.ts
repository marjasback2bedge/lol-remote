import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

// `npm run dev` proxies the API to a locally running `lol-remote run`.
const backend = process.env.LOLR_BACKEND ?? "http://localhost:3000";

export default defineConfig({
  plugins: [preact()],
  server: {
    host: true,
    proxy: {
      "/api": backend,
      "/assets/lcu": backend,
      "/ws": { target: backend.replace(/^http/, "ws"), ws: true },
    },
  },
  build: { outDir: "dist", emptyOutDir: true },
});

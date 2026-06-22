import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// In dev, proxy the API to the local backend (port 81) so the app can call
// /auth and /admin without CORS friction. In prod the app is served as a static
// bundle and talks to the API via VITE_API_BASE (or same origin if unset).
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5180,
    proxy: {
      "/auth": "http://localhost:81",
      "/admin": "http://localhost:81",
    },
  },
});

import { defineConfig } from "vite";

export default defineConfig({
    assetsInclude: ["**/*.xml"],
    clearScreen: false,
    server: {
        port: 5173,
        strictPort: true
    }
});
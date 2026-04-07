import vue from "@vitejs/plugin-vue"
import { defineConfig, loadEnv } from "vite"

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "")
  const frontendHost = env.SDL_FRONTEND_HOST || "localhost"
  const frontendPort = Number(env.SDL_FRONTEND_PORT || "1420")
  const sidecarHost = env.SDL_SIDECAR_HOST || "127.0.0.1"
  const sidecarPort = Number(env.SDL_SIDECAR_PORT || "5321")
  const sidecarOrigin = `http://${sidecarHost}:${sidecarPort}`

  return {
    clearScreen: false,
    plugins: [vue()],
    server: {
      host: frontendHost,
      open: true,
      port: frontendPort,
      strictPort: true,
      proxy: {
        "/api": {
          target: sidecarOrigin,
          changeOrigin: false,
          rewrite: path => path.replace(/^\/api/, ""),
        },
        "/docs": {
          target: sidecarOrigin,
          changeOrigin: false,
        },
        "/openapi.json": {
          target: sidecarOrigin,
          changeOrigin: false,
        },
      },
      watch: {
        ignored: ["**/target/**", "**/.venv/**"],
      },
    },
  }
})

import vue from "@vitejs/plugin-vue"
import { defineConfig, loadEnv } from "vite"

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "")
  const frontendHost = env.SDL_FRONTEND_HOST || "localhost"
  const frontendPort = Number(env.SDL_FRONTEND_PORT || "1420")

  return {
    clearScreen: false,
    plugins: [vue()],
    server: {
      host: frontendHost,
      open: true,
      port: frontendPort,
      strictPort: true,
      watch: {
        ignored: ["**/target/**"],
      },
    },
  }
})

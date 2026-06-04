import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { spawn } from "node:child_process"
import { existsSync, readFileSync } from "node:fs"
import net from "node:net"

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const children = []

function spawnTask(command, args, options = {}) {
  const child = spawn(command, args, {
    cwd: rootDir,
    env: {
      ...process.env,
      ...options.env,
    },
    stdio: "inherit",
    shell: process.platform === "win32",
  })

  children.push(child)

  child.on("exit", code => {
    if (shuttingDown) {
      return
    }

    shuttingDown = true
    shutdown(code ?? 0)
  })

  return child
}

let shuttingDown = false

function shutdown(exitCode) {
  for (const child of children) {
    if (!child.killed) {
      child.kill("SIGTERM")
    }
  }

  setTimeout(() => process.exit(exitCode), 150)
}

process.on("SIGINT", () => shutdown(0))
process.on("SIGTERM", () => shutdown(0))

function loadEnvFile() {
  const envPath = resolve(rootDir, ".env.development")

  if (!existsSync(envPath)) {
    return {}
  }

  const env = {}

  for (const line of readFileSync(envPath, "utf8").split(/\r?\n/)) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith("#")) {
      continue
    }

    const separatorIndex = trimmed.indexOf("=")
    if (separatorIndex === -1) {
      continue
    }

    const key = trimmed.slice(0, separatorIndex).trim()
    const value = trimmed.slice(separatorIndex + 1).trim()
    env[key] = value
  }

  return env
}

function isPortOpen(host, port) {
  return new Promise(resolvePromise => {
    const socket = net.connect({ host, port: Number(port) })

    socket.once("connect", () => {
      socket.end()
      resolvePromise(true)
    })

    socket.once("error", () => {
      resolvePromise(false)
    })
  })
}

const devEnv = loadEnvFile()
const frontendHost = devEnv.SDL_FRONTEND_HOST || "localhost"
const frontendPort = devEnv.SDL_FRONTEND_PORT || "1420"

spawnTask("npm", ["run", "tailwind:watch"])

if (await isPortOpen(frontendHost, frontendPort)) {
  console.log(`Vite dev server already detected on ${frontendHost}:${frontendPort}; reusing it for Tauri.`)
} else {
  spawnTask("npm", ["run", "dev"], {
    env: {
      SDL_OPEN_BROWSER: "false",
    },
  })
}

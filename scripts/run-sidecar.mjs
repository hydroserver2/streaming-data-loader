import { existsSync, readFileSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { spawnSync } from "node:child_process"

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const pythonPath =
  process.platform === "win32"
    ? resolve(rootDir, ".venv", "Scripts", "python.exe")
    : resolve(rootDir, ".venv", "bin", "python")

if (!existsSync(pythonPath)) {
  console.error("Sidecar virtual environment is missing. Run the sidecar bootstrap first.")
  process.exit(1)
}

const envPath = resolve(rootDir, ".env.development")
const env = { ...process.env }

if (existsSync(envPath)) {
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
}

const configDir = env.SDL_CONFIG_DIR
  ? resolve(rootDir, env.SDL_CONFIG_DIR)
  : resolve(rootDir, ".local", "sidecar-dev")

const result = spawnSync(
  pythonPath,
  [
    "-m",
    "sidecar.main",
    "--host",
    env.SDL_SIDECAR_HOST || "127.0.0.1",
    "--port",
    env.SDL_SIDECAR_PORT || "5321",
    "--config-dir",
    configDir,
  ],
  {
    cwd: rootDir,
    stdio: "inherit",
    env,
  }
)

process.exit(result.status ?? 1)

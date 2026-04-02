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

const host = env.SDL_SIDECAR_HOST || "127.0.0.1"
const port = env.SDL_SIDECAR_PORT || "5321"

ensureFreshSidecarProcess({ port, configDir })

const result = spawnSync(
  pythonPath,
  [
    "-m",
    "sidecar.main",
    "--reload",
    "--host",
    host,
    "--port",
    port,
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

function ensureFreshSidecarProcess({ port, configDir }) {
  const currentPid = String(process.pid)
  const listenerPids = findListeningPids(port)

  for (const pid of listenerPids) {
    if (pid === currentPid) {
      continue
    }

    const command = readCommand(pid)
    if (!command.includes("sidecar.main")) {
      continue
    }

    if (!command.includes(configDir)) {
      continue
    }

    terminatePid(pid)
  }

  waitForPortRelease(port)
}

function findListeningPids(port) {
  if (process.platform === "win32") {
    return []
  }

  const result = spawnSync("lsof", ["-tiTCP:" + port, "-sTCP:LISTEN"], {
    cwd: rootDir,
    encoding: "utf8",
  })

  if (result.status !== 0 && !result.stdout) {
    return []
  }

  return result.stdout
    .split(/\r?\n/)
    .map(value => value.trim())
    .filter(Boolean)
}

function readCommand(pid) {
  if (process.platform === "win32") {
    return ""
  }

  const result = spawnSync("ps", ["-p", pid, "-o", "command="], {
    cwd: rootDir,
    encoding: "utf8",
  })

  return result.stdout.trim()
}

function terminatePid(pid) {
  if (process.platform === "win32") {
    return
  }

  spawnSync("kill", ["-TERM", pid], {
    cwd: rootDir,
    encoding: "utf8",
  })
}

function waitForPortRelease(port) {
  if (process.platform === "win32") {
    return
  }

  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (findListeningPids(port).length === 0) {
      return
    }

    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 50)
  }
}

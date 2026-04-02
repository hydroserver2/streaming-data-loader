import { createHash } from "node:crypto"
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { spawnSync } from "node:child_process"

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const cacheDir = resolve(rootDir, ".cache")
const requirementsPath = resolve(rootDir, "sidecar", "requirements.txt")
const stampPath = resolve(cacheDir, "sidecar-requirements.sha256")
const venvDir = resolve(rootDir, ".venv")

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: rootDir,
    stdio: "inherit",
    shell: process.platform === "win32",
    ...options,
  })

  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex")
}

function findSystemPython() {
  const candidates =
    process.platform === "win32"
      ? [
          ["py", ["-3"]],
          ["python", []],
        ]
      : [
          ["python3", []],
          ["python", []],
        ]

  for (const [command, args] of candidates) {
    const result = spawnSync(command, [...args, "--version"], {
      cwd: rootDir,
      stdio: "ignore",
      shell: process.platform === "win32",
    })

    if (result.status === 0) {
      return { command, args }
    }
  }

  throw new Error("Python 3 was not found on PATH.")
}

function venvPythonPath() {
  return process.platform === "win32"
    ? resolve(venvDir, "Scripts", "python.exe")
    : resolve(venvDir, "bin", "python")
}

mkdirSync(cacheDir, { recursive: true })

if (!existsSync(venvPythonPath())) {
  const python = findSystemPython()
  run(python.command, [...python.args, "-m", "venv", ".venv"])
}

const currentHash = sha256(requirementsPath)
const storedHash = existsSync(stampPath) ? readFileSync(stampPath, "utf8") : ""

if (storedHash !== currentHash) {
  const python = venvPythonPath()
  run(python, ["-m", "pip", "install", "--upgrade", "pip"])
  run(python, ["-m", "pip", "install", "-r", "sidecar/requirements.txt"])
  writeFileSync(stampPath, currentHash)
}

import { createHash } from "node:crypto"
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { spawnSync } from "node:child_process"

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const cacheDir = resolve(rootDir, ".cache")
const stampPath = resolve(cacheDir, "frontend-package-lock.sha256")
const lockfilePath = resolve(rootDir, "package-lock.json")
const nodeModulesPath = resolve(rootDir, "node_modules")

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: rootDir,
    stdio: "inherit",
    shell: process.platform === "win32",
  })

  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex")
}

mkdirSync(cacheDir, { recursive: true })

const currentHash = sha256(lockfilePath)
const storedHash = existsSync(stampPath) ? readFileSync(stampPath, "utf8") : ""
const needsInstall = !existsSync(nodeModulesPath) || storedHash !== currentHash

if (needsInstall) {
  run("npm", ["install"])
  writeFileSync(stampPath, currentHash)
}

run("npm", ["run", "tailwind:build"])

import { spawnSync } from "node:child_process"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const tauriArgs = process.argv.slice(2)

function run(command, args, options = {}) {
  return spawnSync(command, args, {
    cwd: rootDir,
    stdio: "inherit",
    shell: process.platform === "win32",
    ...options,
  })
}

function commandExists(command, args = ["--version"]) {
  const result = spawnSync(command, args, {
    cwd: rootDir,
    stdio: "ignore",
    shell: process.platform === "win32",
  })

  return result.status === 0
}

if (!commandExists("cargo")) {
  console.error("")
  console.error("Tauri desktop preview requires the Rust toolchain, but `cargo` is not installed or not on PATH.")
  console.error("")
  console.error("Install it with one of these:")
  console.error("  1. curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh")
  console.error("  2. brew install rustup-init && rustup-init")
  console.error("")
  console.error("Then restart your terminal and verify:")
  console.error("  cargo --version")
  console.error("")
  console.error("After that, run:")
  console.error("  npm run tauri dev")
  console.error("")
  process.exit(1)
}

if (!commandExists("rustc")) {
  console.error("")
  console.error("Rust appears to be partially installed: `cargo` exists but `rustc` does not.")
  console.error("Run `rustup default stable` and try again.")
  console.error("")
  process.exit(1)
}

const result = run("npx", ["--no-install", "tauri", ...tauriArgs], { cwd: resolve(rootDir, "ui") })
process.exit(result.status ?? 1)

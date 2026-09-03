#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const exe = process.platform === "win32" ? "flarelint.exe" : "flarelint";

const PLATFORM_PACKAGES = {
  "darwin-arm64": "@flarelint/darwin-arm64",
  "darwin-x64": "@flarelint/darwin-x64",
  "linux-x64": "@flarelint/linux-x64",
  "linux-arm64": "@flarelint/linux-arm64",
  "win32-x64": "@flarelint/win32-x64",
};

function log(message) {
  console.log(`[flarelint] ${message}`);
}

function runnable(file) {
  if (!fs.existsSync(file)) return false;
  const res = spawnSync(file, ["--version"], { stdio: "ignore", timeout: 10_000 });
  return !res.error && res.status === 0;
}

function alreadyPresent() {
  const pkg = PLATFORM_PACKAGES[`${process.platform}-${process.arch}`];
  if (pkg) {
    try {
      if (runnable(require.resolve(`${pkg}/bin/${exe}`))) return true;
    } catch {
      // Not installed; keep looking
    }
  }

  return ["dist/bin", "target/release"].some((dir) => runnable(path.join(root, dir, exe)));
}

function hasCargo() {
  const res = spawnSync("cargo", ["--version"], { stdio: "ignore" });
  return res.status === 0;
}

function main() {
  if (alreadyPresent()) return;

  if (!hasCargo()) {
    log(
      `No prebuilt binary found for ${process.platform}-${process.arch} and cargo is not installed.\n` +
        `  To build from source, install Rust (https://rustup.rs) and run \`cargo build --release\`,\n` +
        `  or set FLARELINT_BIN to an existing flarelint binary.`,
    );
    return;
  }

  log("No prebuilt binary for this platform — compiling with cargo...");
  const res = spawnSync("cargo", ["build", "--release"], { cwd: root, stdio: "inherit" });

  if (res.status !== 0) {
    log("cargo build failed. Run `cargo build --release` manually to see the error.");
    return;
  }

  const built = path.join(root, "target", "release", exe);
  const destDir = path.join(root, "dist", "bin");
  if (fs.existsSync(built)) {
    fs.mkdirSync(destDir, { recursive: true });
    fs.copyFileSync(built, path.join(destDir, exe));
    if (process.platform !== "win32") fs.chmodSync(path.join(destDir, exe), 0o755);
    log("Engine built successfully.");
  }
}

try {
  main();
} catch (err) {
  log(`Postinstall skipped: ${err.message}`);
}

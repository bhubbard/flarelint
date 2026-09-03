#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const SLUGS = ["darwin-arm64", "darwin-x64", "linux-arm64", "linux-x64", "win32-x64"];

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifestPath = path.join(root, "package.json");
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf-8"));

const optional = {};
for (const slug of SLUGS) {
  optional[`@flarelint/${slug}`] = manifest.version;
}

manifest.optionalDependencies = optional;
delete manifest["//optionalDependencies"];

fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

console.log(
  `[flarelint] Added ${SLUGS.length} optionalDependencies at version ${manifest.version}:`,
);
for (const slug of SLUGS) console.log(`  @flarelint/${slug}`);
console.log("\nThis edit is for publish artifact only — do not commit it.");

if (process.env.CI !== "true") {
  console.log("Warning: not running in CI. `git checkout package.json` when finished.");
}

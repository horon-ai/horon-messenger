#!/usr/bin/env node
// Syncs every npm package's version, and each dependency version, to the
// workspace version in Cargo.toml. Run before publishing so the wrapper and
// bundle packages never drift from the Rust release. Called by
// .github/workflows/release.yml; safe to run locally (idempotent).
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const version = readFileSync(join(repoRoot, "Cargo.toml"), "utf8").match(/^version = "([^"]+)"/m)?.[1];
if (!version) {
  console.error("could not read the workspace version from Cargo.toml");
  process.exit(1);
}

const targets = [
  "npm/cli/package.json",
  "npm/mcp/package.json",
  "npm/server/package.json",
  "npm/bundles/server-mcp/package.json",
  "npm/bundles/server-tui/package.json",
  "npm/bundles/server-mcp-tui/package.json",
  "npm/full/package.json",
];

for (const relative of targets) {
  const path = join(repoRoot, relative);
  const manifest = JSON.parse(readFileSync(path, "utf8"));
  manifest.version = version;
  for (const section of ["dependencies", "optionalDependencies"]) {
    if (!manifest[section]) continue;
    for (const name of Object.keys(manifest[section])) {
      manifest[section][name] = version;
    }
  }
  writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`synced ${relative} -> ${version}`);
}

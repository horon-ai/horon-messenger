#!/usr/bin/env node
// Prints the next version for a current version and bump level.
// usage: node scripts/release/next-version.mjs <currentVersion> [--level minor|patch|major]
const args = process.argv.slice(2);
const current = args[0];
const li = args.indexOf("--level");
const level = li !== -1 ? args[li + 1] : "minor";

if (!current || !/^\d+\.\d+\.\d+$/.test(current)) {
  console.error(`usage: next-version.mjs <X.Y.Z> [--level minor|patch|major] (got: ${current ?? "<none>"})`);
  process.exit(1);
}
if (!["minor", "patch", "major"].includes(level)) {
  console.error(`unknown level: ${level}`);
  process.exit(1);
}

const [major, minor, patch] = current.split(".").map(Number);
const next =
  level === "major"
    ? `${major + 1}.0.0`
    : level === "minor"
      ? `${major}.${minor + 1}.0`
      : `${major}.${minor}.${patch + 1}`;

console.log(next);

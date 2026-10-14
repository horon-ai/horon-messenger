#!/usr/bin/env node
// Rewrites the workspace version in a Cargo.toml: the single version line under
// [workspace.package] and the version token inside every horon-messenger-* pin
// under [workspace.dependencies]. Third-party dependency versions are untouched.
// usage: node scripts/release/bump.mjs <newVersion> [--file <path>]
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const args = process.argv.slice(2);
const next = args[0];
const fi = args.indexOf("--file");
const file = fi !== -1 ? resolve(args[fi + 1]) : join(repoRoot, "Cargo.toml");

if (!next || !/^\d+\.\d+\.\d+$/.test(next)) {
  console.error(`usage: bump.mjs <X.Y.Z> [--file <path>] (got: ${next ?? "<none>"})`);
  process.exit(1);
}

const lines = readFileSync(file, "utf8").split("\n");
let inPackage = false;
let inDependencies = false;
let changed = 0;

const out = lines.map((line) => {
  const section = line.match(/^\[(.+)\]\s*$/);
  if (section) {
    inPackage = section[1] === "workspace.package";
    inDependencies = section[1] === "workspace.dependencies";
    return line;
  }
  if (inPackage && /^version = "/.test(line)) {
    changed += 1;
    return line.replace(/version = "[^"]*"/, `version = "${next}"`);
  }
  if (inDependencies && /^horon-messenger-/.test(line) && /version = "[^"]*"/.test(line)) {
    changed += 1;
    return line.replace(/version = "[^"]*"/, `version = "${next}"`);
  }
  return line;
});

if (changed === 0) {
  console.error(`no version lines changed in ${file}`);
  process.exit(1);
}

writeFileSync(file, out.join("\n"));
console.log(`bumped ${changed} version line(s) in ${file} -> ${next}`);

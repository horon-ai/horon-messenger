import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const dir = dirname(fileURLToPath(import.meta.url));
const nextVersion = join(dir, "next-version.mjs");
const bump = join(dir, "bump.mjs");

function run(script, args) {
  return execFileSync(process.execPath, [script, ...args], { encoding: "utf8" }).trim();
}

test("next-version defaults to a minor bump", () => {
  assert.equal(run(nextVersion, ["0.1.0"]), "0.2.0");
});

test("next-version supports patch and major", () => {
  assert.equal(run(nextVersion, ["0.1.0", "--level", "patch"]), "0.1.1");
  assert.equal(run(nextVersion, ["0.1.0", "--level", "major"]), "1.0.0");
});

test("next-version rejects a malformed version", () => {
  assert.throws(() => run(nextVersion, ["1.2"]));
  assert.throws(() => run(nextVersion, ["abc"]));
});

test("bump rewrites the workspace version and every internal pin, leaving third-party deps", () => {
  const real = readFileSync(join(dir, "..", "..", "Cargo.toml"), "utf8");
  const d = mkdtempSync(join(tmpdir(), "bump-"));
  const f = join(d, "Cargo.toml");
  writeFileSync(f, real);
  run(bump, ["9.9.9", "--file", f]);
  const got = readFileSync(f, "utf8");
  const count = (got.match(/version = "9\.9\.9"/g) || []).length;
  assert.equal(count, 17, "one workspace version plus sixteen internal pins");
  assert.match(got, /\[workspace\.package\]\nversion = "9\.9\.9"/);
  assert.match(got, /horon-messenger-sdk = \{ path = "crates\/sdk", version = "9\.9\.9", default-features = false \}/);
  assert.match(got, /serde = \{ version = "1", features = \["derive"\] \}/);
  assert.match(got, /serde_json = "1"/);
  rmSync(d, { recursive: true, force: true });
});

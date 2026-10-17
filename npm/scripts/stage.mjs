#!/usr/bin/env node
// Stages one built Rust binary into a publishable per-platform npm package and
// prints the directory to publish. Called by .github/workflows/release.yml and
// usable locally to test a wrapper before release.
//
// usage: node npm/scripts/stage.mjs <surface> <platform> <binary-path>
//   surface  : cli | mcp | server | ui
//   platform : darwin-arm64 | darwin-x64 | linux-x64-gnu | linux-arm64-gnu | win32-x64-msvc
import { chmodSync, copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SURFACES = {
  cli: { pkg: "@horon-ai/messenger-cli", bin: "horon-messenger" },
  mcp: { pkg: "@horon-ai/messenger-mcp", bin: "horon-messenger-mcp" },
  server: { pkg: "@horon-ai/messenger-server", bin: "horon-messenger-server" },
  ui: { pkg: "@horon-ai/messenger-tui", bin: "horon-messenger-tui" },
};

const PLATFORMS = {
  "darwin-arm64": { os: ["darwin"], cpu: ["arm64"] },
  "darwin-x64": { os: ["darwin"], cpu: ["x64"] },
  "linux-x64-gnu": { os: ["linux"], cpu: ["x64"], libc: ["glibc"] },
  "linux-arm64-gnu": { os: ["linux"], cpu: ["arm64"], libc: ["glibc"] },
  "win32-x64-msvc": { os: ["win32"], cpu: ["x64"] },
};

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");

const [surface, platform, binaryPath] = process.argv.slice(2);
const spec = SURFACES[surface];
const target = PLATFORMS[platform];
if (!spec || !target) {
  console.error(`usage: node npm/scripts/stage.mjs <${Object.keys(SURFACES).join("|")}> <${Object.keys(PLATFORMS).join("|")}> <binary-path>`);
  process.exit(1);
}
if (!binaryPath) {
  console.error("binary path is required");
  process.exit(1);
}

const version = readFileSync(join(repoRoot, "Cargo.toml"), "utf8").match(/^version = "([^"]+)"/m)?.[1];
if (!version) {
  console.error("could not read the workspace version from Cargo.toml");
  process.exit(1);
}

const outDir = join(repoRoot, "npm", ".staging", `${surface}-${platform}`);
rmSync(outDir, { recursive: true, force: true });
mkdirSync(join(outDir, "bin"), { recursive: true });

const executable = process.platform === "win32" ? `${spec.bin}.exe` : spec.bin;
copyFileSync(resolve(binaryPath), join(outDir, "bin", executable));
if (process.platform !== "win32") {
  chmodSync(join(outDir, "bin", executable), 0o755);
}

const manifest = {
  name: `${spec.pkg}-${platform}`,
  version,
  description: `Prebuilt ${spec.bin} binary for ${platform}. Installed automatically by ${spec.pkg}.`,
  license: "MIT",
  homepage: "https://github.com/horon-ai/messenger",
  repository: { type: "git", url: "git+https://github.com/horon-ai/messenger.git" },
  os: target.os,
  cpu: target.cpu,
  ...(target.libc ? { libc: target.libc } : {}),
  files: ["bin/"],
  publishConfig: { access: "public" },
};
writeFileSync(join(outDir, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`);

console.log(outDir);

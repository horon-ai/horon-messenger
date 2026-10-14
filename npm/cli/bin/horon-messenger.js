#!/usr/bin/env node
"use strict";

const { spawn } = require("node:child_process");
const path = require("node:path");

const PACKAGE = "@horon-ai/messenger-cli";
const UPDATE_PACKAGE = "@horon-ai/messenger-cli";
const BINARY = "horon-messenger";

const PLATFORM_SUFFIX = {
  "darwin-arm64": "darwin-arm64",
  "darwin-x64": "darwin-x64",
  "linux-x64": "linux-x64-gnu",
  "linux-arm64": "linux-arm64-gnu",
  "win32-x64": "win32-x64-msvc",
};

function detectManager() {
  const agent = process.env.npm_config_user_agent || "";
  if (agent.startsWith("npm")) return "npm";
  if (agent.startsWith("pnpm")) return "pnpm";
  if (agent.startsWith("yarn")) return "yarn";
  if (typeof Bun !== "undefined" || process.versions.bun) return "bun";
  return "";
}

const key = `${process.platform}-${process.arch}`;
const suffix = PLATFORM_SUFFIX[key];
if (!suffix) {
  console.error(`${BINARY}: no prebuilt binary for ${key}.`);
  process.exit(1);
}

let directory;
try {
  directory = path.dirname(require.resolve(`${PACKAGE}-${suffix}/package.json`));
} catch {
  console.error(`${BINARY}: the platform package ${PACKAGE}-${suffix} is not installed.`);
  console.error("Reinstall without --no-optional, or build from source: cargo install --path crates/cli");
  process.exit(1);
}

const executable = path.join(directory, "bin", process.platform === "win32" ? `${BINARY}.exe` : BINARY);
const env = { ...process.env, HORON_MESSENGER_PACKAGE: UPDATE_PACKAGE };
const manager = detectManager();
if (manager) env.HORON_MESSENGER_INSTALL_MANAGER = manager;

const child = spawn(executable, process.argv.slice(2), { stdio: "inherit", env });

child.on("error", (error) => {
  console.error(`${BINARY}: failed to launch ${executable}: ${error.message}`);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code === null ? 1 : code);
  }
});

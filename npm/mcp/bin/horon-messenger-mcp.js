#!/usr/bin/env node
"use strict";

const { spawn } = require("node:child_process");
const path = require("node:path");

const PACKAGE = "@horon-ai/messenger-mcp";
const BINARY = "horon-messenger-mcp";

const PLATFORM_SUFFIX = {
  "darwin-arm64": "darwin-arm64",
  "darwin-x64": "darwin-x64",
  "linux-x64": "linux-x64-gnu",
  "linux-arm64": "linux-arm64-gnu",
  "win32-x64": "win32-x64-msvc",
};

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
  console.error("Reinstall without --no-optional, or build from source: cargo install --path crates/mcp");
  process.exit(1);
}

const executable = path.join(directory, "bin", process.platform === "win32" ? `${BINARY}.exe` : BINARY);
const child = spawn(executable, process.argv.slice(2), { stdio: "inherit" });

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

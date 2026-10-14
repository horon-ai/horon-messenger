# npm wrappers

Thin npm packages that ship the prebuilt Rust binaries, so the CLI, MCP server
and HTTP server install with `npx` or `bunx` and no Rust toolchain. All
functionality stays in Rust; these packages only locate and run the compiled
binary. The TUI is not offered on its own; its binary ships inside the two
Server + TUI bundles. `@horon-ai/messenger` installs every surface at once.

## Packages

| Surface | Package | Binary | Platform packages |
|---------|---------|--------|-------------------|
| CLI | `@horon-ai/messenger-cli` | `horon-messenger` | `@horon-ai/messenger-cli-<platform>` |
| MCP | `@horon-ai/messenger-mcp` | `horon-messenger-mcp` | `@horon-ai/messenger-mcp-<platform>` |
| Server | `@horon-ai/messenger-server` | `horon-messenger-server` | `@horon-ai/messenger-server-<platform>` |
| TUI | (bundle only, no standalone package) | `horon-messenger-tui` | `@horon-ai/messenger-tui-<platform>` |
| Everything | `@horon-ai/messenger` | all four | (runs the CLI; pulls the rest) |

Platforms: `darwin-arm64`, `darwin-x64`, `linux-x64-gnu`, `linux-arm64-gnu`,
`win32-x64-msvc`.

## Deployment combinations

Each surface publishes on its own, and three server-anchored bundles publish the
combinations that are meant to run together. A bundle has no binary; it only
depends on the surfaces it names, so installing it puts each binary on the path.

| Deploy | Package | Contains |
|--------|---------|----------|
| CLI | `@horon-ai/messenger-cli` | cli |
| MCP | `@horon-ai/messenger-mcp` | mcp |
| Server | `@horon-ai/messenger-server` | server |
| Server + MCP | `@horon-ai/messenger-server-mcp` | server, mcp |
| Server + TUI | `@horon-ai/messenger-server-tui` | server, tui |
| Server + MCP + TUI | `@horon-ai/messenger-server-mcp-tui` | server, mcp, tui |
| Everything | `@horon-ai/messenger` | cli, mcp, server, tui |

The bundle sources live in `npm/bundles/`. `cli` and `mcp` also run straight
from the registry with `npx` or `bunx`; the bundles are for install (`npm install
-g` / `bun add -g`).

The all-in-one `@horon-ai/messenger` carries the CLI launcher itself, so
`npx @horon-ai/messenger <command>` runs the CLI and pulls every other surface as
a dependency. Its launcher also sets `HORON_MESSENGER_PACKAGE`, so
`horon-messenger update` self-updates the whole install rather than only the CLI.
When the CLI is run through `@horon-ai/messenger-cli` on its own, `update`
installs `@horon-ai/messenger-cli@latest`.

## How it works

Each wrapper package has a `bin` launcher (`bin/<binary>.js`) and lists every
platform package in `optionalDependencies`. The two TUI bundles carry the TUI
launcher themselves and pull the TUI platform package, so the TUI binary reaches
the machine only through a bundle. npm and bun install only the package
whose `os`/`cpu`/`libc` match the host, so an install downloads exactly one
prebuilt binary and no postinstall download is needed (this works offline and
with `--ignore-scripts`). The launcher resolves that binary, spawns it with the
caller's arguments, and forwards stdio and the exit code. This is the same shape
esbuild, biome, oxlint and turbo use.

## Adding a platform

1. Add the platform to `PLATFORMS` in `scripts/stage.mjs` and to
   `PLATFORM_SUFFIX` in each `bin/*.js` launcher.
2. Add the matching `@horon-ai/<surface>-<platform>` entry to the
   `optionalDependencies` of every wrapper package.
3. Add a matrix entry to `.github/workflows/release.yml`.

## Release

Releases are cut from `main`, not by hand. Feature work lands on `dev`; a
`dev` → `main` pull request promotes it. Once CI is green on `main`,
`.github/workflows/release-main.yml` bumps the workspace minor version in
`Cargo.toml`, refreshes `Cargo.lock`, syncs every npm manifest with
`scripts/sync-version.mjs`, commits `chore(release): vX.Y.Z`, tags `vX.Y.Z`, and
dispatches `.github/workflows/release.yml` at that tag (a tag pushed with the
default `GITHUB_TOKEN` cannot start a workflow by itself, so the dispatch is
explicit).

`release.yml` builds the four surfaces with `--all-features` on five platforms,
stages them into platform packages with `scripts/stage.mjs`, publishes those
first, then the wrapper packages, the bundles, and finally the all-in-one
`@horon-ai/messenger`, and creates the GitHub Release with the binaries attached.
Publishing to npm needs an `NPM_TOKEN` repository secret with publish rights to
the `@horon-ai` scope (the scope must exist on npm); without the secret the npm
steps are skipped with a warning and the GitHub Release is still created.

The version lives only in the root `Cargo.toml`; `scripts/release/bump.mjs`
rewrites it (workspace version plus the internal `horon-messenger-*` pins) and
`node --test scripts/release` covers it.

The published binaries are built with every provider enabled. To install a
smaller provider set, use the cargo path instead:
`cargo install --path crates/cli --features discord,telegram`.

## Local test

```bash
cargo build -p horon-messenger-cli
dir=$(node npm/scripts/stage.mjs cli darwin-arm64 target/debug/horon-messenger)
mkdir -p npm/cli/node_modules/@horon-ai
ln -s "$dir" npm/cli/node_modules/@horon-ai/messenger-cli-darwin-arm64
node npm/cli/bin/horon-messenger.js providers
```

`npm/.staging/` and `node_modules/` are gitignored.

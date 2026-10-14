//! Self-update against the latest npm release of the installed package.

use std::process::Command;
use std::time::Duration;

use horon_messenger_sdk::{Error, Result};
use serde_json::{Value, json};

use crate::daemon::which;

/// The package updated when the CLI was not launched from an npm install.
pub const DEFAULT_PACKAGE: &str = "@horon-ai/messenger-cli";

const REGISTRY: &str = "https://registry.npmjs.org";

/// The latest published version of `package` on the npm registry.
pub async fn latest_version(package: &str) -> Result<String> {
    let url = format!("{REGISTRY}/{package}/latest");
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| Error::Internal {
            message: format!("could not build the HTTP client: {error}"),
        })?
        .get(&url)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|error| Error::Internal {
            message: format!("could not reach the npm registry: {error}"),
        })?;
    if !response.status().is_success() {
        return Err(Error::Internal {
            message: format!(
                "the npm registry returned {} for {package}",
                response.status()
            ),
        });
    }
    let body: Value = response.json().await.map_err(|error| Error::Internal {
        message: format!("could not read the registry response for {package}: {error}"),
    })?;
    body.get("version")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::Internal {
            message: format!("the registry response for {package} carried no version"),
        })
}

/// Whether `latest` is a newer semantic version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    parse_version(latest) > parse_version(current)
}

/// Install `package@latest` through the detected package manager, streaming its
/// output to stderr so the JSON envelope on stdout stays intact.
pub fn install(package: &str, current: &str, latest: &str) -> Result<Value> {
    let manager = detect_manager().ok_or_else(|| Error::Internal {
        message: "no supported package manager (npm, bun, pnpm, yarn) was found on PATH".to_owned(),
    })?;
    let spec = format!("{package}@latest");
    let mut command = match manager.as_str() {
        "bun" => {
            let mut command = Command::new("bun");
            command.args(["add", "-g", spec.as_str()]);
            command
        }
        "pnpm" => {
            let mut command = Command::new("pnpm");
            command.args(["add", "-g", spec.as_str()]);
            command
        }
        "yarn" => {
            let mut command = Command::new("yarn");
            command.args(["global", "add", spec.as_str()]);
            command
        }
        _ => {
            let mut command = Command::new("npm");
            command.args(["install", "-g", spec.as_str()]);
            command
        }
    };

    let output = command.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.trim().is_empty() {
        eprint!("{stdout}");
    }
    if !stderr.trim().is_empty() {
        eprint!("{stderr}");
    }
    if !output.status.success() {
        return Err(Error::Internal {
            message: format!(
                "`{manager}` exited with {}; the update did not complete",
                output.status
            ),
        });
    }
    Ok(json!({
        "installed": true,
        "manager": manager,
        "package": package,
        "from": current,
        "to": latest,
    }))
}

fn detect_manager() -> Option<String> {
    if let Ok(value) = std::env::var("HORON_MESSENGER_INSTALL_MANAGER") {
        let value = value.trim().to_ascii_lowercase();
        if matches!(value.as_str(), "npm" | "bun" | "pnpm" | "yarn") {
            return Some(value);
        }
    }
    ["bun", "npm", "pnpm", "yarn"]
        .into_iter()
        .find(|name| which(name).is_some())
        .map(str::to_owned)
}

fn parse_version(version: &str) -> (u64, u64, u64) {
    let core = version
        .trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or("");
    let mut parts = core.split('.').map(|part| part.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

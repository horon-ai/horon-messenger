use std::process::Command;

use serde_json::Value;

fn run(args: &[&str]) -> (bool, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_horon-messenger"))
        .args(args)
        .env(
            "HORON_MESSENGER_CONFIG_DIR",
            "/tmp/horon-messenger-cli-test",
        )
        .env_remove("HORON_MESSENGER_DISCORD_TOKEN")
        .env_remove("E2E_DISCORD_TOKEN")
        .env_remove("HORON_MESSENGER_CHANNELTALK_ACCOUNT_COOKIE")
        .output()
        .expect("run the cli");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let value: Value = serde_json::from_str(&stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not a JSON envelope ({error}): {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), value)
}

fn run_status(args: &[&str]) -> i32 {
    Command::new(env!("CARGO_BIN_EXE_horon-messenger"))
        .args(args)
        .env(
            "HORON_MESSENGER_CONFIG_DIR",
            "/tmp/horon-messenger-cli-test",
        )
        .output()
        .expect("run the cli")
        .status
        .code()
        .expect("the cli exited via a signal")
}

#[test]
fn providers_lists_the_mock_provider() {
    let (ok, envelope) = run(&["providers"]);
    assert!(ok);
    assert_eq!(envelope["ok"], true);
    let providers = envelope["data"].as_array().expect("data is an array");
    assert!(providers.iter().any(|provider| provider["kind"] == "mock"));
}

#[test]
fn chats_returns_seeded_chats() {
    let (ok, envelope) = run(&["chats", "mock"]);
    assert!(ok);
    let chats = envelope["data"].as_array().expect("data is an array");
    assert_eq!(chats.len(), 3);
}

#[test]
fn messages_returns_seeded_messages() {
    let (ok, envelope) = run(&["messages", "mock", "general"]);
    assert!(ok);
    let messages = envelope["data"]["messages"].as_array().expect("messages");
    assert!(messages.iter().any(|message| message["text"] != ""));
}

#[test]
fn send_reports_acceptance() {
    let (ok, envelope) = run(&["send", "mock", "general", "cli send"]);
    assert!(ok);
    assert_eq!(envelope["data"]["accepted"], true);
    assert_eq!(envelope["data"]["delivery"], "confirmed");
    assert!(envelope["data"]["message_id"].is_string());
}

#[test]
fn unknown_provider_is_a_non_zero_envelope() {
    let (ok, envelope) = run(&["chats", "myspace"]);
    assert!(!ok);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "INVALID_INPUT");
}

#[cfg(feature = "discord")]
#[test]
fn platform_provider_without_credentials_requires_auth() {
    let (ok, envelope) = run(&["chats", "discord"]);
    assert!(!ok);
    assert_eq!(envelope["error"]["provider"], "discord");
    assert_eq!(envelope["error"]["code"], "AUTH_REQUIRED");
}

#[test]
fn login_with_params_authenticates_the_mock_provider() {
    let (ok, envelope) = run(&["auth", "login", "mock", "--param", "token=abc"]);
    assert!(ok);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["authenticated"], true);
    let (ok, status) = run(&["auth", "status", "mock"]);
    assert!(ok);
    assert_eq!(status["data"]["authenticated"], true);
}

#[test]
fn login_with_a_param_without_an_equals_is_a_usage_error() {
    assert_eq!(run_status(&["auth", "login", "mock", "--param", "foo"]), 2);
}

use std::process::Command;

use serde_json::Value;

fn run(args: &[&str]) -> (bool, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_hyper-messenger"))
        .args(args)
        .env(
            "HYPER_MESSENGER_CONFIG_DIR",
            "/tmp/hyper-messenger-cli-test",
        )
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

#[test]
fn platform_provider_reports_a_typed_failure() {
    let (ok, envelope) = run(&["chats", "discord"]);
    assert!(!ok);
    assert_eq!(envelope["error"]["provider"], "discord");
    let code = envelope["error"]["code"]
        .as_str()
        .expect("code is a string");
    assert!(
        matches!(code, "NOT_REGISTERED" | "NOT_IMPLEMENTED"),
        "unexpected error code {code}"
    );
}

use horon_messenger_core::{
    Capabilities, ConfigPaths, CredentialStore, Error, ErrorCode, FileCredentialStore, Provider,
    ProviderKind,
};
use serde_json::json;

#[test]
fn provider_kind_round_trips_through_strings() {
    for kind in ProviderKind::ALL {
        assert_eq!(kind.as_str().parse::<ProviderKind>().unwrap(), kind);
    }
    assert_eq!(
        "kakao".parse::<ProviderKind>().unwrap(),
        ProviderKind::KakaoTalk
    );
    assert_eq!(
        "Channel-Talk".parse::<ProviderKind>().unwrap(),
        ProviderKind::ChannelTalk
    );
    assert_eq!(ProviderKind::ALL.len(), 13);

    let aliases = [
        ("slack", ProviderKind::Slack),
        ("slackbot", ProviderKind::SlackBot),
        ("slack-bot", ProviderKind::SlackBot),
        ("telegrambot", ProviderKind::TelegramBot),
        ("telegram-bot", ProviderKind::TelegramBot),
        ("whatsapp", ProviderKind::WhatsApp),
        ("whatsappbot", ProviderKind::WhatsAppBot),
        ("whatsapp-bot", ProviderKind::WhatsAppBot),
        ("line", ProviderKind::Line),
        ("imessage", ProviderKind::IMessage),
        ("i-message", ProviderKind::IMessage),
        ("instagram", ProviderKind::Instagram),
        ("ig", ProviderKind::Instagram),
    ];
    for (text, expected) in aliases {
        assert_eq!(text.parse::<ProviderKind>().unwrap(), expected, "{text}");
    }

    assert!("myspace".parse::<ProviderKind>().is_err());
}

#[test]
fn capability_gate_distinguishes_unbuilt_from_unsupported() {
    struct Gate {
        advertised: Capabilities,
    }

    #[async_trait::async_trait]
    impl Provider for Gate {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Mock
        }
        fn capabilities(&self) -> Capabilities {
            self.advertised
        }
    }

    let advertised = Gate {
        advertised: Capabilities::LIST_CHATS,
    };
    let error = advertised.gate(Capabilities::LIST_CHATS);
    assert_eq!(error.code(), ErrorCode::NotImplemented);

    let unsupported = advertised.gate(Capabilities::SEND_MESSAGE);
    assert_eq!(unsupported.code(), ErrorCode::Unsupported);
    assert!(matches!(unsupported, Error::Unsupported { .. }));
}

#[test]
fn capabilities_report_their_names() {
    let caps = Capabilities::WHOAMI | Capabilities::SEND_MESSAGE;
    assert!(caps.contains(Capabilities::WHOAMI));
    assert!(!caps.contains(Capabilities::LIST_CHATS));
    assert_eq!(caps.names(), vec!["whoami", "send_message"]);
}

#[test]
fn credential_store_round_trips_and_rejects_escaping_ids() {
    let dir = std::env::temp_dir().join(format!("hm-cred-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = FileCredentialStore::new(ConfigPaths::at(&dir));

    assert_eq!(store.load(ProviderKind::Mock, "alice").unwrap(), None);
    store
        .save(ProviderKind::Mock, "alice", &json!({"token": "secret"}))
        .unwrap();
    assert_eq!(
        store.load(ProviderKind::Mock, "alice").unwrap(),
        Some(json!({"token": "secret"}))
    );
    assert_eq!(
        store.list(ProviderKind::Mock).unwrap(),
        vec!["alice".to_owned()]
    );
    store.delete(ProviderKind::Mock, "alice").unwrap();
    assert_eq!(
        store.list(ProviderKind::Mock).unwrap(),
        Vec::<String>::new()
    );

    let error = store
        .save(ProviderKind::Mock, "../escape", &json!({}))
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::InvalidInput);

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn credential_files_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let dir = std::env::temp_dir().join(format!("hm-perm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = FileCredentialStore::new(ConfigPaths::at(&dir));
    store
        .save(ProviderKind::Mock, "bob", &json!({"token": "x"}))
        .unwrap();

    let file = store
        .paths()
        .accounts_dir(ProviderKind::Mock)
        .join("bob.json");
    let mode = std::fs::metadata(&file).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn envelope_renders_success_and_failure_shapes() {
    use horon_messenger_core::{Envelope, Meta};

    let success = Envelope::success(json!({"value": 1}), Meta::now(Some(ProviderKind::Mock)));
    assert!(success.ok);
    let rendered: serde_json::Value = serde_json::from_str(&success.to_json()).unwrap();
    assert_eq!(rendered["ok"], true);
    assert_eq!(rendered["data"]["value"], 1);
    assert_eq!(rendered["meta"]["provider"], "mock");

    let failure = Envelope::failure(
        &Error::NotRegistered {
            provider: ProviderKind::Discord,
        },
        Meta::now(Some(ProviderKind::Discord)),
    );
    let rendered: serde_json::Value = serde_json::from_str(&failure.to_json()).unwrap();
    assert_eq!(rendered["ok"], false);
    assert_eq!(rendered["error"]["code"], "NOT_REGISTERED");
    assert_eq!(rendered["error"]["retryable"], false);
}

#[test]
fn into_value_matches_serialized_envelope_shape() {
    use horon_messenger_core::{Envelope, Meta};

    let success = Envelope::success(
        json!({"value": 1, "nested": {"a": [1, 2, 3]}}),
        Meta::now(Some(ProviderKind::Mock)).with_warning("partial result"),
    );
    assert_eq!(
        success.clone().into_value(),
        serde_json::to_value(&success).unwrap(),
        "success shape"
    );

    let failure = Envelope::failure(
        &Error::NotRegistered {
            provider: ProviderKind::Discord,
        },
        Meta::now(Some(ProviderKind::Discord)),
    );
    assert_eq!(
        failure.clone().into_value(),
        serde_json::to_value(&failure).unwrap(),
        "failure shape"
    );

    let bare = Envelope::success(json!([1, 2, 3]), Meta::now(None));
    assert_eq!(
        bare.clone().into_value(),
        serde_json::to_value(&bare).unwrap(),
        "success shape without provider"
    );
}

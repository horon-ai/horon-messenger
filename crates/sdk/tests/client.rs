use horon_messenger_sdk::{
    ChatKind, ChatQuery, Client, Delivery, MessageQuery, ProviderKind, SendRequest,
};

#[tokio::test]
async fn default_client_registers_the_mock_provider() {
    let client = Client::builder()
        .config_dir("/tmp/horon-messenger-test-config")
        .build()
        .expect("build client");
    assert!(client.registry().kinds().contains(&ProviderKind::Mock));

    let info = client.describe();
    let mock = info
        .iter()
        .find(|info| info.kind == ProviderKind::Mock)
        .expect("the mock provider is described");
    assert!(mock.capabilities.contains(&"send_message"));
}

#[tokio::test]
async fn whoami_and_list_chats_answer_from_memory() {
    let client = Client::builder()
        .config_dir("/tmp/horon-messenger-test-config")
        .build()
        .expect("build client");

    let profile = client.whoami(ProviderKind::Mock).await.expect("whoami");
    assert_eq!(profile.account_id, "mock-user");

    let chats = client
        .list_chats(ProviderKind::Mock, &ChatQuery::default())
        .await
        .expect("list chats");
    assert_eq!(chats.len(), 3);
    assert!(chats.iter().any(|chat| chat.kind == ChatKind::Direct));
}

#[tokio::test]
async fn chat_search_filters_by_name() {
    let client = Client::builder()
        .config_dir("/tmp/horon-messenger-test-config")
        .build()
        .expect("build client");

    let query = ChatQuery {
        search: Some("standup".to_owned()),
        limit: None,
    };
    let chats = client
        .list_chats(ProviderKind::Mock, &query)
        .await
        .expect("chats");
    assert_eq!(chats.len(), 1);
    assert_eq!(chats[0].id, "team");
}

#[tokio::test]
async fn send_message_is_visible_on_the_next_read() {
    let client = Client::builder()
        .config_dir("/tmp/horon-messenger-test-config")
        .build()
        .expect("build client");

    let request = SendRequest {
        chat_id: "general".to_owned(),
        text: "hello from the sdk test".to_owned(),
        reply_to: None,
        attachments: Vec::new(),
        idempotency_key: None,
    };
    let result = client
        .send_message(ProviderKind::Mock, &request)
        .await
        .expect("send");
    assert!(result.accepted);
    assert_eq!(result.delivery, Delivery::Confirmed);
    assert!(result.message_id.is_some());

    let page = client
        .list_messages(ProviderKind::Mock, "general", &MessageQuery::default())
        .await
        .expect("messages");
    assert!(
        page.messages
            .iter()
            .any(|message| message.text == "hello from the sdk test")
    );
}

#[test]
fn an_empty_registry_reports_not_registered() {
    use horon_messenger_sdk::{ErrorCode, ProviderRegistry};

    let registry = ProviderRegistry::new();
    match registry.require(ProviderKind::Discord) {
        Ok(_) => panic!("an empty registry must not resolve discord"),
        Err(error) => {
            assert_eq!(error.code(), ErrorCode::NotRegistered);
            assert_eq!(error.provider(), Some(ProviderKind::Discord));
        }
    }
}

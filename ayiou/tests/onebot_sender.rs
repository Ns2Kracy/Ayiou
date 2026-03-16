use ayiou::adapter::onebot::v11::model::{Message, MessageSegment};
use ayiou::adapter::onebot::v11::sender::OneBotSender;
use ayiou::core::plugin_host::MessageSender;

#[tokio::test]
async fn onebot_sender_serializes_group_message_without_ctx() {
    let (sender, mut rx) = OneBotSender::test_pair();

    sender.send_group_text(42, "hello").await.unwrap();

    let raw = rx.recv().await.unwrap();
    assert!(raw.contains("send_group_msg"));
    assert!(raw.contains("\"group_id\":42"));
    assert!(raw.contains("hello"));
}

#[tokio::test]
async fn onebot_sender_serializes_group_image_message_without_ctx() {
    let (sender, mut rx) = OneBotSender::test_pair();

    sender
        .send_group_message(
            42,
            Message::Array(vec![MessageSegment::Image {
                file: "https://example.com/cover.jpg".to_string(),
                image_type: None,
                url: None,
            }]),
        )
        .await
        .unwrap();

    let raw = rx.recv().await.unwrap();
    assert!(raw.contains("send_group_msg"));
    assert!(raw.contains("\"group_id\":42"));
    assert!(raw.contains("\"type\":\"image\""));
    assert!(raw.contains("https://example.com/cover.jpg"));
}

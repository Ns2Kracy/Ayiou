use ayiou::adapter::onebot::v11::sender::OneBotSender;
use ayiou::core::model::{ChannelRef, MessageSegment, OutboundMessage};
use ayiou::core::plugin::OutboundSender;

#[tokio::test]
async fn onebot_sender_serializes_group_message_without_ctx() {
    let (sender, mut rx) = OneBotSender::test_pair();

    sender
        .send(OutboundMessage::text(
            ChannelRef::group("onebot/v11", "42"),
            "hello",
        ))
        .await
        .unwrap();

    let raw = rx.recv().await.unwrap();
    assert!(raw.contains("send_group_msg"));
    assert!(raw.contains("\"group_id\":42"));
    assert!(raw.contains("hello"));
}

#[tokio::test]
async fn onebot_sender_serializes_group_image_message_without_ctx() {
    let (sender, mut rx) = OneBotSender::test_pair();

    sender
        .send(OutboundMessage::new(
            ChannelRef::group("onebot/v11", "42"),
            vec![MessageSegment::Image {
                url: "https://example.com/cover.jpg".to_string(),
            }],
        ))
        .await
        .unwrap();

    let raw = rx.recv().await.unwrap();
    assert!(raw.contains("send_group_msg"));
    assert!(raw.contains("\"group_id\":42"));
    assert!(raw.contains("\"type\":\"image\""));
    assert!(raw.contains("https://example.com/cover.jpg"));
}

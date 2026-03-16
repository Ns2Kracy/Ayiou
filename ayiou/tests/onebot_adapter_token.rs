use ayiou::adapter::onebot::v11::adapter::OneBotV11Adapter;

#[test]
fn onebot_adapter_accepts_token_constructor() {
    let _adapter = OneBotV11Adapter::with_token("ws://127.0.0.1:3001", "token");
}

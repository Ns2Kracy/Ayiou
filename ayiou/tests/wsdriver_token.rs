use ayiou::driver::wsclient::WsDriver;

#[test]
fn wsdriver_appends_access_token_to_query() {
    let driver = WsDriver::with_access_token("ws://127.0.0.1:3001", "secret-token");
    assert_eq!(
        driver.url().as_str(),
        "ws://127.0.0.1:3001/?access_token=secret-token"
    );
}

#[test]
fn wsdriver_does_not_duplicate_existing_access_token_query() {
    let driver = WsDriver::with_access_token(
        "ws://127.0.0.1:3001/?access_token=already-there",
        "secret-token",
    );
    assert_eq!(
        driver.url().as_str(),
        "ws://127.0.0.1:3001/?access_token=already-there"
    );
}

#[test]
fn wsdriver_redacts_access_token_in_display_url() {
    let driver = WsDriver::with_access_token("ws://127.0.0.1:3001", "secret-token");
    assert_eq!(
        driver.redacted_url(),
        "ws://127.0.0.1:3001/?access_token=***"
    );
}

use ayiou_plugin_bilibili_live::command::CommandAction;

#[test]
fn parses_sub_command() {
    let cmd = CommandAction::parse("sub 123").unwrap();
    assert_eq!(cmd, CommandAction::Sub { uid: 123 });
}

#[test]
fn rejects_non_numeric_uid() {
    let err = CommandAction::parse("sub abc").unwrap_err();
    assert!(err.to_string().contains("uid"));
}

#[test]
fn parses_list_command() {
    let cmd = CommandAction::parse("list").unwrap();
    assert_eq!(cmd, CommandAction::List);
}

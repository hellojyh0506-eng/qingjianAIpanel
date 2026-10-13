//! 协议向前 / 向后兼容：新增的可选字段不能让旧 DLL 与旧 Server 互相听不懂。

use qingjian_platform::protocol::{ClientMessage, SessionId};
use serde_json::json;

#[test]
fn surrounding_from_an_old_dll_has_no_after_field() {
    let old = json!({"Surrounding": {"session": 3, "text": "今天天气"}});
    let message: ClientMessage = serde_json::from_value(old).expect("old DLL message parses");
    assert_eq!(
        message,
        ClientMessage::Surrounding {
            session: SessionId(3),
            text: "今天天气".to_owned(),
            after: String::new(),
        }
    );
}

#[test]
fn surrounding_carries_after_and_round_trips() {
    let message = ClientMessage::Surrounding {
        session: SessionId(3),
        text: "我是一名".to_owned(),
        after: "在公司上班".to_owned(),
    };
    let wire = serde_json::to_value(&message).expect("serialize");
    assert_eq!(wire["Surrounding"]["after"], "在公司上班");
    let back: ClientMessage = serde_json::from_value(wire).expect("deserialize");
    assert_eq!(back, message);
}

#[test]
fn an_old_server_ignores_the_new_field() {
    // 旧 Server 的 Surrounding 只有 session / text 两个字段；serde 默认忽略多出来的字段。
    #[derive(serde::Deserialize, Debug, PartialEq)]
    enum Old {
        Surrounding { session: u64, text: String },
    }
    let new = json!({"Surrounding": {"session": 3, "text": "前文", "after": "后文"}});
    let parsed: Old = serde_json::from_value(new).expect("old server parses new message");
    assert_eq!(
        parsed,
        Old::Surrounding {
            session: 3,
            text: "前文".to_owned()
        }
    );
}

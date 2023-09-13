use near_sdk::{serde::Serialize, AccountId};
use serde_json::json;

use sbt::{EventPayload, NearEvent};

use crate::PollId;

fn emit_event<T: Serialize>(event: EventPayload<T>) {
    NearEvent {
        standard: "ndc-easy-poll",
        version: "1.0.0",
        event,
    }
    .emit();
}

pub(crate) fn emit_create_poll(poll_id: PollId) {
    emit_event(EventPayload {
        event: "create_poll",
        data: json!({ "poll_id": poll_id }),
    });
}

pub(crate) fn emit_respond(poll_id: PollId, responder: AccountId) {
    emit_event(EventPayload {
        event: "respond",
        data: json!({ "poll_id": poll_id, "responder": responder }),
    });
}

#[cfg(test)]
mod unit_tests {
    use near_sdk::{test_utils, AccountId};

    use super::*;

    fn acc(idx: u8) -> AccountId {
        AccountId::new_unchecked(format!("user-{}.near", idx))
    }

    #[test]
    fn log_vote() {
        let expected1 = r#"EVENT_JSON:{"standard":"ndc-easy-poll","version":"1.0.0","event":"create_poll","data":{"poll_id":21}}"#;
        let expected2 = r#"EVENT_JSON:{"standard":"ndc-easy-poll","version":"1.0.0","event":"respond","data":{"poll_id":22,"responder":"user-1.near"}}"#;
        emit_create_poll(21);
        assert_eq!(vec![expected1], test_utils::get_logs());
        emit_respond(22, acc(1));
        assert_eq!(vec![expected1, expected2], test_utils::get_logs());
    }
}

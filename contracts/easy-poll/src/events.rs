use near_sdk::serde::Serialize;
use serde_json::json;

use sbt::{EventPayload, NearEvent};

use crate::PollId;

fn emit_event<T: Serialize>(event: EventPayload<T>) {
    NearEvent {
        standard: "ndc-easy-polls",
        version: "0.0.1",
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

pub(crate) fn emit_respond(poll_id: PollId) {
    emit_event(EventPayload {
        event: "respond",
        data: json!({ "poll_id": poll_id }),
    });
}

#[cfg(test)]
mod unit_tests {
    use near_sdk::test_utils;

    use super::*;

    #[test]
    fn log_vote() {
        let expected1 = r#"EVENT_JSON:{"standard":"ndc-easy-polls","version":"0.0.1","event":"create_poll","data":{"poll_id":21}}"#;
        let expected2 = r#"EVENT_JSON:{"standard":"ndc-easy-polls","version":"0.0.1","event":"respond","data":{"poll_id":22}}"#;
        emit_create_poll(21);
        assert_eq!(vec![expected1], test_utils::get_logs());
        emit_respond(22);
        assert_eq!(vec![expected1, expected2], test_utils::get_logs());
    }
}

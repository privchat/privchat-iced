use std::sync::atomic::{AtomicU64, Ordering};

use privchat_sdk::SdkEvent;

use crate::app::message::AppMessage;
use crate::presentation::vm::{
    ClientTxnId, MessageSendStateVm, OpenToken, TimelinePatchVm, TimelineRevision, UiError,
};

#[derive(Debug, Clone, Default, Hash)]
pub struct EventMapContext {
    pub channel_id: u64,
    pub channel_type: i32,
    pub open_token: OpenToken,
    /// Sorted `(message_id, client_txn_id)` pairs from UI runtime index snapshot.
    pub message_id_to_client_txn: Vec<(u64, ClientTxnId)>,
}

impl EventMapContext {
    pub fn client_txn_id_for_message(&self, message_id: u64) -> Option<ClientTxnId> {
        self.message_id_to_client_txn
            .binary_search_by_key(&message_id, |(id, _)| *id)
            .ok()
            .map(|idx| self.message_id_to_client_txn[idx].1)
    }
}

static PATCH_REVISION_SEQ: AtomicU64 = AtomicU64::new(1);

pub fn allocate_patch_revision() -> TimelineRevision {
    PATCH_REVISION_SEQ.fetch_add(1, Ordering::Relaxed)
}

fn map_send_status(status: i32) -> MessageSendStateVm {
    match status {
        0 => MessageSendStateVm::Queued,
        1 => MessageSendStateVm::Sending,
        2 => MessageSendStateVm::Sent,
        3 => MessageSendStateVm::FailedRetryable {
            reason: UiError::Unknown("send failed".to_string()),
        },
        other => MessageSendStateVm::FailedPermanent {
            reason: UiError::Unknown(format!("unknown send status: {other}")),
        },
    }
}

pub fn map_sdk_event_with_context((context, event): (EventMapContext, SdkEvent)) -> AppMessage {
    map_sdk_event_to_app_message(event, &context).unwrap_or(AppMessage::Noop)
}

/// SDK event ingress mapping.
/// This layer only translates payload shape. It does not mutate state or invent business rules.
pub fn map_sdk_event_to_app_message(
    event: SdkEvent,
    context: &EventMapContext,
) -> Option<AppMessage> {
    match event {
        SdkEvent::TimelineUpdated {
            channel_id,
            channel_type,
            message_id,
            reason: _,
        } => {
            if channel_id != context.channel_id || channel_type != context.channel_type {
                return None;
            }
            Some(AppMessage::TimelineUpdatedIngress {
                channel_id,
                channel_type,
                open_token: context.open_token,
                message_id,
            })
        }
        SdkEvent::MessageSendStatusChanged {
            message_id,
            status,
            server_message_id: _,
        } => context
            .client_txn_id_for_message(message_id)
            .map(|client_txn_id| AppMessage::TimelinePatched {
                channel_id: context.channel_id,
                channel_type: context.channel_type,
                open_token: context.open_token,
                revision: allocate_patch_revision(),
                patch: TimelinePatchVm::UpdateSendState {
                    client_txn_id,
                    send_state: map_send_status(status),
                },
            }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_context() -> EventMapContext {
        EventMapContext {
            channel_id: 100,
            channel_type: 2,
            open_token: 7,
            message_id_to_client_txn: vec![(11, 2001)],
        }
    }

    #[test]
    fn timeline_updated_maps_when_active_channel_matches() {
        let context = base_context();
        let mapped = map_sdk_event_to_app_message(
            SdkEvent::TimelineUpdated {
                channel_id: 100,
                channel_type: 2,
                message_id: 11,
                reason: "sync".to_string(),
            },
            &context,
        );

        assert!(matches!(
            mapped,
            Some(AppMessage::TimelineUpdatedIngress {
                channel_id: 100,
                channel_type: 2,
                open_token: 7,
                message_id: 11
            })
        ));
    }

    #[test]
    fn timeline_updated_is_dropped_when_channel_mismatches() {
        let context = base_context();
        let mapped = map_sdk_event_to_app_message(
            SdkEvent::TimelineUpdated {
                channel_id: 999,
                channel_type: 2,
                message_id: 11,
                reason: "sync".to_string(),
            },
            &context,
        );

        assert!(mapped.is_none());
    }

    #[test]
    fn send_status_maps_through_runtime_index_only() {
        let context = base_context();
        let mapped = map_sdk_event_to_app_message(
            SdkEvent::MessageSendStatusChanged {
                message_id: 11,
                status: 2,
                server_message_id: Some(777),
            },
            &context,
        );
        assert!(matches!(
            mapped,
            Some(AppMessage::TimelinePatched {
                channel_id: 100,
                channel_type: 2,
                open_token: 7,
                patch: TimelinePatchVm::UpdateSendState {
                    client_txn_id: 2001,
                    send_state: MessageSendStateVm::Sent
                },
                ..
            })
        ));

        let unmapped = map_sdk_event_to_app_message(
            SdkEvent::MessageSendStatusChanged {
                message_id: 999,
                status: 2,
                server_message_id: Some(777),
            },
            &context,
        );
        assert!(unmapped.is_none());
    }
}

use std::sync::atomic::{AtomicU64, Ordering};

use privchat_sdk::SdkEvent;
use tracing::{info, warn};

use crate::app::message::AppMessage;
use crate::presentation::vm::{ClientTxnId, OpenToken, TimelineRevision};

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

fn is_channel_entity(entity_type: &str) -> bool {
    matches!(
        entity_type,
        "channel" | "channel_extra" | "channel_unread" | "channel_read_cursor"
    )
}

fn is_message_entity(entity_type: &str) -> bool {
    matches!(
        entity_type,
        "message"
            | "message_send"
            | "message_read"
            | "message_reaction"
            | "message_revoke"
            | "message_status"
    )
}

fn is_contact_entity(entity_type: &str) -> bool {
    matches!(
        entity_type,
        "friend"
            | "friends"
            | "friend_request"
            | "friend_requests"
            | "friend_pending"
            | "group"
            | "groups"
            | "group_member"
            | "group_members"
            | "user"
            | "users"
    )
}

fn ingress_from_context(context: &EventMapContext, message_id: u64) -> AppMessage {
    AppMessage::TimelineUpdatedIngress {
        channel_id: context.channel_id,
        channel_type: context.channel_type,
        open_token: context.open_token,
        message_id,
    }
}

pub fn map_sdk_event_with_context((context, event): (EventMapContext, SdkEvent)) -> AppMessage {
    map_sdk_event(event, Some(&context))
}

pub fn map_sdk_event_without_context(event: SdkEvent) -> AppMessage {
    map_sdk_event(event, None)
}

/// Map SDK events into app messages with optional active-chat context.
/// - Active chat timeline updates are routed to timeline ingress.
/// - Non-active updates trigger session list refresh.
pub fn map_sdk_event(event: SdkEvent, context: Option<&EventMapContext>) -> AppMessage {
    match event {
        SdkEvent::ConnectionStateChanged { from, to } => {
            info!("sdk_event: connection_state_changed {:?} -> {:?}", from, to);
            AppMessage::Noop
        }
        SdkEvent::NetworkHintChanged { from, to } => {
            info!("sdk_event: network_hint_changed {:?} -> {:?}", from, to);
            AppMessage::Noop
        }
        SdkEvent::ResumeSyncStarted => {
            info!("sdk_event: resume_sync_started");
            AppMessage::Noop
        }
        SdkEvent::ResumeSyncCompleted {
            entity_types_synced,
            channels_scanned,
            channels_applied,
            channel_failures,
        } => {
            info!(
                "sdk_event: resume_sync_completed entity_types_synced={} channels_scanned={} channels_applied={} channel_failures={}",
                entity_types_synced, channels_scanned, channels_applied, channel_failures
            );
            AppMessage::RefreshSessionList
        }
        SdkEvent::ResumeSyncFailed {
            classification,
            scope,
            error_code,
            message,
        } => {
            warn!(
                "sdk_event: resume_sync_failed classification={:?} scope={:?} code={} message={}",
                classification, scope, error_code, message
            );
            AppMessage::RefreshSessionList
        }
        SdkEvent::ResumeSyncEscalated {
            classification,
            scope,
            reason,
            entity_type,
            channel_id,
            channel_type,
        } => {
            warn!(
                "sdk_event: resume_sync_escalated classification={:?} scope={:?} reason={} entity_type={:?} channel_id={:?} channel_type={:?}",
                classification, scope, reason, entity_type, channel_id, channel_type
            );
            AppMessage::RefreshSessionList
        }
        SdkEvent::ResumeSyncChannelStarted {
            channel_id,
            channel_type,
        } => {
            info!(
                "sdk_event: resume_sync_channel_started channel_id={} channel_type={}",
                channel_id, channel_type
            );
            AppMessage::Noop
        }
        SdkEvent::ResumeSyncChannelCompleted {
            channel_id,
            channel_type,
            applied,
        } => {
            info!(
                "sdk_event: resume_sync_channel_completed channel_id={} channel_type={} applied={}",
                channel_id, channel_type, applied
            );
            AppMessage::RefreshSessionList
        }
        SdkEvent::ResumeSyncChannelFailed {
            channel_id,
            channel_type,
            classification,
            scope,
            error_code,
            message,
        } => {
            warn!(
                "sdk_event: resume_sync_channel_failed channel_id={} channel_type={} classification={:?} scope={:?} code={} message={}",
                channel_id, channel_type, classification, scope, error_code, message
            );
            AppMessage::RefreshSessionList
        }
        SdkEvent::BootstrapCompleted { user_id } => {
            info!("sdk_event: bootstrap_completed user_id={user_id}");
            AppMessage::RefreshSessionList
        }
        SdkEvent::SyncAllChannelsApplied { applied } => {
            info!("sdk_event: sync_all_channels_applied applied={applied}");
            AppMessage::RefreshSessionList
        }
        SdkEvent::SyncChannelApplied {
            channel_id,
            channel_type,
            applied,
        } => {
            info!(
                "sdk_event: sync_channel_applied channel_id={} channel_type={} applied={}",
                channel_id, channel_type, applied
            );
            if applied > 0 {
                AppMessage::RefreshSessionList
            } else {
                AppMessage::Noop
            }
        }
        SdkEvent::SyncEntitiesApplied {
            entity_type,
            scope,
            queued,
            applied,
            dropped_duplicates,
        } => {
            let entity = entity_type.to_ascii_lowercase();
            info!(
                "sdk_event: sync_entities_applied entity_type={} scope={:?} queued={} applied={} dropped_duplicates={}",
                entity, scope, queued, applied, dropped_duplicates
            );
            if applied == 0 {
                return AppMessage::Noop;
            }
            if is_channel_entity(&entity) || is_message_entity(&entity) {
                AppMessage::RefreshSessionList
            } else if is_contact_entity(&entity) {
                AppMessage::RefreshAddFriendData
            } else {
                AppMessage::Noop
            }
        }
        SdkEvent::SyncEntityChanged {
            entity_type,
            entity_id,
            deleted,
        } => {
            let entity = entity_type.to_ascii_lowercase();
            info!(
                "sdk_event: sync_entity_changed entity_type={} entity_id={} deleted={}",
                entity, entity_id, deleted
            );
            if is_channel_entity(&entity) || is_message_entity(&entity) {
                AppMessage::RefreshSessionList
            } else if is_contact_entity(&entity) {
                AppMessage::RefreshAddFriendData
            } else {
                AppMessage::Noop
            }
        }
        SdkEvent::TimelineUpdated {
            channel_id,
            channel_type,
            message_id,
            reason,
        } => {
            if let Some(context) = context {
                if channel_id == context.channel_id && channel_type == context.channel_type {
                    info!(
                        "sdk_event: timeline_updated active channel_id={} channel_type={} message_id={} reason={}",
                        channel_id, channel_type, message_id, reason
                    );
                    return ingress_from_context(context, message_id);
                }
            }
            info!(
                "sdk_event: timeline_updated background channel_id={} channel_type={} message_id={} reason={}",
                channel_id, channel_type, message_id, reason
            );
            AppMessage::RefreshSessionList
        }
        SdkEvent::MessageSendStatusChanged {
            message_id,
            status,
            server_message_id,
        } => {
            info!(
                "sdk_event: message_send_status_changed message_id={} status={} server_message_id={:?}",
                message_id, status, server_message_id
            );
            match context {
                Some(context) => ingress_from_context(context, message_id),
                None => AppMessage::RefreshSessionList,
            }
        }
        SdkEvent::OutboundQueueUpdated {
            kind,
            action,
            message_id,
            queue_index,
        } => {
            info!(
                "sdk_event: outbound_queue_updated kind={} action={} message_id={:?} queue_index={:?}",
                kind, action, message_id, queue_index
            );
            match (context, message_id) {
                (Some(context), Some(message_id)) => ingress_from_context(context, message_id),
                (None, Some(_)) => AppMessage::RefreshSessionList,
                (_, None) => AppMessage::Noop,
            }
        }
        _ => AppMessage::Noop,
    }
}

/// SDK event ingress mapping.
/// This layer only translates payload shape. It does not mutate state or invent business rules.
pub fn map_sdk_event_to_app_message(
    event: SdkEvent,
    context: &EventMapContext,
) -> Option<AppMessage> {
    let mapped = map_sdk_event(event, Some(context));
    if matches!(
        mapped,
        AppMessage::Noop | AppMessage::RefreshSessionList | AppMessage::RefreshTotalUnreadCount
    ) {
        None
    } else {
        Some(mapped)
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
    fn send_status_always_maps_to_ingress_for_active_context() {
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
            Some(AppMessage::TimelineUpdatedIngress {
                channel_id: 100,
                channel_type: 2,
                open_token: 7,
                message_id: 11,
            })
        ));

        let fallback = map_sdk_event_to_app_message(
            SdkEvent::MessageSendStatusChanged {
                message_id: 999,
                status: 2,
                server_message_id: Some(777),
            },
            &context,
        );
        assert!(matches!(
            fallback,
            Some(AppMessage::TimelineUpdatedIngress {
                channel_id: 100,
                channel_type: 2,
                open_token: 7,
                message_id: 999,
            })
        ));
    }
}

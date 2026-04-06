use std::sync::atomic::{AtomicU64, Ordering};

use privchat_sdk::SdkEvent;
use tracing::{info, warn};

use crate::app::message::{AppMessage, MessageIngressSource};
use crate::app::reporting;
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

fn sdk_event_type(event: &SdkEvent) -> &'static str {
    match event {
        SdkEvent::ConnectionStateChanged { .. } => "connection_state_changed",
        SdkEvent::BootstrapCompleted { .. } => "bootstrap_completed",
        SdkEvent::ResumeSyncStarted => "resume_sync_started",
        SdkEvent::ResumeSyncCompleted { .. } => "resume_sync_completed",
        SdkEvent::ResumeSyncFailed { .. } => "resume_sync_failed",
        SdkEvent::ResumeSyncEscalated { .. } => "resume_sync_escalated",
        SdkEvent::ResumeSyncChannelStarted { .. } => "resume_sync_channel_started",
        SdkEvent::ResumeSyncChannelCompleted { .. } => "resume_sync_channel_completed",
        SdkEvent::ResumeSyncChannelFailed { .. } => "resume_sync_channel_failed",
        SdkEvent::SyncEntitiesApplied { .. } => "sync_entities_applied",
        SdkEvent::SyncEntityChanged { .. } => "sync_entity_changed",
        SdkEvent::SyncChannelApplied { .. } => "sync_channel_applied",
        SdkEvent::SyncAllChannelsApplied { .. } => "sync_all_channels_applied",
        SdkEvent::NetworkHintChanged { .. } => "network_hint_changed",
        SdkEvent::OutboundQueueUpdated { .. } => "outbound_queue_updated",
        SdkEvent::TimelineUpdated { .. } => "timeline_updated",
        SdkEvent::MessageSendStatusChanged { .. } => "message_send_status_changed",
        SdkEvent::TypingSent { .. } => "typing_sent",
        SdkEvent::SubscriptionMessageReceived { .. } => "subscription_message_received",
        SdkEvent::ShutdownStarted => "shutdown_started",
        SdkEvent::ShutdownCompleted => "shutdown_completed",
    }
}

pub fn map_sdk_event_with_context((context, event): (EventMapContext, SdkEvent)) -> AppMessage {
    map_sdk_event(event, Some(&context))
}

pub fn map_sdk_event_without_context(event: SdkEvent) -> AppMessage {
    map_sdk_event(event, None)
}

/// Map SDK events into app messages.
/// Message-related events are normalized into global ingress so the app can
/// resolve message payloads and refresh UI from a single path (like privchat-app).
pub fn map_sdk_event(event: SdkEvent, _context: Option<&EventMapContext>) -> AppMessage {
    reporting::report_sdk_event(sdk_event_type(&event));
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
            AppMessage::RepairChannelSyncRequested {
                channel_id,
                channel_type,
            }
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
            info!(
                "sdk_event: timeline_updated channel_id={} channel_type={} message_id={} reason={}",
                channel_id, channel_type, message_id, reason
            );
            AppMessage::GlobalMessageIngress {
                message_id,
                channel_id: Some(channel_id),
                channel_type: Some(channel_type),
                source: MessageIngressSource::TimelineUpdated,
            }
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
            AppMessage::GlobalMessageIngress {
                message_id,
                channel_id: None,
                channel_type: None,
                source: MessageIngressSource::MessageSendStatusChanged,
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
            match message_id {
                Some(message_id) => AppMessage::GlobalMessageIngress {
                    message_id,
                    channel_id: None,
                    channel_type: None,
                    source: MessageIngressSource::OutboundQueueUpdated,
                },
                None => AppMessage::Noop,
            }
        }
        SdkEvent::SubscriptionMessageReceived {
            channel_id,
            server_message_id,
            ..
        } => match server_message_id {
            Some(message_id) => AppMessage::GlobalMessageIngress {
                message_id,
                channel_id: Some(channel_id),
                channel_type: None,
                source: MessageIngressSource::SubscriptionMessageReceived,
            },
            None => AppMessage::Noop,
        },
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
    fn timeline_updated_maps_to_global_ingress() {
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
            Some(AppMessage::GlobalMessageIngress {
                message_id: 11,
                channel_id: Some(100),
                channel_type: Some(2),
                source: MessageIngressSource::TimelineUpdated,
            })
        ));
    }

    #[test]
    fn timeline_updated_keeps_background_channel() {
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

        assert!(matches!(
            mapped,
            Some(AppMessage::GlobalMessageIngress {
                message_id: 11,
                channel_id: Some(999),
                channel_type: Some(2),
                source: MessageIngressSource::TimelineUpdated,
            })
        ));
    }

    #[test]
    fn send_status_maps_to_global_ingress() {
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
            Some(AppMessage::GlobalMessageIngress {
                message_id: 11,
                channel_id: None,
                channel_type: None,
                source: MessageIngressSource::MessageSendStatusChanged,
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
            Some(AppMessage::GlobalMessageIngress {
                message_id: 999,
                channel_id: None,
                channel_type: None,
                source: MessageIngressSource::MessageSendStatusChanged,
            })
        ));
    }
}

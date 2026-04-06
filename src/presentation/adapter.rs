use privchat_sdk::{StoredChannel, StoredChannelExtra, StoredMessage, TimelineSnapshot};

use crate::presentation::vm::{
    ClientTxnId, HistoryPageVm, MessageSendStateVm, MessageVm, SessionListItemVm, TimelineItemKey,
    TimelineRevision, TimelineSnapshotVm, UiError, UnreadMarkerVm,
};

fn extract_body(content: &str) -> String {
    let parsed = serde_json::from_str::<serde_json::Value>(content);
    let Ok(value) = parsed else {
        return content.to_string();
    };
    if let Some(text) = value
        .get("content")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("text").and_then(|v| v.as_str()))
        .or_else(|| value.get("body").and_then(|v| v.as_str()))
        .or_else(|| {
            value
                .get("content")
                .and_then(|v| v.get("text"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| {
            value
                .get("content")
                .and_then(|v| v.get("body"))
                .and_then(|v| v.as_str())
        })
    {
        return text.to_string();
    }
    content.to_string()
}

fn extract_pts(extra: &str) -> Option<u64> {
    // Only recognise the canonical "pts" field.
    // Do NOT fall back to "version" — semantics unconfirmed; a wrong pts would
    // silently pollute mark_read progression.
    let parsed = serde_json::from_str::<serde_json::Value>(extra).ok()?;
    parsed.get("pts").and_then(|v| v.as_u64())
}

fn channel_display_title(channel: &StoredChannel) -> String {
    if !channel.channel_remark.trim().is_empty() {
        return channel.channel_remark.trim().to_string();
    }
    if !channel.channel_name.trim().is_empty() {
        return channel.channel_name.trim().to_string();
    }
    format!("Channel {}", channel.channel_id)
}

pub fn map_channel_to_session_item(channel: &StoredChannel) -> SessionListItemVm {
    SessionListItemVm {
        channel_id: channel.channel_id,
        channel_type: channel.channel_type,
        title: channel_display_title(channel),
        subtitle: extract_body(&channel.last_msg_content),
        unread_count: channel.unread_count.max(0) as u32,
        last_msg_timestamp: channel.last_msg_timestamp,
    }
}

pub fn map_send_status(status: i32, is_own: bool) -> Option<MessageSendStateVm> {
    if !is_own {
        return None;
    }

    let mapped = match status {
        0 => MessageSendStateVm::Queued,
        1 => MessageSendStateVm::Sending,
        2 => MessageSendStateVm::Sent,
        3 => MessageSendStateVm::FailedRetryable {
            reason: UiError::Unknown("send failed".to_string()),
        },
        // Keep UI stable when backend introduces finer-grained delivered/read statuses.
        other if other >= 4 => MessageSendStateVm::Sent,
        other => MessageSendStateVm::FailedPermanent {
            reason: UiError::Unknown(format!("unknown send status: {other}")),
        },
    };
    Some(mapped)
}

pub fn map_stored_message_to_vm(
    message: &StoredMessage,
    current_uid: Option<u64>,
    client_txn_id: Option<ClientTxnId>,
) -> MessageVm {
    let is_own = current_uid
        .map(|uid| uid == message.from_uid)
        .unwrap_or(false);

    let resolved_client_txn_id = client_txn_id.or(message.local_message_id);
    let key = match message.server_message_id {
        Some(server_message_id) => TimelineItemKey::Remote { server_message_id },
        None => TimelineItemKey::Local(resolved_client_txn_id.unwrap_or(message.message_id)),
    };

    MessageVm {
        key,
        channel_id: message.channel_id,
        channel_type: message.channel_type,
        message_id: message.message_id,
        server_message_id: message.server_message_id,
        client_txn_id: resolved_client_txn_id,
        from_uid: message.from_uid,
        body: extract_body(&message.content),
        message_type: message.message_type,
        created_at: message.created_at,
        pts: extract_pts(&message.extra),
        send_state: map_send_status(message.status, is_own),
        is_own,
        is_deleted: false,
    }
}

fn derive_oldest_server_message_id(messages: &[MessageVm]) -> Option<u64> {
    messages.iter().filter_map(|m| m.server_message_id).min()
}

pub fn map_unread_marker(
    channel: Option<&StoredChannel>,
    extra: Option<&StoredChannelExtra>,
) -> UnreadMarkerVm {
    let unread_count = channel
        .map(|c| c.unread_count.max(0) as u32)
        .unwrap_or_default();
    let first_unread_key = extra.and_then(|e| {
        if e.browse_to > 0 {
            Some(TimelineItemKey::Remote {
                server_message_id: e.browse_to,
            })
        } else {
            None
        }
    });

    UnreadMarkerVm {
        first_unread_key,
        unread_count,
        has_unread_below_viewport: unread_count > 0,
    }
}

pub fn map_snapshot_to_vm(
    snapshot: &TimelineSnapshot,
    current_uid: Option<u64>,
    revision: TimelineRevision,
    unread_marker: UnreadMarkerVm,
) -> TimelineSnapshotVm {
    let mut items: Vec<MessageVm> = snapshot
        .messages
        .iter()
        .map(|m| map_stored_message_to_vm(m, current_uid, None))
        .collect();
    items.sort_by_key(|item| (item.created_at, item.message_id));

    let oldest_server_message_id = derive_oldest_server_message_id(&items);

    TimelineSnapshotVm {
        revision,
        items,
        oldest_server_message_id,
        has_more_before: snapshot.has_more_before,
        unread_marker,
    }
}

pub fn map_history_messages_to_vm(
    messages: &[StoredMessage],
    current_uid: Option<u64>,
    has_more_before: bool,
) -> HistoryPageVm {
    let mut items: Vec<MessageVm> = messages
        .iter()
        .map(|m| map_stored_message_to_vm(m, current_uid, None))
        .collect();
    items.sort_by_key(|item| (item.created_at, item.message_id));

    let oldest_server_message_id = derive_oldest_server_message_id(&items);

    HistoryPageVm {
        items,
        oldest_server_message_id,
        has_more_before,
    }
}

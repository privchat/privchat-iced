use privchat_protocol::message::{
    ContentMessageType, FileMetadata, ImageMetadata, MessagePayloadEnvelope,
};
use privchat_sdk::{StoredChannel, StoredChannelExtra, StoredMessage, TimelineSnapshot};
use std::path::{Path, PathBuf};

use crate::presentation::vm::{
    ClientTxnId, HistoryPageVm, MessageSendStateVm, MessageVm, SessionListItemVm, TimelineItemKey,
    TimelineRevision, TimelineSnapshotVm, UiError, UnreadMarkerVm,
};

const IMAGE_MESSAGE_TYPE: i32 = ContentMessageType::Image as i32;
const FILE_MESSAGE_TYPE: i32 = ContentMessageType::File as i32;
const VIDEO_MESSAGE_TYPE: i32 = ContentMessageType::Video as i32;
const VOICE_MESSAGE_TYPE: i32 = ContentMessageType::Voice as i32;

/// 语音消息本地存储约定：`{message_id}.m4a`（仅 privchat-iced 下载侧使用）。
pub(crate) fn voice_local_filename(message_id: u64) -> String {
    format!("{}.m4a", message_id)
}

/// 仅语音消息（录音气泡），不包含 Audio（音频文件，走 File 文件气泡）。
fn is_voice(message_type: i32) -> bool {
    message_type == VOICE_MESSAGE_TYPE
}

fn looks_like_media_metadata(value: &serde_json::Value) -> bool {
    value
        .as_object()
        .map(|obj| {
            obj.contains_key("file_id")
                || obj.contains_key("thumbnail_file_id")
                || obj.contains_key("file_url")
                || obj.contains_key("thumbnail_url")
                || obj.contains_key("url")
        })
        .unwrap_or(false)
}

fn coerce_media_metadata(value: &serde_json::Value) -> Option<serde_json::Value> {
    if looks_like_media_metadata(value) {
        return Some(value.clone());
    }
    // Some historical rows persist metadata as a JSON string.
    if let Some(as_text) = value.as_str() {
        let parsed = serde_json::from_str::<serde_json::Value>(as_text).ok()?;
        if looks_like_media_metadata(&parsed) {
            return Some(parsed);
        }
    }
    None
}

fn parse_payload_envelope(content: &str) -> Option<MessagePayloadEnvelope> {
    let parsed = serde_json::from_str::<MessagePayloadEnvelope>(content).ok()?;
    let looks_like_envelope = parsed.metadata.is_some()
        || parsed.reply_to_message_id.is_some()
        || parsed.mentioned_user_ids.is_some()
        || parsed.message_source.is_some()
        || !parsed.content.trim().is_empty();
    if looks_like_envelope {
        Some(parsed)
    } else {
        None
    }
}

fn extract_media_metadata(
    content_json: Option<&serde_json::Value>,
    extra_json: Option<&serde_json::Value>,
    envelope: Option<&MessagePayloadEnvelope>,
) -> Option<serde_json::Value> {
    if let Some(metadata) = envelope.and_then(|v| v.metadata.as_ref()) {
        if let Some(resolved) = coerce_media_metadata(metadata) {
            return Some(resolved);
        }
    }
    let candidates = [
        content_json.and_then(|v| v.get("metadata")),
        extra_json.and_then(|v| v.get("metadata")),
        content_json,
        extra_json,
    ];
    for candidate in candidates.into_iter().flatten() {
        if let Some(resolved) = coerce_media_metadata(candidate) {
            return Some(resolved);
        }
    }
    None
}

fn prettify_media_body(body: String, message_type: i32) -> String {
    if body.is_empty() {
        return body;
    }
    if !matches!(
        message_type,
        IMAGE_MESSAGE_TYPE | FILE_MESSAGE_TYPE | VIDEO_MESSAGE_TYPE
    ) {
        return body;
    }
    let path = std::path::Path::new(&body);
    if let Some(filename) = path.file_name().and_then(|v| v.to_str()) {
        if !filename.is_empty() && filename != body {
            return filename.to_string();
        }
    }
    if body.contains('/') || body.contains('\\') {
        let normalized = body.replace('\\', "/");
        if let Some(name) = normalized.rsplit('/').next() {
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    body
}

fn extract_body(content: &str) -> String {
    if let Some(envelope) = parse_payload_envelope(content) {
        if !envelope.content.trim().is_empty() {
            return envelope.content;
        }
    }
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

fn extract_message_type_hint(content: &str) -> Option<i32> {
    let parsed = serde_json::from_str::<serde_json::Value>(content).ok();
    let envelope = parse_payload_envelope(content);
    let metadata = extract_media_metadata(parsed.as_ref(), None, envelope.as_ref())?;
    if metadata.get("file_id").is_none() && metadata.get("thumbnail_file_id").is_none() {
        return None;
    }
    let mime = metadata
        .get("mime_type")
        .and_then(|v| v.as_str())
        .map(|v| v.to_ascii_lowercase())?;
    if mime.starts_with("image/") {
        Some(IMAGE_MESSAGE_TYPE)
    } else if mime.starts_with("video/") {
        Some(VIDEO_MESSAGE_TYPE)
    } else if mime.starts_with("audio/") {
        // 协议层不再区分 Audio 消息；MIME 只是启发式判定 —— 录音（voice_xxx / 携带 duration）
        // 归为 Voice，其它音频文件一律落到 File 消息气泡。
        let looks_like_voice = metadata
            .get("filename")
            .and_then(|v| v.as_str())
            .map(|name| name.to_ascii_lowercase().starts_with("voice_"))
            .unwrap_or(false)
            || metadata.get("duration").is_some();
        if looks_like_voice {
            Some(VOICE_MESSAGE_TYPE)
        } else {
            Some(FILE_MESSAGE_TYPE)
        }
    } else {
        Some(FILE_MESSAGE_TYPE)
    }
}

fn infer_type_from_filename_like(text: &str) -> Option<i32> {
    let candidate = text.trim();
    if candidate.is_empty() {
        return None;
    }
    let ext = std::path::Path::new(candidate)
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| v.to_ascii_lowercase())?;
    let image_exts = [
        "jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "heif", "tiff",
    ];
    if image_exts.contains(&ext.as_str()) {
        return Some(IMAGE_MESSAGE_TYPE);
    }
    let video_exts = [
        "mp4", "mov", "m4v", "mkv", "avi", "webm", "flv", "3gp", "ts",
    ];
    if video_exts.contains(&ext.as_str()) {
        return Some(VIDEO_MESSAGE_TYPE);
    }
    Some(FILE_MESSAGE_TYPE)
}

fn sdk_storage_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(value) = std::env::var("PRIVCHAT_DATA_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        roots.push(PathBuf::from(value));
    }
    if let Some(home) = std::env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        let default_root = PathBuf::from(home).join(".privchat-rust");
        if !roots.iter().any(|existing| existing == &default_root) {
            roots.push(default_root);
        }
    }
    roots
}

fn guess_filename(body: &str, metadata: Option<&serde_json::Value>) -> Option<String> {
    if let Some(filename) = metadata
        .and_then(|m| m.get("filename"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return Some(filename.to_string());
    }
    if let Some(filename) = metadata
        .and_then(|m| m.get("source"))
        .and_then(|v| v.get("original_filename"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return Some(filename.to_string());
    }
    let candidate = Path::new(body)
        .file_name()
        .and_then(|v| v.to_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())?;
    Some(candidate.to_string())
}

fn resolve_local_media_path(
    body: &str,
    metadata: Option<&serde_json::Value>,
    current_uid: Option<u64>,
    message_id: u64,
    created_at: i64,
) -> Option<String> {
    if Path::new(body).is_absolute() && Path::new(body).exists() {
        return Some(body.to_string());
    }

    let filename = guess_filename(body, metadata);

    for root in sdk_storage_roots() {
        let users_root = root.join("users");
        let mut uid_candidates = Vec::new();
        if let Some(uid) = current_uid {
            uid_candidates.push(uid);
        }
        if let Ok(entries) = std::fs::read_dir(&users_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(uid_val) = path
                        .file_name()
                        .and_then(|v| v.to_str())
                        .and_then(|v| v.parse::<u64>().ok())
                    {
                        if !uid_candidates.contains(&uid_val) {
                            uid_candidates.push(uid_val);
                        }
                    }
                }
            }
        }

        for uid_val in uid_candidates {
            if let Some(path) = privchat_sdk::media_store::resolve_attachment_path(
                &root,
                uid_val,
                message_id as i64,
                created_at,
                filename.as_deref(),
            ) {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }
    None
}

fn extract_media_info(
    content: &str,
    extra: &str,
    message_type: i32,
    current_uid: Option<u64>,
    message_id: u64,
    created_at: i64,
) -> (Option<String>, Option<String>, Option<u64>) {
    if !matches!(
        message_type,
        IMAGE_MESSAGE_TYPE | FILE_MESSAGE_TYPE | VIDEO_MESSAGE_TYPE | VOICE_MESSAGE_TYPE
    ) {
        return (None, None, None);
    }

    let body = extract_body(content);

    let content_json = serde_json::from_str::<serde_json::Value>(content).ok();
    let content_envelope = parse_payload_envelope(content);
    let extra_json = serde_json::from_str::<serde_json::Value>(extra).ok();

    let metadata_owned = extract_media_metadata(
        content_json.as_ref(),
        extra_json.as_ref(),
        content_envelope.as_ref(),
    );
    let metadata = metadata_owned.as_ref();

    let typed_image_meta =
        metadata.and_then(|m| serde_json::from_value::<ImageMetadata>(m.clone()).ok());
    let typed_file_meta =
        metadata.and_then(|m| serde_json::from_value::<FileMetadata>(m.clone()).ok());
    let direct_url = typed_image_meta
        .as_ref()
        .and_then(|m| m.url.clone())
        .or_else(|| {
            metadata
                .and_then(|m| {
                    m.get("thumbnail_url")
                        .and_then(|v| v.as_str())
                        .or_else(|| m.get("file_url").and_then(|v| v.as_str()))
                        .or_else(|| m.get("url").and_then(|v| v.as_str()))
                        .or_else(|| {
                            m.get("source")
                                .and_then(|v| v.get("thumbnail_url"))
                                .and_then(|v| v.as_str())
                        })
                        .or_else(|| {
                            m.get("source")
                                .and_then(|v| v.get("file_url"))
                                .and_then(|v| v.as_str())
                        })
                        .or_else(|| {
                            m.get("source")
                                .and_then(|v| v.get("url"))
                                .and_then(|v| v.as_str())
                        })
                })
                .map(str::to_string)
        });

    let file_id_from_metadata = metadata
        .and_then(|m| {
            m.get("file_id").and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
            })
        })
        .or_else(|| {
            metadata.and_then(|m| {
                m.get("source")
                    .and_then(|v| v.get("file_id"))
                    .and_then(|v| {
                        v.as_u64()
                            .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
                    })
            })
        })
        .or_else(|| typed_image_meta.as_ref().map(|m| m.file_id))
        .or_else(|| typed_file_meta.as_ref().map(|m| m.file_id))
        .or_else(|| {
            metadata.and_then(|m| {
                m.get("thumbnail_file_id").and_then(|v| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
                })
            })
        });
    // Deprecated path policy: never synthesize media URL by hardcoded endpoint templates.
    // URL must come from payload metadata or runtime RPC (file/get_url).
    let resolved_url = direct_url;

    let local_path = if is_voice(message_type) {
        // Voice 规范命名：{message_id}.m4a（privchat-iced 下载侧约定）。
        let voice_name = voice_local_filename(message_id);
        resolve_media_in_storage(Some(&voice_name), current_uid, message_id, created_at)
    } else {
        resolve_local_media_path(&body, metadata, current_uid, message_id, created_at)
    };
    (local_path, resolved_url, file_id_from_metadata)
}

fn resolve_media_in_storage(
    expected_filename: Option<&str>,
    current_uid: Option<u64>,
    message_id: u64,
    created_at: i64,
) -> Option<String> {
    for root in sdk_storage_roots() {
        let users_root = root.join("users");
        let mut uid_candidates = Vec::new();
        if let Some(uid) = current_uid {
            uid_candidates.push(uid);
        }
        if let Ok(entries) = std::fs::read_dir(&users_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(uid_val) = path
                        .file_name()
                        .and_then(|v| v.to_str())
                        .and_then(|v| v.parse::<u64>().ok())
                    {
                        if !uid_candidates.contains(&uid_val) {
                            uid_candidates.push(uid_val);
                        }
                    }
                }
            }
        }
        for uid_val in uid_candidates {
            if let Some(path) = privchat_sdk::media_store::resolve_attachment_path(
                &root,
                uid_val,
                message_id as i64,
                created_at,
                expected_filename,
            ) {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }
    None
}

fn resolve_local_thumbnail_path(
    current_uid: Option<u64>,
    message_id: u64,
    created_at: i64,
    message_type: i32,
) -> Option<String> {
    // 仅对图片/视频消息查找缩略图
    if !matches!(message_type, IMAGE_MESSAGE_TYPE | VIDEO_MESSAGE_TYPE) {
        return None;
    }
    // 直接检查文件是否存在，不依赖 thumb_status（SDK 可能异步下载完成）
    for root in sdk_storage_roots() {
        let mut uid_candidates = Vec::new();
        if let Some(uid) = current_uid {
            uid_candidates.push(uid);
        }
        let users_root = root.join("users");
        if let Ok(entries) = std::fs::read_dir(&users_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(uid_val) = path
                        .file_name()
                        .and_then(|v| v.to_str())
                        .and_then(|v| v.parse::<u64>().ok())
                    {
                        if !uid_candidates.contains(&uid_val) {
                            uid_candidates.push(uid_val);
                        }
                    }
                }
            }
        }
        for uid_val in uid_candidates {
            let msg_dir = privchat_sdk::media_store::get_canonical_message_dir(
                &root,
                uid_val,
                message_id as i64,
                created_at,
            );
            // 查找 thumb.* (thumb.webp, thumb.png, thumb.jpg 等)
            for ext in ["webp", "png", "jpg", "jpeg", "gif"] {
                let thumb_path = msg_dir.join(format!("thumb.{ext}"));
                if thumb_path.exists() {
                    return Some(thumb_path.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

fn extract_media_file_size(content: &str, extra: &str) -> Option<u64> {
    let content_json = serde_json::from_str::<serde_json::Value>(content).ok();
    let content_envelope = parse_payload_envelope(content);
    let extra_json = serde_json::from_str::<serde_json::Value>(extra).ok();
    let metadata = extract_media_metadata(
        content_json.as_ref(),
        extra_json.as_ref(),
        content_envelope.as_ref(),
    );
    let metadata = metadata.as_ref();

    metadata
        .and_then(|m| m.get("file_size").and_then(|v| v.as_u64()))
        .or_else(|| {
            metadata
                .and_then(|m| m.get("source"))
                .and_then(|v| v.get("file_size"))
                .and_then(|v| v.as_u64())
        })
}

/// 提取语音消息时长（秒）。优先 extra.duration，兜底扫 metadata.duration / content.duration。
/// Kotlin 侧存储约定：`extra = {"duration":3,...}`，单位为秒（见 PrivchatClient.android.kt L1193）。
fn extract_voice_duration_secs(content: &str, extra: &str) -> Option<u32> {
    fn pick(value: &serde_json::Value) -> Option<u32> {
        let direct = value.get("duration").and_then(|v| v.as_u64());
        let in_metadata = value
            .get("metadata")
            .and_then(|v| v.get("duration"))
            .and_then(|v| v.as_u64());
        let in_content = value
            .get("content")
            .and_then(|v| v.get("duration"))
            .and_then(|v| v.as_u64());
        direct.or(in_metadata).or(in_content).map(|v| v.min(u32::MAX as u64) as u32)
    }
    let extra_json = serde_json::from_str::<serde_json::Value>(extra).ok();
    if let Some(v) = extra_json.as_ref().and_then(pick) {
        return Some(v);
    }
    let content_json = serde_json::from_str::<serde_json::Value>(content).ok();
    content_json.as_ref().and_then(pick)
}

fn extract_pts(extra: &str) -> Option<u64> {
    // Only recognise the canonical "pts" field.
    // Do NOT fall back to "version" — semantics unconfirmed; a wrong pts would
    // silently pollute mark_read progression.
    let parsed = serde_json::from_str::<serde_json::Value>(extra).ok()?;
    parsed.get("pts").and_then(|v| v.as_u64())
}

fn extract_revoked(content: &str, extra: &str) -> bool {
    let content_json = serde_json::from_str::<serde_json::Value>(content).ok();
    let extra_json = serde_json::from_str::<serde_json::Value>(extra).ok();
    content_json
        .as_ref()
        .and_then(|v| v.get("revoked"))
        .and_then(|v| v.as_bool())
        .or_else(|| {
            extra_json
                .as_ref()
                .and_then(|v| v.get("revoked"))
                .and_then(|v| v.as_bool())
        })
        .unwrap_or(false)
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
    let revoked = extract_revoked(&channel.last_msg_content, "");
    let raw_subtitle = extract_body(&channel.last_msg_content);
    let message_type_hint = extract_message_type_hint(&channel.last_msg_content)
        .or_else(|| infer_type_from_filename_like(&raw_subtitle));
    let subtitle = if revoked {
        "[消息已撤回]".to_string()
    } else {
        match message_type_hint {
            Some(IMAGE_MESSAGE_TYPE) => "[图片]".to_string(),
            Some(VIDEO_MESSAGE_TYPE) => "[视频]".to_string(),
            Some(FILE_MESSAGE_TYPE) => "[文件]".to_string(),
            Some(VOICE_MESSAGE_TYPE) => "[语音]".to_string(),
            _ => raw_subtitle,
        }
    };

    SessionListItemVm {
        channel_id: channel.channel_id,
        channel_type: channel.channel_type,
        peer_user_id: None,
        title: channel_display_title(channel),
        subtitle,
        unread_count: channel.unread_count.max(0) as u32,
        last_msg_timestamp: channel.last_msg_timestamp,
        is_pinned: channel.top > 0,
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

    let mut send_state = map_send_status(message.status, is_own);
    // If the backend has assigned a canonical server_message_id, treat the message as sent.
    // This recovers from historical false-failed rows left by older client versions.
    if is_own && message.server_message_id.is_some() {
        send_state = Some(MessageSendStateVm::Sent);
    }

    let (media_local_path, media_url, media_file_id) = extract_media_info(
        &message.content,
        &message.extra,
        message.message_type,
        current_uid,
        message.message_id,
        message.created_at,
    );
    let media_file_size = extract_media_file_size(&message.content, &message.extra);
    let local_thumbnail_path = resolve_local_thumbnail_path(
        current_uid,
        message.message_id,
        message.created_at,
        message.message_type,
    );
    let voice_duration_secs = if is_voice(message.message_type) {
        extract_voice_duration_secs(&message.content, &message.extra)
    } else {
        None
    };

    MessageVm {
        key,
        channel_id: message.channel_id,
        channel_type: message.channel_type,
        message_id: message.message_id,
        server_message_id: message.server_message_id,
        client_txn_id: resolved_client_txn_id,
        from_uid: message.from_uid,
        body: prettify_media_body(extract_body(&message.content), message.message_type),
        message_type: message.message_type,
        media_url,
        media_file_id,
        media_local_path,
        local_thumbnail_path,
        thumb_status: message.thumb_status,
        media_file_size,
        voice_duration_secs,
        created_at: message.created_at,
        pts: message.pts.or_else(|| extract_pts(&message.extra)),
        send_state,
        is_own,
        is_deleted: extract_revoked(&message.content, &message.extra),
        delivered: message.delivered,
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
    // Keep ordering consistent with timeline patch engine: DB row/message_id ascending.
    items.sort_by_key(|item| item.message_id);

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
    // Keep ordering consistent with timeline patch engine: DB row/message_id ascending.
    items.sort_by_key(|item| item.message_id);

    let oldest_server_message_id = derive_oldest_server_message_id(&items);

    HistoryPageVm {
        items,
        oldest_server_message_id,
        has_more_before,
    }
}

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use iced::futures::SinkExt;
use iced::stream;
use iced::Subscription;
use privchat_protocol::message::ContentMessageType;
use privchat_protocol::rpc::account::user::{DetailSourceType, UserType};
use privchat_protocol::rpc::{
    routes, AccountSearchQueryRequest, AccountSearchResponse, AccountUserDetailRequest,
    AccountUserDetailResponse, BlacklistAddRequest, BlacklistAddResponse, BlacklistRemoveRequest,
    BlacklistRemoveResponse, ChannelHideRequest, ChannelHideResponse, ChannelMuteRequest,
    ChannelMuteResponse, ChannelPinRequest, ChannelPinResponse, FileGetUrlRequest,
    FileGetUrlResponse, FriendAcceptRequest, FriendAcceptResponse, FriendApplyRequest,
    FriendApplyResponse, FriendPendingRequest, FriendPendingResponse, FriendRemoveRequest,
    FriendRemoveResponse, GetChannelPtsRequest, GetChannelPtsResponse,
    GroupMemberAddRequest, GroupMemberAddResponse, GroupMemberLeaveRequest,
    GroupMemberLeaveResponse, GroupMemberListRequest, GroupMemberListResponse,
    GroupMemberRemoveRequest, GroupMemberRemoveResponse, MessageRevokeRequest,
    MessageRevokeResponse,
    GetOrCreateDirectChannelRequest, GetOrCreateDirectChannelResponse, MessageStatusReadPtsRequest,
    MessageStatusReadPtsResponse,
};
use privchat_protocol::rpc::message::reaction::{
    MessageReactionAddRequest, MessageReactionAddResponse, MessageReactionRemoveRequest,
    MessageReactionRemoveResponse,
};
use privchat_sdk::{
    MediaDownloadState, NewMessage, PrivchatConfig, PrivchatSdk, SdkEvent, StoredChannel,
    StoredFriend, TypingActionType, UpsertBlacklistInput, UpsertChannelInput,
    UpsertMessageReactionInput,
};
use tokio::sync::broadcast::error::RecvError;

use crate::app::reporting::{self, MarkReadPtsSource};
use crate::config::AppConfig;
use crate::presentation::adapter;
use crate::presentation::vm::{
    AddFriendDetailFieldVm, AddFriendDetailVm, AddFriendSelectionVm, ClientTxnId, FriendListItemVm,
    FriendRequestItemVm, GroupListItemVm, GroupMemberDetailVm, GroupMemberVm, HistoryPageVm,
    LocalAccountVm, LoginSessionVm, MessageVm, PresenceVm, ReactionChipVm, SearchUserVm,
    SessionListItemVm, TimelineSnapshotVm, UiError,
};

fn map_sdk_error(err: privchat_sdk::Error) -> UiError {
    UiError::Unknown(err.to_string())
}

fn derive_file_base_url(server_url: &str) -> Option<String> {
    let scheme_pos = server_url.find("://")?;
    let remainder = &server_url[(scheme_pos + 3)..];
    let host_port = remainder.split('/').next().unwrap_or_default();
    if host_port.is_empty() {
        return None;
    }
    let host = host_port
        .rsplit_once(':')
        .map(|(h, _)| h)
        .unwrap_or(host_port)
        .trim();
    if host.is_empty() {
        return None;
    }
    let file_port = std::env::var("PRIVCHAT_FILE_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(8080);
    Some(format!("http://{host}:{file_port}"))
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn choose_display_name(
    alias: Option<&str>,
    nickname: Option<&str>,
    username: Option<&str>,
    fallback: String,
) -> String {
    non_empty(alias)
        .or_else(|| non_empty(nickname))
        .or_else(|| non_empty(username))
        .unwrap_or(fallback)
}

fn normalize_display_key(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_lowercase())
    }
}

fn build_friend_title_lookup(
    friends: &[StoredFriend],
    current_uid: Option<u64>,
) -> HashMap<String, u64> {
    let mut grouped: HashMap<String, Vec<u64>> = HashMap::new();
    for friend in friends {
        if Some(friend.user_id) == current_uid {
            continue;
        }
        let display_name = choose_display_name(
            friend.alias.as_deref(),
            friend.nickname.as_deref(),
            friend.username.as_deref(),
            String::new(),
        );
        if let Some(key) = normalize_display_key(&display_name) {
            grouped.entry(key).or_default().push(friend.user_id);
        }
    }

    grouped
        .into_iter()
        .filter_map(|(key, mut ids)| {
            ids.sort_unstable();
            ids.dedup();
            if ids.len() == 1 {
                Some((key, ids[0]))
            } else {
                None
            }
        })
        .collect()
}

fn field(label: &str, value: impl Into<String>) -> AddFriendDetailFieldVm {
    AddFriendDetailFieldVm {
        label: label.to_string(),
        value: value.into(),
    }
}

fn format_datetime_ms(timestamp_ms: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(timestamp_ms as i64)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| timestamp_ms.to_string())
}

fn parse_u64_text(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() || !trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    trimmed.parse::<u64>().ok()
}

fn infer_direct_peer_user_id(
    channel: &StoredChannel,
    mapped_title: &str,
    current_uid: Option<u64>,
) -> Option<u64> {
    if channel.channel_type != 1 {
        return None;
    }
    [
        parse_u64_text(&channel.channel_remark),
        parse_u64_text(&channel.channel_name),
        parse_u64_text(mapped_title),
    ]
    .into_iter()
    .flatten()
    .find(|user_id| Some(*user_id) != current_uid)
}

fn infer_direct_peer_from_members(
    members: &[privchat_sdk::StoredChannelMember],
    current_uid: Option<u64>,
) -> Option<u64> {
    members
        .iter()
        .filter(|member| !member.is_deleted)
        .map(|member| member.member_uid)
        .find(|uid| Some(*uid) != current_uid)
}

const TEXT_MESSAGE_TYPE: i32 = ContentMessageType::Text as i32;
const IMAGE_MESSAGE_TYPE: i32 = ContentMessageType::Image as i32;
const FILE_MESSAGE_TYPE: i32 = ContentMessageType::File as i32;
const VIDEO_MESSAGE_TYPE: i32 = ContentMessageType::Video as i32;

#[async_trait]
pub trait SdkBridge: Send + Sync + 'static {
    fn generate_local_message_id(&self) -> Result<ClientTxnId, UiError>;

    async fn restore_session(&self) -> Result<Option<LoginSessionVm>, UiError>;
    async fn load_session_list(&self) -> Result<Vec<SessionListItemVm>, UiError>;
    async fn load_total_unread_count(&self, exclude_muted: bool) -> Result<u32, UiError>;
    async fn sync_channel(&self, channel_id: u64, channel_type: i32) -> Result<usize, UiError>;
    async fn list_local_accounts(&self) -> Result<Vec<LocalAccountVm>, UiError>;
    async fn switch_to_local_account(&self, uid: String) -> Result<LoginSessionVm, UiError>;
    async fn load_active_username(&self) -> Result<String, UiError>;
    async fn logout(&self) -> Result<(), UiError>;
    async fn search_users(&self, query: String) -> Result<Vec<SearchUserVm>, UiError>;
    async fn send_friend_request(
        &self,
        to_user_id: u64,
        remark: Option<String>,
        search_session_id: Option<u64>,
    ) -> Result<u64, UiError>;
    async fn accept_friend_request(&self, from_user_id: u64) -> Result<u64, UiError>;
    async fn load_friend_list(&self) -> Result<Vec<FriendListItemVm>, UiError>;
    async fn batch_get_presence(&self, user_ids: Vec<u64>) -> Result<Vec<PresenceVm>, UiError>;
    async fn load_group_list(&self) -> Result<Vec<GroupListItemVm>, UiError>;
    /// 群聊 @提及候选成员：只对群频道（channel_type=2）有意义。
    /// 去除当前用户，按 remark > member_name 排序。
    async fn load_group_members(
        &self,
        channel_id: u64,
        channel_type: i32,
    ) -> Result<Vec<GroupMemberVm>, UiError>;
    async fn load_friend_request_list(&self) -> Result<Vec<FriendRequestItemVm>, UiError>;
    async fn load_add_friend_detail(
        &self,
        item: AddFriendSelectionVm,
    ) -> Result<AddFriendDetailVm, UiError>;
    async fn load_user_profile(
        &self,
        user_id: u64,
        channel_id: u64,
        fallback_name: Option<String>,
    ) -> Result<AddFriendDetailVm, UiError>;
    async fn set_friend_alias(&self, user_id: u64, alias: String) -> Result<bool, UiError>;
    async fn get_or_create_direct_channel(
        &self,
        target_user_id: u64,
    ) -> Result<(u64, i32), UiError>;

    async fn login_with_password(
        &self,
        username: String,
        password: String,
        device_id: String,
        register: bool,
    ) -> Result<LoginSessionVm, UiError>;

    async fn open_timeline(
        &self,
        channel_id: u64,
        channel_type: i32,
    ) -> Result<TimelineSnapshotVm, UiError>;
    async fn subscribe_channel(&self, channel_id: u64, channel_type: i32) -> Result<(), UiError>;

    async fn send_text_message(
        &self,
        channel_id: u64,
        channel_type: i32,
        client_txn_id: ClientTxnId,
        body: String,
        reply_to_server_message_id: Option<u64>,
        mentioned_user_ids: Option<Vec<u64>>,
    ) -> Result<u64, UiError>;
    async fn send_attachment_message(
        &self,
        channel_id: u64,
        channel_type: i32,
        client_txn_id: ClientTxnId,
        file_path: String,
    ) -> Result<u64, UiError>;
    async fn send_typing(
        &self,
        channel_id: u64,
        channel_type: i32,
        is_typing: bool,
    ) -> Result<(), UiError>;
    async fn revoke_message(&self, channel_id: u64, server_message_id: u64) -> Result<(), UiError>;

    /// 置顶/取消置顶频道：调用 RPC 并将 `top` 同步到本地 channel 表。
    async fn pin_channel(&self, channel_id: u64, pinned: bool) -> Result<(), UiError>;

    /// 免打扰开关：调用 `channel/mute` RPC 并同步本地 channel.mute 字段。
    async fn mute_channel(&self, channel_id: u64, muted: bool) -> Result<(), UiError>;

    /// 删除好友：调用 `contact/friend/remove` RPC 并清理本地 friend 行。
    async fn delete_friend(&self, friend_id: u64) -> Result<(), UiError>;

    /// 拉黑用户：调用 `contact/blacklist/add` RPC 并写本地 blacklist 表。
    async fn add_to_blacklist(&self, blocked_user_id: u64) -> Result<(), UiError>;

    /// 取消拉黑：调用 `contact/blacklist/remove` RPC 并删除本地 blacklist 行。
    async fn remove_from_blacklist(&self, blocked_user_id: u64) -> Result<(), UiError>;

    /// 查询用户是否在本地黑名单中。读本地表，不触达服务端。
    async fn is_user_blacklisted(&self, user_id: u64) -> Result<bool, UiError>;

    /// 群成员详情列表（含 role / joined_at）：直接调 `group/member/list` RPC，
    /// 服务端返回权威数据，不依赖本地 `channel_members` 表同步。
    async fn fetch_group_members_detailed(
        &self,
        group_id: u64,
    ) -> Result<Vec<GroupMemberDetailVm>, UiError>;

    /// 邀请用户加入群组：调 `group/member/add`，默认 role=member。
    async fn add_group_member(&self, group_id: u64, user_id: u64) -> Result<(), UiError>;

    /// 将成员移出群组：调 `group/member/remove`。仅群主/管理员可操作，权限由服务端判定。
    async fn remove_group_member(&self, group_id: u64, user_id: u64) -> Result<(), UiError>;

    /// 主动退出群组：调 `group/member/leave`，成功后清理本地 channel / 消息目录。
    async fn leave_group(&self, group_id: u64) -> Result<(), UiError>;

    /// 隐藏频道：调用 RPC 并同步写本地 hidden 标记。
    async fn hide_channel(&self, channel_id: u64) -> Result<(), UiError>;

    /// 本地删除频道：不触达服务端，清 DB 行、消息及本地附件目录。
    async fn delete_channel_local(&self, channel_id: u64) -> Result<(), UiError>;

    /// 本地删除一条消息：不触达服务端，仅清 DB 行与本地附件目录。
    /// 返回 true 表示命中行被删除；false 表示消息不存在（幂等）。
    async fn delete_message_local(&self, message_id: u64) -> Result<bool, UiError>;

    async fn retry_send(
        &self,
        channel_id: u64,
        channel_type: i32,
        client_txn_id: ClientTxnId,
    ) -> Result<(), UiError>;

    async fn load_history_before(
        &self,
        channel_id: u64,
        channel_type: i32,
        before_server_message_id: Option<u64>,
        limit: usize,
    ) -> Result<HistoryPageVm, UiError>;

    async fn load_message_vm(&self, message_id: u64) -> Result<Option<MessageVm>, UiError>;

    async fn mark_read(
        &self,
        channel_id: u64,
        channel_type: i32,
        last_read_pts: u64,
    ) -> Result<(), UiError>;

    async fn get_file_url(&self, file_id: u64) -> Result<String, UiError>;

    async fn get_peer_read_pts(
        &self,
        channel_id: u64,
        channel_type: i32,
    ) -> Result<Option<u64>, UiError>;

    fn subscribe_timeline(&self, session_epoch: u64) -> Subscription<SdkEvent>;

    async fn load_quick_phrases(&self) -> Result<Vec<String>, UiError>;
    async fn save_quick_phrases(&self, phrases: &[String]) -> Result<(), UiError>;

    async fn start_message_media_download(
        &self,
        message_id: u64,
        download_url: String,
        mime: String,
        filename_hint: Option<String>,
        created_at_ms: i64,
    ) -> Result<(), UiError>;
    async fn pause_message_media_download(&self, message_id: u64);
    async fn resume_message_media_download(&self, message_id: u64);
    async fn cancel_message_media_download(&self, message_id: u64);
    async fn get_media_download_state(&self, message_id: u64) -> MediaDownloadState;

    /// Start a media download and block until it reaches Done or Failed.
    /// Returns the final local path on success.
    async fn download_message_media_blocking(
        &self,
        message_id: u64,
        download_url: String,
        mime: String,
        filename_hint: Option<String>,
        created_at_ms: i64,
    ) -> Result<String, UiError>;

    /// 添加消息反应：调用 `message/reaction/add` RPC，成功后写本地反应表。
    async fn add_reaction(
        &self,
        server_message_id: u64,
        channel_id: u64,
        channel_type: i32,
        message_id: u64,
        my_uid: u64,
        emoji: String,
    ) -> Result<(), UiError>;

    /// 移除消息反应：调用 `message/reaction/remove` RPC，成功后写本地墓碑行。
    async fn remove_reaction(
        &self,
        server_message_id: u64,
        channel_id: u64,
        channel_type: i32,
        message_id: u64,
        my_uid: u64,
        emoji: String,
    ) -> Result<(), UiError>;

    /// 批量从本地反应表读取并聚合为 UI chip，按 (emoji) 分组并以最新 seq 优先去重。
    /// `my_uid` 用于标记 `mine`；0 表示未登录（永不高亮）。
    async fn list_local_message_reactions_batch(
        &self,
        message_ids: Vec<u64>,
        my_uid: u64,
    ) -> Result<HashMap<u64, Vec<ReactionChipVm>>, UiError>;

    /// 把一条本地消息转发到目标会话，返回新的本地 message_id。
    /// 语义与 privchat-sdk-ffi 的 `forward_message` 完全一致：
    /// 1. 读源消息，若已撤回则拒绝；
    /// 2. 以当前登录用户为 from_uid，克隆 content / message_type / mime_type / extra / thumb_status
    ///    调用 `create_local_message_with_id` 写入本地表；
    /// 3. 调 `enqueue_outbound_message` 让发送链路接管；
    /// 4. 若源消息带附件，拷贝源目录全部文件到新目录并 `update_media_downloaded=true`。
    async fn forward_message(
        &self,
        src_message_id: u64,
        target_channel_id: u64,
        target_channel_type: i32,
    ) -> Result<u64, UiError>;
}

#[derive(Clone)]
pub struct PrivchatSdkBridge {
    sdk: PrivchatSdk,
}

#[derive(Clone)]
struct EventSubscriptionSeed {
    sdk: PrivchatSdk,
    /// Included in the hash so that Iced tears down and recreates the event
    /// stream subscription whenever the active user changes. Without this,
    /// a stale broadcast::Receiver from the previous user's session would
    /// keep running after account switch, causing events to be lost.
    session_epoch: u64,
}

impl std::hash::Hash for EventSubscriptionSeed {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&"privchat_sdk_event_stream", state);
        std::hash::Hash::hash(&self.session_epoch, state);
    }
}

fn sdk_event_stream(seed: &EventSubscriptionSeed) -> impl iced::futures::Stream<Item = SdkEvent> {
    let sdk = seed.sdk.clone();
    let session_epoch = seed.session_epoch;
    stream::channel(256, async move |mut output| {
        tracing::info!("sdk event stream start: session_epoch={session_epoch}");
        let mut receiver = sdk.subscribe_events();
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if output.send(event).await.is_err() {
                        tracing::info!(
                            "sdk event stream stop(output closed): session_epoch={session_epoch}"
                        );
                        break;
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(
                        "sdk event stream lagged: session_epoch={} skipped={}",
                        session_epoch,
                        skipped
                    );
                }
                Err(RecvError::Closed) => {
                    tracing::info!("sdk event stream stop(closed): session_epoch={session_epoch}");
                    break;
                }
            }
        }
    })
}

impl PrivchatSdkBridge {
    pub fn new(config: AppConfig) -> Self {
        let server_urls: Vec<_> = config.servers.iter().map(|s| s.url.clone()).collect();

        if std::env::var("PRIVCHAT_FILE_BASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .is_none()
        {
            if let Some(derived) = server_urls
                .first()
                .and_then(|url| derive_file_base_url(url))
            {
                std::env::set_var("PRIVCHAT_FILE_BASE_URL", &derived);
                tracing::info!("derived PRIVCHAT_FILE_BASE_URL={derived}");
            }
        }

        let mut sdk_config = PrivchatConfig::from_server_urls(server_urls.clone(), 10);
        if let Some(data_dir) = std::env::var("PRIVCHAT_DATA_DIR")
            .ok()
            .filter(|value| !value.is_empty())
        {
            sdk_config.data_dir = data_dir;
        }
        tracing::info!("privchat sdk endpoints: {:?}", server_urls);

        Self {
            sdk: PrivchatSdk::new(sdk_config),
        }
    }

    async fn current_uid(&self) -> Result<Option<u64>, UiError> {
        let snapshot = self.sdk.session_snapshot().await.map_err(map_sdk_error)?;
        Ok(snapshot.map(|s| s.user_id))
    }

    /// 快捷消息文件路径: {data_dir}/users/{uid}/quick_phrases.json
    async fn quick_phrases_path(&self) -> Result<std::path::PathBuf, UiError> {
        let uid = self.current_uid().await?
            .ok_or_else(|| UiError::Unknown("未登录，无法加载常用语".to_string()))?;
        let data_dir = std::env::var("PRIVCHAT_DATA_DIR")
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                format!("{}/.privchat-rust", home)
            });
        Ok(std::path::PathBuf::from(data_dir)
            .join("users")
            .join(uid.to_string())
            .join("quick_phrases.json"))
    }

    async fn apply_revoke_flag_to_vm(&self, message: &mut MessageVm) -> Result<(), UiError> {
        if message.is_deleted {
            return Ok(());
        }
        if let Some(extra) = self
            .sdk
            .get_message_extra(message.message_id)
            .await
            .map_err(map_sdk_error)?
        {
            if extra.revoke {
                message.is_deleted = true;
            }
        }
        Ok(())
    }

    async fn apply_revoke_flags_to_vms(&self, messages: &mut [MessageVm]) -> Result<(), UiError> {
        for message in messages {
            self.apply_revoke_flag_to_vm(message).await?;
        }
        Ok(())
    }

    async fn load_user_detail(
        &self,
        user_id: u64,
        source: &str,
        source_id: String,
    ) -> Result<AddFriendDetailVm, UiError> {
        let local = self
            .sdk
            .get_user_by_id(user_id)
            .await
            .map_err(map_sdk_error)?;
        let remote = match self
            .sdk
            .rpc_call_typed::<_, AccountUserDetailResponse>(
                routes::account_user::DETAIL,
                &AccountUserDetailRequest {
                    target_user_id: user_id,
                    source: source.to_string(),
                    source_id,
                    user_id: 0,
                },
            )
            .await
        {
            Ok(resp) => Some(resp),
            Err(e) => {
                tracing::warn!("load_user_detail remote failed for user_id={user_id}: {e:?}");
                None
            }
        };

        if local.is_none() && remote.is_none() {
            // 本地和远程都查不到时，用 user_id 兜底显示基本信息
            return Ok(AddFriendDetailVm {
                title: format!("用户 {user_id}"),
                subtitle: format!("UID: {user_id}"),
                fields: vec![field("用户 ID", user_id.to_string())],
            });
        }

        let username = remote
            .as_ref()
            .and_then(|value| non_empty(Some(value.username.as_str())))
            .or_else(|| {
                local
                    .as_ref()
                    .and_then(|value| non_empty(value.username.as_deref()))
            });
        let nickname = remote
            .as_ref()
            .and_then(|value| non_empty(Some(value.nickname.as_str())))
            .or_else(|| {
                local
                    .as_ref()
                    .and_then(|value| non_empty(value.nickname.as_deref()))
            });
        let alias = local
            .as_ref()
            .and_then(|value| non_empty(value.alias.as_deref()));

        let title = choose_display_name(
            alias.as_deref(),
            nickname.as_deref(),
            username.as_deref(),
            format!("用户 {user_id}"),
        );
        let subtitle = username
            .clone()
            .map(|value| format!("PrivChat ID: {value}"))
            .unwrap_or_else(|| format!("UID: {user_id}"));

        let mut fields = vec![field("用户 ID", user_id.to_string())];

        if let Some(value) = username {
            fields.push(field("用户名", value));
        }
        if let Some(value) = nickname {
            fields.push(field("昵称", value));
        }
        if let Some(value) = alias {
            fields.push(field("备注", value));
        }

        if let Some(remote) = remote {
            fields.push(field("账号类型", UserType::label(remote.user_type)));
            if let Some(value) = non_empty(remote.phone.as_deref()) {
                fields.push(field("手机号", value));
            }
            if let Some(value) = non_empty(remote.email.as_deref()) {
                fields.push(field("邮箱", value));
            }
            if let Some(value) = non_empty(Some(remote.source_type.as_str())) {
                fields.push(field("来源", value));
            }
            let relation = if remote.is_friend {
                "已是好友"
            } else {
                "未添加"
            };
            fields.push(field("关系", relation));
        } else if local.is_some() {
            fields.push(field("关系", "已是好友"));
        }

        Ok(AddFriendDetailVm {
            title,
            subtitle,
            fields,
        })
    }

    async fn get_or_fetch_user_by_id(
        &self,
        user_id: u64,
    ) -> Result<Option<privchat_sdk::StoredUser>, UiError> {
        if let Some(user) = self
            .sdk
            .get_user_by_id(user_id)
            .await
            .map_err(map_sdk_error)?
        {
            return Ok(Some(user));
        }

        let remote: Option<AccountUserDetailResponse> = self
            .sdk
            .rpc_call_typed(
                routes::account_user::DETAIL,
                &AccountUserDetailRequest {
                    target_user_id: user_id,
                    source: DetailSourceType::Friend.as_str().to_string(),
                    source_id: user_id.to_string(),
                    user_id: 0,
                },
            )
            .await
            .ok();

        let Some(remote) = remote else {
            return Ok(None);
        };

        let updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        self.sdk
            .upsert_user(privchat_sdk::UpsertUserInput {
                user_id,
                username: Some(remote.username).filter(|s| !s.trim().is_empty()),
                nickname: Some(remote.nickname).filter(|s| !s.trim().is_empty()),
                alias: None,
                avatar: remote.avatar_url.unwrap_or_default(),
                user_type: remote.user_type as i32,
                is_deleted: false,
                channel_id: String::new(),
                version: 0,
                updated_at,
            })
            .await
            .map_err(map_sdk_error)?;

        self.sdk
            .get_user_by_id(user_id)
            .await
            .map_err(map_sdk_error)
    }

    fn map_friend_item(friend: StoredFriend) -> FriendListItemVm {
        let title = choose_display_name(
            friend.alias.as_deref(),
            friend.nickname.as_deref(),
            friend.username.as_deref(),
            format!("用户 {}", friend.user_id),
        );
        let subtitle = non_empty(friend.username.as_deref())
            .filter(|value| value != &title)
            .unwrap_or_else(|| format!("UID: {}", friend.user_id));

        FriendListItemVm {
            user_id: friend.user_id,
            title,
            subtitle,
            is_added: true,
            is_online: false,
        }
    }

    fn map_request_item(
        request_user: &SearchUserVm,
        request_message: Option<String>,
        is_added: bool,
    ) -> FriendRequestItemVm {
        let title = choose_display_name(
            None,
            if request_user.nickname.is_empty() {
                None
            } else {
                Some(&request_user.nickname)
            },
            if request_user.username.is_empty() {
                None
            } else {
                Some(&request_user.username)
            },
            format!("用户 {}", request_user.user_id),
        );
        let subtitle = request_message
            .as_ref()
            .and_then(|value| non_empty(Some(value.as_str())))
            .unwrap_or_else(|| "请求添加你为好友".to_string());

        FriendRequestItemVm {
            from_user_id: request_user.user_id,
            user: request_user.clone(),
            title,
            subtitle,
            is_added,
        }
    }

    async fn run_post_auth_sync(&self, scene: &str) -> Result<(), UiError> {
        tracing::info!("{scene}: run bootstrap sync");
        self.sdk.run_bootstrap_sync().await.map_err(map_sdk_error)?;
        tracing::info!("{scene}: bootstrap sync completed");

        // Reliability-first policy:
        // account came online from an offline window, so pull full channel diffs once
        // to avoid missing offline messages on channel-scoped resume gaps.
        let applied = self.sdk.sync_all_channels().await.map_err(map_sdk_error)?;
        tracing::info!("{scene}: sync_all_channels completed applied={applied}");

        Ok(())
    }
}

#[async_trait]
impl SdkBridge for PrivchatSdkBridge {
    fn generate_local_message_id(&self) -> Result<ClientTxnId, UiError> {
        self.sdk.generate_local_message_id().map_err(map_sdk_error)
    }

    async fn restore_session(&self) -> Result<Option<LoginSessionVm>, UiError> {
        tracing::info!("restore_session: connect");
        self.sdk.connect().await.map_err(map_sdk_error)?;

        let Some(snapshot) = self.sdk.session_snapshot().await.map_err(map_sdk_error)? else {
            tracing::info!("restore_session: no local session snapshot");
            return Ok(None);
        };
        tracing::info!(
            "restore_session: snapshot found user_id={} device_id={}",
            snapshot.user_id,
            snapshot.device_id
        );

        if self
            .sdk
            .authenticate(
                snapshot.user_id,
                snapshot.token.clone(),
                snapshot.device_id.clone(),
            )
            .await
            .is_err()
        {
            tracing::warn!("restore_session: authenticate failed, fallback to login screen");
            return Ok(None);
        }
        tracing::info!("restore_session: authenticate ok");
        self.run_post_auth_sync("restore_session").await?;

        Ok(Some(LoginSessionVm {
            user_id: snapshot.user_id,
            token: snapshot.token,
            device_id: snapshot.device_id,
        }))
    }

    async fn load_session_list(&self) -> Result<Vec<SessionListItemVm>, UiError> {
        let channels = self
            .sdk
            .list_channels(300, 0)
            .await
            .map_err(map_sdk_error)?;
        let current_uid = self.current_uid().await?;
        let bootstrap_completed = self.sdk.is_bootstrap_completed().await.unwrap_or(false);
        let unread_snapshot = channels
            .iter()
            .map(|channel| {
                format!(
                    "{}:{}:{}",
                    channel.channel_id, channel.channel_type, channel.unread_count
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        tracing::info!(
            "load_session_list: channels={} bootstrap_completed={} unread_snapshot=[{}]",
            channels.len(),
            bootstrap_completed,
            unread_snapshot
        );

        let friend_title_lookup = match self.sdk.list_friends(5000, 0).await {
            Ok(friends) => build_friend_title_lookup(&friends, current_uid),
            Err(error) => {
                tracing::warn!("presence.friend_lookup_load_failed: {}", error);
                HashMap::new()
            }
        };

        let mut items = Vec::with_capacity(channels.len());
        let mut did_entity_repair_sync = false;
        for channel in &channels {
            let mut item = adapter::map_channel_to_session_item(channel);
            if channel.last_local_message_id > 0 {
                if let Some(extra) = self
                    .sdk
                    .get_message_extra(channel.last_local_message_id)
                    .await
                    .map_err(map_sdk_error)?
                {
                    if extra.revoke {
                        item.subtitle = "[消息已撤回]".to_string();
                    }
                }
            }
            let mut peer_user_id =
                infer_direct_peer_user_id(channel, &item.title, current_uid);
            if peer_user_id.is_none() && channel.channel_type == 1 {
                match self
                    .sdk
                    .list_channel_members(channel.channel_id, channel.channel_type, 64, 0)
                    .await
                {
                    Ok(members) => {
                        peer_user_id = infer_direct_peer_from_members(&members, current_uid);
                    }
                    Err(error) => {
                        tracing::warn!(
                            "infer peer by channel members failed: channel_id={} channel_type={} error={}",
                            channel.channel_id,
                            channel.channel_type,
                            error
                        );
                    }
                }
            }
            if peer_user_id.is_none() && channel.channel_type == 1 {
                match self
                    .sdk
                    .query_timeline_snapshot(channel.channel_id, channel.channel_type, 64, 0)
                    .await
                {
                    Ok(snapshot) => {
                        peer_user_id = snapshot
                            .messages
                            .iter()
                            .map(|message| message.from_uid)
                            .find(|uid| *uid > 0 && Some(*uid) != current_uid);
                        tracing::info!(
                            "presence.infer_peer_from_snapshot: channel_id={} channel_type={} resolved_peer_user_id={:?}",
                            channel.channel_id,
                            channel.channel_type,
                            peer_user_id
                        );
                    }
                    Err(error) => {
                        tracing::warn!(
                            "infer peer by timeline snapshot failed: channel_id={} channel_type={} error={}",
                            channel.channel_id,
                            channel.channel_type,
                            error
                        );
                    }
                }
            }
            if peer_user_id.is_none() && channel.channel_type == 1 {
                if let Some(key) = normalize_display_key(&item.title) {
                    peer_user_id = friend_title_lookup.get(&key).copied();
                    tracing::info!(
                        "presence.infer_peer_from_friend_title: channel_id={} channel_type={} title={} resolved_peer_user_id={:?}",
                        channel.channel_id,
                        channel.channel_type,
                        item.title,
                        peer_user_id
                    );
                }
            }

            if let Some(peer_user_id) = peer_user_id {
                let mut user = self.get_or_fetch_user_by_id(peer_user_id).await?;
                if user.is_none() && !did_entity_repair_sync {
                    did_entity_repair_sync = true;
                    for entity in ["user", "friend", "channel"] {
                        if let Err(error) = self.sdk.sync_entities(entity.to_string(), None).await {
                            tracing::warn!(
                                "session title repair sync failed: entity={} error={}",
                                entity,
                                error
                            );
                        }
                    }
                    user = self.get_or_fetch_user_by_id(peer_user_id).await?;
                }
                if let Some(user) = user {
                    item.title = choose_display_name(
                        user.alias.as_deref(),
                        user.nickname.as_deref(),
                        user.username.as_deref(),
                        item.title.clone(),
                    );
                }
                item.peer_user_id = Some(peer_user_id);
            }
            tracing::info!(
                "presence.session_item: channel_id={} channel_type={} title={} peer_user_id={:?}",
                item.channel_id,
                item.channel_type,
                item.title,
                item.peer_user_id
            );
            items.push(item);
        }

        Ok(items)
    }

    async fn load_total_unread_count(&self, exclude_muted: bool) -> Result<u32, UiError> {
        let unread = self
            .sdk
            .get_total_unread_count(exclude_muted)
            .await
            .map_err(map_sdk_error)?;
        Ok(unread.max(0) as u32)
    }

    async fn sync_channel(&self, channel_id: u64, channel_type: i32) -> Result<usize, UiError> {
        self.sdk
            .sync_channel(channel_id, channel_type)
            .await
            .map_err(map_sdk_error)
    }

    async fn list_local_accounts(&self) -> Result<Vec<LocalAccountVm>, UiError> {
        let mut accounts = self
            .sdk
            .list_local_accounts()
            .await
            .map_err(map_sdk_error)?;
        accounts.sort_by(|a, b| b.last_login_at.cmp(&a.last_login_at));
        Ok(accounts
            .into_iter()
            .map(|account| LocalAccountVm {
                uid: account.uid,
                is_active: account.is_active,
                created_at: account.created_at,
                last_login_at: account.last_login_at,
            })
            .collect())
    }

    async fn switch_to_local_account(&self, uid: String) -> Result<LoginSessionVm, UiError> {
        let uid = uid.trim().to_string();
        if uid.is_empty() {
            return Err(UiError::Unknown("uid is required".to_string()));
        }

        // Keep switch flow deterministic: tear down current transport first, then
        // authenticate target account and run a full sync pass.
        if let Err(error) = self.sdk.disconnect().await {
            tracing::warn!("switch_to_local_account: disconnect old session failed: {error}");
        }

        self.sdk
            .set_current_uid(uid.clone())
            .await
            .map_err(map_sdk_error)?;
        self.sdk.connect().await.map_err(map_sdk_error)?;

        let snapshot = self
            .sdk
            .session_snapshot()
            .await
            .map_err(map_sdk_error)?
            .ok_or_else(|| UiError::Unknown(format!("local account not found: {uid}")))?;

        self.sdk
            .authenticate(
                snapshot.user_id,
                snapshot.token.clone(),
                snapshot.device_id.clone(),
            )
            .await
            .map_err(map_sdk_error)?;

        tracing::info!(
            "switch_to_local_account: authenticate ok user_id={} bootstrap_completed={}",
            snapshot.user_id,
            snapshot.bootstrap_completed
        );
        self.run_post_auth_sync("switch_to_local_account").await?;

        Ok(LoginSessionVm {
            user_id: snapshot.user_id,
            token: snapshot.token,
            device_id: snapshot.device_id,
        })
    }

    async fn logout(&self) -> Result<(), UiError> {
        // Best-effort remote logout; local cleanup must still run.
        let _ = self
            .sdk
            .rpc_call("account/auth/logout".to_string(), "{}".to_string())
            .await;
        let _ = self.sdk.clear_local_state().await.map_err(map_sdk_error);
        self.sdk.disconnect().await.map_err(map_sdk_error)
    }

    async fn search_users(&self, query: String) -> Result<Vec<SearchUserVm>, UiError> {
        let query = query.trim().to_string();
        tracing::info!("add_friend.search_users: query={query}");
        let response: AccountSearchResponse = self
            .sdk
            .rpc_call_typed(
                routes::account_search::QUERY,
                &AccountSearchQueryRequest {
                    query,
                    page: Some(1),
                    page_size: Some(50),
                    from_user_id: 0,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        tracing::info!("add_friend.search_users: hits={}", response.users.len());

        Ok(response
            .users
            .into_iter()
            .map(|user| SearchUserVm {
                user_id: user.user_id,
                username: user.username.clone(),
                nickname: if user.nickname.trim().is_empty() {
                    user.username
                } else {
                    user.nickname
                },
                avatar_url: user.avatar_url,
                user_type: user.user_type,
                search_session_id: user.search_session_id,
                is_friend: user.is_friend,
                can_send_message: user.can_send_message,
            })
            .collect())
    }

    async fn send_friend_request(
        &self,
        to_user_id: u64,
        remark: Option<String>,
        search_session_id: Option<u64>,
    ) -> Result<u64, UiError> {
        tracing::info!(
            "add_friend.send_friend_request: to_user_id={} search_session_id={:?}",
            to_user_id,
            search_session_id
        );
        let response: FriendApplyResponse = self
            .sdk
            .rpc_call_typed(
                routes::friend::APPLY,
                &FriendApplyRequest {
                    target_user_id: to_user_id,
                    message: remark,
                    source: Some("search".to_string()),
                    source_id: search_session_id.map(|value| value.to_string()),
                    from_user_id: 0,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        tracing::info!(
            "add_friend.send_friend_request: response_user_id={}",
            response.user_id
        );

        Ok(response.user_id)
    }

    async fn load_friend_list(&self) -> Result<Vec<FriendListItemVm>, UiError> {
        let mut items = self
            .sdk
            .list_friends(5000, 0)
            .await
            .map_err(map_sdk_error)?
            .into_iter()
            .map(Self::map_friend_item)
            .collect::<Vec<_>>();

        items.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Ok(items)
    }

    async fn batch_get_presence(&self, user_ids: Vec<u64>) -> Result<Vec<PresenceVm>, UiError> {
        tracing::info!("presence.batch_get_presence.request: user_ids={:?}", user_ids);
        let statuses = self
            .sdk
            .batch_get_presence(user_ids)
            .await
            .map_err(map_sdk_error)?;
        let mapped = statuses
            .into_iter()
            .map(|status| PresenceVm {
                user_id: status.user_id,
                is_online: status.is_online,
                last_seen_at: status.last_seen_at,
                device_count: status.device_count,
            })
            .collect::<Vec<_>>();
        tracing::info!(
            "presence.batch_get_presence.response: items={}",
            mapped
                .iter()
                .map(|item| format!(
                    "{}:{}:{}:{}",
                    item.user_id, item.is_online, item.last_seen_at, item.device_count
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
        Ok(mapped)
    }

    async fn accept_friend_request(&self, from_user_id: u64) -> Result<u64, UiError> {
        tracing::info!(
            "add_friend.accept_friend_request: from_user_id={}",
            from_user_id
        );
        let channel_id: FriendAcceptResponse = self
            .sdk
            .rpc_call_typed(
                routes::friend::ACCEPT,
                &FriendAcceptRequest {
                    from_user_id,
                    message: None,
                    target_user_id: 0,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        tracing::info!(
            "add_friend.accept_friend_request: created_channel_id={}",
            channel_id
        );
        Ok(from_user_id)
    }

    async fn load_active_username(&self) -> Result<String, UiError> {
        let Some(uid) = self.current_uid().await? else {
            return Err(UiError::Unknown("no active uid".to_string()));
        };
        let Some(user) = self.sdk.get_user_by_id(uid).await.map_err(map_sdk_error)? else {
            return Ok(uid.to_string());
        };
        if let Some(username) = user.username.as_deref().and_then(|v| non_empty(Some(v))) {
            return Ok(username.to_string());
        }
        if let Some(nickname) = user.nickname.as_deref().and_then(|v| non_empty(Some(v))) {
            return Ok(nickname.to_string());
        }
        if let Some(alias) = user.alias.as_deref().and_then(|v| non_empty(Some(v))) {
            return Ok(alias.to_string());
        }
        Ok(uid.to_string())
    }

    async fn load_group_list(&self) -> Result<Vec<GroupListItemVm>, UiError> {
        let mut groups = self
            .sdk
            .list_groups(5000, 0)
            .await
            .map_err(map_sdk_error)?
            .into_iter()
            .map(|group| {
                let title = non_empty(group.name.as_deref())
                    .unwrap_or_else(|| format!("群组 {}", group.group_id));
                GroupListItemVm {
                    group_id: group.group_id,
                    subtitle: format!("Group ID: {}", group.group_id),
                    title,
                }
            })
            .collect::<Vec<_>>();

        groups.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Ok(groups)
    }

    async fn load_group_members(
        &self,
        channel_id: u64,
        channel_type: i32,
    ) -> Result<Vec<GroupMemberVm>, UiError> {
        let members = self
            .sdk
            .list_channel_members(channel_id, channel_type, 5000, 0)
            .await
            .map_err(map_sdk_error)?;
        let current_uid = self.current_uid().await?;

        let mut list: Vec<GroupMemberVm> = members
            .into_iter()
            .filter(|m| Some(m.member_uid) != current_uid && m.member_uid > 0 && !m.is_deleted)
            .map(|m| {
                let display_name = if !m.member_name.trim().is_empty() {
                    m.member_name
                } else {
                    format!("用户 {}", m.member_uid)
                };
                GroupMemberVm {
                    user_id: m.member_uid,
                    display_name,
                    remark: m.member_remark,
                }
            })
            .collect();

        list.sort_by(|a, b| a.best_label().to_lowercase().cmp(&b.best_label().to_lowercase()));
        Ok(list)
    }

    async fn load_friend_request_list(&self) -> Result<Vec<FriendRequestItemVm>, UiError> {
        let friends = self
            .sdk
            .list_friends(5000, 0)
            .await
            .map_err(map_sdk_error)?;
        let friend_ids = friends
            .iter()
            .map(|friend| friend.user_id)
            .collect::<HashSet<_>>();

        let response: FriendPendingResponse = self
            .sdk
            .rpc_call_typed(
                routes::friend::PENDING,
                &FriendPendingRequest { user_id: 0 },
            )
            .await
            .map_err(map_sdk_error)?;

        let mut items = Vec::with_capacity(response.requests.len());
        for request in response.requests {
            let user = SearchUserVm {
                user_id: request.user.user_id,
                username: request.user.username.clone(),
                nickname: request.user.nickname.clone(),
                avatar_url: request.user.avatar_url.clone(),
                user_type: request.user.user_type,
                search_session_id: request.user.search_session_id,
                is_friend: request.user.is_friend,
                can_send_message: request.user.can_send_message,
            };
            items.push(Self::map_request_item(
                &user,
                request.message,
                friend_ids.contains(&request.from_user_id),
            ));
        }
        Ok(items)
    }

    async fn load_add_friend_detail(
        &self,
        item: AddFriendSelectionVm,
    ) -> Result<AddFriendDetailVm, UiError> {
        match item {
            AddFriendSelectionVm::Friend(user_id) => {
                self.load_user_detail(user_id, DetailSourceType::Friend.as_str(), user_id.to_string())
                    .await
            }
            AddFriendSelectionVm::Group(group_id) => {
                let Some(group) = self
                    .sdk
                    .get_group_by_id(group_id)
                    .await
                    .map_err(map_sdk_error)?
                else {
                    return Err(UiError::Unknown(format!("群组不存在: {group_id}")));
                };
                let members = self
                    .sdk
                    .list_group_members(group_id, 5000, 0)
                    .await
                    .map_err(map_sdk_error)?;
                let title = non_empty(group.name.as_deref())
                    .unwrap_or_else(|| format!("群组 {}", group_id));
                let subtitle = format!("{} 位成员", members.len());

                let mut fields = vec![
                    field("群组 ID", group_id.to_string()),
                    field("成员数", members.len().to_string()),
                    field(
                        "状态",
                        if group.is_dismissed {
                            "已解散"
                        } else {
                            "正常"
                        },
                    ),
                ];
                if let Some(owner_id) = group.owner_id {
                    fields.push(field("群主", owner_id.to_string()));
                }

                Ok(AddFriendDetailVm {
                    title,
                    subtitle,
                    fields,
                })
            }
            AddFriendSelectionVm::Request(user_id) => {
                // 优先使用 pending 列表中的真实申请人数据，避免本地 user 表尚未同步时误报“用户不存在”。
                let pending: FriendPendingResponse = self
                    .sdk
                    .rpc_call_typed(
                        routes::friend::PENDING,
                        &FriendPendingRequest { user_id: 0 },
                    )
                    .await
                    .map_err(map_sdk_error)?;
                let request_entry = pending
                    .requests
                    .into_iter()
                    .find(|entry| entry.from_user_id == user_id || entry.user.user_id == user_id);

                let (resolved_user_id, username, nickname, alias) =
                    if let Some(entry) = &request_entry {
                        let user = &entry.user;
                        (
                            user.user_id,
                            non_empty(Some(user.username.as_str())),
                            non_empty(Some(user.nickname.as_str())),
                            None::<String>,
                        )
                    } else {
                        let user = self
                            .sdk
                            .get_user_by_id(user_id)
                            .await
                            .map_err(map_sdk_error)?
                            .ok_or_else(|| UiError::Unknown(format!("用户不存在: {user_id}")))?;
                        (
                            user.user_id,
                            user.username.as_deref().and_then(|v| non_empty(Some(v))),
                            user.nickname.as_deref().and_then(|v| non_empty(Some(v))),
                            user.alias.as_deref().and_then(|v| non_empty(Some(v))),
                        )
                    };

                let title = choose_display_name(
                    alias.as_deref(),
                    nickname.as_deref(),
                    username.as_deref(),
                    format!("用户 {resolved_user_id}"),
                );
                let subtitle = username
                    .as_ref()
                    .map(|value| format!("PrivChat ID: {value}"))
                    .unwrap_or_else(|| format!("UID: {resolved_user_id}"));

                let mut fields = vec![field("用户 ID", resolved_user_id.to_string())];
                if let Some(value) = username.as_ref() {
                    fields.push(field("用户名", value.clone()));
                }
                if let Some(value) = nickname.as_ref() {
                    fields.push(field("昵称", value.clone()));
                }
                if let Some(value) = alias.as_ref() {
                    fields.push(field("备注", value.clone()));
                }

                // 附加申请消息和时间
                if let Some(request) = request_entry {
                    if let Some(message) = request
                        .message
                        .as_ref()
                        .and_then(|value| non_empty(Some(value)))
                    {
                        fields.push(field("申请消息", message));
                    }
                    fields.push(field("申请时间", format_datetime_ms(request.created_at)));
                }

                Ok(AddFriendDetailVm {
                    title,
                    subtitle,
                    fields,
                })
            }
        }
    }

    async fn load_user_profile(
        &self,
        user_id: u64,
        channel_id: u64,
        fallback_name: Option<String>,
    ) -> Result<AddFriendDetailVm, UiError> {
        let mut detail = self
            .load_user_detail(user_id, DetailSourceType::Conversation.as_str(), channel_id.to_string())
            .await?;
        // 如果标题仍是默认的 "用户 {id}"，且调用方提供了 fallback 名称，则替换
        let default_title = format!("用户 {user_id}");
        if detail.title == default_title {
            if let Some(name) = fallback_name.filter(|n| !n.is_empty()) {
                detail.title = name;
            }
        }
        Ok(detail)
    }

    async fn set_friend_alias(&self, user_id: u64, alias: String) -> Result<bool, UiError> {
        use privchat_protocol::rpc::contact::friend::{FriendSetAliasRequest, FriendSetAliasResponse};
        let alias_opt = if alias.is_empty() { None } else { Some(alias.clone()) };
        let result: FriendSetAliasResponse = self
            .sdk
            .rpc_call_typed(
                routes::friend::SET_ALIAS,
                &FriendSetAliasRequest {
                    user_id,
                    alias: alias_opt.clone(),
                },
            )
            .await
            .map_err(map_sdk_error)?;
        if result {
            self.sdk
                .update_user_alias(user_id, alias_opt)
                .await
                .map_err(map_sdk_error)?;
        }
        Ok(result)
    }

    async fn get_or_create_direct_channel(
        &self,
        target_user_id: u64,
    ) -> Result<(u64, i32), UiError> {
        let response: GetOrCreateDirectChannelResponse = self
            .sdk
            .rpc_call_typed(
                routes::channel::DIRECT_GET_OR_CREATE,
                &GetOrCreateDirectChannelRequest {
                    target_user_id,
                    source: Some("contact_list".to_string()),
                    source_id: Some(format!("contact:{target_user_id}")),
                    user_id: 0,
                },
            )
            .await
            .map_err(map_sdk_error)?;

        if response.channel_id == 0 {
            return Err(UiError::Unknown(
                "get_or_create_direct_channel 返回了无效 channel_id=0".to_string(),
            ));
        }

        let mut channel = self
            .sdk
            .get_channel_by_id(response.channel_id)
            .await
            .map_err(map_sdk_error)?;

        if channel.is_none() {
            let _ = self
                .sdk
                .sync_entities("channel".to_string(), None)
                .await
                .map_err(map_sdk_error)?;
            channel = self
                .sdk
                .get_channel_by_id(response.channel_id)
                .await
                .map_err(map_sdk_error)?;
        }

        let channel = channel.ok_or_else(|| {
            UiError::Unknown(format!(
                "未找到会话信息: channel_id={}。请先等待同步完成后重试。",
                response.channel_id
            ))
        })?;
        if channel.channel_type <= 0 {
            return Err(UiError::Unknown(format!(
                "会话类型无效: channel_id={} channel_type={}",
                response.channel_id, channel.channel_type
            )));
        }

        Ok((response.channel_id, channel.channel_type))
    }

    async fn login_with_password(
        &self,
        username: String,
        password: String,
        device_id: String,
        register: bool,
    ) -> Result<LoginSessionVm, UiError> {
        self.sdk.connect().await.map_err(map_sdk_error)?;

        let result = if register {
            self.sdk
                .register(username, password, device_id.clone())
                .await
                .map_err(map_sdk_error)?
        } else {
            self.sdk
                .login(username, password, device_id.clone())
                .await
                .map_err(map_sdk_error)?
        };

        self.sdk
            .authenticate(
                result.user_id,
                result.token.clone(),
                result.device_id.clone(),
            )
            .await
            .map_err(map_sdk_error)?;
        tracing::info!(
            "login_with_password: authenticate ok user_id={}",
            result.user_id
        );
        self.run_post_auth_sync("login_with_password").await?;

        Ok(LoginSessionVm {
            user_id: result.user_id,
            token: result.token,
            device_id: result.device_id,
        })
    }

    async fn open_timeline(
        &self,
        channel_id: u64,
        channel_type: i32,
    ) -> Result<TimelineSnapshotVm, UiError> {
        let current_uid = self.current_uid().await?;
        let snapshot = self
            .sdk
            .query_timeline_snapshot(channel_id, channel_type, 100, 0)
            .await
            .map_err(map_sdk_error)?;

        let channel = self
            .sdk
            .get_channel_by_id(channel_id)
            .await
            .map_err(map_sdk_error)?
            .filter(|c| c.channel_type == channel_type);

        let extra = self
            .sdk
            .get_channel_extra(channel_id, channel_type)
            .await
            .map_err(map_sdk_error)?;

        let unread_marker = adapter::map_unread_marker(channel.as_ref(), extra.as_ref());
        let mut vm = adapter::map_snapshot_to_vm(
            &snapshot,
            current_uid,
            0,
            unread_marker,
        );
        self.apply_revoke_flags_to_vms(&mut vm.items).await?;
        Ok(vm)
    }

    async fn subscribe_channel(&self, channel_id: u64, channel_type: i32) -> Result<(), UiError> {
        let channel_type = u8::try_from(channel_type)
            .map_err(|_| UiError::Unknown(format!("invalid channel_type: {channel_type}")))?;
        tracing::info!(
            "presence.bridge_subscribe_channel.request: channel_id={} channel_type={}",
            channel_id,
            channel_type
        );
        self.sdk
            .subscribe_channel(channel_id, channel_type, None)
            .await
            .map_err(map_sdk_error)?;
        tracing::info!(
            "presence.bridge_subscribe_channel.ok: channel_id={} channel_type={}",
            channel_id,
            channel_type
        );
        Ok(())
    }

    async fn send_text_message(
        &self,
        channel_id: u64,
        channel_type: i32,
        client_txn_id: ClientTxnId,
        body: String,
        reply_to_server_message_id: Option<u64>,
        mentioned_user_ids: Option<Vec<u64>>,
    ) -> Result<u64, UiError> {
        if channel_id == 0 || channel_type <= 0 {
            return Err(UiError::Unknown(format!(
                "invalid send target: channel_id={} channel_type={}",
                channel_id, channel_type
            )));
        }

        let current_uid = self
            .current_uid()
            .await?
            .ok_or_else(|| UiError::Unknown("no active session user".to_string()))?;

        // local-first：带引用或 @ 提及时，把内容包装成 MessagePayloadEnvelope JSON。
        // 服务端不校验原消息存在性，仅透传 reply_to_message_id 与 mentioned_user_ids；
        // 接收端由同步流程写 mentions 表 + extra.has_mention。
        let has_reply = reply_to_server_message_id.is_some();
        let has_mentions = mentioned_user_ids
            .as_ref()
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        let content = if has_reply || has_mentions {
            let envelope = privchat_protocol::message::MessagePayloadEnvelope {
                content: body,
                metadata: None,
                reply_to_message_id: reply_to_server_message_id.map(|id| id.to_string()),
                mentioned_user_ids: if has_mentions {
                    mentioned_user_ids.clone()
                } else {
                    None
                },
                message_source: None,
            };
            serde_json::to_string(&envelope).map_err(|err| {
                UiError::Unknown(format!("serialize send envelope failed: {err}"))
            })?
        } else {
            body
        };

        let local_row_message_id = self
            .sdk
            .create_local_message_with_id(
                NewMessage {
                    channel_id,
                    channel_type,
                    from_uid: current_uid,
                    // Keep aligned with privchat-app sendTextWithLocalId semantics.
                    // Server contract expects text=0.
                    message_type: TEXT_MESSAGE_TYPE,
                    content,
                    searchable_word: String::new(),
                    setting: 0,
                    extra: String::new(),
                    mime_type: None,
                    media_downloaded: false,
                    thumb_status: 0,
                },
                Some(client_txn_id),
            )
            .await
            .map_err(map_sdk_error)?;

        let stored = self
            .sdk
            .get_message_by_id(local_row_message_id)
            .await
            .map_err(map_sdk_error)?
            .ok_or_else(|| {
                UiError::Unknown("local message row missing after create".to_string())
            })?;
        if stored.message_type != TEXT_MESSAGE_TYPE {
            return Err(UiError::Unknown(format!(
                "unexpected local message_type={} for text send",
                stored.message_type
            )));
        }

        let message_id = self
            .sdk
            .enqueue_outbound_message(local_row_message_id, Vec::new())
            .await
            .map_err(map_sdk_error)?;

        tracing::info!(
            "send_text_message: channel_id={} channel_type={} client_txn_id={} message_id={}",
            channel_id,
            channel_type,
            client_txn_id,
            message_id
        );

        Ok(message_id)
    }

    async fn send_attachment_message(
        &self,
        channel_id: u64,
        channel_type: i32,
        client_txn_id: ClientTxnId,
        file_path: String,
    ) -> Result<u64, UiError> {
        if channel_id == 0 || channel_type <= 0 {
            return Err(UiError::Unknown(format!(
                "invalid send target: channel_id={} channel_type={}",
                channel_id, channel_type
            )));
        }
        let path = Path::new(&file_path);
        if !path.exists() || !path.is_file() {
            return Err(UiError::Unknown(format!(
                "attachment file not found: {}",
                file_path
            )));
        }
        let ext = path
            .extension()
            .and_then(|v| v.to_str())
            .map(|v| v.to_ascii_lowercase())
            .unwrap_or_default();
        let message_type = match ext.as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic" => IMAGE_MESSAGE_TYPE,
            "mp4" | "mov" | "mkv" | "avi" | "webm" => VIDEO_MESSAGE_TYPE,
            _ => FILE_MESSAGE_TYPE,
        };

        let current_uid = self
            .current_uid()
            .await?
            .ok_or_else(|| UiError::Unknown("no active session user".to_string()))?;

        let local_row_message_id = self
            .sdk
            .create_local_message_with_id(
                NewMessage {
                    channel_id,
                    channel_type,
                    from_uid: current_uid,
                    message_type,
                    content: file_path.clone(),
                    searchable_word: path
                        .file_name()
                        .and_then(|v| v.to_str())
                        .unwrap_or_default()
                        .to_string(),
                    setting: 0,
                    // Keep metadata object non-null to satisfy server-side media validation.
                    extra: "{}".to_string(),
                    mime_type: None,
                    media_downloaded: false,
                    thumb_status: 0,
                },
                Some(client_txn_id),
            )
            .await
            .map_err(map_sdk_error)?;

        let payload = std::fs::read(path)
            .map_err(|error| UiError::Unknown(format!("read attachment failed: {error}")))?;
        let route_key = format!("channel:{channel_type}:{channel_id}");
        self.sdk
            .enqueue_outbound_file(local_row_message_id, route_key, payload)
            .await
            .map_err(map_sdk_error)?;

        tracing::info!(
            "send_attachment_message: channel_id={} channel_type={} client_txn_id={} message_id={} path={}",
            channel_id,
            channel_type,
            client_txn_id,
            local_row_message_id,
            file_path
        );

        Ok(local_row_message_id)
    }

    async fn send_typing(
        &self,
        channel_id: u64,
        channel_type: i32,
        is_typing: bool,
    ) -> Result<(), UiError> {
        self.sdk
            .send_typing(
                channel_id,
                channel_type,
                is_typing,
                TypingActionType::Typing,
            )
            .await
            .map_err(map_sdk_error)
    }

    async fn revoke_message(&self, channel_id: u64, server_message_id: u64) -> Result<(), UiError> {
        let response: MessageRevokeResponse = self
            .sdk
            .rpc_call_typed(
                routes::message::REVOKE,
                &MessageRevokeRequest {
                    server_message_id,
                    channel_id,
                    user_id: 0,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        if !response {
            return Err(UiError::Unknown("撤回失败".to_string()));
        }
        // Persist revoke state into local SDK storage immediately.
        // Otherwise UI can briefly flip to "revoked" and then be overwritten by a refresh.
        let channel_type = self
            .sdk
            .get_channel_by_id(channel_id)
            .await
            .map_err(map_sdk_error)?
            .map(|channel| channel.channel_type as i32)
            .unwrap_or(1);
        if let Some(message) = self
            .sdk
            .list_messages(channel_id, channel_type, 5000, 0)
            .await
            .map_err(map_sdk_error)?
            .into_iter()
            .find(|message| message.server_message_id == Some(server_message_id))
        {
            self.sdk
                .set_message_revoke(message.message_id, true, None)
                .await
                .map_err(map_sdk_error)?;
        }
        Ok(())
    }

    async fn pin_channel(&self, channel_id: u64, pinned: bool) -> Result<(), UiError> {
        let ok: ChannelPinResponse = self
            .sdk
            .rpc_call_typed(
                routes::channel::PIN,
                &ChannelPinRequest {
                    user_id: 0,
                    channel_id,
                    pinned,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        if !ok {
            return Err(UiError::Unknown("置顶失败".to_string()));
        }
        // Persist `top` locally so the change survives the next session list reload.
        if let Some(channel) = self
            .sdk
            .get_channel_by_id(channel_id)
            .await
            .map_err(map_sdk_error)?
        {
            self.sdk
                .upsert_channel(UpsertChannelInput {
                    channel_id: channel.channel_id,
                    channel_type: channel.channel_type,
                    channel_name: channel.channel_name,
                    channel_remark: channel.channel_remark,
                    avatar: channel.avatar,
                    unread_count: channel.unread_count,
                    top: if pinned { 1 } else { 0 },
                    mute: channel.mute,
                    last_msg_timestamp: channel.last_msg_timestamp,
                    last_local_message_id: channel.last_local_message_id,
                    last_msg_content: channel.last_msg_content,
                    version: channel.version,
                })
                .await
                .map_err(map_sdk_error)?;
        }
        Ok(())
    }

    async fn hide_channel(&self, channel_id: u64) -> Result<(), UiError> {
        let ok: ChannelHideResponse = self
            .sdk
            .rpc_call_typed(
                routes::channel::HIDE,
                &ChannelHideRequest {
                    user_id: 0,
                    channel_id,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        if !ok {
            return Err(UiError::Unknown("隐藏失败".to_string()));
        }
        self.sdk
            .set_channel_hidden(channel_id, true)
            .await
            .map_err(map_sdk_error)?;
        Ok(())
    }

    async fn mute_channel(&self, channel_id: u64, muted: bool) -> Result<(), UiError> {
        let ok: ChannelMuteResponse = self
            .sdk
            .rpc_call_typed(
                routes::channel::MUTE,
                &ChannelMuteRequest {
                    user_id: 0,
                    channel_id,
                    muted,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        if !ok {
            return Err(UiError::Unknown("免打扰设置失败".to_string()));
        }
        if let Some(channel) = self
            .sdk
            .get_channel_by_id(channel_id)
            .await
            .map_err(map_sdk_error)?
        {
            self.sdk
                .upsert_channel(UpsertChannelInput {
                    channel_id: channel.channel_id,
                    channel_type: channel.channel_type,
                    channel_name: channel.channel_name,
                    channel_remark: channel.channel_remark,
                    avatar: channel.avatar,
                    unread_count: channel.unread_count,
                    top: channel.top,
                    mute: if muted { 1 } else { 0 },
                    last_msg_timestamp: channel.last_msg_timestamp,
                    last_local_message_id: channel.last_local_message_id,
                    last_msg_content: channel.last_msg_content,
                    version: channel.version,
                })
                .await
                .map_err(map_sdk_error)?;
        }
        Ok(())
    }

    async fn delete_friend(&self, friend_id: u64) -> Result<(), UiError> {
        let ok: FriendRemoveResponse = self
            .sdk
            .rpc_call_typed(
                routes::friend::DELETE,
                &FriendRemoveRequest {
                    friend_id,
                    user_id: 0,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        if !ok {
            return Err(UiError::Unknown("删除好友失败".to_string()));
        }
        self.sdk
            .delete_friend(friend_id)
            .await
            .map_err(map_sdk_error)?;
        Ok(())
    }

    async fn add_to_blacklist(&self, blocked_user_id: u64) -> Result<(), UiError> {
        let current_uid = self.current_uid().await?.ok_or_else(|| {
            UiError::Unknown("当前未登录，无法拉黑用户".to_string())
        })?;
        let ok: BlacklistAddResponse = self
            .sdk
            .rpc_call_typed(
                routes::blacklist::ADD,
                &BlacklistAddRequest {
                    user_id: current_uid,
                    blocked_user_id,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        if !ok {
            return Err(UiError::Unknown("拉黑失败".to_string()));
        }
        let now_ms = now_millis_i64();
        self.sdk
            .upsert_blacklist_entry(UpsertBlacklistInput {
                blocked_user_id,
                created_at: now_ms,
                updated_at: now_ms,
            })
            .await
            .map_err(map_sdk_error)?;
        Ok(())
    }

    async fn remove_from_blacklist(&self, blocked_user_id: u64) -> Result<(), UiError> {
        let current_uid = self.current_uid().await?.ok_or_else(|| {
            UiError::Unknown("当前未登录，无法移除拉黑".to_string())
        })?;
        let ok: BlacklistRemoveResponse = self
            .sdk
            .rpc_call_typed(
                routes::blacklist::REMOVE,
                &BlacklistRemoveRequest {
                    user_id: current_uid,
                    blocked_user_id,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        if !ok {
            return Err(UiError::Unknown("取消拉黑失败".to_string()));
        }
        self.sdk
            .delete_blacklist_entry(blocked_user_id)
            .await
            .map_err(map_sdk_error)?;
        Ok(())
    }

    async fn is_user_blacklisted(&self, user_id: u64) -> Result<bool, UiError> {
        // 分页拉一次本地表，判断是否命中目标 user_id。
        // 本地黑名单通常规模很小（常见 <100），一次性列举即可。
        let entries = self
            .sdk
            .list_blacklist_entries(1024, 0)
            .await
            .map_err(map_sdk_error)?;
        Ok(entries.iter().any(|e| e.blocked_user_id == user_id))
    }

    async fn fetch_group_members_detailed(
        &self,
        group_id: u64,
    ) -> Result<Vec<GroupMemberDetailVm>, UiError> {
        let resp: GroupMemberListResponse = self
            .sdk
            .rpc_call_typed(
                routes::group_member::LIST,
                &GroupMemberListRequest {
                    group_id,
                    user_id: 0,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        let mut list: Vec<GroupMemberDetailVm> = resp
            .members
            .into_iter()
            .map(|m| {
                let display_name = non_empty(Some(m.nickname.as_str()))
                    .or_else(|| non_empty(Some(m.username.as_str())))
                    .unwrap_or_else(|| format!("用户 {}", m.user_id));
                GroupMemberDetailVm {
                    user_id: m.user_id,
                    display_name,
                    avatar_url: m.avatar_url,
                    role: m.role,
                    joined_at_ms: m.joined_at,
                    is_muted: m.is_muted,
                }
            })
            .collect();
        // 群主 > 管理员 > 普通成员，同级按名称字典序。
        list.sort_by(|a, b| {
            a.role_rank()
                .cmp(&b.role_rank())
                .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()))
        });
        Ok(list)
    }

    async fn add_group_member(&self, group_id: u64, user_id: u64) -> Result<(), UiError> {
        let ok: GroupMemberAddResponse = self
            .sdk
            .rpc_call_typed(
                routes::group_member::ADD,
                &GroupMemberAddRequest {
                    group_id,
                    user_id,
                    role: None,
                    inviter_id: 0,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        if !ok {
            return Err(UiError::Unknown("邀请入群失败".to_string()));
        }
        Ok(())
    }

    async fn remove_group_member(&self, group_id: u64, user_id: u64) -> Result<(), UiError> {
        let ok: GroupMemberRemoveResponse = self
            .sdk
            .rpc_call_typed(
                routes::group_member::REMOVE,
                &GroupMemberRemoveRequest {
                    group_id,
                    user_id,
                    operator_id: 0,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        if !ok {
            return Err(UiError::Unknown("移除成员失败".to_string()));
        }
        // 服务端也会下发成员变更事件，但这里同步清本地行，
        // 让资料页再次拉取详情时更快一致。
        self.sdk
            .delete_group_member(group_id, user_id)
            .await
            .map_err(map_sdk_error)?;
        Ok(())
    }

    async fn leave_group(&self, group_id: u64) -> Result<(), UiError> {
        let ok: GroupMemberLeaveResponse = self
            .sdk
            .rpc_call_typed(
                routes::group_member::LEAVE,
                &GroupMemberLeaveRequest {
                    group_id,
                    user_id: 0,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        if !ok {
            return Err(UiError::Unknown("退出群组失败".to_string()));
        }
        // 群频道 channel_id == group_id、channel_type == 2，
        // 本地删除会顺带清掉消息目录（见 delete_channel_local）。
        self.delete_channel_local(group_id).await?;
        Ok(())
    }

    async fn delete_channel_local(&self, channel_id: u64) -> Result<(), UiError> {
        let messages = self
            .sdk
            .delete_channel_local(channel_id)
            .await
            .map_err(map_sdk_error)?;
        if let Some(uid) = self.current_uid().await? {
            let data_dir = std::env::var("PRIVCHAT_DATA_DIR").unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                format!("{}/.privchat-rust", home)
            });
            let root = std::path::Path::new(&data_dir);
            for stored in &messages {
                let canonical = privchat_sdk::media_store::get_canonical_message_dir(
                    root,
                    uid,
                    stored.message_id as i64,
                    stored.created_at,
                );
                let _ = std::fs::remove_dir_all(&canonical);
                let legacy = root
                    .join("users")
                    .join(uid.to_string())
                    .join("files")
                    .join(stored.message_id.to_string());
                if legacy != canonical {
                    let _ = std::fs::remove_dir_all(&legacy);
                }
            }
        }
        Ok(())
    }

    async fn delete_message_local(&self, message_id: u64) -> Result<bool, UiError> {
        let stored = self
            .sdk
            .delete_message_local(message_id)
            .await
            .map_err(map_sdk_error)?;
        Ok(stored.is_some())
    }

    async fn retry_send(
        &self,
        channel_id: u64,
        channel_type: i32,
        client_txn_id: ClientTxnId,
    ) -> Result<(), UiError> {
        let direct_lookup = self
            .sdk
            .get_message_by_id(client_txn_id)
            .await
            .map_err(map_sdk_error)?;

        let mut message =
            direct_lookup.filter(|m| m.channel_id == channel_id && m.channel_type == channel_type);
        if message.is_none() {
            let messages = self
                .sdk
                .list_messages(channel_id, channel_type, 5000, 0)
                .await
                .map_err(map_sdk_error)?;
            message = messages.into_iter().find(|m| {
                m.local_message_id == Some(client_txn_id) || m.message_id == client_txn_id
            });
        }
        let message = message.ok_or_else(|| {
            UiError::Unknown(format!(
                "retry target missing: channel_id={} channel_type={} client_txn_id={}",
                channel_id, channel_type, client_txn_id
            ))
        })?;

        tracing::info!(
            "retry_send: channel_id={} channel_type={} client_txn_id={} message_id={} local_message_id={:?} status={}",
            channel_id,
            channel_type,
            client_txn_id,
            message.message_id,
            message.local_message_id,
            message.status
        );

        self.sdk
            .enqueue_outbound_message(message.message_id, Vec::new())
            .await
            .map_err(map_sdk_error)?;

        Ok(())
    }

    async fn load_history_before(
        &self,
        channel_id: u64,
        channel_type: i32,
        before_server_message_id: Option<u64>,
        limit: usize,
    ) -> Result<HistoryPageVm, UiError> {
        let t0 = Instant::now();
        let current_uid = self.current_uid().await?;
        // SDK list_messages only supports (limit, offset), no cursor/before_id query.
        // Fetch enough to cover the slice we need; keep limit small to avoid loading
        // the entire channel history on every page request.
        // TODO: if SDK gains a before_server_message_id cursor API, replace this.
        let fetch_limit = (limit * 4).max(200);
        let messages = self
            .sdk
            .list_messages(channel_id, channel_type, fetch_limit, 0)
            .await
            .map_err(map_sdk_error)?;

        let mut all = adapter::map_history_messages_to_vm(&messages, current_uid, false).items;
        self.apply_revoke_flags_to_vms(&mut all).await?;

        let split_index = if let Some(before) = before_server_message_id {
            all.iter()
                .position(|message| message.server_message_id == Some(before))
                .unwrap_or(all.len())
        } else {
            all.len()
        };

        let older_slice = &all[..split_index];
        let page_start = older_slice.len().saturating_sub(limit.max(1));
        let items = older_slice[page_start..].to_vec();
        let oldest_server_message_id = items.iter().filter_map(|m| m.server_message_id).min();

        reporting::report_history_loaded(channel_id, channel_type, items.len(), t0.elapsed());
        Ok(HistoryPageVm {
            has_more_before: page_start > 0,
            items,
            oldest_server_message_id,
        })
    }

    async fn load_message_vm(&self, message_id: u64) -> Result<Option<MessageVm>, UiError> {
        let current_uid = self.current_uid().await?;
        let message = self
            .sdk
            .get_message_by_id(message_id)
            .await
            .map_err(map_sdk_error)?;
        let mut vm = message.map(|stored| adapter::map_stored_message_to_vm(&stored, current_uid, None));
        if let Some(message) = &mut vm {
            self.apply_revoke_flag_to_vm(message).await?;
        }
        Ok(vm)
    }

    async fn mark_read(
        &self,
        channel_id: u64,
        channel_type: i32,
        last_read_pts: u64,
    ) -> Result<(), UiError> {
        if channel_id == 0 || channel_type <= 0 {
            return Err(UiError::Unknown(format!(
                "invalid mark_read target: channel_id={} channel_type={}",
                channel_id, channel_type
            )));
        }

        let channel_type_u8 = u8::try_from(channel_type).map_err(|_| {
            UiError::Unknown(format!(
                "invalid channel_type for mark_read: {}",
                channel_type
            ))
        })?;

        let server_pts = self
            .sdk
            .rpc_call_typed::<_, GetChannelPtsResponse>(
                routes::sync::GET_CHANNEL_PTS,
                &GetChannelPtsRequest {
                    channel_id,
                    channel_type: channel_type_u8,
                },
            )
            .await
            .map(|resp| resp.current_pts)
            .map_err(map_sdk_error)
            .ok();

        let pts_source = if last_read_pts > 0 {
            MarkReadPtsSource::MessagePts
        } else {
            MarkReadPtsSource::RpcFallback
        };
        let read_pts = server_pts.filter(|pts| *pts > 0).unwrap_or(last_read_pts);
        if read_pts == 0 {
            tracing::warn!(
                "bridge.mark_read skip: channel_id={} channel_type={} last_read_pts={} server_pts={:?} resolved_read_pts=0",
                channel_id,
                channel_type,
                last_read_pts,
                server_pts
            );
            return Ok(());
        }

        reporting::report_mark_read(channel_id, channel_type, read_pts, pts_source);
        tracing::info!(
            "bridge.mark_read: channel_id={} channel_type={} last_read_pts={} server_pts={:?} resolved_read_pts={}",
            channel_id,
            channel_type,
            last_read_pts,
            server_pts,
            read_pts
        );

        // Same UX as privchat-app: first apply local projection so unread badge clears immediately.
        self.sdk
            .project_channel_read_cursor(channel_id, channel_type, read_pts)
            .await
            .map_err(map_sdk_error)?;

        let _resp: MessageStatusReadPtsResponse = self
            .sdk
            .rpc_call_typed(
                routes::message_status::READ_PTS,
                &MessageStatusReadPtsRequest {
                    channel_id,
                    read_pts,
                    last_read_message_id: None,
                    client_visible_pts: None,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        tracing::info!(
            "bridge.mark_read rpc ok: channel_id={} channel_type={} read_pts={}",
            channel_id,
            channel_type,
            read_pts
        );
        Ok(())
    }

    async fn get_file_url(&self, file_id: u64) -> Result<String, UiError> {
        let response: FileGetUrlResponse = self
            .sdk
            .rpc_call_typed(
                routes::file::GET_URL,
                &FileGetUrlRequest {
                    file_id,
                    user_id: 0,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        Ok(response.file_url)
    }

    async fn get_peer_read_pts(
        &self,
        channel_id: u64,
        channel_type: i32,
    ) -> Result<Option<u64>, UiError> {
        self.sdk
            .get_peer_read_pts(channel_id, channel_type)
            .await
            .map_err(map_sdk_error)
    }

    fn subscribe_timeline(&self, session_epoch: u64) -> Subscription<SdkEvent> {
        Subscription::run_with(
            EventSubscriptionSeed {
                sdk: self.sdk.clone(),
                session_epoch,
            },
            sdk_event_stream,
        )
    }

    async fn load_quick_phrases(&self) -> Result<Vec<String>, UiError> {
        let path = self.quick_phrases_path().await?;
        match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| UiError::Unknown(format!("解析常用语失败: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(UiError::Unknown(format!("读取常用语失败: {e}"))),
        }
    }

    async fn save_quick_phrases(&self, phrases: &[String]) -> Result<(), UiError> {
        let path = self.quick_phrases_path().await?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| UiError::Unknown(format!("创建用户目录失败: {e}")))?;
        }
        let bytes = serde_json::to_vec_pretty(phrases)
            .map_err(|e| UiError::Unknown(format!("序列化常用语失败: {e}")))?;
        tokio::fs::write(&path, bytes).await
            .map_err(|e| UiError::Unknown(format!("保存常用语失败: {e}")))?;
        Ok(())
    }

    async fn start_message_media_download(
        &self,
        message_id: u64,
        download_url: String,
        mime: String,
        filename_hint: Option<String>,
        created_at_ms: i64,
    ) -> Result<(), UiError> {
        self.sdk
            .start_message_media_download(message_id, download_url, mime, filename_hint, created_at_ms)
            .await
            .map_err(map_sdk_error)
    }

    async fn pause_message_media_download(&self, message_id: u64) {
        self.sdk.pause_message_media_download(message_id).await;
    }

    async fn resume_message_media_download(&self, message_id: u64) {
        self.sdk.resume_message_media_download(message_id).await;
    }

    async fn cancel_message_media_download(&self, message_id: u64) {
        self.sdk.cancel_message_media_download(message_id).await;
    }

    async fn get_media_download_state(&self, message_id: u64) -> MediaDownloadState {
        self.sdk.get_media_download_state(message_id).await
    }

    async fn download_message_media_blocking(
        &self,
        message_id: u64,
        download_url: String,
        mime: String,
        filename_hint: Option<String>,
        created_at_ms: i64,
    ) -> Result<String, UiError> {
        // Subscribe BEFORE starting so we don't miss the Done/Failed event.
        let mut receiver = self.sdk.subscribe_events();

        // Fast-path: if a previous Done state is still cached, reuse it.
        if let MediaDownloadState::Done { path } =
            self.sdk.get_media_download_state(message_id).await
        {
            return Ok(path);
        }

        self.sdk
            .start_message_media_download(
                message_id,
                download_url,
                mime,
                filename_hint,
                created_at_ms,
            )
            .await
            .map_err(map_sdk_error)?;

        loop {
            match receiver.recv().await {
                Ok(SdkEvent::MediaDownloadStateChanged { message_id: mid, state })
                    if mid == message_id =>
                {
                    match state {
                        MediaDownloadState::Done { path } => return Ok(path),
                        MediaDownloadState::Failed { code, message } => {
                            return Err(UiError::Unknown(format!(
                                "media download failed code={code}: {message}"
                            )));
                        }
                        MediaDownloadState::Idle => {
                            // Cancelled by another path.
                            return Err(UiError::Unknown(
                                "media download cancelled".to_string(),
                            ));
                        }
                        _ => continue,
                    }
                }
                Ok(_) => continue,
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => {
                    return Err(UiError::Unknown("sdk event channel closed".to_string()));
                }
            }
        }
    }

    async fn add_reaction(
        &self,
        server_message_id: u64,
        channel_id: u64,
        channel_type: i32,
        message_id: u64,
        my_uid: u64,
        emoji: String,
    ) -> Result<(), UiError> {
        let _: MessageReactionAddResponse = self
            .sdk
            .rpc_call_typed(
                routes::message_reaction::ADD,
                &MessageReactionAddRequest {
                    server_message_id,
                    channel_id: Some(channel_id),
                    emoji: emoji.clone(),
                    user_id: 0,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        let now = now_millis_i64();
        self.sdk
            .upsert_message_reaction(UpsertMessageReactionInput {
                channel_id,
                channel_type,
                uid: my_uid,
                name: format!("u{my_uid}"),
                emoji,
                message_id,
                seq: now,
                is_deleted: false,
                created_at: now,
            })
            .await
            .map_err(map_sdk_error)?;
        Ok(())
    }

    async fn remove_reaction(
        &self,
        server_message_id: u64,
        channel_id: u64,
        channel_type: i32,
        message_id: u64,
        my_uid: u64,
        emoji: String,
    ) -> Result<(), UiError> {
        let _: MessageReactionRemoveResponse = self
            .sdk
            .rpc_call_typed(
                routes::message_reaction::REMOVE,
                &MessageReactionRemoveRequest {
                    server_message_id,
                    emoji: emoji.clone(),
                    user_id: 0,
                },
            )
            .await
            .map_err(map_sdk_error)?;
        let now = now_millis_i64();
        self.sdk
            .upsert_message_reaction(UpsertMessageReactionInput {
                channel_id,
                channel_type,
                uid: my_uid,
                name: format!("u{my_uid}"),
                emoji,
                message_id,
                seq: now,
                is_deleted: true,
                created_at: now,
            })
            .await
            .map_err(map_sdk_error)?;
        Ok(())
    }

    async fn list_local_message_reactions_batch(
        &self,
        message_ids: Vec<u64>,
        my_uid: u64,
    ) -> Result<HashMap<u64, Vec<ReactionChipVm>>, UiError> {
        let mut out: HashMap<u64, Vec<ReactionChipVm>> = HashMap::new();
        for mid in message_ids {
            let rows = self
                .sdk
                .list_message_reactions(mid, 500, 0)
                .await
                .map_err(map_sdk_error)?;
            // rows 顺序：seq DESC, id DESC（本地表层面）。对每个 (uid, emoji) 只保留最新行，
            // 若最新是墓碑则跳过；聚合时按首次出现的 emoji 顺序生成 chip 列表。
            let mut seen_pairs: HashSet<(u64, String)> = HashSet::new();
            let mut order: Vec<String> = Vec::new();
            let mut per_emoji: HashMap<String, Vec<u64>> = HashMap::new();
            for row in rows {
                let key = (row.uid, row.emoji.clone());
                if !seen_pairs.insert(key) {
                    continue;
                }
                if row.is_deleted {
                    continue;
                }
                if !per_emoji.contains_key(&row.emoji) {
                    order.push(row.emoji.clone());
                }
                per_emoji.entry(row.emoji).or_default().push(row.uid);
            }
            let chips: Vec<ReactionChipVm> = order
                .into_iter()
                .filter_map(|emoji| {
                    per_emoji.remove(&emoji).map(|user_ids| {
                        let mine = my_uid != 0 && user_ids.contains(&my_uid);
                        ReactionChipVm {
                            count: user_ids.len(),
                            mine,
                            user_ids,
                            emoji,
                        }
                    })
                })
                .collect();
            if !chips.is_empty() {
                out.insert(mid, chips);
            }
        }
        Ok(out)
    }

    async fn forward_message(
        &self,
        src_message_id: u64,
        target_channel_id: u64,
        target_channel_type: i32,
    ) -> Result<u64, UiError> {
        if target_channel_id == 0 || target_channel_type <= 0 {
            return Err(UiError::Unknown(format!(
                "invalid forward target: channel_id={} channel_type={}",
                target_channel_id, target_channel_type
            )));
        }

        let src = self
            .sdk
            .get_message_by_id(src_message_id)
            .await
            .map_err(map_sdk_error)?
            .ok_or_else(|| {
                UiError::Unknown(format!("source message not found: {}", src_message_id))
            })?;

        if src.revoked {
            return Err(UiError::Unknown(
                "cannot forward a revoked message".to_string(),
            ));
        }

        let from_uid = self
            .current_uid()
            .await?
            .ok_or_else(|| UiError::Unknown("no active session user".to_string()))?;

        let new_message_id = self
            .sdk
            .create_local_message_with_id(
                NewMessage {
                    channel_id: target_channel_id,
                    channel_type: target_channel_type,
                    from_uid,
                    message_type: src.message_type,
                    content: src.content.clone(),
                    searchable_word: String::new(),
                    setting: 0,
                    extra: src.extra.clone(),
                    mime_type: src.mime_type.clone(),
                    media_downloaded: false,
                    thumb_status: src.thumb_status,
                },
                None,
            )
            .await
            .map_err(map_sdk_error)?;

        self.sdk
            .enqueue_outbound_message(new_message_id, Vec::new())
            .await
            .map_err(map_sdk_error)?;

        if src.mime_type.is_some() {
            let data_dir = std::env::var("PRIVCHAT_DATA_DIR").unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                format!("{}/.privchat-rust", home)
            });
            let root = Path::new(&data_dir);
            let src_dir = privchat_sdk::media_store::get_canonical_message_dir(
                root,
                from_uid,
                src.message_id as i64,
                src.created_at,
            );
            let new_created_at = self
                .sdk
                .get_message_by_id(new_message_id)
                .await
                .map_err(map_sdk_error)?
                .map(|m| m.created_at)
                .unwrap_or(src.created_at);
            if let Ok(dst_dir) = privchat_sdk::media_store::ensure_attachment_dir(
                root,
                from_uid,
                new_message_id as i64,
                new_created_at,
            ) {
                let mut copied_any = false;
                if src_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(&src_dir) {
                        for entry in entries.flatten() {
                            let src_path = entry.path();
                            if src_path.is_file() {
                                if let Some(name) = src_path.file_name() {
                                    let dst_path = dst_dir.join(name);
                                    if std::fs::copy(&src_path, &dst_path).is_ok() {
                                        copied_any = true;
                                    }
                                }
                            }
                        }
                    }
                }
                if copied_any {
                    let _ = self
                        .sdk
                        .update_media_downloaded(new_message_id, true)
                        .await;
                }
            }
        }

        Ok(new_message_id)
    }
}

fn now_millis_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

use std::collections::HashSet;

use async_trait::async_trait;
use iced::futures::SinkExt;
use iced::stream;
use iced::Subscription;
use privchat_protocol::message::ContentMessageType;
use privchat_protocol::rpc::{
    routes, AccountSearchQueryRequest, AccountSearchResponse, AccountUserDetailRequest,
    AccountUserDetailResponse, FriendApplyRequest, FriendApplyResponse, FriendPendingRequest,
    FriendPendingResponse,
};
use privchat_sdk::{NewMessage, PrivchatConfig, PrivchatSdk, SdkEvent, StoredFriend, StoredUser};
use tokio::sync::broadcast::error::RecvError;

use crate::presentation::adapter;
use crate::presentation::vm::{
    AddFriendDetailFieldVm, AddFriendDetailVm, AddFriendSelectionVm, ClientTxnId, FriendListItemVm,
    FriendRequestItemVm, GroupListItemVm, HistoryPageVm, LoginSessionVm, MessageVm, SearchUserVm,
    SessionListItemVm, TimelineSnapshotVm, UiError,
};

fn map_sdk_error(err: privchat_sdk::Error) -> UiError {
    UiError::Unknown(err.to_string())
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

fn field(label: &str, value: impl Into<String>) -> AddFriendDetailFieldVm {
    AddFriendDetailFieldVm {
        label: label.to_string(),
        value: value.into(),
    }
}

#[async_trait]
pub trait SdkBridge: Send + Sync + 'static {
    fn generate_local_message_id(&self) -> Result<ClientTxnId, UiError>;

    async fn restore_session(&self) -> Result<Option<LoginSessionVm>, UiError>;
    async fn load_session_list(&self) -> Result<Vec<SessionListItemVm>, UiError>;
    async fn load_total_unread_count(&self, exclude_muted: bool) -> Result<u32, UiError>;
    async fn logout(&self) -> Result<(), UiError>;
    async fn search_users(&self, query: String) -> Result<Vec<SearchUserVm>, UiError>;
    async fn send_friend_request(
        &self,
        to_user_id: u64,
        remark: Option<String>,
        search_session_id: Option<u64>,
    ) -> Result<u64, UiError>;
    async fn load_friend_list(&self) -> Result<Vec<FriendListItemVm>, UiError>;
    async fn load_group_list(&self) -> Result<Vec<GroupListItemVm>, UiError>;
    async fn load_friend_request_list(&self) -> Result<Vec<FriendRequestItemVm>, UiError>;
    async fn load_add_friend_detail(
        &self,
        item: AddFriendSelectionVm,
    ) -> Result<AddFriendDetailVm, UiError>;

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

    async fn send_text_message(
        &self,
        channel_id: u64,
        channel_type: i32,
        client_txn_id: ClientTxnId,
        body: String,
    ) -> Result<u64, UiError>;

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

    fn subscribe_timeline(&self) -> Subscription<SdkEvent>;
}

#[derive(Clone)]
pub struct PrivchatSdkBridge {
    sdk: PrivchatSdk,
}

#[derive(Clone)]
struct EventSubscriptionSeed {
    sdk: PrivchatSdk,
}

impl std::hash::Hash for EventSubscriptionSeed {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&"privchat_sdk_event_stream", state);
    }
}

fn sdk_event_stream(seed: &EventSubscriptionSeed) -> impl iced::futures::Stream<Item = SdkEvent> {
    let sdk = seed.sdk.clone();
    stream::channel(256, async move |mut output| {
        let mut receiver = sdk.subscribe_events();
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if output.send(event).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!("sdk event stream lagged, skipped={skipped}");
                }
                Err(RecvError::Closed) => {
                    break;
                }
            }
        }
    })
}

impl PrivchatSdkBridge {
    pub fn new() -> Self {
        let server_urls = std::env::var("PRIVCHAT_SERVER_URL")
            .ok()
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|url| !url.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .filter(|urls| !urls.is_empty())
            .unwrap_or_else(|| vec!["quic://127.0.0.1:9001".to_string()]);

        let mut config = PrivchatConfig::from_server_urls(server_urls.clone(), 10);
        if let Some(data_dir) = std::env::var("PRIVCHAT_DATA_DIR")
            .ok()
            .filter(|value| !value.is_empty())
        {
            config.data_dir = data_dir;
        }
        tracing::info!("privchat sdk endpoints: {:?}", server_urls);

        Self {
            sdk: PrivchatSdk::new(config),
        }
    }

    async fn current_uid(&self) -> Result<Option<u64>, UiError> {
        let snapshot = self.sdk.session_snapshot().await.map_err(map_sdk_error)?;
        Ok(snapshot.map(|s| s.user_id))
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
        let remote = self
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
            .ok();

        if local.is_none() && remote.is_none() {
            return Err(UiError::Unknown(format!("用户不存在: {user_id}")));
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
            .map(|value| format!("Weixin ID: {value}"))
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
        }
    }

    fn map_request_item(
        request_user_id: u64,
        request_message: Option<String>,
        local_user: Option<&StoredUser>,
        is_added: bool,
    ) -> FriendRequestItemVm {
        let title = choose_display_name(
            local_user.and_then(|value| value.alias.as_deref()),
            local_user.and_then(|value| value.nickname.as_deref()),
            local_user.and_then(|value| value.username.as_deref()),
            format!("用户 {request_user_id}"),
        );
        let subtitle = request_message
            .as_ref()
            .and_then(|value| non_empty(Some(value.as_str())))
            .unwrap_or_else(|| "请求添加你为好友".to_string());

        FriendRequestItemVm {
            from_user_id: request_user_id,
            title,
            subtitle,
            is_added,
        }
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
        tracing::info!("restore_session: authenticate ok, run bootstrap sync");
        self.sdk.run_bootstrap_sync().await.map_err(map_sdk_error)?;
        tracing::info!("restore_session: bootstrap sync completed");

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
        let bootstrap_completed = self.sdk.is_bootstrap_completed().await.unwrap_or(false);
        tracing::info!(
            "load_session_list: channels={} bootstrap_completed={}",
            channels.len(),
            bootstrap_completed
        );

        Ok(channels
            .iter()
            .map(adapter::map_channel_to_session_item)
            .collect())
    }

    async fn load_total_unread_count(&self, exclude_muted: bool) -> Result<u32, UiError> {
        let unread = self
            .sdk
            .get_total_unread_count(exclude_muted)
            .await
            .map_err(map_sdk_error)?;
        Ok(unread.max(0) as u32)
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
            let local_user = self
                .sdk
                .get_user_by_id(request.from_user_id)
                .await
                .map_err(map_sdk_error)?;
            items.push(Self::map_request_item(
                request.from_user_id,
                request.message,
                local_user.as_ref(),
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
                self.load_user_detail(user_id, "contact_list", format!("friend:{user_id}"))
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
                let mut detail = self
                    .load_user_detail(user_id, "friend_pending", format!("request:{user_id}"))
                    .await?;
                detail.subtitle = format!("待处理好友申请 · UID {user_id}");

                let pending: FriendPendingResponse = self
                    .sdk
                    .rpc_call_typed(
                        routes::friend::PENDING,
                        &FriendPendingRequest { user_id: 0 },
                    )
                    .await
                    .map_err(map_sdk_error)?;
                if let Some(request) = pending
                    .requests
                    .into_iter()
                    .find(|entry| entry.from_user_id == user_id)
                {
                    if let Some(message) = request
                        .message
                        .as_ref()
                        .and_then(|value| non_empty(Some(value)))
                    {
                        detail.fields.push(field("申请消息", message));
                    }
                    detail.fields.push(field("申请时间", request.created_at));
                }
                Ok(detail)
            }
        }
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
            "login_with_password: authenticate ok user_id={}, run bootstrap sync",
            result.user_id
        );
        self.sdk.run_bootstrap_sync().await.map_err(map_sdk_error)?;
        tracing::info!("login_with_password: bootstrap sync completed");

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

        let channels = self
            .sdk
            .list_channels(200, 0)
            .await
            .map_err(map_sdk_error)?;
        let channel = channels
            .iter()
            .find(|c| c.channel_id == channel_id && c.channel_type == channel_type);

        let extra = self
            .sdk
            .get_channel_extra(channel_id, channel_type)
            .await
            .map_err(map_sdk_error)?;

        let unread_marker = adapter::map_unread_marker(channel, extra.as_ref());
        Ok(adapter::map_snapshot_to_vm(
            &snapshot,
            current_uid,
            0,
            unread_marker,
        ))
    }

    async fn send_text_message(
        &self,
        channel_id: u64,
        channel_type: i32,
        client_txn_id: ClientTxnId,
        body: String,
    ) -> Result<u64, UiError> {
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
                    // Keep aligned with privchat-app sendTextWithLocalId semantics.
                    message_type: ContentMessageType::Text as i32,
                    content: body,
                    searchable_word: String::new(),
                    setting: 0,
                    extra: String::new(),
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
        if stored.message_type != ContentMessageType::Text as i32 {
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

    async fn retry_send(
        &self,
        channel_id: u64,
        channel_type: i32,
        client_txn_id: ClientTxnId,
    ) -> Result<(), UiError> {
        let messages = self
            .sdk
            .list_messages(channel_id, channel_type, 200, 0)
            .await
            .map_err(map_sdk_error)?;

        let message = messages
            .into_iter()
            .find(|m| m.local_message_id == Some(client_txn_id))
            .ok_or_else(|| UiError::Unknown("client_txn_id not found in timeline".to_string()))?;

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
        let current_uid = self.current_uid().await?;
        let messages = self
            .sdk
            .list_messages(channel_id, channel_type, 3000, 0)
            .await
            .map_err(map_sdk_error)?;

        let all = adapter::map_history_messages_to_vm(&messages, current_uid, false).items;

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

        Ok(message.map(|stored| adapter::map_stored_message_to_vm(&stored, current_uid, None)))
    }

    async fn mark_read(
        &self,
        channel_id: u64,
        channel_type: i32,
        last_read_pts: u64,
    ) -> Result<(), UiError> {
        self.sdk
            .project_channel_read_cursor(channel_id, channel_type, last_read_pts)
            .await
            .map_err(map_sdk_error)
    }

    fn subscribe_timeline(&self) -> Subscription<SdkEvent> {
        Subscription::run_with(
            EventSubscriptionSeed {
                sdk: self.sdk.clone(),
            },
            sdk_event_stream,
        )
    }
}

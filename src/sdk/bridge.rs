use async_trait::async_trait;
use iced::futures::SinkExt;
use iced::stream;
use iced::Subscription;
use privchat_sdk::{NewMessage, PrivchatConfig, PrivchatSdk, SdkEvent};
use tokio::sync::broadcast::error::RecvError;

use crate::presentation::adapter;
use crate::presentation::vm::{
    ClientTxnId, HistoryPageVm, LoginSessionVm, MessageVm, SessionListItemVm, TimelineSnapshotVm,
    UiError,
};

fn map_sdk_error(err: privchat_sdk::Error) -> UiError {
    UiError::Unknown(err.to_string())
}

#[async_trait]
pub trait SdkBridge: Send + Sync + 'static {
    async fn restore_session(&self) -> Result<Option<LoginSessionVm>, UiError>;
    async fn load_session_list(&self) -> Result<Vec<SessionListItemVm>, UiError>;
    async fn load_total_unread_count(&self, exclude_muted: bool) -> Result<u32, UiError>;
    async fn logout(&self) -> Result<(), UiError>;

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
    ) -> Result<(), UiError>;

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
}

#[async_trait]
impl SdkBridge for PrivchatSdkBridge {
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
    ) -> Result<(), UiError> {
        let current_uid = self
            .current_uid()
            .await?
            .ok_or_else(|| UiError::Unknown("no active session user".to_string()))?;

        let local_message_id = self
            .sdk
            .create_local_message_with_id(
                NewMessage {
                    channel_id,
                    channel_type,
                    from_uid: current_uid,
                    message_type: 1,
                    content: body.clone(),
                    searchable_word: body.clone(),
                    setting: 0,
                    extra: String::new(),
                },
                Some(client_txn_id),
            )
            .await
            .map_err(map_sdk_error)?;

        self.sdk
            .enqueue_outbound_message(local_message_id, body.into_bytes())
            .await
            .map_err(map_sdk_error)?;

        Ok(())
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
            .enqueue_outbound_message(message.message_id, message.content.into_bytes())
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

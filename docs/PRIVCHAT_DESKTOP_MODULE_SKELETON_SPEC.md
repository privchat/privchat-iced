# PrivChat Desktop Module Skeleton Spec

Status: Frozen (skeleton established, compiles)  
Owner: privchat-iced  
Depends on: `PRIVCHAT_DESKTOP_V0_3_FROZEN_UI_CONTRACT.md`

## 1. Directory Structure

```
src/
  main.rs                          # Iced application entry point

  presentation/
    mod.rs
    vm.rs                          # Contract v0.3 VM types
    adapter.rs                     # Thin SDK struct -> VM adaptation

  app/                             # Presentation state + UDF update loop
    mod.rs
    message.rs                     # AppMessage enum
    state.rs                       # AppState + token/txn allocators
    update.rs                      # update() — sole mutation entry point
    subscription.rs                # Subscription wiring
    route.rs                       # Route enum

  sdk/                             # Thin SDK bridge layer
    mod.rs
    bridge.rs                      # SdkBridge trait + PrivchatSdkBridge impl
    events.rs                      # SDK event -> AppMessage mapping

  ui/                              # Pure rendering layer
    mod.rs
    screens/
      mod.rs
      chat.rs
      session_list.rs
      settings.rs
    widgets/
      mod.rs
      timeline_list.rs
      message_bubble.rs
      composer.rs
      unread_banner.rs
```

## 2. Module Responsibilities

| Module | Responsibility | Owns |
|---|---|---|
| `presentation::vm` | Contract type definitions | VM structs/enums/type aliases |
| `presentation::adapter` | Thin display adaptation | `StoredMessage` -> `MessageVm`, snapshot/page mapping |
| `app::message` | UI message surface | `AppMessage` |
| `app::state` | Presentation state | `AppState`, `ChatScreenState`, counters |
| `app::update` | State mutation | `update()`, dual guard, revision gate, patch apply |
| `app::subscription` | Subscription assembly | Iced `Subscription<AppMessage>` |
| `app::route` | Navigation | `Route` |
| `sdk::bridge` | SDK call boundary | open/send/retry/load/mark_read/subscribe |
| `sdk::events` | Event ingress mapping | SDK events -> `AppMessage` |
| `ui::screens::*` | Screen composition | Compose widgets into screens |
| `ui::widgets::*` | Leaf rendering | Stateless or local-widget rendering |

## 3. Dependency Rules

### 3.1 Allowed Dependencies

```
main.rs -> app, sdk, presentation, ui

presentation::vm      -> (nothing)
presentation::adapter -> presentation::vm

app::message      -> presentation::vm
app::state        -> presentation::vm, app::route
app::update       -> app::message, app::state, presentation::vm, sdk::bridge
app::subscription -> app::message, app::state, sdk::bridge, sdk::events
app::route        -> (nothing)

sdk::bridge -> presentation::vm, privchat-sdk
sdk::events -> app::message, presentation::vm

ui::screens::* -> app::message, app::state, presentation::vm, ui::widgets::*
ui::widgets::* -> app::message, app::state, presentation::vm
```

### 3.2 Forbidden Dependencies

| Module | MUST NOT depend on |
|---|---|
| `presentation::*` | `app::*`, `ui::*`, `sdk::bridge` |
| `app::*` | `ui::*`, `privchat-sdk` |
| `sdk::*` | `ui::*`, `app::state` |
| `ui::*` | `sdk::*`, `privchat-sdk` |
| `ui::widgets::*` | `sdk::*` |

### 3.3 Direction Summary

```
ui -> app -> sdk
      |
      -> presentation
sdk/events -> app
```

`privchat-sdk` is consumed only inside `sdk::bridge`.

## 4. Minimum Public API

### 4.1 `app::update`

```rust
pub fn update(
    state: &mut AppState,
    message: AppMessage,
    bridge: &Arc<dyn SdkBridge>,
) -> Task<AppMessage>;
```

### 4.2 `app::subscription`

```rust
pub fn subscription(
    bridge: &Arc<dyn SdkBridge>,
    state: &AppState,
) -> Subscription<AppMessage>;
```

### 4.3 `sdk::bridge::SdkBridge`

```rust
#[async_trait]
pub trait SdkBridge: Send + Sync + 'static {
    async fn open_timeline(&self, channel_id: u64, channel_type: i32)
        -> Result<TimelineSnapshotVm, UiError>;
    async fn send_text_message(&self, channel_id: u64, channel_type: i32,
        client_txn_id: ClientTxnId, body: String) -> Result<(), UiError>;
    async fn retry_send(&self, channel_id: u64, channel_type: i32,
        client_txn_id: ClientTxnId) -> Result<(), UiError>;
    async fn load_history_before(&self, channel_id: u64, channel_type: i32,
        before_server_message_id: Option<u64>, limit: usize) -> Result<HistoryPageVm, UiError>;
    async fn mark_read(&self, channel_id: u64, channel_type: i32,
        last_read_pts: u64) -> Result<(), UiError>;
    fn subscribe_timeline(&self) -> Subscription<SdkEvent>;
}
```

### 4.4 `sdk::events`

```rust
pub fn map_sdk_event_to_app_message(
    event: SdkEvent,
    active_open_token: OpenToken,
) -> Option<AppMessage>;
```

### 4.5 `presentation::adapter`

```rust
pub fn map_stored_message_to_vm(...) -> MessageVm;
pub fn map_snapshot_to_vm(...) -> TimelineSnapshotVm;
pub fn map_history_page_to_vm(...) -> HistoryPageVm;
```

### 4.6 `app::state::AppState`

```rust
impl AppState {
    pub fn new() -> Self;
    pub fn allocate_open_token(&mut self) -> OpenToken;
    pub fn allocate_client_txn_id(&mut self) -> ClientTxnId;
}
```

## 5. Architectural Constraints

1. `update()` is the only state mutation entry point.
2. Widgets never call SDK.
3. `sdk::bridge` is the only module importing `privchat-sdk`.
4. `sdk::events` and `presentation::adapter` are thin transformations only.
5. Business truth stays in SDK; desktop MUST NOT recreate timeline truth, send truth, pagination truth, or persistence logic.
6. `OpenToken` and `ClientTxnId` allocation lives in `AppState`.
7. Dual guard and revision gate live in `app::update`.

## 6. Patch Apply Placement

| Operation | Located in |
|---|---|
| Dual guard check | `app::update` |
| Revision gate check | `app::update` |
| Patch apply (`ReplaceLocalEcho`, `UpsertRemote`, `UpdateSendState`, `RemoveMessage`, `UpdateUnreadMarker`) | `app::update` |
| History prepend + dedup | `app::update` |
| SDK event -> `AppMessage` mapping | `sdk::events` |
| SDK model -> VM adaptation | `presentation::adapter` |

## 7. Change Policy

1. Module names and dependency rules above are frozen for v0.3 implementation.
2. New modules MAY be added only if they do not introduce a second business layer.
3. Any reintroduction of `application/infra/facade` style layering requires explicit v0.4 design approval.

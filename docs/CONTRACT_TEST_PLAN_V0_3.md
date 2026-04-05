# Contract Test Plan — v0.3

Status: Ready for implementation  
Covers: `PRIVCHAT_DESKTOP_V0_3_FROZEN_UI_CONTRACT.md` Section 12 (Invariants) + Section 13 (Contract Tests)

## Conventions

- **SUT**: Presentation `update()` function — receives `AppMessage`, mutates `AppState`, returns side-effects.
- All tests operate on `AppState` directly. No real SDK, no real network.
- Helper builders: `msg_vm_local(client_txn_id, ...)`, `msg_vm_remote(server_message_id, ...)`, `snapshot_vm(...)`, `patch(revision, patch_vm)`.
- Assert macros: `assert_items_eq(timeline, expected_keys)`, `assert_item_count(timeline, n)`, `assert_send_state(timeline, client_txn_id, expected)`.

---

## T-01: Dual Guard — Fast Channel Switch

**Contract ref:** Section 6.2, Section 13.1  
**Invariant:** No stale async event can mutate current active chat.

### Setup
1. Open channel A (`channel_id=1, channel_type=1`), record `open_token=1`.
2. Immediately open channel B (`channel_id=2, channel_type=1`), record `open_token=2`.

### Actions
3. Deliver `ConversationOpened { channel_id=1, channel_type=1, open_token=1, snapshot=... }`.
4. Deliver `TimelinePatched { channel_id=1, channel_type=1, open_token=1, revision=10, patch=UpsertRemote{...} }`.

### Assertions
- `active_chat.channel_id == 2`, `active_chat.open_token == 2`.
- Steps 3 and 4 are silently ignored; `active_chat.timeline.items` is empty or reflects channel B's state only.
- No panic, no error.

---

## T-02: Dual Guard — Same Channel, Stale Token

**Contract ref:** Section 6.2

### Setup
1. Open channel A, `open_token=1`.
2. Select channel A again (e.g., retry or re-select), `open_token=2`.

### Actions
3. Deliver `ConversationOpened { channel_id=A, open_token=1, snapshot=... }`.

### Assertions
- Ignored. `active_chat.open_token == 2`, timeline not initialized from stale snapshot.

---

## T-03: Send — Local Echo Appears Immediately

**Contract ref:** Section 7.2, Section 13.2

### Setup
1. Open channel A, receive snapshot (empty timeline, revision=0).
2. Set `composer.draft = "hello"`.

### Actions
3. Deliver `SendPressed`.

### Assertions
- `timeline.items.len() == 1`.
- Item key is `TimelineItemKey::Local(client_txn_id)`.
- `send_state == Some(Queued)` or `Some(Sending)`.
- `composer.draft == ""`.
- `client_txn_id` is a valid `u64` (non-zero if counter starts at 1).

---

## T-04: Send — Empty Draft Ignored

**Contract ref:** Section 7.2.1

### Setup
1. Open channel A, empty timeline.
2. `composer.draft = ""` (or `"   "`).

### Actions
3. Deliver `SendPressed`.

### Assertions
- `timeline.items.len() == 0`.
- No `client_txn_id` was allocated (counter unchanged).

---

## T-05: ReplaceLocalEcho — No Duplication

**Contract ref:** Section 7.3, Section 8.1, Section 12.1, Section 13.3

### Setup
1. Open channel A, revision=0.
2. Send a message, local echo appears with `client_txn_id=1`.

### Actions
3. Deliver `TimelinePatched { revision=1, patch=ReplaceLocalEcho { client_txn_id=1, remote=msg_vm_remote(server_message_id=100, ...) } }`.

### Assertions
- `timeline.items.len() == 1` (not 2).
- Item key is now `TimelineItemKey::Remote { server_message_id: 100 }`.
- `send_state == Some(Sent)`.
- No item with `TimelineItemKey::Local(1)` remains.

---

## T-06: ReplaceLocalEcho — Duplicate Remote Already Present

**Contract ref:** Section 8.1.4

### Setup
1. Open channel A, revision=0.
2. Send message, local echo `client_txn_id=1`.
3. A live `UpsertRemote` with `server_message_id=100` arrives before `ReplaceLocalEcho` (revision=1).

### Actions
4. Deliver `TimelinePatched { revision=2, patch=ReplaceLocalEcho { client_txn_id=1, remote=msg_vm_remote(server_message_id=100, ...) } }`.

### Assertions
- `timeline.items.len() == 1` (duplicate removed).
- The surviving item has key `Remote { server_message_id: 100 }`.

---

## T-07: ReplaceLocalEcho — Missing server_message_id

**Contract ref:** Section 8.1.2

### Actions
1. Deliver `ReplaceLocalEcho` where `remote.server_message_id == None`.

### Assertions
- Patch is ignored.
- Contract violation is logged.
- Timeline unchanged.

---

## T-08: Send Failure — Same Item Shows Failed State

**Contract ref:** Section 7.4, Section 13.4

### Setup
1. Open channel A. Send message, local echo `client_txn_id=1`, `send_state=Some(Sending)`.

### Actions
2. Deliver `TimelinePatched { revision=1, patch=UpdateSendState { client_txn_id=1, send_state=FailedRetryable { reason: ... } } }`.

### Assertions
- `timeline.items.len() == 1`.
- Same item, key still `Local(1)`.
- `send_state == Some(FailedRetryable { ... })`.

---

## T-09: Retry — Reuses Same Item

**Contract ref:** Section 9, Section 13.4

### Setup
1. Continue from T-08 (item is `FailedRetryable`).

### Actions
2. Deliver `RetrySendPressed { channel_id=A, channel_type=1, client_txn_id=1 }`.
3. Deliver `TimelinePatched { revision=2, patch=UpdateSendState { client_txn_id=1, send_state=Retrying } }`.

### Assertions
- `timeline.items.len() == 1` (no second item created).
- Same key `Local(1)`.
- `send_state == Some(Retrying)`.

---

## T-10: Revision Gate — Equal Revision Ignored

**Contract ref:** Section 3, Section 13.5

### Setup
1. Open channel A, snapshot with revision=5, 3 items.

### Actions
2. Deliver `TimelinePatched { revision=5, patch=UpsertRemote { ... } }`.

### Assertions
- Timeline unchanged (still 3 items, revision still 5).
- Patch silently discarded.

---

## T-11: Revision Gate — Older Revision Ignored

**Contract ref:** Section 3

### Setup
1. Open channel A, snapshot revision=5.
2. Receive live patch revision=6 (applied).

### Actions
3. Deliver `TimelinePatched { revision=4, patch=UpsertRemote { ... } }`.

### Assertions
- Timeline unchanged from step 2. Revision still 6.

---

## T-12: Revision Gate — Does NOT Apply to HistoryLoaded

**Contract ref:** Section 3 (excluded), Section 10.3, Section 13.7

### Setup
1. Open channel A, snapshot revision=10, items with `server_message_id` in [90, 95, 100].

### Actions
2. Deliver `HistoryLoaded { channel_id=A, channel_type=1, open_token=current, page=HistoryPageVm { items=[msg(50), msg(55), msg(60)], oldest_server_message_id=Some(50), has_more_before=true } }`.

### Assertions
- All 3 history items are prepended. Total items = 6.
- Order: [50, 55, 60, 90, 95, 100].
- `oldest_server_message_id == Some(50)`.
- Revision watermark unchanged at 10 (history did not touch it).

---

## T-13: HistoryLoaded — Deduplication by server_message_id

**Contract ref:** Section 10.4, Section 13.8

### Setup
1. Open channel A, timeline has items [90, 95, 100].

### Actions
2. Deliver `HistoryLoaded` with page items `[85, 90, 95]`.

### Assertions
- Only `85` is prepended (90, 95 are duplicates, dropped).
- Total items = 4: [85, 90, 95, 100].
- No duplicate `server_message_id`.

---

## T-14: HistoryLoaded — Dual Guard Rejection

**Contract ref:** Section 6.2, Section 10.3

### Setup
1. Open channel A, `open_token=1`.
2. Switch to channel B, `open_token=2`.

### Actions
3. Deliver `HistoryLoaded { channel_id=A, open_token=1, page=... }`.

### Assertions
- Ignored. Channel B's timeline unchanged.

---

## T-15: Pagination — LoadOlderTriggered Guards

**Contract ref:** Section 10.1

### Test 15a: Already loading
1. Set `is_loading_more = true`.
2. Deliver `LoadOlderTriggered`.
3. Assert: no side-effect, no second request.

### Test 15b: No more history
1. Set `has_more_before = false`.
2. Deliver `LoadOlderTriggered`.
3. Assert: no side-effect.

### Test 15c: Happy path
1. `is_loading_more = false`, `has_more_before = true`.
2. Deliver `LoadOlderTriggered`.
3. Assert: `is_loading_more = true`, pagination request initiated with `before_server_message_id` from `oldest_server_message_id`.

---

## T-16: RetryOpenConversation — New Token, Old Events Rejected

**Contract ref:** Section 6.3, Section 13.9

### Setup
1. Open channel A, `open_token=1`.
2. `ConversationOpenFailed` arrives (token=1).

### Actions
3. Deliver `RetryOpenConversation { channel_id=A, channel_type=1 }`.
4. Record new `open_token=2`.
5. Deliver `ConversationOpened { channel_id=A, open_token=1, snapshot=... }`.
6. Deliver `ConversationOpened { channel_id=A, open_token=2, snapshot=... }`.

### Assertions
- Step 5 is ignored (stale token).
- Step 6 is applied (current token).
- `session_list` selection unchanged throughout.

---

## T-17: UpdateSendState — Rejected for Non-Own Message

**Contract ref:** Section 4.1.3, Section 8.3.2, Section 13.10

### Setup
1. Timeline has a remote message from another user: `is_own=false`, `send_state=None`.

### Actions
2. Deliver `UpdateSendState { client_txn_id=999, send_state=Sending }`.

### Assertions
- Patch ignored (item not found by `client_txn_id`, or if matched, `send_state` is `None`).
- Contract violation logged.

---

## T-18: Terminal State — Sent Cannot Transition

**Contract ref:** Section 8.3 terminal states, Section 13.11

### Setup
1. Local echo `client_txn_id=1`, `send_state=Some(Sent)`.

### Actions
2. Deliver `UpdateSendState { client_txn_id=1, send_state=Sending }`.

### Assertions
- Ignored. `send_state` remains `Some(Sent)`.
- Logged as illegal transition.

---

## T-19: Terminal State — FailedPermanent Cannot Transition

**Contract ref:** Section 8.3

### Setup
1. Local echo `client_txn_id=1`, `send_state=Some(FailedPermanent { ... })`.

### Actions
2. Deliver `UpdateSendState { client_txn_id=1, send_state=Retrying }`.

### Assertions
- Ignored. `send_state` remains `Some(FailedPermanent { ... })`.

---

## T-20: SendState — Legal Transitions (Happy Path)

**Contract ref:** Section 8.3

Test the full success path:

1. `Queued -> Sending` (revision N+1) — applied.
2. `Sending -> Sent` (revision N+2) — applied, terminal.

Test the retry path:

1. `Queued -> Sending` — applied.
2. `Sending -> FailedRetryable` — applied.
3. `FailedRetryable -> Retrying` — applied.
4. `Retrying -> Sending` — applied.
5. `Sending -> Sent` — applied, terminal.

---

## T-21: UpsertRemote — No Duplicate Keys

**Contract ref:** Section 8.2.3

### Setup
1. Timeline has remote item `server_message_id=100`.

### Actions
2. Deliver `UpsertRemote` with same `server_message_id=100`, updated body.

### Assertions
- `timeline.items.len()` unchanged.
- Item body is updated in-place.
- No duplicate key.

### Test 21b: Missing `server_message_id`

**Contract ref:** Section 8.2.1

1. Deliver `UpsertRemote` where `remote.server_message_id == None`.
2. Assert: patch ignored, timeline unchanged, contract violation logged.

---

## T-22: send_state Invariant — is_own Correlation

**Contract ref:** Section 12.6

### Assertions (structural, on every MessageVm construction)
- If `is_own == true`, `send_state.is_some()`.
- If `is_own == false`, `send_state.is_none()`.
- This can be enforced via a `MessageVm::new()` constructor that panics on violation, or a `debug_assert!` in `update()`.

---

## T-23: channel_id + channel_type Pairing

**Contract ref:** Section 12.7

### Assertions (structural)
- Every `AppMessage` variant that carries `channel_id` also carries `channel_type`.
- This is enforced at compile time by the enum definition. Test can be a compile-time check or a code review checkpoint.

---

## T-24: RemoveMessage

**Contract ref:** Section 8.4

### Test 24a: Remove by remote key
1. Timeline has remote item `server_message_id=100`.
2. Deliver `RemoveMessage { key: Remote { server_message_id: 100 } }`.
3. Assert: item marked deleted or physically removed (per rendering policy).

### Test 24b: Remove by local key
1. Timeline has local echo `client_txn_id=1`.
2. Deliver `RemoveMessage { key: Local(1) }`.
3. Assert: item marked deleted or physically removed.

### Test 24c: Remove nonexistent key
1. Deliver `RemoveMessage { key: Remote { server_message_id: 999 } }`.
2. Assert: no-op, no panic.

---

## T-25: UpdateUnreadMarker — Atomic Replace

**Contract ref:** Section 8.5

### Setup
1. `unread_marker = UnreadMarkerVm { first_unread_key: Some(Remote { server_message_id: 90 }), unread_count: 3, has_unread_below_viewport: false }`.

### Actions
2. Deliver `UpdateUnreadMarker { unread_marker: UnreadMarkerVm { first_unread_key: Some(Remote { server_message_id: 95 }), unread_count: 1, has_unread_below_viewport: true } }`.

### Assertions
- `unread_marker` fully replaced. `unread_count == 1`, `first_unread_key == Remote { server_message_id: 95 }`.
- `timeline.items` untouched.

---

## T-26: Pagination — is_loading_more Reset on Failure

**Contract ref:** Section 10.7

### Setup
1. `LoadOlderTriggered` → `is_loading_more = true`.

### Actions
2. Deliver `HistoryLoadFailed { channel_id=A, open_token=current, error=... }`.

### Assertions
- `is_loading_more == false`.
- `has_more_before` unchanged (retry still possible).

---

## T-27: mark_read Uses `pts` Only

**Contract ref:** Section 4.5, Section 11.6, Section 11.7

### Setup
1. Timeline includes visible remote items with `pts=Some(500)` and `pts=Some(520)`.
2. Also includes a local echo item with `pts=None`.
3. `at_bottom=true`, `first_visible_item` points into visible remote range.

### Actions
1. Deliver `ViewportChanged { at_bottom=true, ... }`.
2. Let debounce window elapse in test scheduler.

### Assertions
- `mark_read(channel_id, channel_type, last_read_pts)` is called with `last_read_pts=520`.
- `server_message_id` is never used as a substitute for `pts`.
- Local echo item with `pts=None` does not affect chosen `last_read_pts`.

---

## T-28: mark_read Skips None-pts Windows

**Contract ref:** Section 4.5.3, Section 11.7

### Setup
1. Visible window contains only local echoes (`pts=None` for all visible items).

### Actions
1. Deliver `ViewportChanged { at_bottom=true, ... }`.
2. Let debounce window elapse.

### Assertions
- No `mark_read` SDK call is emitted for that window.
- Once a visible item with `pts=Some(x)` appears and `ViewportChanged` re-fires, `mark_read(..., x)` is emitted.

---

## Coverage Matrix

| Invariant (Section 12) | Covered by |
|---|---|
| 1. No duplicate after replacement | T-05, T-06, T-21 |
| 2. No stale async mutation (dual guard) | T-01, T-02, T-14 |
| 3. No old revision mutation (revision gate) | T-10, T-11, T-12 |
| 4. Retry preserves item continuity | T-08, T-09 |
| 5. History prepend preserves viewport | T-12 (structural assert on anchor) |
| 6. send_state ↔ is_own | T-17, T-22 |
| 7. channel_id + channel_type pairing | T-23 |

| Contract Test (Section 13) | Expanded to |
|---|---|
| 13.1 | T-01, T-02 |
| 13.2 | T-03, T-04 |
| 13.3 | T-05, T-06, T-07 |
| 13.4 | T-08, T-09 |
| 13.5 | T-10, T-11 |
| 13.6 | T-12 (viewport anchor assert) |
| 13.7 | T-12 |
| 13.8 | T-13 |
| 13.9 | T-16 |
| 13.10 | T-17 |
| 13.11 | T-18, T-19 |

| Read Marker Contract (Section 11) | T-27, T-28 |

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tracing::{info, warn};

use crate::app::message::MessageIngressSource;
use crate::presentation::vm::MessageVm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelinePatchKind {
    ReplaceLocalEcho,
    UpsertRemote,
}

impl TimelinePatchKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReplaceLocalEcho => "replace_local_echo",
            Self::UpsertRemote => "upsert_remote",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkReadPtsSource {
    /// pts came from the message visible in the timeline.
    MessagePts,
    /// pts came from the GET_CHANNEL_PTS RPC fallback (message had no pts).
    RpcFallback,
}

/// Global reporting abstraction for SDK/UI message lifecycle events.
/// Keep this layer thin so callers can swap sink implementation later
/// (stats backend, telemetry gateway, local audit, etc.).
pub trait ReportSink: Send + Sync + 'static {
    fn on_sdk_event(&self, event_type: &str);
    fn on_message_ingress(
        &self,
        source: MessageIngressSource,
        message_id: u64,
        channel_id: Option<u64>,
        channel_type: Option<i32>,
    );
    fn on_message_loaded(&self, source: MessageIngressSource, message: &MessageVm);
    fn on_message_missing(
        &self,
        source: MessageIngressSource,
        message_id: u64,
        channel_id: Option<u64>,
        channel_type: Option<i32>,
    );
    fn on_message_load_failed(
        &self,
        source: MessageIngressSource,
        message_id: u64,
        channel_id: Option<u64>,
        channel_type: Option<i32>,
        error: &str,
    );
    fn on_timeline_patch(&self, kind: TimelinePatchKind, channel_id: u64, channel_type: i32);
    fn on_mark_read(
        &self,
        channel_id: u64,
        channel_type: i32,
        pts: u64,
        pts_source: MarkReadPtsSource,
    );
    fn on_history_loaded(
        &self,
        channel_id: u64,
        channel_type: i32,
        item_count: usize,
        elapsed: Duration,
    );
}

#[derive(Default)]
pub struct TracingReportSink;

impl ReportSink for TracingReportSink {
    fn on_sdk_event(&self, event_type: &str) {
        info!("report.sdk_event type={event_type}");
    }

    fn on_message_ingress(
        &self,
        source: MessageIngressSource,
        message_id: u64,
        channel_id: Option<u64>,
        channel_type: Option<i32>,
    ) {
        info!(
            "report.message_ingress source={} message_id={} channel_id={:?} channel_type={:?}",
            source.as_str(),
            message_id,
            channel_id,
            channel_type
        );
    }

    fn on_message_loaded(&self, source: MessageIngressSource, message: &MessageVm) {
        info!(
            "report.message_loaded source={} message_id={} channel_id={} channel_type={} server_message_id={:?}",
            source.as_str(),
            message.message_id,
            message.channel_id,
            message.channel_type,
            message.server_message_id
        );
    }

    fn on_message_missing(
        &self,
        source: MessageIngressSource,
        message_id: u64,
        channel_id: Option<u64>,
        channel_type: Option<i32>,
    ) {
        warn!(
            "report.message_missing source={} message_id={} channel_id={:?} channel_type={:?}",
            source.as_str(),
            message_id,
            channel_id,
            channel_type
        );
    }

    fn on_message_load_failed(
        &self,
        source: MessageIngressSource,
        message_id: u64,
        channel_id: Option<u64>,
        channel_type: Option<i32>,
        error: &str,
    ) {
        warn!(
            "report.message_load_failed source={} message_id={} channel_id={:?} channel_type={:?} error={}",
            source.as_str(),
            message_id,
            channel_id,
            channel_type,
            error
        );
    }

    fn on_timeline_patch(&self, kind: TimelinePatchKind, channel_id: u64, channel_type: i32) {
        info!(
            "report.timeline_patch kind={} channel_id={} channel_type={}",
            kind.as_str(),
            channel_id,
            channel_type,
        );
    }

    fn on_mark_read(
        &self,
        channel_id: u64,
        channel_type: i32,
        pts: u64,
        pts_source: MarkReadPtsSource,
    ) {
        let source_str = match pts_source {
            MarkReadPtsSource::MessagePts => "message_pts",
            MarkReadPtsSource::RpcFallback => "rpc_fallback",
        };
        info!(
            "report.mark_read channel_id={} channel_type={} pts={} pts_source={}",
            channel_id, channel_type, pts, source_str,
        );
    }

    fn on_history_loaded(
        &self,
        channel_id: u64,
        channel_type: i32,
        item_count: usize,
        elapsed: Duration,
    ) {
        info!(
            "report.history_loaded channel_id={} channel_type={} item_count={} elapsed_ms={}",
            channel_id,
            channel_type,
            item_count,
            elapsed.as_millis(),
        );
    }
}

static REPORT_SINK: OnceLock<Arc<dyn ReportSink>> = OnceLock::new();

fn sink() -> &'static Arc<dyn ReportSink> {
    REPORT_SINK.get_or_init(|| Arc::new(TracingReportSink))
}

pub fn install_report_sink(s: Arc<dyn ReportSink>) {
    let _ = REPORT_SINK.set(s);
}

pub fn report_sdk_event(event_type: &str) {
    sink().on_sdk_event(event_type);
}

pub fn report_message_ingress(
    source: MessageIngressSource,
    message_id: u64,
    channel_id: Option<u64>,
    channel_type: Option<i32>,
) {
    sink().on_message_ingress(source, message_id, channel_id, channel_type);
}

pub fn report_message_loaded(source: MessageIngressSource, message: &MessageVm) {
    sink().on_message_loaded(source, message);
}

pub fn report_message_missing(
    source: MessageIngressSource,
    message_id: u64,
    channel_id: Option<u64>,
    channel_type: Option<i32>,
) {
    sink().on_message_missing(source, message_id, channel_id, channel_type);
}

pub fn report_message_load_failed(
    source: MessageIngressSource,
    message_id: u64,
    channel_id: Option<u64>,
    channel_type: Option<i32>,
    error: &str,
) {
    sink().on_message_load_failed(source, message_id, channel_id, channel_type, error);
}

pub fn report_timeline_patch(kind: TimelinePatchKind, channel_id: u64, channel_type: i32) {
    sink().on_timeline_patch(kind, channel_id, channel_type);
}

pub fn report_mark_read(
    channel_id: u64,
    channel_type: i32,
    pts: u64,
    pts_source: MarkReadPtsSource,
) {
    sink().on_mark_read(channel_id, channel_type, pts, pts_source);
}

pub fn report_history_loaded(
    channel_id: u64,
    channel_type: i32,
    item_count: usize,
    elapsed: Duration,
) {
    sink().on_history_loaded(channel_id, channel_type, item_count, elapsed);
}

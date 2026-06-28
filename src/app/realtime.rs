use std::sync::{Arc, Mutex};
use tokio::{runtime::Runtime, sync::mpsc, task::JoinHandle};
use zenapi::client;

use crate::ui::AppWindow;

mod sse;
mod websocket;

#[cfg(test)]
pub(super) use self::sse::{
    MAX_SSE_STREAM_EVENTS, format_sse_exchange, format_sse_stream_events, latest_sse_event_id,
    parse_positive_usize, push_bounded_sse_stream_event, sse_stream_event_done,
    sse_stream_event_last_id, sse_stream_meta, sse_stream_status, sse_stream_tone,
};
#[cfg(test)]
pub(super) use self::websocket::{
    format_websocket_exchange, format_websocket_session_events, parse_websocket_binary_message,
    parse_websocket_protocols, websocket_session_command, websocket_session_status,
};

type WebSocketSessionState =
    Arc<Mutex<Option<mpsc::UnboundedSender<client::WebSocketSessionCommand>>>>;
type SseStreamState = Arc<Mutex<Option<JoinHandle<()>>>>;

pub(super) fn wire_realtime_actions(app: &AppWindow, runtime: Arc<Runtime>) {
    let websocket_session: WebSocketSessionState = Arc::new(Mutex::new(None));
    let sse_stream: SseStreamState = Arc::new(Mutex::new(None));

    websocket::wire_websocket_actions(app, runtime.clone(), websocket_session);
    sse::wire_sse_actions(app, runtime, sse_stream);
}

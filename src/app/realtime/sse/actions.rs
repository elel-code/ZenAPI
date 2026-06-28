use slint::ComponentHandle;
use std::sync::Arc;
use tokio::{runtime::Runtime, sync::mpsc};
use zenapi::client;

use crate::ui::AppWindow;

use super::super::{
    super::{
        clipboard_ui::copy_text_to_clipboard, request_projection::parse_key_value_lines,
        set_response,
    },
    SseStreamState,
};
use super::format::{
    format_sse_exchange, format_sse_stream_events, latest_sse_event_id, parse_positive_usize,
    push_bounded_sse_stream_event, sse_stream_event_done, sse_stream_event_last_id,
    sse_stream_meta, sse_stream_status, sse_stream_tone,
};

fn publish_sse_stream_event(
    weak_app: &slint::Weak<AppWindow>,
    stream_state: &SseStreamState,
    stream_events: &mut Vec<client::SseStreamEvent>,
    event: client::SseStreamEvent,
) -> bool {
    let done = sse_stream_event_done(&event);
    let last_event_id = sse_stream_event_last_id(&event).map(str::to_string);
    let status = sse_stream_status(&event);
    let tone = sse_stream_tone(&event);
    push_bounded_sse_stream_event(stream_events, event);
    let body = format_sse_stream_events(stream_events);
    let meta = sse_stream_meta(stream_events.len());
    let weak_app = weak_app.clone();
    let stream_state = stream_state.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak_app.upgrade() {
            set_response(&app, status, &meta, tone, &body);
            app.set_sse_event_history(body.into());
            if let Some(last_event_id) = last_event_id {
                app.set_sse_last_event_id(last_event_id.into());
            }
            if done {
                app.set_activity("".into());
                app.set_sse_streaming(false);
                if let Ok(mut stream) = stream_state.lock() {
                    *stream = None;
                }
            } else {
                app.set_activity("SSE stream active".into());
                app.set_sse_streaming(true);
            }
        }
    });
    done
}

pub(in crate::app::realtime) fn wire_sse_actions(
    app: &AppWindow,
    runtime: Arc<Runtime>,
    sse_stream: SseStreamState,
) {
    let weak_app = app.as_weak();
    let sse_runtime = runtime.clone();
    app.on_run_sse(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }
        if app.get_sse_streaming() {
            set_response(
                &app,
                "SSE stream active",
                "",
                "neutral",
                "Stop the active SSE stream before running a bounded preview.",
            );
            return;
        }

        let max_events = match parse_positive_usize(&app.get_sse_max_events(), "SSE events") {
            Ok(max_events) => max_events,
            Err(error) => {
                set_response(&app, "SSE failed", "", "error", &error.to_string());
                return;
            }
        };
        let headers = match parse_key_value_lines(&app.get_request_headers(), "SSE header") {
            Ok(headers) => headers,
            Err(error) => {
                set_response(&app, "SSE failed", "", "error", &error.to_string());
                return;
            }
        };
        let url = app.get_url().to_string();

        app.set_busy(true);
        app.set_sse_event_history("".into());
        app.set_activity("Collecting SSE events".into());
        set_response(
            &app,
            "SSE collecting",
            "",
            "busy",
            &format!("Waiting for {max_events} event(s)\n\n{url}"),
        );

        let weak_app = app.as_weak();
        sse_runtime.spawn(async move {
            let result = client::collect_sse_events_with_headers(&url, max_events, headers).await;
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak_app.upgrade() else {
                    return;
                };
                match result {
                    Ok(exchange) => {
                        if let Some(last_event_id) = latest_sse_event_id(&exchange.events) {
                            app.set_sse_last_event_id(last_event_id.into());
                        }
                        let body = format_sse_exchange(&exchange);
                        app.set_sse_event_history(body.clone().into());
                        set_response(
                            &app,
                            "SSE complete",
                            &format!(
                                "{} ms / {} events",
                                exchange.elapsed_ms,
                                exchange.events.len()
                            ),
                            "success",
                            &body,
                        );
                    }
                    Err(error) => set_response(&app, "SSE failed", "", "error", &error.to_string()),
                }
                app.set_activity("".into());
                app.set_busy(false);
            });
        });
    });

    let weak_app = app.as_weak();
    let open_sse_runtime = runtime;
    let open_sse_stream = sse_stream.clone();
    app.on_open_sse_stream(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let url = app.get_url().to_string();
        let headers = match parse_key_value_lines(&app.get_request_headers(), "SSE header") {
            Ok(headers) => headers,
            Err(error) => {
                set_response(&app, "SSE stream failed", "", "error", &error.to_string());
                return;
            }
        };
        let last_event_id = app.get_sse_last_event_id().to_string();
        let last_event_id = if last_event_id.trim().is_empty() {
            None
        } else {
            Some(last_event_id.trim().to_string())
        };
        let options = client::SseSubscriptionOptions {
            last_event_id,
            headers,
            ..Default::default()
        };

        let Ok(mut stream) = open_sse_stream.lock() else {
            return;
        };
        if stream.is_some() {
            set_response(
                &app,
                "SSE stream active",
                "",
                "neutral",
                "Stop the current SSE stream before opening another.",
            );
            return;
        }

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        app.set_sse_streaming(true);
        app.set_sse_event_history("".into());
        app.set_activity("SSE stream active".into());
        set_response(
            &app,
            "SSE streaming",
            "",
            "busy",
            &format!("Connecting\n\n{url}"),
        );

        let weak_app = app.as_weak();
        let stream_state = open_sse_stream.clone();
        let handle = open_sse_runtime.spawn(async move {
            let subscription = client::run_sse_subscription_with_options(url, options, event_tx);
            tokio::pin!(subscription);

            let mut stream_events = Vec::new();
            let mut terminal_seen = false;

            loop {
                tokio::select! {
                    () = &mut subscription => {
                        break;
                    }
                    event = event_rx.recv() => {
                        let Some(event) = event else {
                            break;
                        };
                        if publish_sse_stream_event(
                            &weak_app,
                            &stream_state,
                            &mut stream_events,
                            event,
                        ) {
                            terminal_seen = true;
                            break;
                        }
                    }
                }
            }

            while let Some(event) = event_rx.recv().await {
                if publish_sse_stream_event(&weak_app, &stream_state, &mut stream_events, event) {
                    terminal_seen = true;
                }
            }

            if !terminal_seen {
                let weak_app = weak_app.clone();
                let stream_state = stream_state.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak_app.upgrade() {
                        if app.get_sse_streaming() {
                            set_response(
                                &app,
                                "SSE stopped",
                                "",
                                "neutral",
                                "SSE subscription ended.",
                            );
                        }
                        app.set_activity("".into());
                        app.set_sse_streaming(false);
                        if let Ok(mut stream) = stream_state.lock() {
                            *stream = None;
                        }
                    }
                });
            }
        });
        *stream = Some(handle);
    });

    let weak_app = app.as_weak();
    let close_sse_stream = sse_stream;
    app.on_close_sse_stream(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let Some(handle) = close_sse_stream
            .lock()
            .ok()
            .and_then(|mut stream| stream.take())
        else {
            app.set_sse_streaming(false);
            set_response(&app, "SSE idle", "", "neutral", "No SSE stream is active.");
            return;
        };

        handle.abort();
        app.set_activity("".into());
        app.set_sse_streaming(false);
        set_response(
            &app,
            "SSE stopped",
            "",
            "neutral",
            "SSE subscription stopped.",
        );
    });

    let weak_app = app.as_weak();
    app.on_copy_sse_events(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        let history = app.get_sse_event_history().to_string();
        if history.is_empty() {
            app.set_activity("No SSE events to copy".into());
            return;
        }

        match copy_text_to_clipboard(&history) {
            Ok(()) => app.set_activity("Copied SSE events".into()),
            Err(error) => app.set_activity(format!("Copy failed: {error}").into()),
        }
    });

    let weak_app = app.as_weak();
    app.on_clear_sse_events(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_sse_streaming() {
            set_response(
                &app,
                "SSE stream active",
                "",
                "neutral",
                "Stop the active SSE stream before clearing events.",
            );
            return;
        }

        app.set_sse_event_history("".into());
        set_response(&app, "SSE history cleared", "", "neutral", "No SSE events.");
    });
}

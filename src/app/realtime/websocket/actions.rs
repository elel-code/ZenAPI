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
    WebSocketSessionState,
};
use super::format::{
    format_websocket_exchange, format_websocket_session_events, normalize_websocket_message_mode,
    parse_websocket_protocols, websocket_message_mode_label, websocket_session_command,
    websocket_session_event_done, websocket_session_status, websocket_session_tone,
};

pub(in crate::app::realtime) fn wire_websocket_actions(
    app: &AppWindow,
    runtime: Arc<Runtime>,
    websocket_session: WebSocketSessionState,
) {
    let weak_app = app.as_weak();
    let open_runtime = runtime.clone();
    let open_session = websocket_session.clone();
    app.on_open_websocket(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let url = app.get_url().to_string();
        let headers = match parse_key_value_lines(&app.get_request_headers(), "WebSocket header") {
            Ok(headers) => headers,
            Err(error) => {
                set_response(
                    &app,
                    "WebSocket open failed",
                    "",
                    "error",
                    &error.to_string(),
                );
                return;
            }
        };
        let protocols = parse_websocket_protocols(&app.get_websocket_protocols());
        let options = client::WebSocketSessionOptions { headers, protocols };

        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let Ok(mut session) = open_session.lock() else {
            return;
        };
        if session.is_some() {
            set_response(
                &app,
                "WebSocket already open",
                "",
                "neutral",
                "Close the current WebSocket session before opening another.",
            );
            return;
        }
        *session = Some(command_tx);
        drop(session);

        app.set_activity("Opening WebSocket session".into());
        set_response(
            &app,
            "WebSocket opening",
            "",
            "busy",
            &format!("Connecting\n\n{url}"),
        );
        app.set_websocket_event_history("".into());

        let weak_app = app.as_weak();
        let session_state = open_session.clone();
        open_runtime.spawn(async move {
            let session_task = tokio::spawn(client::run_websocket_session_with_options(
                url, options, command_rx, event_tx,
            ));
            let mut events = Vec::new();

            while let Some(event) = event_rx.recv().await {
                let done = websocket_session_event_done(&event);
                events.push(event);
                let body = format_websocket_session_events(&events);
                let status = websocket_session_status(events.last().expect("session event"));
                let tone = websocket_session_tone(events.last().expect("session event"));
                let meta = format!("{} events", events.len());
                let weak_app = weak_app.clone();
                let session_state = session_state.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak_app.upgrade() {
                        set_response(&app, status, &meta, tone, &body);
                        app.set_websocket_event_history(body.into());
                        if done {
                            app.set_activity("".into());
                            if let Ok(mut session) = session_state.lock() {
                                *session = None;
                            }
                        } else {
                            app.set_activity("WebSocket session open".into());
                        }
                    }
                });
            }

            let _ = session_task.await;
        });
    });

    let weak_app = app.as_weak();
    let websocket_runtime = runtime.clone();
    let send_session = websocket_session.clone();
    app.on_run_websocket(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let url = app.get_url().to_string();
        let message = app.get_realtime_message().to_string();
        let message_mode = app.get_websocket_message_mode().to_string();
        if let Some(sender) = send_session.lock().ok().and_then(|session| session.clone()) {
            let command = match websocket_session_command(&message_mode, &message) {
                Ok(command) => command,
                Err(error) => {
                    set_response(
                        &app,
                        "WebSocket send failed",
                        "",
                        "error",
                        &error.to_string(),
                    );
                    return;
                }
            };
            match sender.send(command) {
                Ok(()) => {
                    set_response(
                        &app,
                        "WebSocket queued",
                        "",
                        "busy",
                        &format!(
                            "Queued {} session message\n\n{message}",
                            websocket_message_mode_label(&message_mode)
                        ),
                    );
                }
                Err(error) => {
                    set_response(
                        &app,
                        "WebSocket send failed",
                        "",
                        "error",
                        &error.to_string(),
                    );
                }
            }
            return;
        }

        if normalize_websocket_message_mode(&message_mode) == "binary" {
            set_response(
                &app,
                "WebSocket session required",
                "",
                "neutral",
                "Open a WebSocket session before sending binary messages.",
            );
            return;
        }
        if !parse_websocket_protocols(&app.get_websocket_protocols()).is_empty() {
            set_response(
                &app,
                "WebSocket session required",
                "",
                "neutral",
                "Open a WebSocket session before using subprotocols.",
            );
            return;
        }

        app.set_busy(true);
        app.set_activity("Sending WebSocket message".into());
        set_response(
            &app,
            "WebSocket sending",
            "",
            "busy",
            &format!("Connecting\n\n{url}"),
        );
        app.set_websocket_event_history("".into());

        let weak_app = app.as_weak();
        websocket_runtime.spawn(async move {
            let result = client::send_websocket_message(&url, &message).await;
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak_app.upgrade() else {
                    return;
                };
                match result {
                    Ok(exchange) => {
                        let body = format_websocket_exchange(&exchange);
                        app.set_websocket_event_history(body.clone().into());
                        set_response(
                            &app,
                            "WebSocket complete",
                            &format!("{} ms", exchange.elapsed_ms),
                            "success",
                            &body,
                        );
                    }
                    Err(error) => {
                        set_response(&app, "WebSocket failed", "", "error", &error.to_string())
                    }
                }
                app.set_activity("".into());
                app.set_busy(false);
            });
        });
    });

    let weak_app = app.as_weak();
    let close_session = websocket_session.clone();
    app.on_close_websocket(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let Some(sender) = close_session
            .lock()
            .ok()
            .and_then(|mut session| session.take())
        else {
            set_response(
                &app,
                "WebSocket idle",
                "",
                "neutral",
                "No WebSocket session is open.",
            );
            return;
        };

        match sender.send(client::WebSocketSessionCommand::Close) {
            Ok(()) => {
                app.set_activity("Closing WebSocket session".into());
                set_response(&app, "WebSocket closing", "", "busy", "Close command sent.");
            }
            Err(error) => {
                app.set_activity("".into());
                set_response(
                    &app,
                    "WebSocket close failed",
                    "",
                    "error",
                    &error.to_string(),
                );
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_copy_websocket_events(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        let history = app.get_websocket_event_history().to_string();
        if history.is_empty() {
            app.set_activity("No WebSocket events to copy".into());
            return;
        }

        match copy_text_to_clipboard(&history) {
            Ok(()) => app.set_activity("Copied WebSocket events".into()),
            Err(error) => app.set_activity(format!("Copy failed: {error}").into()),
        }
    });

    let weak_app = app.as_weak();
    app.on_clear_websocket_events(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        app.set_websocket_event_history("".into());
        set_response(
            &app,
            "WebSocket history cleared",
            "",
            "neutral",
            "No WebSocket events.",
        );
    });
}

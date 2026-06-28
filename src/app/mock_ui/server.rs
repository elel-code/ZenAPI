use slint::ComponentHandle;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use zenapi::mock_server::MockServer;

use crate::ui::AppWindow;

use super::super::{AppState, ServerAction, set_response};
use super::{filtered_mock_log_model, mock_log_model, push_mock_log};

pub(in crate::app) fn wire_mock_server(
    app: &AppWindow,
    runtime: Arc<Runtime>,
    state: Arc<Mutex<AppState>>,
) {
    let weak_app = app.as_weak();
    app.on_toggle_server(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        app.set_busy(true);
        app.set_activity("Updating mock server".into());
        let weak_app = app.as_weak();
        let state = state.clone();

        runtime.spawn(async move {
            let action = {
                let mut guard = state.lock().expect("state lock");
                guard.next_server_action()
            };

            match action {
                ServerAction::Stop(server) => {
                    server.stop().await;
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(app) = weak_app.upgrade() {
                            app.set_server_running(false);
                            app.set_server_status("Stopped".into());
                            app.set_activity("".into());
                            app.set_busy(false);
                        }
                    });
                }
                ServerAction::Start(routes) => {
                    if routes.is_empty() {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = weak_app.upgrade() {
                                app.set_server_running(false);
                                app.set_server_status("No routes".into());
                                set_response(
                                    &app,
                                    "Mock needs routes",
                                    "",
                                    "error",
                                    "Import an OpenAPI file before starting the mock server.",
                                );
                                app.set_activity("".into());
                                app.set_busy(false);
                            }
                        });
                        return;
                    }

                    let (log_tx, mut log_rx) = tokio::sync::mpsc::unbounded_channel();
                    match MockServer::start_with_logs(routes, 8080, log_tx).await {
                        Ok(server) => {
                            let addr = server.addr();
                            if let Ok(mut guard) = state.lock() {
                                guard.server = Some(server);
                                guard.mock_logs.clear();
                            }

                            let log_state = state.clone();
                            let log_weak_app = weak_app.clone();
                            tokio::spawn(async move {
                                while let Some(log) = log_rx.recv().await {
                                    let logs = if let Ok(mut guard) = log_state.lock() {
                                        push_mock_log(&mut guard.mock_logs, log, 50);
                                        Some(guard.mock_logs.clone())
                                    } else {
                                        None
                                    };
                                    if let Some(logs) = logs {
                                        let weak_app = log_weak_app.clone();
                                        let _ = slint::invoke_from_event_loop(move || {
                                            if let Some(app) = weak_app.upgrade() {
                                                app.set_mock_logs(filtered_mock_log_model(
                                                    &logs,
                                                    &app.get_mock_log_filter(),
                                                ));
                                            }
                                        });
                                    }
                                }
                            });

                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(app) = weak_app.upgrade() {
                                    app.set_server_running(true);
                                    app.set_server_status(addr.to_string().into());
                                    app.set_mock_logs(mock_log_model(&[]));
                                    app.set_activity("".into());
                                    app.set_busy(false);
                                }
                            });
                        }
                        Err(error) => {
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(app) = weak_app.upgrade() {
                                    app.set_server_running(false);
                                    app.set_server_status("Failed".into());
                                    set_response(
                                        &app,
                                        "Mock server failed",
                                        "",
                                        "error",
                                        &error.to_string(),
                                    );
                                    app.set_activity("".into());
                                    app.set_busy(false);
                                }
                            });
                        }
                    }
                }
            }
        });
    });
}

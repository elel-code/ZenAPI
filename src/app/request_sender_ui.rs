use slint::ComponentHandle;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use zenapi::{
    assertions::evaluate_response_assertions,
    client,
    history::{HistoryRequest, HistoryResponse},
};

use crate::ui::AppWindow;

use super::{
    AppState,
    history_ui::{filtered_history_model, history_request, success_history_response},
    request_editor_ui::request_projection_input,
    request_projection::build_codegen_request_projection,
    response_assertion_parser::parse_response_assertions,
    response_format::{
        format_cookies, format_headers, response_body_with_assertions,
        response_status_with_assertions, response_tone_with_assertions, truncate_preview,
    },
    set_response, set_response_payload,
};

pub(super) fn wire_request_sender(
    app: &AppWindow,
    runtime: Arc<Runtime>,
    state: Arc<Mutex<AppState>>,
) {
    let weak_app = app.as_weak();
    app.on_send_request(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let input = request_projection_input(&app);
        let request = match build_codegen_request_projection(&input) {
            Ok(request) => request,
            Err(error) => {
                set_response(&app, "Build failed", "", "error", &error.to_string());
                return;
            }
        };
        let request_tests = app.get_request_tests().to_string();
        let assertions = match parse_response_assertions(&request_tests) {
            Ok(assertions) => assertions,
            Err(error) => {
                set_response(&app, "Tests failed", "", "error", &error.to_string());
                return;
            }
        };

        let method = request.method.clone();
        let url = request.url.clone();
        let headers = request.headers.clone();
        let query_params = request.query_params.clone();
        let body = request.body.clone();
        let history_snapshot = history_request(&request, &input, &request_tests);

        app.set_busy(true);
        app.set_activity("Sending request".into());
        set_response(
            &app,
            "Sending",
            "",
            "busy",
            &format!("Waiting for response\n\n{method} {url}"),
        );

        let weak_app = app.as_weak();
        let state = state.clone();
        runtime.spawn(async move {
            let result =
                client::send_request_with_body(&method, &url, &headers, &query_params, body).await;
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak_app.upgrade() else {
                    return;
                };
                match result {
                    Ok(response) => {
                        let response_headers = format_headers(&response.headers);
                        let response_cookies = format_cookies(&response.headers);
                        let assertion_results =
                            evaluate_response_assertions(&response, &assertions);
                        let response_status =
                            response_status_with_assertions(response.status, &assertion_results);
                        let response_meta =
                            format!("{} ms / {} B", response.elapsed_ms, response.body_bytes);
                        let response_tone =
                            response_tone_with_assertions(response.status, &assertion_results);
                        let response_body =
                            response_body_with_assertions(&response.body, &assertion_results);
                        let response_raw_body =
                            response_body_with_assertions(&response.raw_body, &assertion_results);
                        record_history(
                            &state,
                            history_snapshot.clone(),
                            success_history_response(&response),
                        );
                        if let Ok(state) = state.lock() {
                            app.set_history_rows(filtered_history_model(
                                &state.history,
                                &app.get_history_filter(),
                            ));
                        }
                        set_response(
                            &app,
                            &response_status,
                            &response_meta,
                            response_tone,
                            &response_body,
                        );
                        set_response_payload(
                            &app,
                            &response_body,
                            &response_raw_body,
                            &response_headers,
                            &response_cookies,
                        );
                    }
                    Err(error) => {
                        let body = error.to_string();
                        record_history(
                            &state,
                            history_snapshot.clone(),
                            HistoryResponse {
                                status: "ERR".to_string(),
                                meta: String::new(),
                                body_preview: truncate_preview(&body, 1200),
                            },
                        );
                        if let Ok(state) = state.lock() {
                            app.set_history_rows(filtered_history_model(
                                &state.history,
                                &app.get_history_filter(),
                            ));
                        }
                        set_response(&app, "Request failed", "", "error", &error.to_string());
                    }
                }
                app.set_activity("".into());
                app.set_busy(false);
            });
        });
    });
}

fn record_history(
    state: &Arc<Mutex<AppState>>,
    request: HistoryRequest,
    response: HistoryResponse,
) {
    if let Ok(mut state) = state.lock() {
        state.history.record(request, response);
        state.save_history_to_disk();
    }
}

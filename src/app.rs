use anyhow::{Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde_json::Value;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::sync::{Arc, Mutex};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tokio::runtime::Runtime;
use zenapi::{
    assertions::{
        ResponseAssertion, ResponseAssertionKind, ResponseAssertionResult,
        evaluate_response_assertions,
    },
    client::{self, ClientResponse, RequestBody},
    codegen::{CodegenRequest, SnippetLanguage, generate_snippet},
    collection_runner::{
        CollectionRunResult, CollectionRunSummary, FailureStrategy, RunnerOptions, run_collection,
    },
    collections::{ApiCollection, CollectionBody, CollectionItem, CollectionRequest, NameValue},
    grpc::{GrpcRequestDraft, build_grpc_request_draft},
    history::{HistoryRequest, HistoryResponse, RequestHistory},
    mock_server::{MockRequestLog, MockServer},
    openapi::{ApiRoute, ApiSpec, load_openapi_file},
    pre_request::{execute_pre_request_actions, resolve_codegen_request_templates},
    variables::{Variable, VariableStore, replace_variables},
};

use crate::ui::{AppWindow, CollectionRow, HistoryRow, MockLogRow, RouteRow, RunnerRow};

const HISTORY_FILE_NAME: &str = ".zenapi-history.json";

pub fn run() -> Result<()> {
    let runtime = Arc::new(Runtime::new()?);
    let mut initial_state = AppState::default();
    let history_load_error = initial_state.load_history_from_disk().err();
    let app = AppWindow::new().map_err(|err| anyhow!(err.to_string()))?;
    app.set_history_rows(filtered_history_model(&initial_state.history, ""));
    if let Some(error) = history_load_error {
        set_response(
            &app,
            "History load failed",
            HISTORY_FILE_NAME,
            "error",
            &error.to_string(),
        );
    }

    let state = Arc::new(Mutex::new(initial_state));

    wire_import(&app, runtime.clone(), state.clone());
    wire_route_filter(&app, state.clone());
    wire_route_selection(&app, state.clone());
    wire_history_selection(&app, state.clone());
    wire_history_actions(&app, state.clone());
    wire_collection_actions(&app, state.clone());
    wire_mock_log_filter(&app, state.clone());
    wire_request_sender(&app, runtime.clone(), state.clone());
    wire_codegen(&app);
    wire_collection_runner(&app, runtime.clone(), state.clone());
    wire_realtime_actions(&app, runtime.clone());
    wire_grpc_draft(&app);
    wire_mock_server(&app, runtime, state);

    app.run().map_err(|err| anyhow!(err.to_string()))
}

struct AppState {
    routes: Vec<ApiRoute>,
    visible_routes: Vec<ApiRoute>,
    collection: ApiCollection,
    history: RequestHistory,
    history_path: PathBuf,
    mock_logs: Vec<MockRequestLog>,
    server: Option<MockServer>,
}

enum ServerAction {
    Start(Vec<ApiRoute>),
    Stop(MockServer),
}

struct RequestProjectionInput {
    method: String,
    url: String,
    query_params: String,
    headers: String,
    auth_mode: String,
    auth_config: String,
    body_mode: String,
    body: String,
    graphql_variables: String,
    pre_request_script: String,
    global_variables: String,
    environment_name: String,
    environment_variables: String,
}

fn request_projection_input(app: &AppWindow) -> RequestProjectionInput {
    RequestProjectionInput {
        method: app.get_method().to_string(),
        url: app.get_url().to_string(),
        query_params: app.get_query_params().to_string(),
        headers: app.get_request_headers().to_string(),
        auth_mode: app.get_auth_mode().to_string(),
        auth_config: app.get_auth_config().to_string(),
        body_mode: app.get_body_mode().to_string(),
        body: app.get_request_body().to_string(),
        graphql_variables: app.get_graphql_variables().to_string(),
        pre_request_script: app.get_pre_request_script().to_string(),
        global_variables: app.get_global_variables().to_string(),
        environment_name: app.get_environment_name().to_string(),
        environment_variables: app.get_environment_variables().to_string(),
    }
}

impl AppState {
    fn load_history_from_disk(&mut self) -> Result<()> {
        if !self.history_path.exists() {
            return Ok(());
        }
        self.history = RequestHistory::load_file(&self.history_path)?;
        Ok(())
    }

    fn save_history_to_disk(&self) {
        let _ = self.history.save_file(&self.history_path);
    }

    fn next_server_action(&mut self) -> ServerAction {
        if let Some(server) = self.server.take() {
            ServerAction::Stop(server)
        } else {
            ServerAction::Start(self.routes.clone())
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            routes: Vec::new(),
            visible_routes: Vec::new(),
            collection: ApiCollection::new("ZenAPI Collection"),
            history: RequestHistory::default(),
            history_path: PathBuf::from(HISTORY_FILE_NAME),
            mock_logs: Vec::new(),
            server: None,
        }
    }
}

fn wire_import(app: &AppWindow, runtime: Arc<Runtime>, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    app.on_import_openapi(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let path = path.trim();
        if path.is_empty() {
            set_response(
                &app,
                "Import needs a file path",
                "",
                "error",
                "Enter a local OpenAPI or Swagger JSON/YAML file path.",
            );
            return;
        }

        app.set_busy(true);
        app.set_activity("Importing OpenAPI spec".into());

        match load_openapi_file(path) {
            Ok(spec) => {
                let routes = spec.routes.clone();
                let stopped_server = state.lock().ok().and_then(|mut state| {
                    let stopped_server = state.server.take();
                    state.routes = routes.clone();
                    state.visible_routes = routes.clone();
                    stopped_server
                });

                let spec_name = display_spec_name(&spec);
                let spec_label = display_spec_label(path);
                let weak_app = app.as_weak();
                runtime.spawn(async move {
                    if let Some(server) = stopped_server {
                        server.stop().await;
                    }

                    let _ = slint::invoke_from_event_loop(move || {
                        let Some(app) = weak_app.upgrade() else {
                            return;
                        };

                        app.set_routes(route_model(&routes));
                        app.set_selected_route(-1);
                        app.set_route_filter("".into());
                        app.set_total_route_count(routes.len() as i32);
                        app.set_spec_label(spec_label.into());
                        app.set_spec_loaded(true);
                        app.set_server_running(false);
                        app.set_server_status(if routes.is_empty() {
                            "No mock routes".into()
                        } else {
                            "Ready".into()
                        });
                        set_response(
                            &app,
                            &format!("Imported {spec_name}"),
                            &format!("{} routes", routes.len()),
                            "success",
                            &format!("Ready: {} routes parsed.", routes.len()),
                        );
                        app.set_activity("".into());
                        app.set_busy(false);
                    });
                });
            }
            Err(error) => {
                set_response(&app, "Import failed", "", "error", &error.to_string());
                app.set_activity("".into());
                app.set_busy(false);
            }
        }
    });
}

fn wire_route_filter(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    app.on_filter_routes(move |query| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        let filtered = state
            .lock()
            .map(|mut state| {
                state.visible_routes = filter_routes(&state.routes, query.as_str());
                state.visible_routes.clone()
            })
            .unwrap_or_default();

        app.set_routes(route_model(&filtered));
        app.set_selected_route(-1);
    });
}

fn wire_route_selection(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    app.on_select_route(move |index| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        let route = state
            .lock()
            .ok()
            .and_then(|state| state.visible_routes.get(index as usize).cloned());

        if let Some(route) = route {
            app.set_method(SharedString::from(route.method));
            app.set_url(SharedString::from(format!(
                "http://127.0.0.1:8080{}",
                route.path
            )));
            app.set_query_params("".into());
            app.set_request_headers("Accept: application/json".into());
            app.set_body_mode(default_body_mode(&app.get_method()).into());
            app.set_request_body(default_request_body(&app.get_method()).into());
            app.set_graphql_variables("{}".into());
            set_response(
                &app,
                "Route selected",
                route.summary.as_str(),
                "neutral",
                &pretty_json(&route.mock_body),
            );
        }
    });
}

fn wire_history_selection(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    app.on_select_history(move |id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }
        if id < 0 {
            return;
        }

        let entry = state
            .lock()
            .ok()
            .and_then(|state| state.history.find(id as u64).cloned());

        if let Some(entry) = entry {
            app.set_method(entry.request.method.into());
            app.set_url(entry.request.url.into());
            app.set_query_params(format_key_value_preview(&entry.request.query_params).into());
            app.set_request_headers(format_key_value_preview(&entry.request.headers).into());
            app.set_auth_mode(normalized_history_auth_mode(&entry.request.auth_mode).into());
            app.set_auth_config(entry.request.auth_config.into());
            app.set_body_mode(entry.request.body_kind.into());
            app.set_request_body(entry.request.body_preview.into());
            app.set_graphql_variables("{}".into());
            app.set_pre_request_script(entry.request.pre_request_script.into());
            app.set_request_tests(entry.request.request_tests.into());
            set_response(
                &app,
                "History restored",
                &entry.response.status,
                "neutral",
                &entry.response.body_preview,
            );
        }
    });
}

fn wire_history_actions(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    let filter_state = state.clone();
    app.on_filter_history(move |query| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let model = filter_state
            .lock()
            .ok()
            .map(|state| filtered_history_model(&state.history, query.as_str()))
            .unwrap_or_else(empty_history_model);
        app.set_history_rows(model);
    });

    let weak_app = app.as_weak();
    let delete_state = state.clone();
    app.on_delete_history(move |id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() || id < 0 {
            return;
        }

        let result = delete_state.lock().ok().and_then(|mut state| {
            let model = delete_history_entry(
                &mut state.history,
                id as u64,
                app.get_history_filter().as_str(),
            );
            if model.is_some() {
                state.save_history_to_disk();
            }
            model
        });

        if let Some(model) = result {
            app.set_history_rows(model);
            set_response(
                &app,
                "History deleted",
                "",
                "neutral",
                "The selected request was removed from history.",
            );
        }
    });

    let weak_app = app.as_weak();
    app.on_clear_history(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }
        if let Ok(mut state) = state.lock() {
            state.history.clear();
            state.save_history_to_disk();
        }
        app.set_history_filter("".into());
        app.set_history_rows(empty_history_model());
        set_response(
            &app,
            "History cleared",
            "",
            "neutral",
            "Request history is empty.",
        );
    });
}

fn wire_collection_actions(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    let import_state = state.clone();
    app.on_import_collection(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let path = path.trim();
        if path.is_empty() {
            app.set_collection_status("Collection path required".into());
            set_response(
                &app,
                "Collection import failed",
                "",
                "error",
                "Enter a local native or Postman collection JSON file path.",
            );
            return;
        }

        match ApiCollection::load_file(path) {
            Ok(collection) => {
                let name = collection.name.clone();
                let request_count = count_collection_requests(&collection.items);
                let rows = collection_model(&collection);
                if let Ok(mut state) = import_state.lock() {
                    state.collection = collection;
                }
                app.set_collection_name(name.clone().into());
                app.set_collection_rows(rows);
                app.set_collection_status(format!("Loaded {request_count} requests").into());
                set_response(
                    &app,
                    "Collection loaded",
                    path,
                    "success",
                    &format!("{name}\n{request_count} requests"),
                );
            }
            Err(error) => {
                app.set_collection_status("Load failed".into());
                set_response(
                    &app,
                    "Collection import failed",
                    path,
                    "error",
                    &error.to_string(),
                );
            }
        }
    });

    let weak_app = app.as_weak();
    let save_state = state.clone();
    app.on_save_collection(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let path = path.trim();
        if path.is_empty() {
            app.set_collection_status("Collection path required".into());
            set_response(
                &app,
                "Collection save failed",
                "",
                "error",
                "Enter a target native collection JSON file path.",
            );
            return;
        }

        let Some(collection) = save_state.lock().ok().map(|state| state.collection.clone()) else {
            return;
        };
        match collection.save_file(path) {
            Ok(()) => {
                app.set_collection_status("Saved native JSON".into());
                set_response(&app, "Collection saved", path, "success", &collection.name);
            }
            Err(error) => {
                app.set_collection_status("Save failed".into());
                set_response(
                    &app,
                    "Collection save failed",
                    path,
                    "error",
                    &error.to_string(),
                );
            }
        }
    });

    let weak_app = app.as_weak();
    let export_state = state.clone();
    app.on_export_postman_collection(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let path = path.trim();
        if path.is_empty() {
            app.set_collection_status("Collection path required".into());
            set_response(
                &app,
                "Postman export failed",
                "",
                "error",
                "Enter a target Postman collection JSON file path.",
            );
            return;
        }

        let Some(collection) = export_state
            .lock()
            .ok()
            .map(|state| state.collection.clone())
        else {
            return;
        };
        match collection.save_postman_file(path) {
            Ok(()) => {
                app.set_collection_status("Exported Postman JSON".into());
                set_response(
                    &app,
                    "Postman collection exported",
                    path,
                    "success",
                    &collection.name,
                );
            }
            Err(error) => {
                app.set_collection_status("Export failed".into());
                set_response(
                    &app,
                    "Postman export failed",
                    path,
                    "error",
                    &error.to_string(),
                );
            }
        }
    });

    let weak_app = app.as_weak();
    let add_state = state.clone();
    app.on_save_current_request(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let input = request_projection_input(&app);
        let tests = match parse_response_assertions(&app.get_request_tests()) {
            Ok(tests) => tests,
            Err(error) => {
                app.set_collection_status("Add failed".into());
                set_response(
                    &app,
                    "Collection add failed",
                    "",
                    "error",
                    &error.to_string(),
                );
                return;
            }
        };
        let collection_request = match collection_request_from_editor(&input, tests) {
            Ok(request) => request,
            Err(error) => {
                app.set_collection_status("Add failed".into());
                set_response(
                    &app,
                    "Collection add failed",
                    "",
                    "error",
                    &error.to_string(),
                );
                return;
            }
        };
        let request_name = collection_request.name.clone();
        let request_url = collection_request.url.clone();

        let Some((collection_name, request_count, rows)) =
            add_state.lock().ok().map(|mut state| {
                state
                    .collection
                    .items
                    .push(CollectionItem::Request(collection_request));
                (
                    state.collection.name.clone(),
                    count_collection_requests(&state.collection.items),
                    collection_model(&state.collection),
                )
            })
        else {
            return;
        };

        app.set_collection_name(collection_name.into());
        app.set_collection_rows(rows);
        app.set_collection_status(format!("Saved {request_count} requests").into());
        set_response(
            &app,
            "Request saved to collection",
            &request_name,
            "success",
            &request_url,
        );
    });

    let weak_app = app.as_weak();
    let duplicate_state = state.clone();
    app.on_duplicate_collection_request(move |id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() || id < 0 {
            return;
        }

        let Some((duplicate, request_count, rows)) =
            duplicate_state.lock().ok().and_then(|mut state| {
                duplicate_collection_request_at(&mut state.collection, id as usize).map(|request| {
                    let request_count = count_collection_requests(&state.collection.items);
                    let rows = collection_model(&state.collection);
                    (request, request_count, rows)
                })
            })
        else {
            return;
        };

        app.set_collection_rows(rows);
        app.set_collection_status(format!("Saved {request_count} requests").into());
        set_response(
            &app,
            "Collection request duplicated",
            &duplicate.name,
            "success",
            &duplicate.url,
        );
    });

    let weak_app = app.as_weak();
    let delete_state = state.clone();
    app.on_delete_collection_request(move |id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() || id < 0 {
            return;
        }

        let Some((removed, request_count, rows)) =
            delete_state.lock().ok().and_then(|mut state| {
                remove_collection_request_at(&mut state.collection, id as usize).map(|request| {
                    let request_count = count_collection_requests(&state.collection.items);
                    let rows = collection_model(&state.collection);
                    (request, request_count, rows)
                })
            })
        else {
            return;
        };

        app.set_collection_rows(rows);
        app.set_collection_status(format!("Deleted {request_count} requests").into());
        set_response(
            &app,
            "Collection request deleted",
            &removed.name,
            "neutral",
            &removed.url,
        );
    });

    let weak_app = app.as_weak();
    let select_state = state;
    app.on_select_collection_request(move |id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() || id < 0 {
            return;
        }

        let request = select_state
            .lock()
            .ok()
            .and_then(|state| collection_request_at(&state.collection, id as usize).cloned());

        if let Some(request) = request {
            restore_collection_request(&app, &request);
        }
    });
}

fn wire_mock_log_filter(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    let filter_state = state.clone();
    app.on_filter_mock_logs(move |query| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let model = filter_state
            .lock()
            .ok()
            .map(|state| filtered_mock_log_model(&state.mock_logs, query.as_str()))
            .unwrap_or_else(|| mock_log_model(&[]));
        app.set_mock_logs(model);
    });

    let weak_app = app.as_weak();
    app.on_save_mock_logs(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let filter = app.get_mock_log_filter();
        let result = state
            .lock()
            .map_err(|_| anyhow!("mock log state is unavailable"))
            .and_then(|state| save_mock_logs(path.as_str(), &state.mock_logs, filter.as_str()));

        match result {
            Ok(count) => {
                set_response(
                    &app,
                    "Mock logs saved",
                    path.as_str(),
                    "success",
                    &format!("{count} mock log entries exported."),
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Mock log save failed",
                    path.as_str(),
                    "error",
                    &error.to_string(),
                );
            }
        }
    });
}

fn wire_request_sender(app: &AppWindow, runtime: Arc<Runtime>, state: Arc<Mutex<AppState>>) {
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
                        app.set_response_headers(response_headers.into());
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

fn wire_codegen(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_generate_code(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let request = match build_codegen_request_projection(&request_projection_input(&app)) {
            Ok(request) => request,
            Err(error) => {
                set_response(&app, "Codegen failed", "", "error", &error.to_string());
                return;
            }
        };
        let language = snippet_language(&app.get_codegen_language());
        let snippet = generate_snippet(&request, language);
        app.set_codegen_output(snippet.into());
        set_response(
            &app,
            "Codegen ready",
            &app.get_codegen_language(),
            "success",
            "Snippet generated.",
        );
    });

    let weak_app = app.as_weak();
    app.on_save_codegen(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let request = match build_codegen_request_projection(&request_projection_input(&app)) {
            Ok(request) => request,
            Err(error) => {
                set_response(&app, "Codegen save failed", "", "error", &error.to_string());
                return;
            }
        };
        let language = snippet_language(&app.get_codegen_language());
        let snippet = generate_snippet(&request, language);

        match save_codegen_snippet(path.as_str(), &snippet) {
            Ok(()) => {
                app.set_codegen_output(snippet.into());
                set_response(
                    &app,
                    "Codegen saved",
                    path.as_str(),
                    "success",
                    "Snippet exported to disk.",
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Codegen save failed",
                    path.as_str(),
                    "error",
                    &error.to_string(),
                );
            }
        }
    });
}

fn wire_collection_runner(app: &AppWindow, runtime: Arc<Runtime>, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    app.on_run_collection(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let options =
            match runner_options(&app.get_runner_delay_ms(), app.get_runner_stop_on_failure()) {
                Ok(options) => options,
                Err(error) => {
                    set_response(&app, "Runner failed", "", "error", &error.to_string());
                    return;
                }
            };
        let (variables, active_environment) = match build_variable_store(
            &app.get_global_variables(),
            &app.get_environment_name(),
            &app.get_environment_variables(),
        ) {
            Ok(variables) => variables,
            Err(error) => {
                set_response(&app, "Runner failed", "", "error", &error.to_string());
                return;
            }
        };

        let Some(collection) = state.lock().ok().map(|state| state.collection.clone()) else {
            return;
        };
        let request_count = count_collection_requests(&collection.items);
        if request_count == 0 {
            app.set_runner_summary("No requests to run".into());
            app.set_runner_rows(empty_runner_model());
            set_response(
                &app,
                "Runner idle",
                &collection.name,
                "neutral",
                "Save or load collection requests before running.",
            );
            return;
        }

        app.set_busy(true);
        app.set_activity("Running collection".into());
        app.set_runner_summary(format!("Running {request_count} requests").into());
        app.set_runner_rows(empty_runner_model());
        set_response(
            &app,
            "Runner running",
            &collection.name,
            "busy",
            &format!("Running {request_count} requests"),
        );

        let weak_app = app.as_weak();
        runtime.spawn(async move {
            let summary = run_collection(
                &collection,
                &variables,
                active_environment.as_deref(),
                options,
            )
            .await;
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak_app.upgrade() else {
                    return;
                };

                let response_tone = runner_response_tone(&summary);
                let response_status = runner_response_status(&summary);
                let response_meta = format!("{} ms", summary.elapsed_ms);
                app.set_runner_rows(runner_model(&summary.results));
                app.set_runner_summary(runner_summary_line(&summary).into());
                set_response(
                    &app,
                    &response_status,
                    &response_meta,
                    response_tone,
                    &format_runner_summary(&summary),
                );
                app.set_activity("".into());
                app.set_busy(false);
            });
        });
    });
}

fn wire_realtime_actions(app: &AppWindow, runtime: Arc<Runtime>) {
    let weak_app = app.as_weak();
    let websocket_runtime = runtime.clone();
    app.on_run_websocket(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let url = app.get_url().to_string();
        let message = app.get_realtime_message().to_string();
        app.set_busy(true);
        app.set_activity("Sending WebSocket message".into());
        set_response(
            &app,
            "WebSocket sending",
            "",
            "busy",
            &format!("Connecting\n\n{url}"),
        );

        let weak_app = app.as_weak();
        websocket_runtime.spawn(async move {
            let result = client::send_websocket_message(&url, &message).await;
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak_app.upgrade() else {
                    return;
                };
                match result {
                    Ok(exchange) => set_response(
                        &app,
                        "WebSocket complete",
                        &format!("{} ms", exchange.elapsed_ms),
                        "success",
                        &format_websocket_exchange(&exchange),
                    ),
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
    let sse_runtime = runtime;
    app.on_run_sse(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
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
                    Ok(exchange) => set_response(
                        &app,
                        "SSE complete",
                        &format!(
                            "{} ms / {} events",
                            exchange.elapsed_ms,
                            exchange.events.len()
                        ),
                        "success",
                        &format_sse_exchange(&exchange),
                    ),
                    Err(error) => set_response(&app, "SSE failed", "", "error", &error.to_string()),
                }
                app.set_activity("".into());
                app.set_busy(false);
            });
        });
    });
}

fn wire_grpc_draft(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_build_grpc_draft(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match build_grpc_request_draft(
            &app.get_grpc_endpoint(),
            &app.get_grpc_method(),
            &app.get_grpc_metadata(),
            &app.get_grpc_message(),
        ) {
            Ok(draft) => set_response(
                &app,
                "gRPC draft ready",
                &draft.method_path(),
                "success",
                &format_grpc_draft(&draft),
            ),
            Err(error) => set_response(&app, "gRPC draft failed", "", "error", &error.to_string()),
        }
    });
}

fn wire_mock_server(app: &AppWindow, runtime: Arc<Runtime>, state: Arc<Mutex<AppState>>) {
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

fn set_response(app: &AppWindow, status: &str, meta: &str, tone: &str, body: &str) {
    app.set_response_status(status.into());
    app.set_response_meta(meta.into());
    app.set_response_tone(tone.into());
    app.set_response_body(body.into());
    app.set_response_headers("".into());
}

fn display_spec_name(spec: &ApiSpec) -> String {
    if spec.version.is_empty() {
        spec.title.clone()
    } else {
        format!("{} {}", spec.title, spec.version)
    }
}

fn display_spec_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn route_model(routes: &[ApiRoute]) -> ModelRc<RouteRow> {
    ModelRc::new(VecModel::from_iter(routes.iter().map(|route| RouteRow {
        method: route.method.clone().into(),
        path: route.path.clone().into(),
        summary: route.summary.clone().into(),
    })))
}

fn collection_model(collection: &ApiCollection) -> ModelRc<CollectionRow> {
    let mut rows = Vec::new();
    let mut next_id = 0;
    collect_collection_rows(&collection.items, "", &mut next_id, &mut rows);
    ModelRc::new(VecModel::from_iter(rows))
}

fn collect_collection_rows(
    items: &[CollectionItem],
    folder_path: &str,
    next_id: &mut i32,
    rows: &mut Vec<CollectionRow>,
) {
    for item in items {
        match item {
            CollectionItem::Folder(folder) => {
                let nested_path = if folder_path.is_empty() {
                    folder.name.clone()
                } else {
                    format!("{folder_path} / {}", folder.name)
                };
                collect_collection_rows(&folder.items, &nested_path, next_id, rows);
            }
            CollectionItem::Request(request) => {
                let name = if folder_path.is_empty() {
                    request.name.clone()
                } else {
                    format!("{folder_path} / {}", request.name)
                };
                rows.push(CollectionRow {
                    id: *next_id,
                    method: request.method.clone().into(),
                    name: name.into(),
                    url: request.url.clone().into(),
                });
                *next_id += 1;
            }
        }
    }
}

fn count_collection_requests(items: &[CollectionItem]) -> usize {
    items
        .iter()
        .map(|item| match item {
            CollectionItem::Folder(folder) => count_collection_requests(&folder.items),
            CollectionItem::Request(_) => 1,
        })
        .sum()
}

fn collection_request_at(collection: &ApiCollection, index: usize) -> Option<&CollectionRequest> {
    let mut current = 0;
    collection_request_at_items(&collection.items, index, &mut current)
}

fn collection_request_at_items<'a>(
    items: &'a [CollectionItem],
    target: usize,
    current: &mut usize,
) -> Option<&'a CollectionRequest> {
    for item in items {
        match item {
            CollectionItem::Folder(folder) => {
                if let Some(request) = collection_request_at_items(&folder.items, target, current) {
                    return Some(request);
                }
            }
            CollectionItem::Request(request) => {
                if *current == target {
                    return Some(request);
                }
                *current += 1;
            }
        }
    }
    None
}

fn duplicate_collection_request_at(
    collection: &mut ApiCollection,
    index: usize,
) -> Option<CollectionRequest> {
    let mut current = 0;
    duplicate_collection_request_at_items(&mut collection.items, index, &mut current)
}

fn duplicate_collection_request_at_items(
    items: &mut Vec<CollectionItem>,
    target: usize,
    current: &mut usize,
) -> Option<CollectionRequest> {
    let mut position = 0;
    while position < items.len() {
        match &mut items[position] {
            CollectionItem::Request(request) => {
                if *current == target {
                    let mut duplicate = request.clone();
                    duplicate.name = format!("{} Copy", duplicate.name);
                    items.insert(position + 1, CollectionItem::Request(duplicate.clone()));
                    return Some(duplicate);
                }
                *current += 1;
            }
            CollectionItem::Folder(folder) => {
                if let Some(request) =
                    duplicate_collection_request_at_items(&mut folder.items, target, current)
                {
                    return Some(request);
                }
            }
        }
        position += 1;
    }
    None
}

fn remove_collection_request_at(
    collection: &mut ApiCollection,
    index: usize,
) -> Option<CollectionRequest> {
    let mut current = 0;
    remove_collection_request_at_items(&mut collection.items, index, &mut current)
}

fn remove_collection_request_at_items(
    items: &mut Vec<CollectionItem>,
    target: usize,
    current: &mut usize,
) -> Option<CollectionRequest> {
    let mut position = 0;
    while position < items.len() {
        if matches!(&items[position], CollectionItem::Request(_)) {
            if *current == target {
                let CollectionItem::Request(request) = items.remove(position) else {
                    unreachable!("collection item kind checked before removal");
                };
                return Some(request);
            }
            *current += 1;
            position += 1;
            continue;
        }

        if let CollectionItem::Folder(folder) = &mut items[position] {
            if let Some(request) =
                remove_collection_request_at_items(&mut folder.items, target, current)
            {
                return Some(request);
            }
        }
        position += 1;
    }
    None
}

fn collection_request_from_codegen(request: &CodegenRequest) -> CollectionRequest {
    CollectionRequest {
        name: format!("{} {}", request.method, request.url),
        method: request.method.clone(),
        url: request.url.clone(),
        headers: name_values_from_pairs(&request.headers),
        query_params: name_values_from_pairs(&request.query_params),
        body: collection_body_from_request_body(&request.body),
        pre_request_script: String::new(),
        tests: Vec::new(),
    }
}

fn collection_request_from_editor(
    input: &RequestProjectionInput,
    tests: Vec<ResponseAssertion>,
) -> Result<CollectionRequest> {
    let mut headers = parse_key_value_lines(&input.headers, "header")?;
    let mut query_params = parse_key_value_lines(&input.query_params, "query param")?;
    let (auth_headers, auth_query_params) =
        build_auth_entries(&input.auth_mode, &input.auth_config)?;
    for (name, value) in auth_headers {
        upsert_pair(&mut headers, name, value, true);
    }
    for (name, value) in auth_query_params {
        upsert_pair(&mut query_params, name, value, false);
    }
    let request = CodegenRequest {
        method: input.method.clone(),
        url: input.url.trim().to_string(),
        headers,
        query_params,
        body: build_request_body(&input.body_mode, &input.body, &input.graphql_variables)?,
    };
    if request.url.trim().is_empty() {
        bail!("request URL is empty");
    }

    let mut collection_request = collection_request_from_codegen(&request);
    collection_request.pre_request_script = input.pre_request_script.trim().to_string();
    collection_request.tests = tests;
    Ok(collection_request)
}

fn name_values_from_pairs(pairs: &[(String, String)]) -> Vec<NameValue> {
    pairs
        .iter()
        .map(|(name, value)| NameValue {
            name: name.clone(),
            value: value.clone(),
        })
        .collect()
}

fn collection_body_from_request_body(body: &RequestBody) -> CollectionBody {
    match body {
        RequestBody::None => CollectionBody::None,
        RequestBody::Raw { content_type, body } => CollectionBody::Raw {
            content_type: content_type
                .clone()
                .unwrap_or_else(|| "application/json".to_string()),
            body: body.clone(),
        },
        RequestBody::FormUrlEncoded(fields) => CollectionBody::UrlEncoded {
            fields: name_values_from_pairs(fields),
        },
        RequestBody::Multipart(fields) => CollectionBody::FormData {
            fields: name_values_from_pairs(fields),
        },
        RequestBody::BinaryFile { path, content_type } => CollectionBody::Binary {
            path: path.clone(),
            content_type: content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_string()),
        },
    }
}

fn restore_collection_request(app: &AppWindow, request: &CollectionRequest) {
    let (body_mode, request_body) = collection_body_to_slint(&request.body);
    app.set_method(request.method.clone().into());
    app.set_url(request.url.clone().into());
    app.set_query_params(format_name_values(&request.query_params).into());
    app.set_request_headers(format_name_values(&request.headers).into());
    app.set_auth_mode("none".into());
    app.set_auth_config("".into());
    app.set_body_mode(body_mode.into());
    app.set_request_body(request_body.into());
    app.set_graphql_variables("{}".into());
    app.set_pre_request_script(request.pre_request_script.clone().into());
    app.set_request_tests(format_response_assertions(&request.tests).into());
    app.set_collection_status(format!("Selected {}", request.name).into());
    set_response(
        app,
        "Collection request loaded",
        &request.name,
        "neutral",
        &request.url,
    );
}

fn collection_body_to_slint(body: &CollectionBody) -> (String, String) {
    match body {
        CollectionBody::None => ("none".to_string(), String::new()),
        CollectionBody::Raw { body, .. } => ("raw".to_string(), body.clone()),
        CollectionBody::FormData { fields } => ("form".to_string(), format_name_values(fields)),
        CollectionBody::UrlEncoded { fields } => ("urlenc".to_string(), format_name_values(fields)),
        CollectionBody::Binary { path, .. } => ("binary".to_string(), path.clone()),
    }
}

fn format_name_values(values: &[NameValue]) -> String {
    values
        .iter()
        .map(|value| format!("{}={}", value.name, value.value))
        .collect::<Vec<_>>()
        .join("\n")
}

fn empty_runner_model() -> ModelRc<RunnerRow> {
    ModelRc::new(VecModel::from_iter(Vec::<RunnerRow>::new()))
}

fn runner_model(results: &[CollectionRunResult]) -> ModelRc<RunnerRow> {
    ModelRc::new(VecModel::from_iter(results.iter().map(|result| {
        RunnerRow {
            method: result.method.clone().into(),
            name: result.path.join(" / ").into(),
            status: runner_result_status(result).into(),
            detail: runner_result_detail(result).into(),
            tone: if result.success { "success" } else { "error" }.into(),
        }
    })))
}

fn runner_options(delay_ms: &str, stop_on_failure: bool) -> Result<RunnerOptions> {
    let delay_ms = delay_ms.trim();
    let delay_ms = if delay_ms.is_empty() {
        0
    } else {
        delay_ms
            .parse::<u64>()
            .map_err(|_| anyhow!("runner delay must be a non-negative integer"))?
    };
    Ok(RunnerOptions {
        delay_ms,
        failure_strategy: if stop_on_failure {
            FailureStrategy::StopOnFailure
        } else {
            FailureStrategy::Continue
        },
    })
}

fn runner_response_tone(summary: &CollectionRunSummary) -> &'static str {
    if summary.total == 0 {
        "neutral"
    } else if summary.failed == 0 {
        "success"
    } else {
        "error"
    }
}

fn runner_response_status(summary: &CollectionRunSummary) -> String {
    if summary.failed == 0 {
        "Runner passed".to_string()
    } else {
        "Runner failed".to_string()
    }
}

fn runner_summary_line(summary: &CollectionRunSummary) -> String {
    let stop = if summary.stopped_early {
        " / stopped"
    } else {
        ""
    };
    format!(
        "{}: {} passed, {} failed, {} total / {} ms{stop}",
        summary.collection_name, summary.passed, summary.failed, summary.total, summary.elapsed_ms
    )
}

fn format_runner_summary(summary: &CollectionRunSummary) -> String {
    let mut lines = vec![runner_summary_line(summary)];
    for result in &summary.results {
        lines.push(format_runner_result(result));
    }
    lines.join("\n")
}

fn format_runner_result(result: &CollectionRunResult) -> String {
    let path = result.path.join(" / ");
    let mut line = format!(
        "[{}] {} {} {} ({path})",
        runner_result_status(result),
        result_status_label(result),
        result.method,
        result.url
    );
    if let Some(error) = &result.error {
        line.push_str(&format!(" - {error}"));
    }
    if !result.pre_request_actions.is_empty() {
        line.push_str(&format!(
            " - pre-request {}",
            result.pre_request_actions.len()
        ));
    }
    if !result.assertions.is_empty() {
        let passed = result
            .assertions
            .iter()
            .filter(|assertion| assertion.passed)
            .count();
        line.push_str(&format!(" - tests {passed}/{}", result.assertions.len()));
    }
    line
}

fn runner_result_status(result: &CollectionRunResult) -> &'static str {
    if result.success { "PASS" } else { "FAIL" }
}

fn runner_result_detail(result: &CollectionRunResult) -> String {
    let mut parts = vec![
        result_status_label(result),
        format!("{} ms", result.elapsed_ms),
        format!("{} B", result.body_bytes),
    ];
    if !result.assertions.is_empty() {
        let passed = result
            .assertions
            .iter()
            .filter(|assertion| assertion.passed)
            .count();
        parts.push(format!("tests {passed}/{}", result.assertions.len()));
    }
    if !result.pre_request_actions.is_empty() {
        parts.push(format!("pre {}", result.pre_request_actions.len()));
    }
    if let Some(error) = &result.error {
        parts.push(error.clone());
    }
    parts.join(" / ")
}

fn result_status_label(result: &CollectionRunResult) -> String {
    result
        .status
        .map(|status| format!("HTTP {status}"))
        .unwrap_or_else(|| "ERR".to_string())
}

fn parse_positive_usize(input: &str, field_name: &str) -> Result<usize> {
    let value = input.trim();
    let parsed = value
        .parse::<usize>()
        .map_err(|_| anyhow!("{field_name} must be a positive integer"))?;
    if parsed == 0 {
        bail!("{field_name} must be greater than zero");
    }
    Ok(parsed)
}

fn format_websocket_exchange(exchange: &client::WebSocketExchange) -> String {
    let mut lines = vec![
        format!("URL: {}", exchange.url),
        format!("Sent: {}", exchange.sent),
    ];
    for (index, message) in exchange.received.iter().enumerate() {
        lines.push(format!(
            "Received {} [{}]: {}",
            index + 1,
            websocket_message_kind(&message.kind),
            message.data
        ));
    }
    lines.join("\n")
}

fn websocket_message_kind(kind: &client::WebSocketMessageKind) -> &'static str {
    match kind {
        client::WebSocketMessageKind::Text => "text",
        client::WebSocketMessageKind::Binary => "binary",
        client::WebSocketMessageKind::Ping => "ping",
        client::WebSocketMessageKind::Pong => "pong",
        client::WebSocketMessageKind::Close => "close",
    }
}

fn format_sse_exchange(exchange: &client::SseExchange) -> String {
    let mut lines = vec![format!("URL: {}", exchange.url)];
    for (index, event) in exchange.events.iter().enumerate() {
        lines.push(format_sse_event(index + 1, event));
    }
    lines.join("\n")
}

fn format_sse_event(index: usize, event: &client::SseEvent) -> String {
    let event_name = event.event.as_deref().unwrap_or("message");
    let id = event
        .id
        .as_deref()
        .map(|id| format!(" / id {id}"))
        .unwrap_or_default();
    let retry = event
        .retry
        .map(|retry| format!(" / retry {retry}"))
        .unwrap_or_default();
    format!("{index}. {event_name}{id}{retry}\n{}", event.data)
}

fn format_grpc_draft(draft: &GrpcRequestDraft) -> String {
    let metadata = if draft.metadata.is_empty() {
        "No metadata".to_string()
    } else {
        draft
            .metadata
            .iter()
            .map(|entry| format!("{}: {}", entry.name, entry.value))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let message =
        serde_json::to_string_pretty(&draft.message).unwrap_or_else(|_| draft.message.to_string());

    format!(
        "Endpoint: {}\nMethod: {}\n\nMetadata\n{}\n\nMessage\n{}",
        draft.endpoint,
        draft.method_path(),
        metadata,
        message
    )
}

fn mock_log_model(logs: &[MockRequestLog]) -> ModelRc<MockLogRow> {
    mock_log_model_from_iter(logs.iter())
}

fn filtered_mock_log_model(logs: &[MockRequestLog], query: &str) -> ModelRc<MockLogRow> {
    mock_log_model_from_iter(filtered_mock_logs(logs, query).into_iter())
}

fn filtered_mock_logs<'a>(logs: &'a [MockRequestLog], query: &str) -> Vec<&'a MockRequestLog> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return logs.iter().collect();
    }

    logs.iter()
        .filter(|log| {
            log.method.to_lowercase().contains(&query)
                || log.path.to_lowercase().contains(&query)
                || log.status.to_string().contains(&query)
        })
        .collect()
}

fn mock_log_model_from_iter<'a>(
    logs: impl Iterator<Item = &'a MockRequestLog>,
) -> ModelRc<MockLogRow> {
    ModelRc::new(VecModel::from_iter(logs.map(|log| MockLogRow {
        method: log.method.clone().into(),
        path: log.path.clone().into(),
        status: log.status.to_string().into(),
    })))
}

fn push_mock_log(logs: &mut Vec<MockRequestLog>, log: MockRequestLog, limit: usize) {
    logs.insert(0, log);
    logs.truncate(limit);
}

fn save_mock_logs(path: &str, logs: &[MockRequestLog], query: &str) -> Result<usize> {
    let exported = filtered_mock_logs(logs, query);
    let body = serde_json::to_string_pretty(&exported)
        .map_err(|err| anyhow!("serialize mock log export: {err}"))?;
    write_text_file(path, &body, "mock log export")?;
    Ok(exported.len())
}

fn filtered_history_model(history: &RequestHistory, query: &str) -> ModelRc<HistoryRow> {
    let entries = history.filtered(query);
    history_model_from_entries(entries.into_iter())
}

fn delete_history_entry(
    history: &mut RequestHistory,
    id: u64,
    query: &str,
) -> Option<ModelRc<HistoryRow>> {
    if !history.remove(id) {
        return None;
    }
    Some(filtered_history_model(history, query))
}

fn empty_history_model() -> ModelRc<HistoryRow> {
    history_model_from_entries(std::iter::empty())
}

fn history_model_from_entries<'a>(
    entries: impl Iterator<Item = &'a zenapi::history::HistoryEntry>,
) -> ModelRc<HistoryRow> {
    ModelRc::new(VecModel::from_iter(entries.map(|entry| HistoryRow {
        id: entry.id as i32,
        method: entry.request.method.clone().into(),
        url: entry.request.url.clone().into(),
        status: entry.response.status.clone().into(),
    })))
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

fn history_request(
    request: &CodegenRequest,
    input: &RequestProjectionInput,
    request_tests: &str,
) -> HistoryRequest {
    let (body_kind, body_preview) = request_body_preview(&request.body);
    HistoryRequest {
        method: request.method.clone(),
        url: request.url.clone(),
        query_params: request.query_params.clone(),
        headers: request.headers.clone(),
        auth_mode: input.auth_mode.clone(),
        auth_config: input.auth_config.clone(),
        body_kind,
        body_preview,
        pre_request_script: input.pre_request_script.clone(),
        request_tests: request_tests.to_string(),
    }
}

fn normalized_history_auth_mode(mode: &str) -> &str {
    let mode = mode.trim();
    if mode.is_empty() { "none" } else { mode }
}

fn success_history_response(response: &ClientResponse) -> HistoryResponse {
    HistoryResponse {
        status: format!("HTTP {}", response.status),
        meta: format!("{} ms / {} B", response.elapsed_ms, response.body_bytes),
        body_preview: truncate_preview(&response.body, 1200),
    }
}

fn request_body_preview(body: &RequestBody) -> (String, String) {
    match body {
        RequestBody::None => ("none".to_string(), String::new()),
        RequestBody::Raw { body, .. } => ("raw".to_string(), body.clone()),
        RequestBody::FormUrlEncoded(fields) => {
            ("urlenc".to_string(), format_key_value_preview(fields))
        }
        RequestBody::Multipart(fields) => ("form".to_string(), format_key_value_preview(fields)),
        RequestBody::BinaryFile { path, .. } => ("binary".to_string(), path.clone()),
    }
}

fn format_key_value_preview(fields: &[(String, String)]) -> String {
    fields
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate_preview(input: &str, max_chars: usize) -> String {
    let mut preview = input.chars().take(max_chars).collect::<String>();
    if input.chars().count() > max_chars {
        preview.push_str("\n...");
    }
    preview
}

fn filter_routes(routes: &[ApiRoute], query: &str) -> Vec<ApiRoute> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return routes.to_vec();
    }

    routes
        .iter()
        .filter(|route| {
            route.method.to_lowercase().contains(&query)
                || route.path.to_lowercase().contains(&query)
                || route.summary.to_lowercase().contains(&query)
        })
        .cloned()
        .collect()
}

fn parse_key_value_lines(input: &str, field_name: &str) -> Result<Vec<(String, String)>> {
    input
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                None
            } else {
                Some((index + 1, line))
            }
        })
        .map(|(line_number, line)| {
            let Some((key, value)) = split_key_value_line(line) else {
                bail!("{field_name} line {line_number} must use key=value or key: value");
            };
            let key = key.trim();
            if key.is_empty() {
                bail!("{field_name} line {line_number} has an empty name");
            }
            Ok((key.to_string(), value.trim().to_string()))
        })
        .collect()
}

fn build_codegen_request_projection(input: &RequestProjectionInput) -> Result<CodegenRequest> {
    let (variables, active_environment) = build_variable_store(
        &input.global_variables,
        &input.environment_name,
        &input.environment_variables,
    )?;
    let active_environment = active_environment.as_deref();

    let mut request = build_unresolved_editor_request(input, &variables, active_environment)?;
    let execution = execute_pre_request_actions(
        &input.pre_request_script,
        request,
        variables,
        active_environment,
    )?;
    request = resolve_codegen_request_templates(
        execution.request,
        &execution.variables,
        active_environment,
    )?;
    request.url = request.url.trim().to_string();
    if request.url.is_empty() {
        bail!("request URL is empty");
    }

    Ok(request)
}

fn build_unresolved_editor_request(
    input: &RequestProjectionInput,
    variables: &VariableStore,
    active_environment: Option<&str>,
) -> Result<CodegenRequest> {
    let mut headers = parse_key_value_lines(&input.headers, "header")?;
    let mut query_params = parse_key_value_lines(&input.query_params, "query param")?;
    let auth_config = resolve_text(&input.auth_config, &variables, active_environment)?;
    let (auth_headers, auth_query_params) = build_auth_entries(&input.auth_mode, &auth_config)?;
    for (name, value) in auth_headers {
        upsert_pair(&mut headers, name, value, true);
    }
    for (name, value) in auth_query_params {
        upsert_pair(&mut query_params, name, value, false);
    }

    Ok(CodegenRequest {
        method: input.method.clone(),
        url: input.url.trim().to_string(),
        headers,
        query_params,
        body: build_request_body(&input.body_mode, &input.body, &input.graphql_variables)?,
    })
}

fn split_key_value_line(line: &str) -> Option<(&str, &str)> {
    let separator = match (line.find('='), line.find(':')) {
        (Some(eq), Some(colon)) => eq.min(colon),
        (Some(eq), None) => eq,
        (None, Some(colon)) => colon,
        (None, None) => return None,
    };

    Some((&line[..separator], &line[separator + 1..]))
}

fn build_request_body(mode: &str, input: &str, graphql_variables: &str) -> Result<RequestBody> {
    match mode {
        "none" => Ok(RequestBody::None),
        "form" => Ok(RequestBody::Multipart(parse_key_value_lines(
            input,
            "form field",
        )?)),
        "urlenc" => Ok(RequestBody::FormUrlEncoded(parse_key_value_lines(
            input,
            "urlencoded field",
        )?)),
        "binary" => {
            let path = input.trim();
            if path.is_empty() {
                bail!("binary body path is empty");
            }
            Ok(RequestBody::BinaryFile {
                path: path.to_string(),
                content_type: None,
            })
        }
        "graphql" => build_graphql_request_body(input, graphql_variables),
        _ => Ok(RequestBody::Raw {
            content_type: Some("application/json".to_string()),
            body: input.to_string(),
        }),
    }
}

fn build_graphql_request_body(query: &str, variables: &str) -> Result<RequestBody> {
    let query = query.trim();
    if query.is_empty() {
        bail!("GraphQL query is empty");
    }
    let variables = parse_graphql_variables(variables)?;
    let payload = serde_json::json!({
        "query": query,
        "variables": variables,
    });

    Ok(RequestBody::Raw {
        content_type: Some("application/json".to_string()),
        body: serde_json::to_string(&payload)?,
    })
}

fn parse_graphql_variables(input: &str) -> Result<Value> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Value::Object(Default::default()));
    }

    let value = serde_json::from_str::<Value>(input)
        .map_err(|error| anyhow!("GraphQL variables JSON is invalid: {error}"))?;
    if !value.is_object() {
        bail!("GraphQL variables must be a JSON object");
    }
    Ok(value)
}

fn build_auth_entries(
    mode: &str,
    input: &str,
) -> Result<(Vec<(String, String)>, Vec<(String, String)>)> {
    let input = input.trim();
    match mode {
        "none" => Ok((Vec::new(), Vec::new())),
        "bearer" | "jwt" => {
            if input.is_empty() {
                bail!("bearer token is empty");
            }
            Ok((
                vec![("Authorization".to_string(), format!("Bearer {input}"))],
                Vec::new(),
            ))
        }
        "basic" => {
            let Some((username, password)) = input.split_once(':') else {
                bail!("basic auth must use username:password");
            };
            if username.trim().is_empty() {
                bail!("basic auth username is empty");
            }
            let encoded = BASE64_STANDARD.encode(format!("{}:{}", username.trim(), password));
            Ok((
                vec![("Authorization".to_string(), format!("Basic {encoded}"))],
                Vec::new(),
            ))
        }
        "api-header" => {
            let values = parse_key_value_lines(input, "api key")?;
            if values.is_empty() {
                bail!("api key header is empty");
            }
            Ok((values, Vec::new()))
        }
        "api-query" => {
            let values = parse_key_value_lines(input, "api key")?;
            if values.is_empty() {
                bail!("api key query is empty");
            }
            Ok((Vec::new(), values))
        }
        _ => Ok((Vec::new(), Vec::new())),
    }
}

fn build_variable_store(
    global_input: &str,
    environment_name: &str,
    environment_input: &str,
) -> Result<(VariableStore, Option<String>)> {
    let environment_name = environment_name.trim();
    let environment_pairs = parse_key_value_lines(environment_input, "environment variable")?;
    if !environment_pairs.is_empty() && environment_name.is_empty() {
        bail!("environment name is empty");
    }

    let mut store = VariableStore::new();
    for (name, value) in parse_key_value_lines(global_input, "global variable")? {
        store.upsert(Variable::global(name, value));
    }

    for (name, value) in environment_pairs {
        store.upsert(Variable::environment(environment_name, name, value));
    }

    let active_environment = (!environment_name.is_empty()).then(|| environment_name.to_string());
    Ok((store, active_environment))
}

fn resolve_text(
    input: &str,
    variables: &VariableStore,
    active_environment: Option<&str>,
) -> Result<String> {
    replace_variables(input, variables, active_environment)
}

#[cfg(test)]
fn resolve_pairs(
    pairs: Vec<(String, String)>,
    variables: &VariableStore,
    active_environment: Option<&str>,
) -> Result<Vec<(String, String)>> {
    pairs
        .into_iter()
        .map(|(name, value)| {
            Ok((
                replace_variables(&name, variables, active_environment)?,
                replace_variables(&value, variables, active_environment)?,
            ))
        })
        .collect()
}

fn upsert_pair(
    pairs: &mut Vec<(String, String)>,
    name: String,
    value: String,
    case_insensitive: bool,
) {
    if let Some((_, existing_value)) = pairs.iter_mut().find(|(existing_name, _)| {
        if case_insensitive {
            existing_name.eq_ignore_ascii_case(&name)
        } else {
            existing_name == &name
        }
    }) {
        *existing_value = value;
    } else {
        pairs.push((name, value));
    }
}

fn format_headers(headers: &[(String, String)]) -> String {
    if headers.is_empty() {
        return "No headers".to_string();
    }

    headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_response_assertions(input: &str) -> Result<Vec<ResponseAssertion>> {
    input
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            (!line.is_empty() && !line.starts_with('#') && !line.starts_with("//"))
                .then_some((index, line))
        })
        .map(|(index, line)| {
            parse_response_assertion_line(line)
                .map_err(|error| anyhow!("test line {}: {error}", index + 1))
        })
        .collect()
}

fn parse_response_assertion_line(line: &str) -> Result<ResponseAssertion> {
    let (kind, args) = split_assertion_command(line)?;
    let assertion_kind = match kind {
        "status" | "status_equals" | "status=" => ResponseAssertionKind::StatusEquals {
            status: parse_status(args, kind)?,
        },
        "range" | "status_in_range" => {
            let mut parts = args.split_whitespace();
            let min = parts
                .next()
                .ok_or_else(|| anyhow!("{kind} expects min max"))
                .and_then(|value| parse_status(value, kind))?;
            let max = parts
                .next()
                .ok_or_else(|| anyhow!("{kind} expects min max"))
                .and_then(|value| parse_status(value, kind))?;
            ResponseAssertionKind::StatusInRange { min, max }
        }
        "header" | "header_exists" | "header?" => ResponseAssertionKind::HeaderExists {
            name: require_assertion_arg(args, kind)?,
        },
        "header_equals" | "header=" => {
            let (name, value) = split_first_arg(args, kind)?;
            ResponseAssertionKind::HeaderEquals { name, value }
        }
        "body" | "body_contains" | "body?" => ResponseAssertionKind::BodyContains {
            text: require_assertion_arg(args, kind)?,
        },
        "json" | "json_path_equals" | "json=" => {
            let (path, value) = split_first_arg(args, kind)?;
            ResponseAssertionKind::JsonPathEquals {
                path,
                value: parse_json_assertion_value(&value),
            }
        }
        _ => bail!("unknown assertion kind: {kind}"),
    };

    Ok(ResponseAssertion {
        name: format!("{kind} {args}"),
        kind: assertion_kind,
    })
}

fn split_assertion_command(line: &str) -> Result<(&str, &str)> {
    let Some((kind, args)) = line.split_once(char::is_whitespace) else {
        bail!("assertion needs arguments: {line}");
    };
    Ok((kind.trim(), args.trim()))
}

fn require_assertion_arg(args: &str, kind: &str) -> Result<String> {
    let args = args.trim();
    if args.is_empty() {
        bail!("{kind} expects a value");
    }
    Ok(args.to_string())
}

fn split_first_arg(args: &str, kind: &str) -> Result<(String, String)> {
    let Some((first, rest)) = args.trim().split_once(char::is_whitespace) else {
        bail!("{kind} expects target and expected value");
    };
    let first = first.trim();
    let rest = rest.trim();
    if first.is_empty() || rest.is_empty() {
        bail!("{kind} expects target and expected value");
    }
    Ok((first.to_string(), rest.to_string()))
}

fn parse_status(value: &str, kind: &str) -> Result<u16> {
    value
        .trim()
        .parse::<u16>()
        .map_err(|_| anyhow!("{kind} expects an HTTP status code"))
}

fn parse_json_assertion_value(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()))
}

fn format_response_assertions(assertions: &[ResponseAssertion]) -> String {
    assertions
        .iter()
        .map(|assertion| match &assertion.kind {
            ResponseAssertionKind::StatusEquals { status } => format!("status_equals {status}"),
            ResponseAssertionKind::StatusInRange { min, max } => {
                format!("status_in_range {min} {max}")
            }
            ResponseAssertionKind::HeaderExists { name } => format!("header_exists {name}"),
            ResponseAssertionKind::HeaderEquals { name, value } => {
                format!("header_equals {name} {value}")
            }
            ResponseAssertionKind::BodyContains { text } => format!("body_contains {text}"),
            ResponseAssertionKind::JsonPathEquals { path, value } => {
                format!("json_path_equals {path} {value}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn response_status_with_assertions(
    status: u16,
    assertion_results: &[ResponseAssertionResult],
) -> String {
    if assertion_results.is_empty() {
        return format!("HTTP {status}");
    }
    let passed = assertion_results
        .iter()
        .filter(|result| result.passed)
        .count();
    format!("HTTP {status} / tests {passed}/{}", assertion_results.len())
}

fn response_tone_with_assertions(
    status: u16,
    assertion_results: &[ResponseAssertionResult],
) -> &'static str {
    if assertion_results.iter().any(|result| !result.passed) {
        "error"
    } else {
        response_tone(status)
    }
}

fn response_body_with_assertions(
    body: &str,
    assertion_results: &[ResponseAssertionResult],
) -> String {
    if assertion_results.is_empty() {
        return body.to_string();
    }
    format!(
        "{body}\n\nTests\n{}",
        format_assertion_results(assertion_results)
    )
}

fn format_assertion_results(assertion_results: &[ResponseAssertionResult]) -> String {
    assertion_results
        .iter()
        .map(|result| {
            let outcome = if result.passed { "PASS" } else { "FAIL" };
            match &result.error {
                Some(error) => format!("[{outcome}] {} - {error}", result.name),
                None => format!("[{outcome}] {}", result.name),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn response_tone(status: u16) -> &'static str {
    if (200..400).contains(&status) {
        "success"
    } else if status >= 400 {
        "error"
    } else {
        "neutral"
    }
}

fn default_request_body(method: &str) -> &'static str {
    match method {
        "POST" | "PUT" | "PATCH" => "{\n  \n}",
        _ => "",
    }
}

fn default_body_mode(method: &str) -> &'static str {
    match method {
        "POST" | "PUT" | "PATCH" => "raw",
        _ => "none",
    }
}

fn snippet_language(language: &str) -> SnippetLanguage {
    match language {
        "python" => SnippetLanguage::PythonRequests,
        "js" => SnippetLanguage::JavaScriptFetch,
        "rust" => SnippetLanguage::RustReqwest,
        "go" => SnippetLanguage::GoNetHttp,
        _ => SnippetLanguage::Curl,
    }
}

fn save_codegen_snippet(path: &str, snippet: &str) -> Result<()> {
    write_text_file(path, snippet, "snippet export")
}

fn write_text_file(path: &str, contents: &str, label: &str) -> Result<()> {
    let path = path.trim();
    if path.is_empty() {
        bail!("{label} path is required");
    }

    let output_path = Path::new(path);
    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|err| anyhow!("create {label} directory {}: {err}", parent.display()))?;
    }
    fs::write(output_path, contents)
        .map_err(|err| anyhow!("write {label} {}: {err}", output_path.display()))
}

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn route(method: &str, path: &str, summary: &str) -> ApiRoute {
        ApiRoute {
            method: method.to_string(),
            path: path.to_string(),
            summary: summary.to_string(),
            mock_body: json!({}),
        }
    }

    fn saved_request(name: &str, method: &str, url: &str) -> CollectionRequest {
        CollectionRequest {
            name: name.to_string(),
            method: method.to_string(),
            url: url.to_string(),
            headers: Vec::new(),
            query_params: Vec::new(),
            body: CollectionBody::None,
            pre_request_script: String::new(),
            tests: Vec::new(),
        }
    }

    #[test]
    fn filters_routes_by_method_path_or_summary() {
        let routes = vec![
            route("GET", "/users", "List accounts"),
            route("POST", "/sessions", "Create login session"),
            route("DELETE", "/users/{id}", "Remove account"),
        ];

        assert_eq!(filter_routes(&routes, "post"), vec![routes[1].clone()]);
        assert_eq!(filter_routes(&routes, "sessions"), vec![routes[1].clone()]);
        assert_eq!(filter_routes(&routes, "remove"), vec![routes[2].clone()]);
    }

    #[test]
    fn maps_http_status_to_response_tone() {
        assert_eq!(response_tone(200), "success");
        assert_eq!(response_tone(302), "success");
        assert_eq!(response_tone(100), "neutral");
        assert_eq!(response_tone(404), "error");
    }

    #[test]
    fn parses_query_and_header_text_lines() {
        assert_eq!(
            parse_key_value_lines(
                "Accept: application/json\nsearch = rust slint\nbaseUrl=https://api.example.com\n# ignored\n\nlimit=20",
                "header"
            )
            .expect("parse"),
            vec![
                ("Accept".to_string(), "application/json".to_string()),
                ("search".to_string(), "rust slint".to_string()),
                (
                    "baseUrl".to_string(),
                    "https://api.example.com".to_string()
                ),
                ("limit".to_string(), "20".to_string()),
            ]
        );
    }

    #[test]
    fn rejects_malformed_key_value_lines() {
        let error = parse_key_value_lines("Accept: application/json\nmissing-separator", "header")
            .expect_err("invalid line");

        assert!(error.to_string().contains("line 2"));
    }

    #[test]
    fn formats_response_headers_for_display() {
        assert_eq!(
            format_headers(&[
                ("content-type".to_string(), "application/json".to_string()),
                ("x-request-id".to_string(), "abc".to_string())
            ]),
            "content-type: application/json\nx-request-id: abc"
        );
        assert_eq!(format_headers(&[]), "No headers");
    }

    #[test]
    fn builds_request_body_from_slint_mode() {
        assert_eq!(
            build_request_body("none", "ignored", "").unwrap(),
            RequestBody::None
        );
        assert_eq!(
            build_request_body("raw", "{\"name\":\"Zen\"}", "").unwrap(),
            RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"name\":\"Zen\"}".to_string(),
            }
        );
        assert_eq!(
            build_request_body("urlenc", "search=rust slint\nlimit: 20", "").unwrap(),
            RequestBody::FormUrlEncoded(vec![
                ("search".to_string(), "rust slint".to_string()),
                ("limit".to_string(), "20".to_string())
            ])
        );
        assert_eq!(
            build_request_body("form", "file=@/tmp/upload.txt", "").unwrap(),
            RequestBody::Multipart(vec![("file".to_string(), "@/tmp/upload.txt".to_string())])
        );
        assert_eq!(
            build_request_body("binary", "/tmp/body.bin", "").unwrap(),
            RequestBody::BinaryFile {
                path: "/tmp/body.bin".to_string(),
                content_type: None,
            }
        );
    }

    #[test]
    fn rejects_empty_binary_body_path() {
        let error = build_request_body("binary", "  ", "").expect_err("empty path");

        assert!(error.to_string().contains("path is empty"));
    }

    #[test]
    fn builds_graphql_payload_body() {
        assert_eq!(
            build_request_body(
                "graphql",
                "query User($id: ID!) { user(id: $id) { name } }",
                r#"{"id":"u_123"}"#,
            )
            .unwrap(),
            RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: r#"{"query":"query User($id: ID!) { user(id: $id) { name } }","variables":{"id":"u_123"}}"#.to_string(),
            }
        );

        let error = build_request_body("graphql", "{ viewer { id } }", "[]")
            .expect_err("invalid variables");
        assert!(error.to_string().contains("JSON object"));
    }

    #[test]
    fn builds_auth_headers_and_query_params() {
        assert_eq!(
            build_auth_entries("bearer", "secret").unwrap(),
            (
                vec![("Authorization".to_string(), "Bearer secret".to_string())],
                Vec::new()
            )
        );
        assert_eq!(
            build_auth_entries("basic", "user:pass").unwrap(),
            (
                vec![(
                    "Authorization".to_string(),
                    "Basic dXNlcjpwYXNz".to_string()
                )],
                Vec::new()
            )
        );
        assert_eq!(
            build_auth_entries("api-query", "api_key=secret").unwrap(),
            (
                Vec::new(),
                vec![("api_key".to_string(), "secret".to_string())]
            )
        );
    }

    #[test]
    fn auth_entries_override_existing_headers() {
        let mut headers = vec![
            ("accept".to_string(), "application/json".to_string()),
            ("authorization".to_string(), "old".to_string()),
        ];
        for (name, value) in build_auth_entries("jwt", "new-token").unwrap().0 {
            upsert_pair(&mut headers, name, value, true);
        }

        assert_eq!(
            headers,
            vec![
                ("accept".to_string(), "application/json".to_string()),
                ("authorization".to_string(), "Bearer new-token".to_string()),
            ]
        );
    }

    #[test]
    fn builds_variable_store_and_resolves_environment_overrides() {
        let (variables, active_environment) = build_variable_store(
            "baseUrl=https://api.example.com\ntoken=global",
            "dev",
            "baseUrl=http://localhost:8080",
        )
        .expect("variables");

        assert_eq!(active_environment.as_deref(), Some("dev"));
        assert_eq!(
            resolve_text(
                "{{baseUrl}}/users",
                &variables,
                active_environment.as_deref()
            )
            .expect("resolve"),
            "http://localhost:8080/users"
        );
        assert_eq!(
            resolve_text(
                "Bearer {{token}}",
                &variables,
                active_environment.as_deref()
            )
            .expect("resolve"),
            "Bearer global"
        );
    }

    #[test]
    fn resolves_variables_in_pairs() {
        let (variables, active_environment) =
            build_variable_store("token=secret", "", "").expect("variables");

        assert_eq!(
            resolve_pairs(
                vec![("Authorization".to_string(), "Bearer {{token}}".to_string())],
                &variables,
                active_environment.as_deref()
            )
            .expect("resolve"),
            vec![("Authorization".to_string(), "Bearer secret".to_string())]
        );
    }

    #[test]
    fn rejects_environment_variables_without_environment_name() {
        let error = build_variable_store("", "", "baseUrl=http://localhost:8080")
            .expect_err("missing env name");

        assert!(error.to_string().contains("environment name is empty"));
    }

    #[test]
    fn projects_current_slint_request_for_codegen() {
        let request = build_codegen_request_projection(&RequestProjectionInput {
            method: "POST".to_string(),
            url: "{{baseUrl}}/users".to_string(),
            query_params: "debug=true".to_string(),
            headers: "Accept: application/json".to_string(),
            auth_mode: "bearer".to_string(),
            auth_config: "{{token}}".to_string(),
            body_mode: "raw".to_string(),
            body: "{\"name\":\"{{name}}\"}".to_string(),
            graphql_variables: String::new(),
            pre_request_script: String::new(),
            global_variables: "baseUrl=https://api.example.com\ntoken=secret\nname=Zen".to_string(),
            environment_name: String::new(),
            environment_variables: String::new(),
        })
        .expect("request");

        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "https://api.example.com/users");
        assert_eq!(
            request.headers,
            vec![
                ("Accept".to_string(), "application/json".to_string()),
                ("Authorization".to_string(), "Bearer secret".to_string())
            ]
        );
        assert_eq!(
            request.query_params,
            vec![("debug".to_string(), "true".to_string())]
        );
        assert_eq!(
            request.body,
            RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"name\":\"Zen\"}".to_string(),
            }
        );
    }

    #[test]
    fn applies_pre_request_actions_before_projection_resolution() {
        let request = build_codegen_request_projection(&RequestProjectionInput {
            method: "GET".to_string(),
            url: "{{baseUrl}}/users".to_string(),
            query_params: String::new(),
            headers: String::new(),
            auth_mode: "none".to_string(),
            auth_config: String::new(),
            body_mode: "none".to_string(),
            body: String::new(),
            graphql_variables: String::new(),
            pre_request_script:
                "set_var baseUrl=http://127.0.0.1:8080\nset_header X-Mode=test\nset_query debug=true"
                    .to_string(),
            global_variables: "baseUrl=https://api.example.com".to_string(),
            environment_name: String::new(),
            environment_variables: String::new(),
        })
        .expect("request");

        assert_eq!(request.url, "http://127.0.0.1:8080/users");
        assert_eq!(
            request.headers,
            vec![("X-Mode".to_string(), "test".to_string())]
        );
        assert_eq!(
            request.query_params,
            vec![("debug".to_string(), "true".to_string())]
        );
    }

    #[test]
    fn parses_and_formats_native_test_assertions() {
        let assertions = parse_response_assertions(
            "status_equals 200\nheader_equals Content-Type application/json\njson_path_equals ok true",
        )
        .expect("assertions");

        assert_eq!(
            assertions,
            vec![
                ResponseAssertion {
                    name: "status_equals 200".to_string(),
                    kind: ResponseAssertionKind::StatusEquals { status: 200 },
                },
                ResponseAssertion {
                    name: "header_equals Content-Type application/json".to_string(),
                    kind: ResponseAssertionKind::HeaderEquals {
                        name: "Content-Type".to_string(),
                        value: "application/json".to_string(),
                    },
                },
                ResponseAssertion {
                    name: "json_path_equals ok true".to_string(),
                    kind: ResponseAssertionKind::JsonPathEquals {
                        path: "ok".to_string(),
                        value: Value::Bool(true),
                    },
                },
            ]
        );
        assert_eq!(
            format_response_assertions(&assertions),
            "status_equals 200\nheader_equals Content-Type application/json\njson_path_equals ok true"
        );
    }

    #[test]
    fn formats_single_response_assertion_results() {
        let results = vec![
            ResponseAssertionResult {
                name: "status_equals 200".to_string(),
                passed: true,
                error: None,
            },
            ResponseAssertionResult {
                name: "body_contains ok".to_string(),
                passed: false,
                error: Some("response body does not contain \"ok\"".to_string()),
            },
        ];

        assert_eq!(
            response_status_with_assertions(200, &results),
            "HTTP 200 / tests 1/2"
        );
        assert_eq!(response_tone_with_assertions(200, &results), "error");
        assert_eq!(
            response_body_with_assertions("{}", &results),
            "{}\n\nTests\n[PASS] status_equals 200\n[FAIL] body_contains ok - response body does not contain \"ok\""
        );
    }

    #[test]
    fn saves_editor_request_with_pre_request_and_tests() {
        let tests = parse_response_assertions("status_equals 201").expect("assertions");
        let request = collection_request_from_editor(
            &RequestProjectionInput {
                method: "POST".to_string(),
                url: "{{baseUrl}}/users".to_string(),
                query_params: "debug=true".to_string(),
                headers: "Accept: application/json".to_string(),
                auth_mode: "none".to_string(),
                auth_config: String::new(),
                body_mode: "raw".to_string(),
                body: "{\"name\":\"{{name}}\"}".to_string(),
                graphql_variables: String::new(),
                pre_request_script: "set_header X-Mode=test".to_string(),
                global_variables: String::new(),
                environment_name: String::new(),
                environment_variables: String::new(),
            },
            tests.clone(),
        )
        .expect("collection request");

        assert_eq!(request.url, "{{baseUrl}}/users");
        assert_eq!(request.pre_request_script, "set_header X-Mode=test");
        assert_eq!(request.tests, tests);
    }

    #[test]
    fn maps_slint_codegen_language_names() {
        assert_eq!(snippet_language("curl"), SnippetLanguage::Curl);
        assert_eq!(snippet_language("python"), SnippetLanguage::PythonRequests);
        assert_eq!(snippet_language("js"), SnippetLanguage::JavaScriptFetch);
        assert_eq!(snippet_language("rust"), SnippetLanguage::RustReqwest);
        assert_eq!(snippet_language("go"), SnippetLanguage::GoNetHttp);
        assert_eq!(snippet_language("unknown"), SnippetLanguage::Curl);
    }

    #[test]
    fn saves_codegen_snippet_to_disk() {
        let path =
            std::env::temp_dir().join(format!("zenapi-codegen-snippet-{}.txt", std::process::id()));
        let _ = fs::remove_file(&path);

        save_codegen_snippet(path.to_str().expect("utf-8 temp path"), "curl example")
            .expect("save snippet");

        assert_eq!(
            fs::read_to_string(&path).expect("saved snippet"),
            "curl example"
        );
        assert!(save_codegen_snippet("   ", "curl example").is_err());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn saves_projected_request_as_collection_request() {
        let request = CodegenRequest {
            method: "POST".to_string(),
            url: "https://api.example.com/users".to_string(),
            headers: vec![("Authorization".to_string(), "Bearer secret".to_string())],
            query_params: vec![("debug".to_string(), "true".to_string())],
            body: RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"name\":\"Zen\"}".to_string(),
            },
        };

        let saved = collection_request_from_codegen(&request);

        assert_eq!(saved.name, "POST https://api.example.com/users");
        assert_eq!(saved.method, "POST");
        assert_eq!(saved.url, "https://api.example.com/users");
        assert_eq!(
            saved.headers,
            vec![NameValue {
                name: "Authorization".to_string(),
                value: "Bearer secret".to_string(),
            }]
        );
        assert_eq!(
            saved.query_params,
            vec![NameValue {
                name: "debug".to_string(),
                value: "true".to_string(),
            }]
        );
        assert_eq!(
            saved.body,
            CollectionBody::Raw {
                content_type: "application/json".to_string(),
                body: "{\"name\":\"Zen\"}".to_string(),
            }
        );
    }

    #[test]
    fn finds_nested_collection_requests_by_flattened_row_id() {
        let collection = ApiCollection {
            name: "Demo".to_string(),
            description: String::new(),
            items: vec![
                CollectionItem::Folder(zenapi::collections::CollectionFolder {
                    name: "Users".to_string(),
                    description: String::new(),
                    items: vec![
                        CollectionItem::Request(saved_request(
                            "List users",
                            "GET",
                            "https://api.example.com/users",
                        )),
                        CollectionItem::Request(saved_request(
                            "Create user",
                            "POST",
                            "https://api.example.com/users",
                        )),
                    ],
                }),
                CollectionItem::Request(saved_request(
                    "Health",
                    "GET",
                    "https://api.example.com/health",
                )),
            ],
        };

        assert_eq!(count_collection_requests(&collection.items), 3);
        assert_eq!(
            collection_request_at(&collection, 0).map(|request| request.name.as_str()),
            Some("List users")
        );
        assert_eq!(
            collection_request_at(&collection, 1).map(|request| request.name.as_str()),
            Some("Create user")
        );
        assert_eq!(
            collection_request_at(&collection, 2).map(|request| request.name.as_str()),
            Some("Health")
        );
        assert!(collection_request_at(&collection, 3).is_none());
    }

    #[test]
    fn removes_nested_collection_requests_by_flattened_row_id() {
        let mut collection = ApiCollection {
            name: "Demo".to_string(),
            description: String::new(),
            items: vec![
                CollectionItem::Folder(zenapi::collections::CollectionFolder {
                    name: "Users".to_string(),
                    description: String::new(),
                    items: vec![
                        CollectionItem::Request(saved_request(
                            "List users",
                            "GET",
                            "https://api.example.com/users",
                        )),
                        CollectionItem::Request(saved_request(
                            "Create user",
                            "POST",
                            "https://api.example.com/users",
                        )),
                    ],
                }),
                CollectionItem::Request(saved_request(
                    "Health",
                    "GET",
                    "https://api.example.com/health",
                )),
            ],
        };

        let removed = remove_collection_request_at(&mut collection, 1).expect("removed request");

        assert_eq!(removed.name, "Create user");
        assert_eq!(count_collection_requests(&collection.items), 2);
        assert_eq!(
            collection_request_at(&collection, 1).map(|request| request.name.as_str()),
            Some("Health")
        );
        assert!(remove_collection_request_at(&mut collection, 5).is_none());
        assert_eq!(count_collection_requests(&collection.items), 2);
    }

    #[test]
    fn duplicates_nested_collection_requests_by_flattened_row_id() {
        let mut collection = ApiCollection {
            name: "Demo".to_string(),
            description: String::new(),
            items: vec![
                CollectionItem::Folder(zenapi::collections::CollectionFolder {
                    name: "Users".to_string(),
                    description: String::new(),
                    items: vec![
                        CollectionItem::Request(saved_request(
                            "List users",
                            "GET",
                            "https://api.example.com/users",
                        )),
                        CollectionItem::Request(saved_request(
                            "Create user",
                            "POST",
                            "https://api.example.com/users",
                        )),
                    ],
                }),
                CollectionItem::Request(saved_request(
                    "Health",
                    "GET",
                    "https://api.example.com/health",
                )),
            ],
        };

        let duplicate =
            duplicate_collection_request_at(&mut collection, 1).expect("duplicated request");

        assert_eq!(duplicate.name, "Create user Copy");
        assert_eq!(count_collection_requests(&collection.items), 4);
        assert_eq!(
            collection_request_at(&collection, 2).map(|request| request.name.as_str()),
            Some("Create user Copy")
        );
        assert_eq!(
            collection_request_at(&collection, 3).map(|request| request.name.as_str()),
            Some("Health")
        );
        assert!(duplicate_collection_request_at(&mut collection, 9).is_none());
    }

    #[test]
    fn maps_collection_body_and_fields_back_to_slint_editors() {
        assert_eq!(
            format_name_values(&[
                NameValue {
                    name: "Accept".to_string(),
                    value: "application/json".to_string(),
                },
                NameValue {
                    name: "X-Request-Id".to_string(),
                    value: "abc".to_string(),
                },
            ]),
            "Accept=application/json\nX-Request-Id=abc"
        );
        assert_eq!(
            collection_body_to_slint(&CollectionBody::UrlEncoded {
                fields: vec![NameValue {
                    name: "search".to_string(),
                    value: "slint".to_string(),
                }],
            }),
            ("urlenc".to_string(), "search=slint".to_string())
        );
        assert_eq!(
            collection_body_to_slint(&CollectionBody::Binary {
                path: "/tmp/body.bin".to_string(),
                content_type: "application/octet-stream".to_string(),
            }),
            ("binary".to_string(), "/tmp/body.bin".to_string())
        );
    }

    #[test]
    fn parses_runner_options_from_slint_controls() {
        assert_eq!(
            runner_options("25", true).unwrap(),
            RunnerOptions {
                delay_ms: 25,
                failure_strategy: FailureStrategy::StopOnFailure,
            }
        );
        assert_eq!(
            runner_options(" ", false).unwrap(),
            RunnerOptions {
                delay_ms: 0,
                failure_strategy: FailureStrategy::Continue,
            }
        );

        let error = runner_options("soon", false).expect_err("invalid delay");
        assert!(error.to_string().contains("non-negative integer"));
    }

    #[test]
    fn formats_runner_summary_for_response_panel() {
        let result = CollectionRunResult {
            index: 0,
            path: vec!["Demo".to_string(), "Health".to_string()],
            name: "Health".to_string(),
            method: "GET".to_string(),
            url: "https://api.example.com/health".to_string(),
            status: Some(200),
            success: true,
            elapsed_ms: 12,
            body_bytes: 42,
            pre_request_actions: Vec::new(),
            assertions: Vec::new(),
            error: None,
        };
        let summary = CollectionRunSummary {
            collection_name: "Demo".to_string(),
            total: 1,
            passed: 1,
            failed: 0,
            stopped_early: false,
            elapsed_ms: 15,
            results: vec![result],
        };

        assert_eq!(runner_response_tone(&summary), "success");
        assert_eq!(runner_response_status(&summary), "Runner passed");
        assert_eq!(
            runner_summary_line(&summary),
            "Demo: 1 passed, 0 failed, 1 total / 15 ms"
        );
        assert_eq!(
            format_runner_summary(&summary),
            "Demo: 1 passed, 0 failed, 1 total / 15 ms\n[PASS] HTTP 200 GET https://api.example.com/health (Demo / Health)"
        );
    }

    #[test]
    fn formats_failed_runner_result_details() {
        let result = CollectionRunResult {
            index: 0,
            path: vec!["Demo".to_string(), "Create".to_string()],
            name: "Create".to_string(),
            method: "POST".to_string(),
            url: "https://api.example.com/users".to_string(),
            status: None,
            success: false,
            elapsed_ms: 0,
            body_bytes: 0,
            pre_request_actions: vec!["set header X-Debug".to_string()],
            assertions: Vec::new(),
            error: Some("connection refused".to_string()),
        };

        assert_eq!(runner_result_status(&result), "FAIL");
        assert_eq!(
            runner_result_detail(&result),
            "ERR / 0 ms / 0 B / pre 1 / connection refused"
        );
        assert_eq!(
            format_runner_result(&result),
            "[FAIL] ERR POST https://api.example.com/users (Demo / Create) - connection refused - pre-request 1"
        );
    }

    #[test]
    fn parses_realtime_event_limits() {
        assert_eq!(parse_positive_usize("3", "SSE events").unwrap(), 3);
        assert!(
            parse_positive_usize("0", "SSE events")
                .expect_err("zero")
                .to_string()
                .contains("greater than zero")
        );
        assert!(
            parse_positive_usize("many", "SSE events")
                .expect_err("invalid")
                .to_string()
                .contains("positive integer")
        );
    }

    #[test]
    fn formats_websocket_and_sse_output() {
        let websocket = client::WebSocketExchange {
            url: "ws://localhost/socket".to_string(),
            sent: "hello".to_string(),
            received: vec![client::WebSocketMessage {
                kind: client::WebSocketMessageKind::Text,
                data: "echo:hello".to_string(),
            }],
            elapsed_ms: 12,
        };
        assert_eq!(
            format_websocket_exchange(&websocket),
            "URL: ws://localhost/socket\nSent: hello\nReceived 1 [text]: echo:hello"
        );

        let sse = client::SseExchange {
            url: "http://localhost/events".to_string(),
            events: vec![client::SseEvent {
                event: Some("update".to_string()),
                data: "{\"ok\":true}".to_string(),
                id: Some("42".to_string()),
                retry: None,
            }],
            elapsed_ms: 14,
        };
        assert_eq!(
            format_sse_exchange(&sse),
            "URL: http://localhost/events\n1. update / id 42\n{\"ok\":true}"
        );
    }

    #[test]
    fn formats_grpc_draft_for_response_panel() {
        let draft = build_grpc_request_draft(
            "http://localhost:50051",
            "demo.Users/GetUser",
            "authorization=Bearer token",
            r#"{"id":"u_123"}"#,
        )
        .expect("draft");

        assert_eq!(
            format_grpc_draft(&draft),
            "Endpoint: http://localhost:50051\nMethod: /demo.Users/GetUser\n\nMetadata\nauthorization: Bearer token\n\nMessage\n{\n  \"id\": \"u_123\"\n}"
        );
    }

    #[test]
    fn keeps_recent_mock_logs_first_and_bounded() {
        let mut logs = Vec::new();
        push_mock_log(
            &mut logs,
            MockRequestLog {
                method: "GET".to_string(),
                path: "/old".to_string(),
                status: 200,
            },
            2,
        );
        push_mock_log(
            &mut logs,
            MockRequestLog {
                method: "POST".to_string(),
                path: "/new".to_string(),
                status: 404,
            },
            2,
        );
        push_mock_log(
            &mut logs,
            MockRequestLog {
                method: "PUT".to_string(),
                path: "/latest".to_string(),
                status: 204,
            },
            2,
        );

        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].path, "/latest");
        assert_eq!(logs[1].path, "/new");
    }

    #[test]
    fn filters_mock_logs_for_sidebar_model() {
        use slint::Model;

        let logs = vec![
            MockRequestLog {
                method: "GET".to_string(),
                path: "/users".to_string(),
                status: 200,
            },
            MockRequestLog {
                method: "POST".to_string(),
                path: "/sessions".to_string(),
                status: 404,
            },
        ];

        let by_path = filtered_mock_log_model(&logs, "sessions");
        assert_eq!(by_path.row_count(), 1);
        assert_eq!(
            by_path.row_data(0).expect("mock log").method.as_str(),
            "POST"
        );

        let by_status = filtered_mock_log_model(&logs, "200");
        assert_eq!(by_status.row_count(), 1);
        assert_eq!(
            by_status.row_data(0).expect("mock log").path.as_str(),
            "/users"
        );
    }

    #[test]
    fn saves_filtered_mock_logs_to_disk() {
        let logs = vec![
            MockRequestLog {
                method: "GET".to_string(),
                path: "/users".to_string(),
                status: 200,
            },
            MockRequestLog {
                method: "POST".to_string(),
                path: "/sessions".to_string(),
                status: 404,
            },
        ];
        let path = std::env::temp_dir().join(format!(
            "zenapi-mock-log-export-{}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);

        let count = save_mock_logs(path.to_str().expect("utf-8 temp path"), &logs, "sessions")
            .expect("save mock logs");
        let exported: Vec<MockRequestLog> =
            serde_json::from_str(&fs::read_to_string(&path).expect("mock log export"))
                .expect("mock log json");

        assert_eq!(count, 1);
        assert_eq!(exported[0].path, "/sessions");
        assert!(save_mock_logs("   ", &logs, "").is_err());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn builds_history_request_from_projected_request() {
        let request = CodegenRequest {
            method: "POST".to_string(),
            url: "https://api.example.com/users".to_string(),
            headers: vec![("Authorization".to_string(), "Bearer token".to_string())],
            query_params: vec![("debug".to_string(), "true".to_string())],
            body: RequestBody::FormUrlEncoded(vec![
                ("name".to_string(), "Zen".to_string()),
                ("role".to_string(), "admin".to_string()),
            ]),
        };
        let input = RequestProjectionInput {
            method: "POST".to_string(),
            url: "{{baseUrl}}/users".to_string(),
            query_params: "debug=true".to_string(),
            headers: String::new(),
            auth_mode: "bearer".to_string(),
            auth_config: "{{token}}".to_string(),
            body_mode: "urlenc".to_string(),
            body: "name=Zen\nrole=admin".to_string(),
            graphql_variables: String::new(),
            pre_request_script: "set_header X-Trace=yes".to_string(),
            global_variables: "baseUrl=https://api.example.com\ntoken=secret".to_string(),
            environment_name: String::new(),
            environment_variables: String::new(),
        };

        assert_eq!(
            history_request(&request, &input, "status_equals 201"),
            HistoryRequest {
                method: "POST".to_string(),
                url: "https://api.example.com/users".to_string(),
                query_params: vec![("debug".to_string(), "true".to_string())],
                headers: vec![("Authorization".to_string(), "Bearer token".to_string())],
                auth_mode: "bearer".to_string(),
                auth_config: "{{token}}".to_string(),
                body_kind: "urlenc".to_string(),
                body_preview: "name=Zen\nrole=admin".to_string(),
                pre_request_script: "set_header X-Trace=yes".to_string(),
                request_tests: "status_equals 201".to_string(),
            }
        );
    }

    #[test]
    fn filters_history_rows_for_sidebar_model() {
        use slint::Model;

        let mut history = RequestHistory::new();
        history.record_at(
            1,
            HistoryRequest {
                method: "GET".to_string(),
                url: "https://api.example.com/users".to_string(),
                query_params: Vec::new(),
                headers: Vec::new(),
                auth_mode: "none".to_string(),
                auth_config: String::new(),
                body_kind: "none".to_string(),
                body_preview: String::new(),
                pre_request_script: String::new(),
                request_tests: String::new(),
            },
            HistoryResponse {
                status: "HTTP 200".to_string(),
                meta: "12 ms".to_string(),
                body_preview: "{}".to_string(),
            },
        );
        history.record_at(
            2,
            HistoryRequest {
                method: "POST".to_string(),
                url: "https://api.example.com/sessions".to_string(),
                query_params: Vec::new(),
                headers: Vec::new(),
                auth_mode: "none".to_string(),
                auth_config: String::new(),
                body_kind: "raw".to_string(),
                body_preview: "{}".to_string(),
                pre_request_script: String::new(),
                request_tests: String::new(),
            },
            HistoryResponse {
                status: "HTTP 401".to_string(),
                meta: "auth".to_string(),
                body_preview: "{}".to_string(),
            },
        );

        let model = filtered_history_model(&history, "sessions");

        assert_eq!(model.row_count(), 1);
        let row = model.row_data(0).expect("history row");
        assert_eq!(row.method.as_str(), "POST");
        assert_eq!(row.status.as_str(), "HTTP 401");
    }

    #[test]
    fn deletes_history_entry_and_refreshes_filtered_rows() {
        use slint::Model;

        let mut history = RequestHistory::new();
        let users_id = history.record_at(
            1,
            HistoryRequest {
                method: "GET".to_string(),
                url: "https://api.example.com/users".to_string(),
                query_params: Vec::new(),
                headers: Vec::new(),
                auth_mode: "none".to_string(),
                auth_config: String::new(),
                body_kind: "none".to_string(),
                body_preview: String::new(),
                pre_request_script: String::new(),
                request_tests: String::new(),
            },
            HistoryResponse {
                status: "HTTP 200".to_string(),
                meta: "12 ms".to_string(),
                body_preview: "{}".to_string(),
            },
        );
        history.record_at(
            2,
            HistoryRequest {
                method: "POST".to_string(),
                url: "https://api.example.com/sessions".to_string(),
                query_params: Vec::new(),
                headers: Vec::new(),
                auth_mode: "none".to_string(),
                auth_config: String::new(),
                body_kind: "raw".to_string(),
                body_preview: "{}".to_string(),
                pre_request_script: String::new(),
                request_tests: String::new(),
            },
            HistoryResponse {
                status: "HTTP 401".to_string(),
                meta: "auth".to_string(),
                body_preview: "{}".to_string(),
            },
        );

        let model = delete_history_entry(&mut history, users_id, "users").expect("delete model");

        assert_eq!(model.row_count(), 0);
        assert!(history.find(users_id).is_none());
        assert!(delete_history_entry(&mut history, users_id, "").is_none());
    }

    #[test]
    fn previews_request_body_modes_for_history() {
        assert_eq!(
            request_body_preview(&RequestBody::None),
            ("none".to_string(), String::new())
        );
        assert_eq!(
            request_body_preview(&RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"ok\":true}".to_string(),
            }),
            ("raw".to_string(), "{\"ok\":true}".to_string())
        );
        assert_eq!(
            request_body_preview(&RequestBody::Multipart(vec![(
                "file".to_string(),
                "@/tmp/upload.txt".to_string()
            )])),
            ("form".to_string(), "file=@/tmp/upload.txt".to_string())
        );
        assert_eq!(
            request_body_preview(&RequestBody::BinaryFile {
                path: "/tmp/body.bin".to_string(),
                content_type: None,
            }),
            ("binary".to_string(), "/tmp/body.bin".to_string())
        );
    }

    #[test]
    fn truncates_long_history_response_previews() {
        assert_eq!(truncate_preview("abc", 3), "abc");
        assert_eq!(truncate_preview("abcdef", 3), "abc\n...");
    }
}

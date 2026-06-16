use anyhow::{Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use zenapi::{
    client::{self, ClientResponse, RequestBody},
    codegen::{CodegenRequest, SnippetLanguage, generate_snippet},
    collection_runner::{
        CollectionRunResult, CollectionRunSummary, FailureStrategy, RunnerOptions, run_collection,
    },
    collections::{ApiCollection, CollectionBody, CollectionItem, CollectionRequest, NameValue},
    history::{HistoryRequest, HistoryResponse, RequestHistory},
    mock_server::MockServer,
    openapi::{ApiRoute, ApiSpec, load_openapi_file},
    variables::{Variable, VariableStore, replace_variables},
};

use crate::ui::{AppWindow, CollectionRow, HistoryRow, RouteRow, RunnerRow};

pub fn run() -> Result<()> {
    let runtime = Arc::new(Runtime::new()?);
    let state = Arc::new(Mutex::new(AppState::default()));
    let app = AppWindow::new().map_err(|err| anyhow!(err.to_string()))?;

    wire_import(&app, runtime.clone(), state.clone());
    wire_route_filter(&app, state.clone());
    wire_route_selection(&app, state.clone());
    wire_history_selection(&app, state.clone());
    wire_collection_actions(&app, state.clone());
    wire_request_sender(&app, runtime.clone(), state.clone());
    wire_codegen(&app);
    wire_collection_runner(&app, runtime.clone(), state.clone());
    wire_mock_server(&app, runtime, state);

    app.run().map_err(|err| anyhow!(err.to_string()))
}

struct AppState {
    routes: Vec<ApiRoute>,
    visible_routes: Vec<ApiRoute>,
    collection: ApiCollection,
    history: RequestHistory,
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
        global_variables: app.get_global_variables().to_string(),
        environment_name: app.get_environment_name().to_string(),
        environment_variables: app.get_environment_variables().to_string(),
    }
}

impl AppState {
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
            app.set_body_mode(entry.request.body_kind.into());
            app.set_request_body(entry.request.body_preview.into());
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

        let request = match build_codegen_request_projection(&request_projection_input(&app)) {
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
        let collection_request = collection_request_from_codegen(&request);
        let request_name = collection_request.name.clone();

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
            &request.url,
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

fn wire_request_sender(app: &AppWindow, runtime: Arc<Runtime>, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    app.on_send_request(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let request = match build_codegen_request_projection(&request_projection_input(&app)) {
            Ok(request) => request,
            Err(error) => {
                set_response(&app, "Build failed", "", "error", &error.to_string());
                return;
            }
        };

        let method = request.method.clone();
        let url = request.url.clone();
        let headers = request.headers.clone();
        let query_params = request.query_params.clone();
        let body = request.body.clone();

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
                        let response_status = format!("HTTP {}", response.status);
                        let response_meta =
                            format!("{} ms / {} B", response.elapsed_ms, response.body_bytes);
                        record_history(&state, &request, success_history_response(&response));
                        if let Ok(state) = state.lock() {
                            app.set_history_rows(history_model(state.history.entries()));
                        }
                        set_response(
                            &app,
                            &response_status,
                            &response_meta,
                            response_tone(response.status),
                            &response.body,
                        );
                        app.set_response_headers(response_headers.into());
                    }
                    Err(error) => {
                        let body = error.to_string();
                        record_history(
                            &state,
                            &request,
                            HistoryResponse {
                                status: "ERR".to_string(),
                                meta: String::new(),
                                body_preview: truncate_preview(&body, 1200),
                            },
                        );
                        if let Ok(state) = state.lock() {
                            app.set_history_rows(history_model(state.history.entries()));
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

                    let result = MockServer::start(routes, 8080).await;
                    let _ = slint::invoke_from_event_loop(move || {
                        let Some(app) = weak_app.upgrade() else {
                            return;
                        };

                        match result {
                            Ok(server) => {
                                let addr = server.addr();
                                if let Ok(mut guard) = state.lock() {
                                    guard.server = Some(server);
                                }
                                app.set_server_running(true);
                                app.set_server_status(addr.to_string().into());
                            }
                            Err(error) => {
                                app.set_server_running(false);
                                app.set_server_status("Failed".into());
                                set_response(
                                    &app,
                                    "Mock server failed",
                                    "",
                                    "error",
                                    &error.to_string(),
                                );
                            }
                        }
                        app.set_activity("".into());
                        app.set_busy(false);
                    });
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

fn history_model(entries: &[zenapi::history::HistoryEntry]) -> ModelRc<HistoryRow> {
    ModelRc::new(VecModel::from_iter(entries.iter().map(|entry| {
        HistoryRow {
            id: entry.id as i32,
            method: entry.request.method.clone().into(),
            url: entry.request.url.clone().into(),
            status: entry.response.status.clone().into(),
        }
    })))
}

fn record_history(
    state: &Arc<Mutex<AppState>>,
    request: &CodegenRequest,
    response: HistoryResponse,
) {
    if let Ok(mut state) = state.lock() {
        state.history.record(history_request(request), response);
    }
}

fn history_request(request: &CodegenRequest) -> HistoryRequest {
    let (body_kind, body_preview) = request_body_preview(&request.body);
    HistoryRequest {
        method: request.method.clone(),
        url: request.url.clone(),
        body_kind,
        body_preview,
    }
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
    let url = resolve_text(&input.url, &variables, active_environment)?
        .trim()
        .to_string();
    if url.is_empty() {
        bail!("request URL is empty");
    }

    let mut headers = resolve_pairs(
        parse_key_value_lines(&input.headers, "header")?,
        &variables,
        active_environment,
    )?;
    let mut query_params = resolve_pairs(
        parse_key_value_lines(&input.query_params, "query param")?,
        &variables,
        active_environment,
    )?;
    let auth_config = resolve_text(&input.auth_config, &variables, active_environment)?;
    let (auth_headers, auth_query_params) = build_auth_entries(&input.auth_mode, &auth_config)?;
    for (name, value) in auth_headers {
        upsert_pair(&mut headers, name, value, true);
    }
    for (name, value) in auth_query_params {
        upsert_pair(&mut query_params, name, value, false);
    }
    let request_body = resolve_text(&input.body, &variables, active_environment)?;

    Ok(CodegenRequest {
        method: input.method.clone(),
        url,
        headers,
        query_params,
        body: build_request_body(&input.body_mode, &request_body)?,
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

fn build_request_body(mode: &str, input: &str) -> Result<RequestBody> {
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
        "graphql" => Ok(RequestBody::Raw {
            content_type: Some("application/json".to_string()),
            body: input.to_string(),
        }),
        _ => Ok(RequestBody::Raw {
            content_type: Some("application/json".to_string()),
            body: input.to_string(),
        }),
    }
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
            build_request_body("none", "ignored").unwrap(),
            RequestBody::None
        );
        assert_eq!(
            build_request_body("raw", "{\"name\":\"Zen\"}").unwrap(),
            RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"name\":\"Zen\"}".to_string(),
            }
        );
        assert_eq!(
            build_request_body("urlenc", "search=rust slint\nlimit: 20").unwrap(),
            RequestBody::FormUrlEncoded(vec![
                ("search".to_string(), "rust slint".to_string()),
                ("limit".to_string(), "20".to_string())
            ])
        );
        assert_eq!(
            build_request_body("form", "file=@/tmp/upload.txt").unwrap(),
            RequestBody::Multipart(vec![("file".to_string(), "@/tmp/upload.txt".to_string())])
        );
        assert_eq!(
            build_request_body("binary", "/tmp/body.bin").unwrap(),
            RequestBody::BinaryFile {
                path: "/tmp/body.bin".to_string(),
                content_type: None,
            }
        );
    }

    #[test]
    fn rejects_empty_binary_body_path() {
        let error = build_request_body("binary", "  ").expect_err("empty path");

        assert!(error.to_string().contains("path is empty"));
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
    fn maps_slint_codegen_language_names() {
        assert_eq!(snippet_language("curl"), SnippetLanguage::Curl);
        assert_eq!(snippet_language("python"), SnippetLanguage::PythonRequests);
        assert_eq!(snippet_language("js"), SnippetLanguage::JavaScriptFetch);
        assert_eq!(snippet_language("rust"), SnippetLanguage::RustReqwest);
        assert_eq!(snippet_language("go"), SnippetLanguage::GoNetHttp);
        assert_eq!(snippet_language("unknown"), SnippetLanguage::Curl);
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
    fn builds_history_request_from_projected_request() {
        let request = CodegenRequest {
            method: "POST".to_string(),
            url: "https://api.example.com/users".to_string(),
            headers: Vec::new(),
            query_params: Vec::new(),
            body: RequestBody::FormUrlEncoded(vec![
                ("name".to_string(), "Zen".to_string()),
                ("role".to_string(), "admin".to_string()),
            ]),
        };

        assert_eq!(
            history_request(&request),
            HistoryRequest {
                method: "POST".to_string(),
                url: "https://api.example.com/users".to_string(),
                body_kind: "urlenc".to_string(),
                body_preview: "name=Zen\nrole=admin".to_string(),
            }
        );
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

use anyhow::{Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use copypasta::{ClipboardContext, ClipboardProvider};
use serde_json::Value;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::{runtime::Runtime, sync::mpsc, task::JoinHandle};
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

use crate::ui::{
    AppWindow, CollectionRow, HistoryRow, KeyValueTableRow, MockLogRow, RouteRow, RunnerRow,
    TestAssertionRow, VariableTableRow,
};

const HISTORY_FILE_NAME: &str = ".zenapi-history.json";
const MAX_SSE_STREAM_EVENTS: usize = 200;

pub fn run() -> Result<()> {
    let runtime = Arc::new(Runtime::new()?);
    let mut initial_state = AppState::default();
    let history_load_error = initial_state.load_history_from_disk().err();
    let app = AppWindow::new().map_err(|err| anyhow!(err.to_string()))?;
    refresh_variable_table(&app);
    refresh_query_param_rows(&app);
    refresh_header_rows(&app);
    refresh_auth_key_rows(&app);
    refresh_basic_auth_fields(&app);
    refresh_body_field_rows(&app);
    refresh_test_assertion_rows(&app);
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

    wire_page_navigation(&app);
    wire_import(&app, runtime.clone(), state.clone());
    wire_route_filter(&app, state.clone());
    wire_route_selection(&app, state.clone());
    wire_history_selection(&app, state.clone());
    wire_history_actions(&app, state.clone());
    wire_collection_actions(&app, state.clone());
    wire_mock_log_filter(&app, state.clone());
    wire_mock_response_actions(&app, state.clone());
    wire_header_helpers(&app);
    wire_query_param_actions(&app);
    wire_auth_key_actions(&app);
    wire_body_field_actions(&app);
    wire_test_assertion_actions(&app);
    wire_environment_actions(&app);
    wire_request_sender(&app, runtime.clone(), state.clone());
    wire_response_actions(&app);
    wire_graphql_helpers(&app);
    wire_codegen(&app);
    wire_collection_runner(&app, runtime.clone(), state.clone());
    wire_realtime_actions(&app, runtime.clone());
    wire_grpc_draft(&app);
    wire_mock_server(&app, runtime, state);

    app.run().map_err(|err| anyhow!(err.to_string()))
}

fn wire_page_navigation(app: &AppWindow) {
    let weak_app = app.as_weak();

    app.on_navigate_page(move |page_index| {
        if !(0..=10).contains(&page_index) {
            return;
        }

        let Some(app) = weak_app.upgrade() else {
            return;
        };

        app.set_active_page_index(page_index);
    });
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
    raw_body_subtype: String,
    body: String,
    graphql_variables: String,
    pre_request_script: String,
    global_variables: String,
    environment_name: String,
    environment_variables: String,
}

#[derive(Debug, Default)]
struct EnvironmentProfiles {
    active_name: String,
    values_by_name: BTreeMap<String, String>,
}

impl EnvironmentProfiles {
    fn new(active_name: &str, active_values: &str) -> Self {
        let mut profiles = Self {
            active_name: active_name.trim().to_string(),
            values_by_name: BTreeMap::new(),
        };
        profiles.save_active(active_values);
        profiles
    }

    fn switch_to(&mut self, next_name: &str, current_values: &str) -> String {
        self.save_active(current_values);
        self.active_name = next_name.trim().to_string();
        self.values_by_name
            .get(&self.active_name)
            .cloned()
            .unwrap_or_default()
    }

    fn save_active(&mut self, values: &str) {
        let active_name = self.active_name.trim();
        if !active_name.is_empty() {
            self.values_by_name
                .insert(active_name.to_string(), values.to_string());
        }
    }

    fn set_active_name(&mut self, active_name: &str) {
        self.active_name = active_name.trim().to_string();
    }
}

#[derive(Clone, Copy)]
struct GraphqlTemplate {
    name: &'static str,
    query: &'static str,
    variables: &'static str,
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
        raw_body_subtype: app.get_raw_body_subtype().to_string(),
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
            collection: ApiCollection::new("Users"),
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
                        clear_selected_mock_route(&app);
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
        clear_selected_mock_route(&app);
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
            set_selected_mock_route(&app, &route);
            app.set_method(SharedString::from(route.method));
            app.set_url(SharedString::from(format!(
                "http://127.0.0.1:8080{}",
                route.path
            )));
            app.set_query_params("".into());
            refresh_query_param_rows(&app);
            app.set_request_headers("Accept: application/json".into());
            refresh_header_rows(&app);
            app.set_body_mode(default_body_mode(&app.get_method()).into());
            app.set_raw_body_subtype("json".into());
            app.set_request_body(default_request_body(&app.get_method()).into());
            refresh_body_field_rows(&app);
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
            let (body_mode, raw_body_subtype) = history_body_to_slint(&entry.request);
            app.set_method(entry.request.method.into());
            app.set_url(entry.request.url.into());
            app.set_query_params(format_key_value_preview(&entry.request.query_params).into());
            refresh_query_param_rows(&app);
            app.set_request_headers(format_key_value_preview(&entry.request.headers).into());
            refresh_header_rows(&app);
            app.set_auth_mode(normalized_history_auth_mode(&entry.request.auth_mode).into());
            app.set_auth_config(entry.request.auth_config.into());
            refresh_auth_key_rows(&app);
            refresh_basic_auth_fields(&app);
            app.set_body_mode(body_mode.into());
            app.set_raw_body_subtype(raw_body_subtype.into());
            app.set_request_body(entry.request.body_preview.into());
            refresh_body_field_rows(&app);
            app.set_graphql_variables("{}".into());
            app.set_pre_request_script(entry.request.pre_request_script.into());
            app.set_request_tests(entry.request.request_tests.into());
            refresh_test_assertion_rows(&app);
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
                app.set_selected_collection_request(-1);
                app.set_collection_request_name("".into());
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
        app.set_selected_collection_request(request_count.saturating_sub(1) as i32);
        app.set_collection_request_name(request_name.clone().into());
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
        app.set_selected_collection_request(id + 1);
        app.set_collection_request_name(duplicate.name.clone().into());
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
        let selected = app.get_selected_collection_request();
        if selected == id {
            app.set_selected_collection_request(-1);
            app.set_collection_request_name("".into());
        } else if selected > id {
            app.set_selected_collection_request(selected - 1);
        }
        set_response(
            &app,
            "Collection request deleted",
            &removed.name,
            "neutral",
            &removed.url,
        );
    });

    let weak_app = app.as_weak();
    let rename_state = state.clone();
    app.on_rename_collection_request(move |id, name| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() || id < 0 {
            return;
        }

        let name = name.trim();
        if name.is_empty() {
            app.set_collection_status("Rename failed".into());
            set_response(
                &app,
                "Collection rename failed",
                "",
                "error",
                "Enter a request name before renaming.",
            );
            return;
        }

        let Some((renamed, request_count, rows)) =
            rename_state.lock().ok().and_then(|mut state| {
                rename_collection_request_at(&mut state.collection, id as usize, name).map(
                    |request| {
                        let request_count = count_collection_requests(&state.collection.items);
                        let rows = collection_model(&state.collection);
                        (request, request_count, rows)
                    },
                )
            })
        else {
            app.set_collection_status("Rename failed".into());
            set_response(
                &app,
                "Collection rename failed",
                "",
                "error",
                "Select a saved request to rename.",
            );
            return;
        };

        app.set_collection_rows(rows);
        app.set_collection_status(format!("Renamed {request_count} requests").into());
        app.set_collection_request_name(renamed.name.clone().into());
        set_response(
            &app,
            "Collection request renamed",
            &renamed.name,
            "success",
            &renamed.url,
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
            app.set_selected_collection_request(id);
            app.set_collection_request_name(request.name.clone().into());
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
    let clear_state = state.clone();
    app.on_clear_mock_logs(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let Some(cleared) = clear_state.lock().ok().map(|mut state| {
            let cleared = clear_mock_logs(&mut state.mock_logs);
            cleared
        }) else {
            return;
        };

        app.set_mock_log_filter("".into());
        app.set_mock_logs(mock_log_model(&[]));
        set_response(
            &app,
            "Mock logs cleared",
            "",
            "neutral",
            &format!("{cleared} mock log entries removed."),
        );
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

fn wire_mock_response_actions(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    app.on_save_mock_response(move |body| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let selected_route = app.get_selected_route();
        let result = state
            .lock()
            .map_err(|_| anyhow!("mock route state is unavailable"))
            .and_then(|mut state| {
                update_selected_mock_response(&mut state, selected_route, body.as_str())
            });

        match result {
            Ok(route) => {
                set_selected_mock_route(&app, &route);
                set_response(
                    &app,
                    "Mock response saved",
                    route.path.as_str(),
                    "success",
                    &pretty_json(&route.mock_body),
                );
                app.set_activity(if app.get_server_running() {
                    "Mock response saved; restart server to apply".into()
                } else {
                    "Mock response saved".into()
                });
            }
            Err(error) => {
                set_response(
                    &app,
                    "Mock response save failed",
                    "",
                    "error",
                    &error.to_string(),
                );
            }
        }
    });
}

fn wire_header_helpers(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_apply_header_preset(move |preset| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match apply_header_preset(&app.get_request_headers(), preset.as_str()) {
            Ok(headers) => {
                app.set_request_headers(headers.into());
                refresh_header_rows(&app);
                set_response(
                    &app,
                    "Header preset applied",
                    preset.as_str(),
                    "success",
                    "Request headers updated.",
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Header preset failed",
                    preset.as_str(),
                    "error",
                    &error.to_string(),
                );
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_copy_headers(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        match copy_text_to_clipboard(app.get_request_headers().as_str()) {
            Ok(()) => app.set_activity("Copied request headers".into()),
            Err(error) => app.set_activity(format!("Copy headers failed: {error}").into()),
        }
    });

    let weak_app = app.as_weak();
    app.on_paste_headers(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let result = read_text_from_clipboard().and_then(|clipboard| {
            merge_key_value_text(
                app.get_request_headers().as_str(),
                clipboard.as_str(),
                "header",
                true,
                true,
            )
        });
        match result {
            Ok((headers, count)) => {
                app.set_request_headers(headers.into());
                refresh_header_rows(&app);
                set_response(
                    &app,
                    "Headers pasted",
                    "",
                    "success",
                    &format!("{count} header rows imported from clipboard."),
                );
            }
            Err(error) => {
                set_response(&app, "Header paste failed", "", "error", &error.to_string())
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_import_headers(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match merge_key_value_file(
            app.get_request_headers().as_str(),
            path.as_str(),
            "header",
            true,
            true,
        ) {
            Ok((headers, count)) => {
                app.set_request_headers(headers.into());
                refresh_header_rows(&app);
                set_response(
                    &app,
                    "Headers imported",
                    path.as_str(),
                    "success",
                    &format!("{count} header rows imported."),
                );
            }
            Err(error) => set_response(
                &app,
                "Header import failed",
                path.as_str(),
                "error",
                &error.to_string(),
            ),
        }
    });

    let weak_app = app.as_weak();
    app.on_add_header(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = add_key_value_text(app.get_request_headers().as_str(), "Header");
        app.set_request_headers(updated.into());
        refresh_header_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_update_header_row(move |row_id, key, value| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = update_key_value_text(
            app.get_request_headers().as_str(),
            row_id,
            key.as_str(),
            value.as_str(),
        );
        app.set_request_headers(updated.into());
        refresh_header_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_delete_header_row(move |row_id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = delete_key_value_text(app.get_request_headers().as_str(), row_id);
        app.set_request_headers(updated.into());
        refresh_header_rows(&app);
    });
}

fn wire_query_param_actions(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_paste_query_params(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let result = read_text_from_clipboard().and_then(|clipboard| {
            merge_key_value_text(
                app.get_query_params().as_str(),
                clipboard.as_str(),
                "query param",
                false,
                false,
            )
        });
        match result {
            Ok((params, count)) => {
                app.set_query_params(params.into());
                refresh_query_param_rows(&app);
                set_response(
                    &app,
                    "Params pasted",
                    "",
                    "success",
                    &format!("{count} query parameter rows imported from clipboard."),
                );
            }
            Err(error) => {
                set_response(&app, "Params paste failed", "", "error", &error.to_string())
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_import_query_params(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match merge_key_value_file(
            app.get_query_params().as_str(),
            path.as_str(),
            "query param",
            false,
            false,
        ) {
            Ok((params, count)) => {
                app.set_query_params(params.into());
                refresh_query_param_rows(&app);
                set_response(
                    &app,
                    "Params imported",
                    path.as_str(),
                    "success",
                    &format!("{count} query parameter rows imported."),
                );
            }
            Err(error) => set_response(
                &app,
                "Params import failed",
                path.as_str(),
                "error",
                &error.to_string(),
            ),
        }
    });

    let weak_app = app.as_weak();
    app.on_add_query_param(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = add_key_value_text(app.get_query_params().as_str(), "param");
        app.set_query_params(updated.into());
        refresh_query_param_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_update_query_param_row(move |row_id, key, value| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = update_key_value_text(
            app.get_query_params().as_str(),
            row_id,
            key.as_str(),
            value.as_str(),
        );
        app.set_query_params(updated.into());
        refresh_query_param_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_delete_query_param_row(move |row_id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = delete_key_value_text(app.get_query_params().as_str(), row_id);
        app.set_query_params(updated.into());
        refresh_query_param_rows(&app);
    });
}

fn wire_auth_key_actions(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_auth_mode_changed(move |mode| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if mode.as_str() == "basic" {
            refresh_basic_auth_fields(&app);
        }
        refresh_auth_key_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_update_basic_auth(move |username, password| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        app.set_auth_config(format_basic_auth_config(username.as_str(), password.as_str()).into());
        refresh_auth_key_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_add_auth_key(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let base_key = if app.get_auth_mode().as_str() == "api-query" {
            "api_key"
        } else {
            "x-api-key"
        };
        let updated = add_key_value_text(app.get_auth_config().as_str(), base_key);
        app.set_auth_config(updated.into());
        refresh_auth_key_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_update_auth_key_row(move |row_id, key, value| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = update_key_value_text(
            app.get_auth_config().as_str(),
            row_id,
            key.as_str(),
            value.as_str(),
        );
        app.set_auth_config(updated.into());
        refresh_auth_key_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_delete_auth_key_row(move |row_id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = delete_key_value_text(app.get_auth_config().as_str(), row_id);
        app.set_auth_config(updated.into());
        refresh_auth_key_rows(&app);
    });
}

fn wire_body_field_actions(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_body_mode_changed(move |_mode| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        refresh_body_field_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_add_body_field(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = add_key_value_text(app.get_request_body().as_str(), "field");
        app.set_request_body(updated.into());
        refresh_body_field_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_add_body_file_field(move |field, path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match add_form_file_field_text(
            app.get_request_body().as_str(),
            field.as_str(),
            path.as_str(),
        ) {
            Ok(updated) => {
                app.set_request_body(updated.into());
                app.set_form_file_field(
                    unique_key_value_name(app.get_request_body().as_str(), "file").into(),
                );
                app.set_form_file_path("".into());
                refresh_body_field_rows(&app);
                set_response(
                    &app,
                    "Form file added",
                    field.as_str(),
                    "success",
                    "Multipart file field appended.",
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Form file failed",
                    field.as_str(),
                    "error",
                    &error.to_string(),
                );
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_update_body_field_row(move |row_id, key, value| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = update_key_value_text(
            app.get_request_body().as_str(),
            row_id,
            key.as_str(),
            value.as_str(),
        );
        app.set_request_body(updated.into());
        refresh_body_field_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_delete_body_field_row(move |row_id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = delete_key_value_text(app.get_request_body().as_str(), row_id);
        app.set_request_body(updated.into());
        refresh_body_field_rows(&app);
    });
}

fn wire_test_assertion_actions(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_add_test_assertion(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = add_test_assertion_text(app.get_request_tests().as_str());
        app.set_request_tests(updated.into());
        refresh_test_assertion_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_add_test_assertion_template(move |template| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match add_test_assertion_template_text(app.get_request_tests().as_str(), template.as_str())
        {
            Ok(updated) => {
                app.set_request_tests(updated.into());
                refresh_test_assertion_rows(&app);
                set_response(
                    &app,
                    "Test template added",
                    template.as_str(),
                    "success",
                    "Assertion row appended.",
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Test template failed",
                    "",
                    "error",
                    &error.to_string(),
                );
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_update_test_assertion_row(move |row_id, kind, target, expected| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = update_test_assertion_text(
            app.get_request_tests().as_str(),
            row_id,
            kind.as_str(),
            target.as_str(),
            expected.as_str(),
        );
        app.set_request_tests(updated.into());
        refresh_test_assertion_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_cycle_test_assertion_kind(move |row_id, kind| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let (next_kind, target, expected) = next_test_assertion_template(kind.as_str());
        let updated = update_test_assertion_text(
            app.get_request_tests().as_str(),
            row_id,
            next_kind,
            target,
            expected,
        );
        app.set_request_tests(updated.into());
        refresh_test_assertion_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_delete_test_assertion_row(move |row_id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = delete_test_assertion_text(app.get_request_tests().as_str(), row_id);
        app.set_request_tests(updated.into());
        refresh_test_assertion_rows(&app);
    });
}

fn wire_environment_actions(app: &AppWindow) {
    let environment_profiles = Arc::new(Mutex::new(EnvironmentProfiles::new(
        app.get_environment_name().as_str(),
        app.get_environment_variables().as_str(),
    )));

    let weak_app = app.as_weak();
    let profiles = environment_profiles.clone();
    app.on_select_environment(move |environment| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let variables = profiles
            .lock()
            .map(|mut profiles| {
                profiles.switch_to(
                    environment.as_str(),
                    app.get_environment_variables().as_str(),
                )
            })
            .unwrap_or_default();
        app.set_environment_name(environment);
        app.set_environment_variables(variables.into());
        refresh_variable_table(&app);
    });

    let weak_app = app.as_weak();
    let profiles = environment_profiles.clone();
    app.on_environment_name_changed(move |environment| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let variables = profiles
            .lock()
            .map(|mut profiles| {
                profiles.switch_to(
                    environment.as_str(),
                    app.get_environment_variables().as_str(),
                )
            })
            .unwrap_or_default();
        app.set_environment_name(environment);
        app.set_environment_variables(variables.into());
        refresh_variable_table(&app);
    });

    let weak_app = app.as_weak();
    let profiles = environment_profiles.clone();
    app.on_update_variable_row(move |row_id, scope, name, value| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        if is_global_scope(scope.as_str()) {
            let updated = update_variable_text(
                app.get_global_variables().as_str(),
                row_id,
                name.as_str(),
                value.as_str(),
            );
            app.set_global_variables(updated.into());
        } else {
            let updated = update_variable_text(
                app.get_environment_variables().as_str(),
                row_id,
                name.as_str(),
                value.as_str(),
            );
            app.set_environment_variables(updated.into());
            if let Ok(mut profiles) = profiles.lock() {
                profiles.save_active(app.get_environment_variables().as_str());
            }
        }
        refresh_variable_table(&app);
    });

    let weak_app = app.as_weak();
    let profiles = environment_profiles.clone();
    app.on_add_variable(move |scope| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        if is_global_scope(scope.as_str()) {
            let updated = add_variable_text(app.get_global_variables().as_str(), "GLOBAL_VAR");
            app.set_global_variables(updated.into());
        } else {
            if app.get_environment_name().trim().is_empty() {
                app.set_environment_name("dev".into());
                if let Ok(mut profiles) = profiles.lock() {
                    profiles.set_active_name("dev");
                }
            }
            let updated = add_variable_text(app.get_environment_variables().as_str(), "ENV_VAR");
            app.set_environment_variables(updated.into());
            if let Ok(mut profiles) = profiles.lock() {
                profiles.save_active(app.get_environment_variables().as_str());
            }
        }
        refresh_variable_table(&app);
    });

    let weak_app = app.as_weak();
    let profiles = environment_profiles;
    app.on_delete_variable_row(move |row_id, scope| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        if is_global_scope(scope.as_str()) {
            let updated = delete_variable_text(app.get_global_variables().as_str(), row_id);
            app.set_global_variables(updated.into());
        } else {
            let updated = delete_variable_text(app.get_environment_variables().as_str(), row_id);
            app.set_environment_variables(updated.into());
            if let Ok(mut profiles) = profiles.lock() {
                profiles.save_active(app.get_environment_variables().as_str());
            }
        }
        refresh_variable_table(&app);
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
                        app.set_response_raw_body(response_raw_body.into());
                        app.set_response_headers(response_headers.into());
                        app.set_response_cookies(response_cookies.into());
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

fn wire_response_actions(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_copy_response(move |tab, text| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        match copy_text_to_clipboard(text.as_str()) {
            Ok(()) => {
                app.set_activity(format!("Copied {} response", response_tab_label(&tab)).into())
            }
            Err(error) => app.set_activity(format!("Copy failed: {error}").into()),
        }
    });

    let weak_app = app.as_weak();
    app.on_format_response(move |tab| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let current = match tab.as_str() {
            "pretty" => app.get_response_body().to_string(),
            "raw" => app.get_response_raw_body().to_string(),
            _ => {
                app.set_activity("Format is available for Pretty and Raw responses".into());
                return;
            }
        };

        match format_json_response_text(&current) {
            Ok(formatted) => {
                match tab.as_str() {
                    "pretty" => app.set_response_body(formatted.into()),
                    "raw" => app.set_response_raw_body(formatted.into()),
                    _ => {}
                }
                app.set_activity(format!("Formatted {} response", response_tab_label(&tab)).into());
            }
            Err(error) => app.set_activity(format!("Format failed: {error}").into()),
        }
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
        app.set_codegen_metadata(
            codegen_metadata(&request, codegen_language_label(language), &snippet).into(),
        );
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
    app.on_copy_codegen(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let snippet = app.get_codegen_output().to_string();
        if snippet.is_empty() {
            app.set_activity("Generate a snippet before copying".into());
            return;
        }

        match copy_text_to_clipboard(&snippet) {
            Ok(()) => app.set_activity("Copied code snippet".into()),
            Err(error) => app.set_activity(format!("Copy failed: {error}").into()),
        }
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
        let metadata = codegen_metadata(&request, codegen_language_label(language), &snippet);

        match save_codegen_snippet(path.as_str(), &snippet) {
            Ok(()) => {
                app.set_codegen_output(snippet.into());
                app.set_codegen_metadata(metadata.into());
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
    let active_run = Arc::new(Mutex::new(None::<JoinHandle<()>>));
    let last_summary = Arc::new(Mutex::new(None::<CollectionRunSummary>));

    let weak_app = app.as_weak();
    let cancel_run = active_run.clone();
    let cancel_summary = last_summary.clone();
    app.on_cancel_collection_run(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        let Some(handle) = cancel_run.lock().ok().and_then(|mut run| run.take()) else {
            set_response(
                &app,
                "Runner idle",
                "",
                "neutral",
                "No collection run is active.",
            );
            return;
        };

        handle.abort();
        app.set_runner_active(false);
        app.set_busy(false);
        app.set_activity("".into());
        app.set_runner_summary("Cancelled".into());
        if let Ok(mut summary) = cancel_summary.lock() {
            *summary = None;
        }
        set_response(
            &app,
            "Runner cancelled",
            "",
            "neutral",
            "The active collection run was cancelled.",
        );
    });

    let weak_app = app.as_weak();
    let save_summary = last_summary.clone();
    app.on_save_runner_report(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() || app.get_runner_active() {
            return;
        }

        let Some(summary) = save_summary.lock().ok().and_then(|summary| summary.clone()) else {
            set_response(
                &app,
                "Runner report failed",
                "",
                "error",
                "Run a collection before saving a report.",
            );
            return;
        };

        let format = app.get_runner_report_format().to_string();
        match save_runner_report(path.as_str(), &summary, &format) {
            Ok(()) => set_response(
                &app,
                "Runner report saved",
                path.as_str(),
                "success",
                &format!(
                    "Collection runner {} report exported.",
                    normalize_runner_report_format(&format)
                ),
            ),
            Err(error) => set_response(
                &app,
                "Runner report failed",
                path.as_str(),
                "error",
                &error.to_string(),
            ),
        }
    });

    let weak_app = app.as_weak();
    let active_run_for_start = active_run.clone();
    let summary_for_start = last_summary.clone();
    app.on_run_collection(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }
        if active_run_for_start
            .lock()
            .ok()
            .and_then(|run| run.as_ref().map(|_| ()))
            .is_some()
        {
            set_response(
                &app,
                "Runner already running",
                "",
                "neutral",
                "Cancel the active collection run before starting another.",
            );
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
            if let Ok(mut summary) = summary_for_start.lock() {
                *summary = None;
            }
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
        app.set_runner_active(true);
        app.set_activity("Running collection".into());
        app.set_runner_summary(format!("Running {request_count} requests").into());
        app.set_runner_rows(empty_runner_model());
        if let Ok(mut summary) = summary_for_start.lock() {
            *summary = None;
        }
        set_response(
            &app,
            "Runner running",
            &collection.name,
            "busy",
            &format!("Running {request_count} requests"),
        );

        let weak_app = app.as_weak();
        let active_run = active_run_for_start.clone();
        let last_summary = summary_for_start.clone();
        let handle = runtime.spawn(async move {
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
                if let Ok(mut last_summary) = last_summary.lock() {
                    *last_summary = Some(summary.clone());
                }
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
                app.set_runner_active(false);
                app.set_busy(false);
                if let Ok(mut run) = active_run.lock() {
                    *run = None;
                }
            });
        });
        if let Ok(mut run) = active_run_for_start.lock() {
            *run = Some(handle);
        }
    });
}

fn wire_graphql_helpers(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_apply_graphql_template(move |template| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let Some(template) = graphql_template(template.as_str()) else {
            set_response(
                &app,
                "GraphQL template failed",
                "",
                "error",
                "Unknown GraphQL template.",
            );
            return;
        };

        app.set_body_mode("graphql".into());
        app.set_request_body(template.query.into());
        refresh_body_field_rows(&app);
        app.set_graphql_variables(template.variables.into());
        set_response(
            &app,
            "GraphQL template ready",
            template.name,
            "success",
            "Template applied to the request body.",
        );
    });
}

fn wire_realtime_actions(app: &AppWindow, runtime: Arc<Runtime>) {
    let websocket_session = Arc::new(Mutex::new(
        None::<mpsc::UnboundedSender<client::WebSocketSessionCommand>>,
    ));
    let sse_stream = Arc::new(Mutex::new(None::<JoinHandle<()>>));

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
            &app.get_grpc_method_catalog(),
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
    let (response_time, response_size) = split_response_meta(meta);
    app.set_response_status(status.into());
    app.set_response_meta(meta.into());
    app.set_response_time(response_time.into());
    app.set_response_size(response_size.into());
    app.set_response_tone(tone.into());
    app.set_response_body(body.into());
    app.set_response_raw_body(body.into());
    app.set_response_headers("".into());
    app.set_response_cookies("No cookies".into());
}

fn copy_text_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard =
        ClipboardContext::new().map_err(|error| anyhow!("failed to access clipboard: {error}"))?;
    clipboard
        .set_contents(text.to_string())
        .map_err(|error| anyhow!("failed to write clipboard: {error}"))
}

fn read_text_from_clipboard() -> Result<String> {
    let mut clipboard =
        ClipboardContext::new().map_err(|error| anyhow!("failed to access clipboard: {error}"))?;
    clipboard
        .get_contents()
        .map_err(|error| anyhow!("failed to read clipboard: {error}"))
}

fn format_json_response_text(text: &str) -> Result<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("response is empty");
    }

    let value: Value = serde_json::from_str(trimmed)
        .map_err(|error| anyhow!("response is not valid JSON: {error}"))?;
    serde_json::to_string_pretty(&value).map_err(|error| anyhow!("failed to format JSON: {error}"))
}

fn response_tab_label(tab: &str) -> &'static str {
    match tab {
        "pretty" => "Pretty",
        "raw" => "Raw",
        "headers" => "Headers",
        "cookies" => "Cookies",
        _ => "active",
    }
}

fn split_response_meta(meta: &str) -> (String, String) {
    let meta = meta.trim();
    if meta.is_empty() {
        return (String::new(), String::new());
    }

    if let Some((time, size)) = meta.split_once(" / ") {
        (time.trim().to_string(), size.trim().to_string())
    } else {
        (meta.to_string(), String::new())
    }
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

fn set_selected_mock_route(app: &AppWindow, route: &ApiRoute) {
    app.set_selected_mock_method(route.method.clone().into());
    app.set_selected_mock_path(route.path.clone().into());
    app.set_selected_mock_summary(route.summary.clone().into());
    app.set_selected_mock_body(pretty_json(&route.mock_body).into());
}

fn clear_selected_mock_route(app: &AppWindow) {
    app.set_selected_mock_method("".into());
    app.set_selected_mock_path("".into());
    app.set_selected_mock_summary("".into());
    app.set_selected_mock_body("".into());
}

fn update_selected_mock_response(
    state: &mut AppState,
    selected_route: i32,
    body: &str,
) -> Result<ApiRoute> {
    let selected_route: usize = selected_route
        .try_into()
        .map_err(|_| anyhow!("select a mock route before saving a response"))?;
    let selected = state
        .visible_routes
        .get(selected_route)
        .cloned()
        .ok_or_else(|| anyhow!("select a mock route before saving a response"))?;
    let mock_body = serde_json::from_str::<Value>(body.trim())
        .map_err(|err| anyhow!("mock response body must be valid JSON: {err}"))?;

    let route = state
        .routes
        .iter_mut()
        .find(|route| route.method == selected.method && route.path == selected.path)
        .ok_or_else(|| anyhow!("selected mock route is no longer available"))?;
    route.mock_body = mock_body.clone();
    let updated = route.clone();

    if let Some(visible_route) = state.visible_routes.get_mut(selected_route) {
        visible_route.mock_body = mock_body;
    }

    Ok(updated)
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

fn rename_collection_request_at(
    collection: &mut ApiCollection,
    index: usize,
    name: &str,
) -> Option<CollectionRequest> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    let mut current = 0;
    rename_collection_request_at_items(&mut collection.items, index, &mut current, name)
}

fn rename_collection_request_at_items(
    items: &mut [CollectionItem],
    target: usize,
    current: &mut usize,
    name: &str,
) -> Option<CollectionRequest> {
    for item in items {
        match item {
            CollectionItem::Request(request) => {
                if *current == target {
                    request.name = name.to_string();
                    return Some(request.clone());
                }
                *current += 1;
            }
            CollectionItem::Folder(folder) => {
                if let Some(request) =
                    rename_collection_request_at_items(&mut folder.items, target, current, name)
                {
                    return Some(request);
                }
            }
        }
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
        body: build_request_body(
            &input.body_mode,
            &input.body,
            &input.graphql_variables,
            &input.raw_body_subtype,
        )?,
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
    let (body_mode, request_body, raw_body_subtype) = collection_body_to_slint(&request.body);
    app.set_method(request.method.clone().into());
    app.set_url(request.url.clone().into());
    app.set_query_params(format_name_values(&request.query_params).into());
    refresh_query_param_rows(app);
    app.set_request_headers(format_name_values(&request.headers).into());
    refresh_header_rows(app);
    app.set_auth_mode("none".into());
    app.set_auth_config("".into());
    refresh_auth_key_rows(app);
    refresh_basic_auth_fields(app);
    app.set_body_mode(body_mode.into());
    app.set_raw_body_subtype(raw_body_subtype.into());
    app.set_request_body(request_body.into());
    refresh_body_field_rows(app);
    app.set_graphql_variables("{}".into());
    app.set_pre_request_script(request.pre_request_script.clone().into());
    app.set_request_tests(format_response_assertions(&request.tests).into());
    refresh_test_assertion_rows(app);
    app.set_collection_status(format!("Selected {}", request.name).into());
    set_response(
        app,
        "Collection request loaded",
        &request.name,
        "neutral",
        &request.url,
    );
}

fn collection_body_to_slint(body: &CollectionBody) -> (String, String, String) {
    match body {
        CollectionBody::None => ("none".to_string(), String::new(), "json".to_string()),
        CollectionBody::Raw { body, content_type } => (
            "raw".to_string(),
            body.clone(),
            raw_body_subtype_from_content_type(content_type),
        ),
        CollectionBody::FormData { fields } => (
            "form".to_string(),
            format_name_values(fields),
            "json".to_string(),
        ),
        CollectionBody::UrlEncoded { fields } => (
            "urlenc".to_string(),
            format_name_values(fields),
            "json".to_string(),
        ),
        CollectionBody::Binary { path, .. } => {
            ("binary".to_string(), path.clone(), "json".to_string())
        }
    }
}

fn format_name_values(values: &[NameValue]) -> String {
    values
        .iter()
        .map(|value| format!("{}={}", value.name, value.value))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_header_lines(headers: &[(String, String)]) -> String {
    headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
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

fn normalize_runner_report_format(format: &str) -> &'static str {
    match format.trim().to_ascii_lowercase().as_str() {
        "json" => "json",
        _ => "text",
    }
}

fn format_runner_report(summary: &CollectionRunSummary, format: &str) -> Result<String> {
    match normalize_runner_report_format(format) {
        "json" => Ok(serde_json::to_string_pretty(summary)?),
        _ => Ok(format_runner_summary(summary)),
    }
}

fn save_runner_report(path: &str, summary: &CollectionRunSummary, format: &str) -> Result<()> {
    let report = format_runner_report(summary, format)?;
    write_text_file(path, &report, "runner report")
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

fn parse_websocket_protocols(input: &str) -> Vec<String> {
    input
        .split([',', '\n'])
        .map(str::trim)
        .filter(|protocol| !protocol.is_empty())
        .map(str::to_string)
        .collect()
}

fn normalize_websocket_message_mode(mode: &str) -> &'static str {
    if mode.trim().eq_ignore_ascii_case("binary") {
        "binary"
    } else {
        "text"
    }
}

fn websocket_message_mode_label(mode: &str) -> &'static str {
    match normalize_websocket_message_mode(mode) {
        "binary" => "binary",
        _ => "text",
    }
}

fn websocket_session_command(mode: &str, message: &str) -> Result<client::WebSocketSessionCommand> {
    match normalize_websocket_message_mode(mode) {
        "binary" => Ok(client::WebSocketSessionCommand::SendBinary(
            parse_websocket_binary_message(message)?,
        )),
        _ => Ok(client::WebSocketSessionCommand::SendText(
            message.to_string(),
        )),
    }
}

fn parse_websocket_binary_message(input: &str) -> Result<Vec<u8>> {
    let tokens = input
        .split(|character: char| character.is_whitespace() || character == ',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("WebSocket binary message is empty");
    }

    tokens
        .into_iter()
        .map(|token| {
            let token = token
                .strip_prefix("0x")
                .or_else(|| token.strip_prefix("0X"))
                .unwrap_or(token);
            if token.is_empty() || token.len() > 2 {
                bail!("invalid WebSocket binary byte: {token}");
            }
            u8::from_str_radix(token, 16)
                .map_err(|_| anyhow!("invalid WebSocket binary byte: {token}"))
        })
        .collect()
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

fn format_websocket_session_events(events: &[client::WebSocketSessionEvent]) -> String {
    events
        .iter()
        .enumerate()
        .map(|(index, event)| format_websocket_session_event(index + 1, event))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_websocket_session_event(index: usize, event: &client::WebSocketSessionEvent) -> String {
    match event {
        client::WebSocketSessionEvent::Connected { url } => format!("{index}. connected {url}"),
        client::WebSocketSessionEvent::Sent(message) => {
            format!(
                "{index}. sent [{}]: {}",
                websocket_message_kind(&message.kind),
                message.data
            )
        }
        client::WebSocketSessionEvent::Received(message) => {
            format!(
                "{index}. received [{}]: {}",
                websocket_message_kind(&message.kind),
                message.data
            )
        }
        client::WebSocketSessionEvent::Closed(reason) => format!("{index}. closed {reason}"),
        client::WebSocketSessionEvent::Error(error) => format!("{index}. error {error}"),
    }
}

fn websocket_session_event_done(event: &client::WebSocketSessionEvent) -> bool {
    matches!(
        event,
        client::WebSocketSessionEvent::Closed(_) | client::WebSocketSessionEvent::Error(_)
    )
}

fn websocket_session_status(event: &client::WebSocketSessionEvent) -> &'static str {
    match event {
        client::WebSocketSessionEvent::Connected { .. } => "WebSocket open",
        client::WebSocketSessionEvent::Sent(_) => "WebSocket sent",
        client::WebSocketSessionEvent::Received(_) => "WebSocket received",
        client::WebSocketSessionEvent::Closed(_) => "WebSocket closed",
        client::WebSocketSessionEvent::Error(_) => "WebSocket failed",
    }
}

fn websocket_session_tone(event: &client::WebSocketSessionEvent) -> &'static str {
    match event {
        client::WebSocketSessionEvent::Error(_) => "error",
        client::WebSocketSessionEvent::Closed(_) => "neutral",
        _ => "success",
    }
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

fn publish_sse_stream_event(
    weak_app: &slint::Weak<AppWindow>,
    stream_state: &Arc<Mutex<Option<JoinHandle<()>>>>,
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

fn push_bounded_sse_stream_event(
    events: &mut Vec<client::SseStreamEvent>,
    event: client::SseStreamEvent,
) {
    events.push(event);
    if events.len() > MAX_SSE_STREAM_EVENTS {
        let overflow = events.len() - MAX_SSE_STREAM_EVENTS;
        drop(events.drain(..overflow));
    }
}

fn sse_stream_meta(event_count: usize) -> String {
    if event_count == MAX_SSE_STREAM_EVENTS {
        format!("latest {event_count} events")
    } else {
        format!("{event_count} events")
    }
}

fn sse_stream_event_done(event: &client::SseStreamEvent) -> bool {
    matches!(
        event,
        client::SseStreamEvent::Closed(_) | client::SseStreamEvent::Error(_)
    )
}

fn sse_stream_event_last_id(event: &client::SseStreamEvent) -> Option<&str> {
    match event {
        client::SseStreamEvent::Event(event) => event
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty()),
        _ => None,
    }
}

fn sse_stream_status(event: &client::SseStreamEvent) -> &'static str {
    match event {
        client::SseStreamEvent::Connected { .. } => "SSE stream open",
        client::SseStreamEvent::Event(_) => "SSE event",
        client::SseStreamEvent::Reconnecting { .. } => "SSE reconnecting",
        client::SseStreamEvent::Closed(_) => "SSE closed",
        client::SseStreamEvent::Error(_) => "SSE failed",
    }
}

fn sse_stream_tone(event: &client::SseStreamEvent) -> &'static str {
    match event {
        client::SseStreamEvent::Error(_) => "error",
        client::SseStreamEvent::Closed(_) => "neutral",
        client::SseStreamEvent::Reconnecting { .. } => "busy",
        _ => "success",
    }
}

fn format_sse_stream_events(events: &[client::SseStreamEvent]) -> String {
    events
        .iter()
        .enumerate()
        .map(|(index, event)| format_sse_stream_event(index + 1, event))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_sse_stream_event(index: usize, event: &client::SseStreamEvent) -> String {
    match event {
        client::SseStreamEvent::Connected { url } => format!("{index}. connected {url}"),
        client::SseStreamEvent::Event(event) => format_sse_event(index, event),
        client::SseStreamEvent::Reconnecting {
            attempt,
            delay_ms,
            reason,
        } => format!("{index}. reconnecting attempt {attempt} in {delay_ms} ms\n{reason}"),
        client::SseStreamEvent::Closed(reason) => format!("{index}. closed\n{reason}"),
        client::SseStreamEvent::Error(error) => format!("{index}. error\n{error}"),
    }
}

fn latest_sse_event_id(events: &[client::SseEvent]) -> Option<&str> {
    events.iter().rev().find_map(|event| {
        event
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
    })
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
    let descriptor = draft
        .descriptor
        .as_ref()
        .map(|descriptor| {
            format!(
                "Kind: {}\nRequest: {}\nResponse: {}",
                descriptor.kind.label(),
                descriptor.request_type,
                descriptor.response_type
            )
        })
        .unwrap_or_else(|| "No method catalog".to_string());
    let message =
        serde_json::to_string_pretty(&draft.message).unwrap_or_else(|_| draft.message.to_string());

    format!(
        "Endpoint: {}\nMethod: {}\n\nDescriptor\n{}\n\nMetadata\n{}\n\nMessage\n{}",
        draft.endpoint,
        draft.method_path(),
        descriptor,
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

fn clear_mock_logs(logs: &mut Vec<MockRequestLog>) -> usize {
    let cleared = logs.len();
    logs.clear();
    cleared
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
    let (body_kind, raw_body_subtype, body_preview) = request_body_preview(&request.body);
    HistoryRequest {
        method: request.method.clone(),
        url: request.url.clone(),
        query_params: request.query_params.clone(),
        headers: request.headers.clone(),
        auth_mode: input.auth_mode.clone(),
        auth_config: input.auth_config.clone(),
        body_kind,
        raw_body_subtype,
        body_preview,
        pre_request_script: input.pre_request_script.clone(),
        request_tests: request_tests.to_string(),
    }
}

fn normalized_history_auth_mode(mode: &str) -> &str {
    let mode = mode.trim();
    if mode.is_empty() { "none" } else { mode }
}

fn history_body_to_slint(request: &HistoryRequest) -> (String, String) {
    let body_mode = request.body_kind.trim();
    let body_mode = if body_mode.is_empty() {
        "none"
    } else {
        body_mode
    };
    let raw_body_subtype = if body_mode == "raw" {
        normalize_raw_body_subtype(&request.raw_body_subtype)
    } else {
        "json"
    };
    (body_mode.to_string(), raw_body_subtype.to_string())
}

fn success_history_response(response: &ClientResponse) -> HistoryResponse {
    HistoryResponse {
        status: format!("HTTP {}", response.status),
        meta: format!("{} ms / {} B", response.elapsed_ms, response.body_bytes),
        body_preview: truncate_preview(&response.body, 1200),
    }
}

fn request_body_preview(body: &RequestBody) -> (String, String, String) {
    match body {
        RequestBody::None => ("none".to_string(), "json".to_string(), String::new()),
        RequestBody::Raw { body, content_type } => (
            "raw".to_string(),
            content_type
                .as_deref()
                .map(raw_body_subtype_from_content_type)
                .unwrap_or_else(|| "json".to_string()),
            body.clone(),
        ),
        RequestBody::FormUrlEncoded(fields) => (
            "urlenc".to_string(),
            "json".to_string(),
            format_key_value_preview(fields),
        ),
        RequestBody::Multipart(fields) => (
            "form".to_string(),
            "json".to_string(),
            format_key_value_preview(fields),
        ),
        RequestBody::BinaryFile { path, .. } => {
            ("binary".to_string(), "json".to_string(), path.clone())
        }
    }
}

fn format_key_value_preview(fields: &[(String, String)]) -> String {
    fields
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn refresh_query_param_rows(app: &AppWindow) {
    app.set_query_param_rows(key_value_table_model(app.get_query_params().as_str()));
}

fn refresh_header_rows(app: &AppWindow) {
    app.set_header_rows(key_value_table_model(app.get_request_headers().as_str()));
}

fn refresh_auth_key_rows(app: &AppWindow) {
    app.set_auth_key_rows(key_value_table_model(app.get_auth_config().as_str()));
}

fn refresh_basic_auth_fields(app: &AppWindow) {
    let (username, password) = split_basic_auth_config(app.get_auth_config().as_str());
    app.set_auth_basic_username(username.into());
    app.set_auth_basic_password(password.into());
}

fn refresh_body_field_rows(app: &AppWindow) {
    app.set_body_field_rows(key_value_table_model(app.get_request_body().as_str()));
}

fn refresh_test_assertion_rows(app: &AppWindow) {
    app.set_test_assertion_rows(test_assertion_table_model(app.get_request_tests().as_str()));
}

fn key_value_table_model(input: &str) -> ModelRc<KeyValueTableRow> {
    ModelRc::new(VecModel::from_iter(
        key_value_ui_entries(input)
            .into_iter()
            .enumerate()
            .map(|(row_id, (_, key, value))| KeyValueTableRow {
                row_id: row_id as i32,
                key: key.into(),
                value: value.into(),
            }),
    ))
}

fn key_value_ui_entries(input: &str) -> Vec<(usize, String, String)> {
    input
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            split_key_value_line(line)
                .map(|(key, value)| (line_index, key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

fn update_key_value_text(input: &str, row_id: i32, key: &str, value: &str) -> String {
    let mut lines = input.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = key_value_ui_entries(input);
    let new_line = format!("{}={}", key.trim(), value.trim());

    if let Some((line_index, _, _)) = row_id
        .try_into()
        .ok()
        .and_then(|row_id: usize| entries.get(row_id))
    {
        lines[*line_index] = new_line;
    } else {
        lines.push(new_line);
    }

    lines.join("\n")
}

fn add_key_value_text(input: &str, base_key: &str) -> String {
    let key = unique_key_value_name(input, base_key);
    append_key_value_line(input, &format!("{key}="))
}

fn add_form_file_field_text(input: &str, field: &str, path: &str) -> Result<String> {
    let field = field.trim();
    if field.is_empty() {
        bail!("form file field name is required");
    }

    let path = path.trim();
    if path.is_empty() {
        bail!("form file path is required");
    }

    let file_path = Path::new(path);
    if !file_path.is_file() {
        bail!("form file path does not exist or is not a file: {path}");
    }

    Ok(append_key_value_line(input, &format!("{field}=@{path}")))
}

fn merge_key_value_text(
    input: &str,
    imported: &str,
    field_name: &str,
    case_insensitive: bool,
    colon_output: bool,
) -> Result<(String, usize)> {
    let mut values = parse_key_value_lines(input, field_name)?;
    let imported_values = parse_key_value_lines(imported, field_name)?;
    if imported_values.is_empty() {
        bail!("clipboard does not contain any {field_name} rows");
    }

    let count = imported_values.len();
    for (name, value) in imported_values {
        upsert_pair(&mut values, name, value, case_insensitive);
    }

    let output = if colon_output {
        format_header_lines(&values)
    } else {
        format_key_value_preview(&values)
    };
    Ok((output, count))
}

fn merge_key_value_file(
    input: &str,
    path: &str,
    field_name: &str,
    case_insensitive: bool,
    colon_output: bool,
) -> Result<(String, usize)> {
    let contents = read_text_file(path, &format!("{field_name} import"))?;
    merge_key_value_text(
        input,
        contents.as_str(),
        field_name,
        case_insensitive,
        colon_output,
    )
}

fn delete_key_value_text(input: &str, row_id: i32) -> String {
    let mut lines = input.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = key_value_ui_entries(input);

    if let Some((line_index, _, _)) = row_id
        .try_into()
        .ok()
        .and_then(|row_id: usize| entries.get(row_id))
    {
        lines.remove(*line_index);
    }

    lines.join("\n")
}

fn test_assertion_table_model(input: &str) -> ModelRc<TestAssertionRow> {
    ModelRc::new(VecModel::from_iter(
        test_assertion_ui_entries(input)
            .into_iter()
            .enumerate()
            .map(|(row_id, (_, kind, target, expected))| TestAssertionRow {
                row_id: row_id as i32,
                kind: kind.into(),
                target: target.into(),
                expected: expected.into(),
            }),
    ))
}

fn test_assertion_ui_entries(input: &str) -> Vec<(usize, String, String, String)> {
    input
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                return None;
            }
            let (kind, args) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let args = args.trim();
            let (target, expected) = args.split_once(char::is_whitespace).unwrap_or((args, ""));
            Some((
                line_index,
                kind.trim().to_string(),
                target.trim().to_string(),
                expected.trim().to_string(),
            ))
        })
        .collect()
}

fn update_test_assertion_text(
    input: &str,
    row_id: i32,
    kind: &str,
    target: &str,
    expected: &str,
) -> String {
    let mut lines = input.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = test_assertion_ui_entries(input);
    let new_line = format_test_assertion_line(kind, target, expected);

    if let Some((line_index, _, _, _)) = row_id
        .try_into()
        .ok()
        .and_then(|row_id: usize| entries.get(row_id))
    {
        lines[*line_index] = new_line;
    } else {
        lines.push(new_line);
    }

    lines.join("\n")
}

fn add_test_assertion_text(input: &str) -> String {
    let (kind, target, expected) = test_assertion_template("status").expect("status template");
    append_test_assertion_line(input, &format_test_assertion_line(kind, target, expected))
}

fn add_test_assertion_template_text(input: &str, template: &str) -> Result<String> {
    let (kind, target, expected) = test_assertion_template(template)
        .ok_or_else(|| anyhow!("unknown test assertion template: {template}"))?;

    Ok(append_test_assertion_line(
        input,
        &format_test_assertion_line(kind, target, expected),
    ))
}

fn append_test_assertion_line(input: &str, new_line: &str) -> String {
    if input.trim().is_empty() {
        new_line.to_string()
    } else {
        format!("{}\n{new_line}", input.trim_end())
    }
}

fn test_assertion_template(template: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match template.trim().to_ascii_lowercase().as_str() {
        "status" | "status_equals" => Some(("status_equals", "200", "")),
        "range" | "status_in_range" => Some(("status_in_range", "200", "299")),
        "header" | "header_equals" => Some(("header_equals", "content-type", "application/json")),
        "body" | "body_contains" => Some(("body_contains", "ok", "")),
        "json" | "json_path" | "json_path_equals" => Some(("json_path_equals", "data.id", "1")),
        _ => None,
    }
}

fn next_test_assertion_template(kind: &str) -> (&'static str, &'static str, &'static str) {
    match kind.trim().to_ascii_lowercase().as_str() {
        "status" | "status_equals" | "status=" => ("status_in_range", "200", "299"),
        "range" | "status_in_range" => ("header_exists", "content-type", ""),
        "header" | "header_exists" | "header?" => {
            ("header_equals", "content-type", "application/json")
        }
        "header_equals" | "header=" => ("body_contains", "ok", ""),
        "body" | "body_contains" | "body?" => ("json_path_equals", "data.id", "1"),
        "json" | "json_path_equals" | "json=" => ("status_equals", "200", ""),
        _ => ("status_equals", "200", ""),
    }
}

fn delete_test_assertion_text(input: &str, row_id: i32) -> String {
    let mut lines = input.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = test_assertion_ui_entries(input);

    if let Some((line_index, _, _, _)) = row_id
        .try_into()
        .ok()
        .and_then(|row_id: usize| entries.get(row_id))
    {
        lines.remove(*line_index);
    }

    lines.join("\n")
}

fn format_test_assertion_line(kind: &str, target: &str, expected: &str) -> String {
    let kind = kind.trim();
    let kind = if kind.is_empty() {
        "status_equals"
    } else {
        kind
    };
    let target = target.trim();
    let expected = expected.trim();
    if expected.is_empty() {
        format!("{kind} {target}")
    } else {
        format!("{kind} {target} {expected}")
    }
}

fn append_key_value_line(input: &str, line: &str) -> String {
    if input.trim().is_empty() {
        line.to_string()
    } else if input.ends_with('\n') {
        format!("{input}{line}")
    } else {
        format!("{input}\n{line}")
    }
}

fn unique_key_value_name(input: &str, base_key: &str) -> String {
    let existing = key_value_ui_entries(input)
        .into_iter()
        .map(|(_, key, _)| key)
        .collect::<Vec<_>>();
    if !existing.iter().any(|key| key == base_key) {
        return base_key.to_string();
    }

    (2..)
        .map(|index| format!("{base_key}_{index}"))
        .find(|candidate| !existing.iter().any(|key| key == candidate))
        .unwrap_or_else(|| base_key.to_string())
}

fn refresh_variable_table(app: &AppWindow) {
    let global_variables = app.get_global_variables();
    let environment_name = app.get_environment_name();
    let environment_variables = app.get_environment_variables();

    app.set_variable_rows(variable_table_model(
        global_variables.as_str(),
        environment_variables.as_str(),
    ));
    app.set_variables_json_preview(
        variables_json_preview(
            global_variables.as_str(),
            environment_name.as_str(),
            environment_variables.as_str(),
        )
        .into(),
    );
}

fn variable_table_model(global_input: &str, environment_input: &str) -> ModelRc<VariableTableRow> {
    let global_rows = variable_ui_entries(global_input)
        .into_iter()
        .enumerate()
        .map(|(row_id, (_, name, value))| variable_table_row(row_id, "global", name, value));
    let environment_rows = variable_ui_entries(environment_input)
        .into_iter()
        .enumerate()
        .map(|(row_id, (_, name, value))| variable_table_row(row_id, "environment", name, value));

    ModelRc::new(VecModel::from_iter(global_rows.chain(environment_rows)))
}

fn variable_table_row(
    row_id: usize,
    scope: &'static str,
    name: String,
    value: String,
) -> VariableTableRow {
    VariableTableRow {
        row_id: row_id as i32,
        scope: scope.into(),
        name: name.into(),
        initial_value: value.clone().into(),
        current_value: value.into(),
    }
}

fn variable_ui_entries(input: &str) -> Vec<(usize, String, String)> {
    input
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            split_key_value_line(line).map(|(name, value)| {
                (
                    line_index,
                    name.trim().to_string(),
                    value.trim().to_string(),
                )
            })
        })
        .collect()
}

fn update_variable_text(input: &str, row_id: i32, name: &str, value: &str) -> String {
    let mut lines = input.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = variable_ui_entries(input);
    let new_line = format!("{}={}", name.trim(), value.trim());

    if let Some((line_index, _, _)) = row_id
        .try_into()
        .ok()
        .and_then(|row_id: usize| entries.get(row_id))
    {
        lines[*line_index] = new_line;
    } else {
        lines.push(new_line);
    }

    lines.join("\n")
}

fn add_variable_text(input: &str, base_name: &str) -> String {
    let name = unique_variable_name(input, base_name);
    append_variable_line(input, &format!("{name}="))
}

fn delete_variable_text(input: &str, row_id: i32) -> String {
    let mut lines = input.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = variable_ui_entries(input);

    if let Some((line_index, _, _)) = row_id
        .try_into()
        .ok()
        .and_then(|row_id: usize| entries.get(row_id))
    {
        lines.remove(*line_index);
    }

    lines.join("\n")
}

fn append_variable_line(input: &str, line: &str) -> String {
    if input.trim().is_empty() {
        line.to_string()
    } else if input.ends_with('\n') {
        format!("{input}{line}")
    } else {
        format!("{input}\n{line}")
    }
}

fn unique_variable_name(input: &str, base_name: &str) -> String {
    let existing = variable_ui_entries(input)
        .into_iter()
        .map(|(_, name, _)| name)
        .collect::<Vec<_>>();
    if !existing.iter().any(|name| name == base_name) {
        return base_name.to_string();
    }

    (2..)
        .map(|index| format!("{base_name}_{index}"))
        .find(|candidate| !existing.iter().any(|name| name == candidate))
        .unwrap_or_else(|| base_name.to_string())
}

fn variables_json_preview(
    global_input: &str,
    environment_name: &str,
    environment_input: &str,
) -> String {
    let global = variable_scope_json(global_input);
    let environment = variable_scope_json(environment_input);
    let active_environment = environment_name.trim();
    let preview = serde_json::json!({
        "activeEnvironment": if active_environment.is_empty() {
            Value::Null
        } else {
            Value::String(active_environment.to_string())
        },
        "globals": global,
        "environment": environment,
    });

    serde_json::to_string_pretty(&preview).unwrap_or_else(|_| "{}".to_string())
}

fn variable_scope_json(input: &str) -> Value {
    let mut values = serde_json::Map::new();
    for (_, name, value) in variable_ui_entries(input) {
        values.insert(
            name.clone(),
            Value::String(mask_variable_preview_value(&name, &value)),
        );
    }
    Value::Object(values)
}

fn mask_variable_preview_value(name: &str, value: &str) -> String {
    let name = name.to_ascii_lowercase();
    if name.contains("key") || name.contains("token") || name.contains("secret") {
        "********".to_string()
    } else {
        value.to_string()
    }
}

fn is_global_scope(scope: &str) -> bool {
    scope == "global"
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

fn apply_header_preset(input: &str, preset: &str) -> Result<String> {
    let (name, value) = match preset {
        "accept-json" => ("Accept", "application/json"),
        "content-json" => ("Content-Type", "application/json"),
        "bearer-token" => ("Authorization", "Bearer {{token}}"),
        _ => bail!("unknown header preset: {preset}"),
    };
    let mut headers = parse_key_value_lines(input, "header")?;
    upsert_pair(&mut headers, name.to_string(), value.to_string(), true);
    Ok(format_header_lines(&headers))
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
        body: build_request_body(
            &input.body_mode,
            &input.body,
            &input.graphql_variables,
            &input.raw_body_subtype,
        )?,
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

fn build_request_body(
    mode: &str,
    input: &str,
    graphql_variables: &str,
    raw_body_subtype: &str,
) -> Result<RequestBody> {
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
            content_type: Some(raw_body_content_type(raw_body_subtype).to_string()),
            body: input.to_string(),
        }),
    }
}

fn normalize_raw_body_subtype(subtype: &str) -> &'static str {
    match subtype.trim().to_lowercase().as_str() {
        "text" | "plain" | "txt" => "text",
        "xml" => "xml",
        _ => "json",
    }
}

fn raw_body_content_type(subtype: &str) -> &'static str {
    match normalize_raw_body_subtype(subtype) {
        "text" => "text/plain",
        "xml" => "application/xml",
        _ => "application/json",
    }
}

fn raw_body_subtype_from_content_type(content_type: &str) -> String {
    let normalized = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    match normalized.as_str() {
        "text/plain" => "text".to_string(),
        "application/xml" | "text/xml" => "xml".to_string(),
        _ => "json".to_string(),
    }
}

const GRAPHQL_INTROSPECTION_QUERY: &str = r#"query IntrospectionQuery {
  __schema {
    queryType {
      name
    }
    mutationType {
      name
    }
    subscriptionType {
      name
    }
    types {
      kind
      name
      fields(includeDeprecated: true) {
        name
        args {
          name
          type {
            kind
            name
            ofType {
              kind
              name
            }
          }
        }
        type {
          kind
          name
          ofType {
            kind
            name
          }
        }
        isDeprecated
        deprecationReason
      }
    }
  }
}"#;

const GRAPHQL_QUERY_TEMPLATE: &str = r#"query Example($id: ID!) {
  node(id: $id) {
    id
    __typename
  }
}"#;

const GRAPHQL_MUTATION_TEMPLATE: &str = r#"mutation Example($input: ExampleInput!) {
  example(input: $input) {
    id
  }
}"#;

fn graphql_template(template: &str) -> Option<GraphqlTemplate> {
    match template {
        "introspection" | "intro" => Some(GraphqlTemplate {
            name: "Introspection",
            query: GRAPHQL_INTROSPECTION_QUERY,
            variables: "{}",
        }),
        "query" => Some(GraphqlTemplate {
            name: "Query",
            query: GRAPHQL_QUERY_TEMPLATE,
            variables: r#"{
  "id": "example-id"
}"#,
        }),
        "mutation" => Some(GraphqlTemplate {
            name: "Mutation",
            query: GRAPHQL_MUTATION_TEMPLATE,
            variables: r#"{
  "input": {}
}"#,
        }),
        _ => None,
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

fn split_basic_auth_config(input: &str) -> (String, String) {
    let input = input.trim();
    if input.is_empty() {
        return (String::new(), String::new());
    }

    input
        .split_once(':')
        .map(|(username, password)| (username.to_string(), password.to_string()))
        .unwrap_or_else(|| (input.to_string(), String::new()))
}

fn format_basic_auth_config(username: &str, password: &str) -> String {
    if username.is_empty() && password.is_empty() {
        String::new()
    } else {
        format!("{username}:{password}")
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

fn format_cookies(headers: &[(String, String)]) -> String {
    let cookies = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    if cookies.is_empty() {
        "No cookies".to_string()
    } else {
        cookies
            .iter()
            .enumerate()
            .map(|(index, value)| format!("{} {}", index + 1, value))
            .collect::<Vec<_>>()
            .join("\n")
    }
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

fn codegen_language_label(language: SnippetLanguage) -> &'static str {
    match language {
        SnippetLanguage::Curl => "cURL",
        SnippetLanguage::PythonRequests => "Python requests",
        SnippetLanguage::JavaScriptFetch => "JavaScript fetch",
        SnippetLanguage::RustReqwest => "Rust reqwest",
        SnippetLanguage::GoNetHttp => "Go net/http",
    }
}

fn codegen_body_label(body: &RequestBody) -> &'static str {
    match body {
        RequestBody::None => "none",
        RequestBody::Raw { .. } => "raw",
        RequestBody::FormUrlEncoded(_) => "urlencoded",
        RequestBody::Multipart(_) => "multipart",
        RequestBody::BinaryFile { .. } => "binary",
    }
}

fn codegen_metadata(request: &CodegenRequest, language_label: &str, snippet: &str) -> String {
    let line_count = snippet.lines().count();
    let byte_count = snippet.len();
    format!(
        "Language: {language_label}\nRequest: {} {}\nLines: {line_count} / Bytes: {byte_count}\nHeaders: {} / Query params: {} / Body: {}",
        request.method,
        request.url,
        request.headers.len(),
        request.query_params.len(),
        codegen_body_label(&request.body)
    )
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

fn read_text_file(path: &str, label: &str) -> Result<String> {
    let path = path.trim();
    if path.is_empty() {
        bail!("{label} path is required");
    }

    let input_path = Path::new(path);
    fs::read_to_string(input_path)
        .map_err(|err| anyhow!("read {label} {}: {err}", input_path.display()))
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
    fn updates_selected_mock_response_body() {
        let routes = vec![
            route("GET", "/users", "List accounts"),
            route("POST", "/sessions", "Create login session"),
        ];
        let mut state = AppState {
            routes: routes.clone(),
            visible_routes: vec![routes[1].clone()],
            ..AppState::default()
        };

        let updated =
            update_selected_mock_response(&mut state, 0, r#"{ "token": "abc", "ok": true }"#)
                .expect("mock body update");

        assert_eq!(updated.method, "POST");
        assert_eq!(updated.path, "/sessions");
        assert_eq!(updated.mock_body, json!({ "token": "abc", "ok": true }));
        assert_eq!(state.routes[0].mock_body, json!({}));
        assert_eq!(
            state.routes[1].mock_body,
            json!({ "token": "abc", "ok": true })
        );
        assert_eq!(
            state.visible_routes[0].mock_body,
            json!({ "token": "abc", "ok": true })
        );

        assert!(update_selected_mock_response(&mut state, -1, "{}").is_err());
        assert!(update_selected_mock_response(&mut state, 0, "{ nope").is_err());
        assert_eq!(
            state.routes[1].mock_body,
            json!({ "token": "abc", "ok": true })
        );
    }

    #[test]
    fn maps_http_status_to_response_tone() {
        assert_eq!(response_tone(200), "success");
        assert_eq!(response_tone(302), "success");
        assert_eq!(response_tone(100), "neutral");
        assert_eq!(response_tone(404), "error");
    }

    #[test]
    fn splits_response_meta_for_header_columns() {
        assert_eq!(
            split_response_meta("142 ms / 842 B"),
            ("142 ms".to_string(), "842 B".to_string())
        );
        assert_eq!(
            split_response_meta("17 ms"),
            ("17 ms".to_string(), String::new())
        );
        assert_eq!(split_response_meta(""), (String::new(), String::new()));
    }

    #[test]
    fn formats_json_response_text_for_viewer() {
        assert_eq!(
            format_json_response_text("{\"ok\":true,\"items\":[1,2]}").expect("format"),
            "{\n  \"items\": [\n    1,\n    2\n  ],\n  \"ok\": true\n}"
        );
        assert!(format_json_response_text("not json").is_err());
        assert!(format_json_response_text("").is_err());
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
    fn applies_header_presets_to_text_headers() {
        assert_eq!(
            apply_header_preset("accept: text/plain\nX-Trace=abc", "accept-json")
                .expect("accept preset"),
            "accept: application/json\nX-Trace: abc"
        );
        assert_eq!(
            apply_header_preset("Accept: application/json", "content-json")
                .expect("content preset"),
            "Accept: application/json\nContent-Type: application/json"
        );
        assert_eq!(
            apply_header_preset("", "bearer-token").expect("bearer preset"),
            "Authorization: Bearer {{token}}"
        );
        assert!(
            apply_header_preset("Accept: application/json", "unknown")
                .expect_err("unknown preset")
                .to_string()
                .contains("unknown header preset")
        );
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
    fn formats_response_cookies_for_display() {
        assert_eq!(
            format_cookies(&[
                ("content-type".to_string(), "application/json".to_string()),
                (
                    "Set-Cookie".to_string(),
                    "session=abc; Path=/; HttpOnly".to_string()
                ),
                (
                    "set-cookie".to_string(),
                    "theme=dark; Max-Age=3600".to_string()
                )
            ]),
            "1 session=abc; Path=/; HttpOnly\n2 theme=dark; Max-Age=3600"
        );
        assert_eq!(format_cookies(&[]), "No cookies");
        assert_eq!(
            format_cookies(&[("content-type".to_string(), "application/json".to_string())]),
            "No cookies"
        );
    }

    #[test]
    fn builds_request_body_from_slint_mode() {
        assert_eq!(
            build_request_body("none", "ignored", "", "").unwrap(),
            RequestBody::None
        );
        assert_eq!(
            build_request_body("raw", "{\"name\":\"Zen\"}", "", "json").unwrap(),
            RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"name\":\"Zen\"}".to_string(),
            }
        );
        assert_eq!(
            build_request_body("raw", "hello", "", "text").unwrap(),
            RequestBody::Raw {
                content_type: Some("text/plain".to_string()),
                body: "hello".to_string(),
            }
        );
        assert_eq!(
            build_request_body("raw", "<ok/>", "", "xml").unwrap(),
            RequestBody::Raw {
                content_type: Some("application/xml".to_string()),
                body: "<ok/>".to_string(),
            }
        );
        assert_eq!(
            build_request_body("urlenc", "search=rust slint\nlimit: 20", "", "").unwrap(),
            RequestBody::FormUrlEncoded(vec![
                ("search".to_string(), "rust slint".to_string()),
                ("limit".to_string(), "20".to_string())
            ])
        );
        assert_eq!(
            build_request_body("form", "file=@/tmp/upload.txt", "", "").unwrap(),
            RequestBody::Multipart(vec![("file".to_string(), "@/tmp/upload.txt".to_string())])
        );
        assert_eq!(
            build_request_body("binary", "/tmp/body.bin", "", "").unwrap(),
            RequestBody::BinaryFile {
                path: "/tmp/body.bin".to_string(),
                content_type: None,
            }
        );
    }

    #[test]
    fn rejects_empty_binary_body_path() {
        let error = build_request_body("binary", "  ", "", "").expect_err("empty path");

        assert!(error.to_string().contains("path is empty"));
    }

    #[test]
    fn builds_graphql_payload_body() {
        assert_eq!(
            build_request_body(
                "graphql",
                "query User($id: ID!) { user(id: $id) { name } }",
                r#"{"id":"u_123"}"#,
                "",
            )
            .unwrap(),
            RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: r#"{"query":"query User($id: ID!) { user(id: $id) { name } }","variables":{"id":"u_123"}}"#.to_string(),
            }
        );

        let error = build_request_body("graphql", "{ viewer { id } }", "[]", "")
            .expect_err("invalid variables");
        assert!(error.to_string().contains("JSON object"));
    }

    #[test]
    fn returns_graphql_helper_templates() {
        let introspection = graphql_template("introspection").expect("introspection template");
        assert_eq!(introspection.name, "Introspection");
        assert!(introspection.query.contains("__schema"));
        assert_eq!(introspection.variables, "{}");

        let query = graphql_template("query").expect("query template");
        assert!(query.query.contains("query Example"));
        assert!(query.variables.contains("example-id"));

        let mutation = graphql_template("mutation").expect("mutation template");
        assert!(mutation.query.contains("mutation Example"));
        assert!(mutation.variables.contains("\"input\""));
        assert!(graphql_template("unknown").is_none());
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
    fn splits_and_formats_basic_auth_config_for_ui_fields() {
        assert_eq!(
            split_basic_auth_config("user:pass"),
            ("user".to_string(), "pass".to_string())
        );
        assert_eq!(
            split_basic_auth_config("user:p:a:s:s"),
            ("user".to_string(), "p:a:s:s".to_string())
        );
        assert_eq!(
            split_basic_auth_config("legacy-user"),
            ("legacy-user".to_string(), String::new())
        );

        assert_eq!(format_basic_auth_config("user", "pass"), "user:pass");
        assert_eq!(format_basic_auth_config("", ""), "");
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
    fn preserves_environment_profile_values_by_name() {
        let mut profiles = EnvironmentProfiles::new("dev", "baseUrl=http://localhost:8080");

        assert_eq!(
            profiles.switch_to("prod", "baseUrl=http://localhost:8080"),
            ""
        );
        profiles.save_active("baseUrl=https://api.example.com");
        assert_eq!(
            profiles.switch_to("dev", "baseUrl=https://api.example.com"),
            "baseUrl=http://localhost:8080"
        );
        assert_eq!(
            profiles.switch_to("prod", "baseUrl=http://localhost:8080"),
            "baseUrl=https://api.example.com"
        );
        assert_eq!(
            profiles.switch_to("local", "baseUrl=https://api.example.com"),
            ""
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
    fn builds_variable_rows_for_environment_page() {
        use slint::Model;

        let rows = variable_table_model(
            "baseUrl=https://api.example.com\ntoken=global",
            "baseUrl=http://localhost:8080",
        );

        assert_eq!(rows.row_count(), 3);
        let global = rows.row_data(0).expect("global row");
        assert_eq!(global.row_id, 0);
        assert_eq!(global.scope.as_str(), "global");
        assert_eq!(global.name.as_str(), "baseUrl");
        assert_eq!(global.current_value.as_str(), "https://api.example.com");

        let env = rows.row_data(2).expect("env row");
        assert_eq!(env.row_id, 0);
        assert_eq!(env.scope.as_str(), "environment");
        assert_eq!(env.name.as_str(), "baseUrl");
        assert_eq!(env.current_value.as_str(), "http://localhost:8080");
    }

    #[test]
    fn updates_adds_and_deletes_variable_text_rows() {
        let input = "# keep\nbaseUrl=https://api.example.com\ntoken=global";

        assert_eq!(
            update_variable_text(input, 1, "token", "changed"),
            "# keep\nbaseUrl=https://api.example.com\ntoken=changed"
        );

        assert_eq!(
            add_variable_text("baseUrl=https://api.example.com", "GLOBAL_VAR"),
            "baseUrl=https://api.example.com\nGLOBAL_VAR="
        );

        assert_eq!(
            add_variable_text("GLOBAL_VAR=one", "GLOBAL_VAR"),
            "GLOBAL_VAR=one\nGLOBAL_VAR_2="
        );

        assert_eq!(delete_variable_text(input, 0), "# keep\ntoken=global");
    }

    #[test]
    fn builds_key_value_rows_for_query_params() {
        use slint::Model;

        let rows = key_value_table_model("# keep\nsearch=slint\nlimit: 20");

        assert_eq!(rows.row_count(), 2);
        let search = rows.row_data(0).expect("search row");
        assert_eq!(search.row_id, 0);
        assert_eq!(search.key.as_str(), "search");
        assert_eq!(search.value.as_str(), "slint");

        let limit = rows.row_data(1).expect("limit row");
        assert_eq!(limit.row_id, 1);
        assert_eq!(limit.key.as_str(), "limit");
        assert_eq!(limit.value.as_str(), "20");
    }

    #[test]
    fn updates_adds_and_deletes_key_value_text_rows() {
        let input = "# keep\nsearch=slint\nlimit=20";

        assert_eq!(
            update_key_value_text(input, 1, "limit", "50"),
            "# keep\nsearch=slint\nlimit=50"
        );
        assert_eq!(
            add_key_value_text("search=slint", "param"),
            "search=slint\nparam="
        );
        assert_eq!(
            add_key_value_text("param=one", "param"),
            "param=one\nparam_2="
        );
        assert_eq!(
            merge_key_value_text(
                "search=slint\nlimit=20",
                "limit=50\nsort: desc",
                "query param",
                false,
                false,
            )
            .expect("merge params"),
            ("search=slint\nlimit=50\nsort=desc".to_string(), 2)
        );
        assert_eq!(
            merge_key_value_text(
                "Accept: application/json\nX-Trace=one",
                "accept: text/plain\nAuthorization=Bearer token",
                "header",
                true,
                true,
            )
            .expect("merge headers"),
            (
                "Accept: text/plain\nX-Trace: one\nAuthorization: Bearer token".to_string(),
                2
            )
        );
        assert!(
            merge_key_value_text("search=slint", "   \n# none", "query param", false, false)
                .expect_err("empty clipboard")
                .to_string()
                .contains("does not contain any query param rows")
        );
        let import_path =
            std::env::temp_dir().join(format!("zenapi-query-import-{}.txt", std::process::id()));
        fs::write(&import_path, "limit=50\nsort: desc").expect("write import fixture");
        assert_eq!(
            merge_key_value_file(
                "search=slint\nlimit=20",
                import_path.to_str().expect("utf-8 import path"),
                "query param",
                false,
                false,
            )
            .expect("merge params from file"),
            ("search=slint\nlimit=50\nsort=desc".to_string(), 2)
        );
        assert!(
            merge_key_value_file("search=slint", " ", "query param", false, false)
                .expect_err("empty import path")
                .to_string()
                .contains("query param import path is required")
        );
        assert_eq!(
            add_form_file_field_text(
                "search=slint",
                "upload",
                import_path.to_str().expect("utf-8 import path"),
            )
            .expect("add form file"),
            format!("search=slint\nupload=@{}", import_path.display())
        );
        assert!(
            add_form_file_field_text("search=slint", "", import_path.to_str().unwrap())
                .expect_err("empty field")
                .to_string()
                .contains("field name is required")
        );
        assert!(
            add_form_file_field_text("search=slint", "upload", "/tmp/zenapi-missing-upload")
                .expect_err("missing file")
                .to_string()
                .contains("does not exist")
        );
        let _ = fs::remove_file(import_path);
        assert_eq!(delete_key_value_text(input, 0), "# keep\nlimit=20");
    }

    #[test]
    fn updates_adds_and_deletes_test_assertion_rows() {
        use slint::Model;

        let input = "# keep\nstatus_equals 200\nheader_equals content-type application/json\nbody_contains ok";
        let rows = test_assertion_table_model(input);
        assert_eq!(rows.row_count(), 3);

        let status = rows.row_data(0).expect("status row");
        assert_eq!(status.row_id, 0);
        assert_eq!(status.kind.as_str(), "status_equals");
        assert_eq!(status.target.as_str(), "200");
        assert_eq!(status.expected.as_str(), "");

        let header = rows.row_data(1).expect("header row");
        assert_eq!(header.kind.as_str(), "header_equals");
        assert_eq!(header.target.as_str(), "content-type");
        assert_eq!(header.expected.as_str(), "application/json");

        assert_eq!(
            update_test_assertion_text(input, 1, "json_path_equals", "data.id", "1"),
            "# keep\nstatus_equals 200\njson_path_equals data.id 1\nbody_contains ok"
        );
        assert_eq!(
            next_test_assertion_template("status_equals"),
            ("status_in_range", "200", "299")
        );
        let (kind, target, expected) = next_test_assertion_template("header_exists");
        assert_eq!(
            update_test_assertion_text(input, 1, kind, target, expected),
            "# keep\nstatus_equals 200\nheader_equals content-type application/json\nbody_contains ok"
        );
        assert_eq!(
            next_test_assertion_template("json_path_equals"),
            ("status_equals", "200", "")
        );
        assert_eq!(
            add_test_assertion_text("status_equals 200"),
            "status_equals 200\nstatus_equals 200"
        );
        assert_eq!(
            add_test_assertion_template_text("status_equals 200", "header").unwrap(),
            "status_equals 200\nheader_equals content-type application/json"
        );
        assert_eq!(
            add_test_assertion_template_text("status_equals 200", "body").unwrap(),
            "status_equals 200\nbody_contains ok"
        );
        assert_eq!(
            add_test_assertion_template_text("status_equals 200", "json").unwrap(),
            "status_equals 200\njson_path_equals data.id 1"
        );
        assert_eq!(
            test_assertion_template("range"),
            Some(("status_in_range", "200", "299"))
        );
        assert!(add_test_assertion_template_text(input, "unknown").is_err());
        assert_eq!(
            delete_test_assertion_text(input, 0),
            "# keep\nheader_equals content-type application/json\nbody_contains ok"
        );
    }

    #[test]
    fn masks_secret_values_in_variable_preview() {
        let preview = variables_json_preview(
            "token=global-secret\nbaseUrl=https://api.example.com",
            "dev",
            "API_KEY=local-secret",
        );

        assert!(preview.contains("\"activeEnvironment\": \"dev\""));
        assert!(preview.contains("\"token\": \"********\""));
        assert!(preview.contains("\"API_KEY\": \"********\""));
        assert!(preview.contains("\"baseUrl\": \"https://api.example.com\""));
        assert!(!preview.contains("global-secret"));
        assert!(!preview.contains("local-secret"));
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
            raw_body_subtype: "json".to_string(),
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
            raw_body_subtype: "json".to_string(),
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
                raw_body_subtype: "text".to_string(),
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
        assert_eq!(
            request.body,
            CollectionBody::Raw {
                content_type: "text/plain".to_string(),
                body: "{\"name\":\"{{name}}\"}".to_string(),
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
    fn formats_codegen_metadata_for_generated_snippets() {
        let request = CodegenRequest {
            method: "POST".to_string(),
            url: "https://api.example.com/users".to_string(),
            headers: vec![("Accept".to_string(), "application/json".to_string())],
            query_params: vec![("debug".to_string(), "true".to_string())],
            body: RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"ok\":true}".to_string(),
            },
        };

        let metadata = codegen_metadata(
            &request,
            codegen_language_label(SnippetLanguage::Curl),
            "a\nb",
        );

        assert!(metadata.contains("Language: cURL"));
        assert!(metadata.contains("Request: POST https://api.example.com/users"));
        assert!(metadata.contains("Lines: 2 / Bytes: 3"));
        assert!(metadata.contains("Headers: 1 / Query params: 1 / Body: raw"));
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
    fn renames_nested_collection_requests_by_flattened_row_id() {
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

        let renamed =
            rename_collection_request_at(&mut collection, 1, "Create team").expect("renamed");

        assert_eq!(renamed.name, "Create team");
        assert_eq!(
            collection_request_at(&collection, 1).map(|request| request.name.as_str()),
            Some("Create team")
        );
        assert_eq!(
            collection_request_at(&collection, 2).map(|request| request.name.as_str()),
            Some("Health")
        );
        assert!(rename_collection_request_at(&mut collection, 9, "Missing").is_none());
        assert!(rename_collection_request_at(&mut collection, 1, "   ").is_none());
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
            (
                "urlenc".to_string(),
                "search=slint".to_string(),
                "json".to_string()
            )
        );
        assert_eq!(
            collection_body_to_slint(&CollectionBody::Raw {
                content_type: "application/xml; charset=utf-8".to_string(),
                body: "<ok/>".to_string(),
            }),
            ("raw".to_string(), "<ok/>".to_string(), "xml".to_string())
        );
        assert_eq!(
            collection_body_to_slint(&CollectionBody::Binary {
                path: "/tmp/body.bin".to_string(),
                content_type: "application/octet-stream".to_string(),
            }),
            (
                "binary".to_string(),
                "/tmp/body.bin".to_string(),
                "json".to_string()
            )
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
    fn saves_runner_report_formats_to_disk() {
        let summary = CollectionRunSummary {
            collection_name: "Demo".to_string(),
            total: 1,
            passed: 1,
            failed: 0,
            stopped_early: false,
            elapsed_ms: 15,
            results: vec![CollectionRunResult {
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
            }],
        };
        let text_path =
            std::env::temp_dir().join(format!("zenapi-runner-report-{}.txt", std::process::id()));
        let json_path =
            std::env::temp_dir().join(format!("zenapi-runner-report-{}.json", std::process::id()));
        let _ = fs::remove_file(&text_path);
        let _ = fs::remove_file(&json_path);

        save_runner_report(
            text_path.to_str().expect("utf-8 temp path"),
            &summary,
            "text",
        )
        .expect("save runner report");

        assert_eq!(
            fs::read_to_string(&text_path).expect("runner report"),
            "Demo: 1 passed, 0 failed, 1 total / 15 ms\n[PASS] HTTP 200 GET https://api.example.com/health (Demo / Health)"
        );

        save_runner_report(
            json_path.to_str().expect("utf-8 temp path"),
            &summary,
            "JSON",
        )
        .expect("save json runner report");
        let parsed: CollectionRunSummary =
            serde_json::from_str(&fs::read_to_string(&json_path).expect("json runner report"))
                .expect("parse runner json");
        assert_eq!(parsed, summary);

        assert_eq!(normalize_runner_report_format(" json "), "json");
        assert_eq!(normalize_runner_report_format("csv"), "text");
        assert!(save_runner_report("   ", &summary, "json").is_err());
        let _ = fs::remove_file(text_path);
        let _ = fs::remove_file(json_path);
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
    fn parses_websocket_protocols_and_binary_messages() {
        assert_eq!(
            parse_websocket_protocols("chat, superchat\ntrace"),
            vec![
                "chat".to_string(),
                "superchat".to_string(),
                "trace".to_string()
            ]
        );
        assert_eq!(
            parse_websocket_binary_message("00 01 ff,0x10").unwrap(),
            vec![0, 1, 255, 16]
        );
        assert!(parse_websocket_binary_message("").is_err());
        assert!(parse_websocket_binary_message("100").is_err());
        assert_eq!(
            websocket_session_command("text", "hello").unwrap(),
            client::WebSocketSessionCommand::SendText("hello".to_string())
        );
        assert_eq!(
            websocket_session_command("binary", "0a ff").unwrap(),
            client::WebSocketSessionCommand::SendBinary(vec![10, 255])
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
        let session_events = vec![
            client::WebSocketSessionEvent::Connected {
                url: "ws://localhost/socket".to_string(),
            },
            client::WebSocketSessionEvent::Sent(client::WebSocketMessage {
                kind: client::WebSocketMessageKind::Text,
                data: "hello".to_string(),
            }),
            client::WebSocketSessionEvent::Received(client::WebSocketMessage {
                kind: client::WebSocketMessageKind::Text,
                data: "echo:hello".to_string(),
            }),
        ];
        assert_eq!(
            format_websocket_session_events(&session_events),
            "1. connected ws://localhost/socket\n2. sent [text]: hello\n3. received [text]: echo:hello"
        );
        assert_eq!(
            websocket_session_status(session_events.last().expect("event")),
            "WebSocket received"
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
        assert_eq!(latest_sse_event_id(&sse.events), Some("42"));

        let stream_events = vec![
            client::SseStreamEvent::Connected {
                url: "http://localhost/events".to_string(),
            },
            client::SseStreamEvent::Event(client::SseEvent {
                event: Some("update".to_string()),
                data: "{\"ok\":true}".to_string(),
                id: Some("42".to_string()),
                retry: None,
            }),
            client::SseStreamEvent::Reconnecting {
                attempt: 1,
                delay_ms: 500,
                reason: "stream ended".to_string(),
            },
            client::SseStreamEvent::Closed("stream ended".to_string()),
        ];
        assert_eq!(
            format_sse_stream_events(&stream_events),
            "1. connected http://localhost/events\n2. update / id 42\n{\"ok\":true}\n3. reconnecting attempt 1 in 500 ms\nstream ended\n4. closed\nstream ended"
        );
        assert_eq!(sse_stream_event_last_id(&stream_events[1]), Some("42"));
        assert_eq!(sse_stream_status(&stream_events[2]), "SSE reconnecting");
        assert_eq!(sse_stream_tone(&stream_events[2]), "busy");
        assert!(sse_stream_event_done(&stream_events[3]));
    }

    #[test]
    fn bounds_sse_stream_event_history() {
        let mut events = Vec::new();
        for index in 0..(MAX_SSE_STREAM_EVENTS + 3) {
            push_bounded_sse_stream_event(
                &mut events,
                client::SseStreamEvent::Event(client::SseEvent {
                    event: None,
                    data: format!("event {index}"),
                    id: Some(index.to_string()),
                    retry: None,
                }),
            );
        }

        assert_eq!(events.len(), MAX_SSE_STREAM_EVENTS);
        assert_eq!(sse_stream_event_last_id(&events[0]), Some("3"));
        assert_eq!(sse_stream_meta(events.len()), "latest 200 events");
    }

    #[test]
    fn formats_grpc_draft_for_response_panel() {
        let draft = build_grpc_request_draft(
            "http://localhost:50051",
            "demo.Users/GetUser",
            "authorization=Bearer token",
            r#"{"id":"u_123"}"#,
            "unary demo.Users/GetUser demo.GetUserRequest demo.GetUserResponse",
        )
        .expect("draft");

        assert_eq!(
            format_grpc_draft(&draft),
            "Endpoint: http://localhost:50051\nMethod: /demo.Users/GetUser\n\nDescriptor\nKind: unary\nRequest: demo.GetUserRequest\nResponse: demo.GetUserResponse\n\nMetadata\nauthorization: Bearer token\n\nMessage\n{\n  \"id\": \"u_123\"\n}"
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

        assert_eq!(clear_mock_logs(&mut logs), 2);
        assert!(logs.is_empty());
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
            raw_body_subtype: "json".to_string(),
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
                raw_body_subtype: "json".to_string(),
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
                raw_body_subtype: "json".to_string(),
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
                raw_body_subtype: "json".to_string(),
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
                raw_body_subtype: "json".to_string(),
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
                raw_body_subtype: "json".to_string(),
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
            ("none".to_string(), "json".to_string(), String::new())
        );
        assert_eq!(
            request_body_preview(&RequestBody::Raw {
                content_type: Some("text/plain".to_string()),
                body: "{\"ok\":true}".to_string(),
            }),
            (
                "raw".to_string(),
                "text".to_string(),
                "{\"ok\":true}".to_string()
            )
        );
        assert_eq!(
            request_body_preview(&RequestBody::Multipart(vec![(
                "file".to_string(),
                "@/tmp/upload.txt".to_string()
            )])),
            (
                "form".to_string(),
                "json".to_string(),
                "file=@/tmp/upload.txt".to_string()
            )
        );
        assert_eq!(
            request_body_preview(&RequestBody::BinaryFile {
                path: "/tmp/body.bin".to_string(),
                content_type: None,
            }),
            (
                "binary".to_string(),
                "json".to_string(),
                "/tmp/body.bin".to_string()
            )
        );
    }

    #[test]
    fn truncates_long_history_response_previews() {
        assert_eq!(truncate_preview("abc", 3), "abc");
        assert_eq!(truncate_preview("abcdef", 3), "abc\n...");
    }
}

mod input;
mod read_only_text;

use std::{ops::Range, path::Path, sync::Arc};

use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, Entity, FontWeight, HighlightStyle, Hsla, MouseButton,
    MouseDownEvent, MouseUpEvent, Render, SharedString, StyledText, Window, WindowBounds,
    WindowOptions, div, px, rgb, size,
};
use tokio::{
    runtime::Runtime,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use zenapi::{
    assertions::{
        ResponseAssertion, ResponseAssertionKind, ResponseAssertionResult,
        evaluate_response_assertions,
    },
    client::{self, RequestBody},
    codegen::{CodegenRequest, SnippetLanguage, generate_snippet},
    collection_runner::{
        self, CollectionRunResult, CollectionRunSummary, FailureStrategy, RunnerOptions,
    },
    collections::{
        ApiCollection, CollectionBody, CollectionFolder, CollectionItem, CollectionRequest,
        NameValue,
    },
    history::{HistoryRequest, HistoryResponse, RequestHistory},
    mock_server::{MockRequestLog, MockServer},
    openapi::{ApiRoute, ApiSpec, load_openapi_file},
    pre_request::{
        execute_pre_request_actions, pre_request_action_labels, resolve_codegen_request_templates,
    },
    variables::{Variable, VariableStore},
};

use self::{
    input::{TextAccepted, TextChanged, TextInput, bind_text_input_keys},
    read_only_text::{ReadOnlyTextView, bind_read_only_text_keys},
};

#[cfg(test)]
use zenapi::variables::replace_variables;

const PLATFORM_UI_FONT: &str = ".SystemUIFont";
const PLATFORM_MONOSPACE_FONT: &str = "monospace";
const INITIAL_RESPONSE_BODY: &str = "Import an OpenAPI or Swagger document to begin.";
const UI_COLOR_SURFACE: u32 = 0xffffff;
const UI_COLOR_SURFACE_MUTED: u32 = 0xf9fafb;
const UI_COLOR_HOVER: u32 = 0xf3f4f6;
const UI_COLOR_BORDER: u32 = 0xe5e7eb;
const UI_COLOR_BORDER_STRONG: u32 = 0xd1d5db;
const UI_COLOR_TEXT_PRIMARY: u32 = 0x111827;
const UI_COLOR_TEXT_SECONDARY: u32 = 0x6b7280;
const UI_COLOR_TEXT_MUTED: u32 = 0x9ca3af;
const UI_COLOR_TEXT_BODY: u32 = 0x374151;
const UI_COLOR_ACCENT: u32 = 0x2563eb;
const UI_COLOR_ACCENT_TEXT: u32 = 0x1d4ed8;
const UI_COLOR_ACCENT_SURFACE: u32 = 0xeff6ff;
const KEY_VALUE_KEY_COLUMN_WIDTH: f32 = 150.;
const TEST_ASSERTION_NAME_COLUMN_WIDTH: f32 = 132.;
const TEST_ASSERTION_KIND_COLUMN_WIDTH: f32 = 132.;
const COLLECTION_TREE_ROOT_ROW_HEIGHT: f32 = 30.;
const COLLECTION_TREE_FOLDER_ROW_HEIGHT: f32 = 30.;
const COLLECTION_TREE_REQUEST_ROW_HEIGHT: f32 = 36.;
const COLLECTION_TREE_INDENT_BASE: f32 = 8.;
const COLLECTION_TREE_INDENT_STEP: f32 = 14.;
const COLLECTION_TREE_MARKER_WIDTH: f32 = 14.;
const HTTP_METHOD_LABEL_WIDTH: f32 = 58.;
const GRAPHQL_SCHEMA_FIELD_LIMIT: usize = 12;
const GRAPHQL_SCHEMA_TYPE_LIMIT: usize = 18;
const GRAPHQL_QUERY_TEMPLATE_LIMIT: usize = 5;
const WEBSOCKET_LOG_LIMIT: usize = 24;
const SSE_EVENT_FETCH_LIMIT: usize = 6;
const SSE_LOG_LIMIT: usize = 24;
const GRAPHQL_INTROSPECTION_QUERY: &str = "query IntrospectionQuery { __schema { queryType { name } mutationType { name } subscriptionType { name } types { kind name description fields(includeDeprecated: true) { name description args { name description type { kind name ofType { kind name ofType { kind name } } } defaultValue } type { kind name ofType { kind name ofType { kind name } } } isDeprecated deprecationReason } inputFields { name description type { kind name ofType { kind name ofType { kind name } } } defaultValue } interfaces { kind name ofType { kind name } } enumValues(includeDeprecated: true) { name description isDeprecated deprecationReason } possibleTypes { kind name ofType { kind name } } } directives { name description locations args { name description type { kind name ofType { kind name ofType { kind name } } } defaultValue } } } }";

pub fn run() -> Result<()> {
    let runtime = Arc::new(Runtime::new()?);

    gpui_platform::application().run(move |cx: &mut App| {
        bind_text_input_keys(cx);
        bind_read_only_text_keys(cx);

        let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            {
                let runtime = runtime.clone();
                move |_, cx| cx.new(|cx| ZenApiApp::new(runtime, cx))
            },
        )
        .expect("open ZenAPI window");
        cx.activate(true);
    });

    Ok(())
}

struct ZenApiApp {
    runtime: Arc<Runtime>,
    import_path: Entity<TextInput>,
    collection_path: Entity<TextInput>,
    collection_rename_input: Entity<TextInput>,
    route_filter: Entity<TextInput>,
    history_filter: Entity<TextInput>,
    url: Entity<TextInput>,
    environment_name_input: Entity<TextInput>,
    active_environment: Option<String>,
    global_variables: Vec<KeyValueRow>,
    environments: Vec<EnvironmentConfig>,
    query_params: Vec<KeyValueRow>,
    request_headers: Vec<KeyValueRow>,
    auth_mode: AuthMode,
    bearer_token: Entity<TextInput>,
    basic_username: Entity<TextInput>,
    basic_password: Entity<TextInput>,
    jwt_token: Entity<TextInput>,
    api_key_name: Entity<TextInput>,
    api_key_value: Entity<TextInput>,
    api_key_placement: ApiKeyPlacement,
    pre_request_script: Entity<TextInput>,
    pre_request_status: String,
    last_pre_request_actions: Vec<String>,
    request_body_mode: RequestBodyMode,
    raw_body_format: RawBodyFormat,
    request_body: Entity<TextInput>,
    graphql_query: Entity<TextInput>,
    graphql_variables: Entity<TextInput>,
    graphql_schema_summary: String,
    graphql_schema_browser: String,
    graphql_query_templates: Vec<GraphqlQueryTemplate>,
    websocket_url: Entity<TextInput>,
    websocket_protocols: Entity<TextInput>,
    websocket_headers: Vec<KeyValueRow>,
    websocket_message: Entity<TextInput>,
    websocket_message_mode: WebSocketMessageMode,
    websocket_status: String,
    websocket_running: bool,
    websocket_command_tx: Option<mpsc::UnboundedSender<client::WebSocketSessionCommand>>,
    websocket_messages: Vec<WebSocketLogEntry>,
    sse_url: Entity<TextInput>,
    sse_status: String,
    sse_running: bool,
    sse_subscription: Option<JoinHandle<()>>,
    sse_last_event_id: Option<String>,
    sse_events: Vec<SseLogEntry>,
    form_data_body: Vec<KeyValueRow>,
    urlencoded_body: Vec<KeyValueRow>,
    binary_body_path: Entity<TextInput>,
    request_assertions: Vec<TestAssertionRow>,
    last_assertion_results: Vec<ResponseAssertionResult>,
    routes: Vec<ApiRoute>,
    visible_routes: Vec<ApiRoute>,
    selected_route: Option<usize>,
    collection: ApiCollection,
    expanded_collection_nodes: Vec<String>,
    collection_status: String,
    collection_context_menu: Option<CollectionContextMenu>,
    method: String,
    spec_label: String,
    response_status: String,
    response_meta: String,
    response_tone: ResponseTone,
    response_body: String,
    response_raw_body: String,
    response_headers: String,
    response_view: ResponseView,
    response_pretty_collapsed: bool,
    response_body_viewer: Entity<ReadOnlyTextView>,
    codegen_language: SnippetLanguage,
    codegen_menu_open: bool,
    server: Option<MockServer>,
    server_running: bool,
    server_status: String,
    mock_logs: Vec<MockRequestLog>,
    runner_running: bool,
    runner_stop_on_failure: bool,
    runner_status: String,
    runner_results: Vec<CollectionRunResult>,
    history: RequestHistory,
    history_query: String,
    busy: bool,
}

struct KeyValueRow {
    key: Entity<TextInput>,
    value: Entity<TextInput>,
}

struct TestAssertionRow {
    name: Entity<TextInput>,
    kind: TestAssertionKind,
    target: Entity<TextInput>,
    expected: Entity<TextInput>,
}

struct EnvironmentConfig {
    name: String,
    variables: Vec<KeyValueRow>,
}

#[derive(Clone)]
struct CollectionContextMenu {
    node_id: String,
    label: String,
    kind: CollectionNodeKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CollectionNodeKind {
    Root,
    Folder,
    Request,
}

#[derive(Clone)]
struct DraggedCollectionNode {
    node_id: String,
    label: String,
}

struct CollectionDragPreview {
    label: String,
}

struct RequestBuild {
    request: CodegenRequest,
    pre_request_actions: usize,
    pre_request_action_labels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GraphqlQueryTemplate {
    field_name: String,
    operation: String,
    variables: String,
}

struct GraphqlOperationArg {
    name: String,
    type_ref: String,
    default_value: Option<String>,
    placeholder: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WebSocketLogEntry {
    direction: WebSocketDirection,
    kind: String,
    data: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WebSocketDirection {
    Sent,
    Received,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WebSocketMessageMode {
    Text,
    BinaryHex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SseLogEntry {
    event: String,
    data: String,
    id: Option<String>,
}

fn ui_surface() -> Hsla {
    rgb(UI_COLOR_SURFACE).into()
}

fn ui_surface_muted() -> Hsla {
    rgb(UI_COLOR_SURFACE_MUTED).into()
}

fn ui_hover() -> Hsla {
    rgb(UI_COLOR_HOVER).into()
}

fn ui_border() -> Hsla {
    rgb(UI_COLOR_BORDER).into()
}

fn ui_border_strong() -> Hsla {
    rgb(UI_COLOR_BORDER_STRONG).into()
}

fn ui_text_primary() -> Hsla {
    rgb(UI_COLOR_TEXT_PRIMARY).into()
}

fn ui_text_secondary() -> Hsla {
    rgb(UI_COLOR_TEXT_SECONDARY).into()
}

fn ui_text_muted() -> Hsla {
    rgb(UI_COLOR_TEXT_MUTED).into()
}

fn ui_text_body() -> Hsla {
    rgb(UI_COLOR_TEXT_BODY).into()
}

fn ui_accent() -> Hsla {
    rgb(UI_COLOR_ACCENT).into()
}

fn ui_accent_text() -> Hsla {
    rgb(UI_COLOR_ACCENT_TEXT).into()
}

fn ui_accent_surface() -> Hsla {
    rgb(UI_COLOR_ACCENT_SURFACE).into()
}

impl ZenApiApp {
    fn new(runtime: Arc<Runtime>, cx: &mut Context<Self>) -> Self {
        let import_path = cx.new(|cx| TextInput::new(cx, "OpenAPI / Swagger file path", true));
        let collection_path = cx.new(|cx| TextInput::new(cx, "Collection JSON path", true));
        let collection_rename_input = cx.new(|cx| TextInput::new(cx, "Collection item name", true));
        let route_filter =
            cx.new(|cx| TextInput::new(cx, "Filter method, path, or summary", false));
        let history_filter = cx.new(|cx| TextInput::new(cx, "Filter history", false));
        let url = cx.new(|cx| TextInput::new(cx, "Request URL", true));
        let environment_name_input = cx.new(|cx| TextInput::new(cx, "New environment", true));
        let global_variables = key_value_rows(
            cx,
            &[
                ("baseUrl", "https://api.example.com"),
                ("token", "secret"),
                ("", ""),
            ],
        );
        let environments = vec![
            environment_config(
                cx,
                "dev",
                &[
                    ("baseUrl", "http://localhost:8080"),
                    ("token", "dev-token"),
                    ("", ""),
                ],
            ),
            environment_config(
                cx,
                "test",
                &[
                    ("baseUrl", "https://test.example.com"),
                    ("token", "test-token"),
                    ("", ""),
                ],
            ),
            environment_config(
                cx,
                "prod",
                &[
                    ("baseUrl", "https://api.example.com"),
                    ("token", "prod-token"),
                    ("", ""),
                ],
            ),
        ];
        let query_params = key_value_rows(
            cx,
            &[("page", "1"), ("limit", "20"), ("search", "term"), ("", "")],
        );
        let request_headers = key_value_rows(
            cx,
            &[
                ("Accept", "application/json"),
                ("Authorization", "Bearer token"),
                ("X-Request-Id", "request-id"),
                ("", ""),
            ],
        );
        let bearer_token = cx.new(|cx| TextInput::new(cx, "Bearer token", true));
        let basic_username = cx.new(|cx| TextInput::new(cx, "Username", true));
        let basic_password = cx.new(|cx| TextInput::new(cx, "Password", true));
        let jwt_token = cx.new(|cx| TextInput::new(cx, "JWT token", true));
        let api_key_name = cx.new(|cx| TextInput::new(cx, "X-API-Key", true));
        let api_key_value = cx.new(|cx| TextInput::new(cx, "API key value", true));
        let pre_request_script = cx.new(|cx| TextInput::new(cx, "Pre-request action line", true));
        let request_body = cx.new(|cx| TextInput::new(cx, "JSON body", true));
        let graphql_query = cx.new(|cx| TextInput::new(cx, "GraphQL query", true));
        let graphql_variables = cx.new(|cx| TextInput::new(cx, "GraphQL variables JSON", true));
        let websocket_url = cx.new(|cx| TextInput::new(cx, "ws://localhost:8080/socket", true));
        let websocket_protocols = cx.new(|cx| TextInput::new(cx, "Subprotocols: chat, json", true));
        let websocket_headers = key_value_rows(cx, &[("X-Token", "token"), ("", "")]);
        let websocket_message = cx.new(|cx| TextInput::new(cx, "WebSocket message", true));
        let sse_url = cx.new(|cx| TextInput::new(cx, "http://localhost:8080/events", true));
        let response_body_viewer = cx.new(|cx| ReadOnlyTextView::new(cx, INITIAL_RESPONSE_BODY));
        let form_data_body = key_value_rows(
            cx,
            &[
                ("field", "value"),
                ("file", "@/path/to/file"),
                ("", ""),
                ("", ""),
            ],
        );
        let urlencoded_body = key_value_rows(
            cx,
            &[
                ("username", "dev"),
                ("password", "secret"),
                ("", ""),
                ("", ""),
            ],
        );
        let binary_body_path = cx.new(|cx| TextInput::new(cx, "Binary file path", true));
        let request_assertions = assertion_rows_from_assertions(cx, &[]);

        cx.subscribe(&import_path, |app, _input, _event: &TextAccepted, cx| {
            app.import_openapi(cx);
        })
        .detach();

        cx.subscribe(
            &collection_path,
            |app, _input, _event: &TextAccepted, cx| {
                app.import_collection(cx);
            },
        )
        .detach();

        cx.subscribe(
            &collection_rename_input,
            |app, _input, _event: &TextAccepted, cx| {
                app.rename_collection_target(cx);
            },
        )
        .detach();

        cx.subscribe(&route_filter, |app, _input, event: &TextChanged, cx| {
            app.apply_route_filter(&event.text);
            cx.notify();
        })
        .detach();

        cx.subscribe(&history_filter, |app, _input, event: &TextChanged, cx| {
            app.history_query = event.text.clone();
            cx.notify();
        })
        .detach();

        cx.subscribe(&url, |app, _input, _event: &TextAccepted, cx| {
            app.send_request(cx);
        })
        .detach();

        cx.subscribe(
            &environment_name_input,
            |app, _input, _event: &TextAccepted, cx| {
                app.add_environment(cx);
            },
        )
        .detach();

        Self {
            runtime,
            import_path,
            collection_path,
            collection_rename_input,
            route_filter,
            history_filter,
            url,
            environment_name_input,
            active_environment: None,
            global_variables,
            environments,
            query_params,
            request_headers,
            auth_mode: AuthMode::None,
            bearer_token,
            basic_username,
            basic_password,
            jwt_token,
            api_key_name,
            api_key_value,
            api_key_placement: ApiKeyPlacement::Header,
            pre_request_script,
            pre_request_status: "idle".to_string(),
            last_pre_request_actions: Vec::new(),
            request_body_mode: RequestBodyMode::None,
            raw_body_format: RawBodyFormat::Json,
            request_body,
            graphql_query,
            graphql_variables,
            graphql_schema_summary: String::new(),
            graphql_schema_browser: String::new(),
            graphql_query_templates: Vec::new(),
            websocket_url,
            websocket_protocols,
            websocket_headers,
            websocket_message,
            websocket_message_mode: WebSocketMessageMode::Text,
            websocket_status: "idle".to_string(),
            websocket_running: false,
            websocket_command_tx: None,
            websocket_messages: Vec::new(),
            sse_url,
            sse_status: "idle".to_string(),
            sse_running: false,
            sse_subscription: None,
            sse_last_event_id: None,
            sse_events: Vec::new(),
            form_data_body,
            urlencoded_body,
            binary_body_path,
            request_assertions,
            last_assertion_results: Vec::new(),
            routes: Vec::new(),
            visible_routes: Vec::new(),
            selected_route: None,
            collection: ApiCollection::new("ZenAPI Collection"),
            expanded_collection_nodes: vec!["collection".to_string()],
            collection_status: "No collection file".to_string(),
            collection_context_menu: None,
            method: "GET".to_string(),
            spec_label: "No spec loaded".to_string(),
            response_status: "Idle".to_string(),
            response_meta: String::new(),
            response_tone: ResponseTone::Neutral,
            response_body: INITIAL_RESPONSE_BODY.to_string(),
            response_raw_body: INITIAL_RESPONSE_BODY.to_string(),
            response_headers: String::new(),
            response_view: ResponseView::Pretty,
            response_pretty_collapsed: false,
            response_body_viewer,
            codegen_language: SnippetLanguage::Curl,
            codegen_menu_open: false,
            server: None,
            server_running: false,
            server_status: "Mock stopped".to_string(),
            mock_logs: Vec::new(),
            runner_running: false,
            runner_stop_on_failure: false,
            runner_status: "Runner idle".to_string(),
            runner_results: Vec::new(),
            history: RequestHistory::new(),
            history_query: String::new(),
            busy: false,
        }
    }

    fn import_openapi(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        let path = self.import_path.read(cx).text();
        let path = path.trim();
        if path.is_empty() {
            self.set_response(
                "Import needs a file path",
                "",
                ResponseTone::Error,
                "Enter a local OpenAPI or Swagger JSON/YAML file path.",
            );
            cx.notify();
            return;
        }

        match load_openapi_file(path) {
            Ok(spec) => {
                if let Some(server) = self.server.take() {
                    let runtime = self.runtime.clone();
                    runtime.spawn(async move {
                        server.stop().await;
                    });
                }

                let spec_name = display_spec_name(&spec);
                let routes = spec.routes;
                self.visible_routes = routes.clone();
                self.routes = routes;
                self.selected_route = None;
                self.spec_label = display_spec_label(path);
                self.server_running = false;
                self.server_status = if self.routes.is_empty() {
                    "No mock routes".to_string()
                } else {
                    "Mock ready".to_string()
                };
                self.route_filter
                    .update(cx, |input, cx| input.set_text("", cx));
                self.set_response(
                    format!("Imported {spec_name}"),
                    format!("{} routes", self.routes.len()),
                    ResponseTone::Success,
                    format!("Ready: {} routes parsed.", self.routes.len()),
                );
            }
            Err(error) => {
                self.set_response("Import failed", "", ResponseTone::Error, error.to_string());
            }
        }

        cx.notify();
    }

    fn import_collection(&mut self, cx: &mut Context<Self>) {
        let path = self.collection_path.read(cx).text();
        let path = path.trim();
        if path.is_empty() {
            self.set_response(
                "Collection path needed",
                "",
                ResponseTone::Error,
                "Enter a ZenAPI or Postman collection JSON path.",
            );
            cx.notify();
            return;
        }

        match ApiCollection::load_file(path) {
            Ok(collection) => {
                self.collection_status = format!("Imported {}", collection.name);
                self.collection = collection;
                self.expanded_collection_nodes = vec!["collection".to_string()];
                self.set_response(
                    "Collection imported",
                    self.collection.items.len().to_string(),
                    ResponseTone::Success,
                    format!("Loaded collection: {}", self.collection.name),
                );
            }
            Err(error) => {
                self.set_response(
                    "Collection failed",
                    "",
                    ResponseTone::Error,
                    error.to_string(),
                );
            }
        }
        cx.notify();
    }

    fn export_collection(&mut self, postman: bool, cx: &mut Context<Self>) {
        let path = self.collection_path.read(cx).text();
        let path = path.trim();
        if path.is_empty() {
            self.set_response(
                "Collection path needed",
                "",
                ResponseTone::Error,
                "Enter a target collection JSON path.",
            );
            cx.notify();
            return;
        }

        let result = if postman {
            self.collection.save_postman_file(path)
        } else {
            self.collection.save_file(path)
        };

        match result {
            Ok(()) => {
                self.collection_status = if postman {
                    "Exported Postman".to_string()
                } else {
                    "Exported ZenAPI".to_string()
                };
                self.set_response(
                    "Collection exported",
                    "",
                    ResponseTone::Success,
                    format!("Wrote collection: {path}"),
                );
            }
            Err(error) => {
                self.set_response("Export failed", "", ResponseTone::Error, error.to_string());
            }
        }
        cx.notify();
    }

    fn save_current_request_to_collection(&mut self, cx: &mut Context<Self>) {
        let raw_request = self.current_raw_codegen_request(cx);
        match self.current_request_build(cx) {
            Ok(build) if build.request.url.is_empty() => {
                self.pre_request_status = pre_request_status_label(build.pre_request_actions);
                self.last_pre_request_actions = build.pre_request_action_labels;
                self.set_response(
                    "Save needs URL",
                    "",
                    ResponseTone::Error,
                    "Enter a request URL before saving to the collection.",
                );
                cx.notify();
                return;
            }
            Ok(build) => {
                self.pre_request_status = pre_request_status_label(build.pre_request_actions);
                self.last_pre_request_actions = build.pre_request_action_labels;
            }
            Err(error) => {
                self.pre_request_status = pre_request_error_label(&error.to_string());
                self.last_pre_request_actions.clear();
                self.set_response("Save failed", "", ResponseTone::Error, error.to_string());
                cx.notify();
                return;
            }
        }
        let tests = match self.current_response_assertions(cx) {
            Ok(tests) => tests,
            Err(error) => {
                self.set_response("Save failed", "", ResponseTone::Error, error.to_string());
                cx.notify();
                return;
            }
        };

        let collection_request = collection_request_for_save(
            &raw_request,
            self.pre_request_script.read(cx).text(),
            tests,
        );
        self.collection
            .items
            .push(CollectionItem::Request(collection_request));
        self.collection_status = format!("{} items", self.collection.items.len());
        self.set_response(
            "Request saved",
            self.collection.name.clone(),
            ResponseTone::Success,
            "Saved current request to collection.",
        );
        cx.notify();
    }

    fn open_collection_menu(&mut self, menu: CollectionContextMenu, cx: &mut Context<Self>) {
        let label = menu.label.clone();
        self.collection_rename_input
            .update(cx, |input, cx| input.set_text(label, cx));
        self.collection_context_menu = Some(menu);
        cx.notify();
    }

    fn close_collection_menu(&mut self, cx: &mut Context<Self>) {
        self.collection_context_menu = None;
        cx.notify();
    }

    fn add_collection_request(&mut self, target_id: String, cx: &mut Context<Self>) {
        let request = CollectionItem::Request(blank_collection_request());
        if insert_collection_item(&mut self.collection.items, &target_id, request) {
            self.ensure_collection_node_expanded(target_id);
            self.refresh_collection_status("Request created");
        } else {
            self.collection_status = "Create failed".to_string();
        }
        cx.notify();
    }

    fn add_collection_folder(&mut self, target_id: String, cx: &mut Context<Self>) {
        let folder = CollectionItem::Folder(CollectionFolder {
            name: "New Folder".to_string(),
            description: String::new(),
            items: Vec::new(),
        });
        if insert_collection_item(&mut self.collection.items, &target_id, folder) {
            self.ensure_collection_node_expanded(target_id);
            self.refresh_collection_status("Folder created");
        } else {
            self.collection_status = "Create failed".to_string();
        }
        cx.notify();
    }

    fn copy_collection_target(&mut self, target_id: String, cx: &mut Context<Self>) {
        if duplicate_collection_item(&mut self.collection.items, &target_id) {
            self.refresh_collection_status("Item copied");
            self.collection_context_menu = None;
        } else {
            self.collection_status = "Copy failed".to_string();
        }
        cx.notify();
    }

    fn delete_collection_target(&mut self, target_id: String, cx: &mut Context<Self>) {
        if remove_collection_item(&mut self.collection.items, &target_id).is_some() {
            self.expanded_collection_nodes
                .retain(|node| !node.starts_with(&target_id));
            self.refresh_collection_status("Item deleted");
            self.collection_context_menu = None;
        } else {
            self.collection_status = "Delete failed".to_string();
        }
        cx.notify();
    }

    fn move_collection_target(
        &mut self,
        source_id: String,
        target_id: String,
        cx: &mut Context<Self>,
    ) {
        if move_collection_item(&mut self.collection.items, &source_id, &target_id) {
            self.ensure_collection_node_expanded("collection".to_string());
            self.refresh_collection_status("Item moved");
            self.collection_context_menu = None;
        } else if source_id != target_id {
            self.collection_status = "Move failed".to_string();
        }
        cx.notify();
    }

    fn rename_collection_target(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.collection_context_menu.clone() else {
            return;
        };
        let name = self.collection_rename_input.read(cx).text();
        let name = name.trim();
        if name.is_empty() {
            self.collection_status = "Name needed".to_string();
            cx.notify();
            return;
        }

        if rename_collection_node(&mut self.collection, &menu.node_id, name) {
            self.refresh_collection_status("Item renamed");
            self.collection_context_menu = None;
        } else {
            self.collection_status = "Rename failed".to_string();
        }
        cx.notify();
    }

    fn ensure_collection_node_expanded(&mut self, node_id: String) {
        if !self
            .expanded_collection_nodes
            .iter()
            .any(|expanded| expanded == &node_id)
        {
            self.expanded_collection_nodes.push(node_id);
        }
    }

    fn refresh_collection_status(&mut self, prefix: &str) {
        self.collection_status = format!(
            "{prefix}: {} requests",
            collection_item_count(&self.collection.items)
        );
    }

    fn toggle_collection_node(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(index) = self
            .expanded_collection_nodes
            .iter()
            .position(|expanded| expanded == &id)
        {
            self.expanded_collection_nodes.remove(index);
        } else {
            self.expanded_collection_nodes.push(id);
        }
        cx.notify();
    }

    fn restore_collection_request(&mut self, request: CollectionRequest, cx: &mut Context<Self>) {
        self.method = request.method;
        self.url
            .update(cx, |input, cx| input.set_text(request.url, cx));
        set_key_value_rows(&self.request_headers, request.headers, cx);
        set_key_value_rows(&self.query_params, request.query_params, cx);
        self.apply_collection_body(request.body, cx);
        self.pre_request_script.update(cx, |input, cx| {
            input.set_text(request.pre_request_script, cx)
        });
        self.pre_request_status = "idle".to_string();
        self.last_pre_request_actions.clear();
        self.request_assertions = assertion_rows_from_assertions(cx, &request.tests);
        self.last_assertion_results.clear();
        self.set_response(
            "Collection request",
            "",
            ResponseTone::Neutral,
            "Restored request from collection.",
        );
        cx.notify();
    }

    fn apply_collection_body(&mut self, body: CollectionBody, cx: &mut Context<Self>) {
        match body {
            CollectionBody::None => {
                self.request_body_mode = RequestBodyMode::None;
                self.request_body
                    .update(cx, |input, cx| input.set_text("", cx));
            }
            CollectionBody::Raw { content_type, body } => {
                if let Some((query, variables)) = graphql_fields_from_body(&content_type, &body) {
                    self.request_body_mode = RequestBodyMode::GraphQL;
                    self.graphql_query
                        .update(cx, |input, cx| input.set_text(query, cx));
                    self.graphql_variables
                        .update(cx, |input, cx| input.set_text(variables, cx));
                } else {
                    self.request_body_mode = RequestBodyMode::Raw;
                    self.raw_body_format = raw_format_from_content_type(&content_type);
                    self.request_body
                        .update(cx, |input, cx| input.set_text(body, cx));
                }
            }
            CollectionBody::FormData { fields } => {
                self.request_body_mode = RequestBodyMode::FormData;
                set_key_value_rows(&self.form_data_body, fields, cx);
            }
            CollectionBody::UrlEncoded { fields } => {
                self.request_body_mode = RequestBodyMode::UrlEncoded;
                set_key_value_rows(&self.urlencoded_body, fields, cx);
            }
            CollectionBody::Binary { path, .. } => {
                self.request_body_mode = RequestBodyMode::Binary;
                self.binary_body_path
                    .update(cx, |input, cx| input.set_text(path, cx));
            }
        }
    }

    fn add_response_assertion_row(&mut self, cx: &mut Context<Self>) {
        self.request_assertions.push(blank_assertion_row(cx));
        cx.notify();
    }

    fn clear_response_assertion_results(&mut self, cx: &mut Context<Self>) {
        self.last_assertion_results.clear();
        cx.notify();
    }

    fn load_graphql_introspection_query(&mut self, cx: &mut Context<Self>) {
        self.request_body_mode = RequestBodyMode::GraphQL;
        self.graphql_query.update(cx, |input, cx| {
            input.set_text(GRAPHQL_INTROSPECTION_QUERY, cx)
        });
        self.graphql_variables
            .update(cx, |input, cx| input.set_text("{}", cx));
        cx.notify();
    }

    fn apply_graphql_query_template(
        &mut self,
        operation: String,
        variables: String,
        cx: &mut Context<Self>,
    ) {
        self.request_body_mode = RequestBodyMode::GraphQL;
        self.graphql_query
            .update(cx, |input, cx| input.set_text(operation, cx));
        self.graphql_variables
            .update(cx, |input, cx| input.set_text(variables, cx));
        cx.notify();
    }

    fn apply_route_filter(&mut self, query: &str) {
        self.visible_routes = filter_routes(&self.routes, query);
        self.selected_route = None;
    }

    fn select_route(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(route) = self.visible_routes.get(index).cloned() else {
            return;
        };

        self.selected_route = Some(index);
        self.method = route.method.clone();
        self.url.update(cx, |input, cx| {
            input.set_text(format!("http://localhost:8080{}", route.path), cx)
        });
        self.request_body.update(cx, |input, cx| {
            input.set_text(default_request_body(&route.method), cx)
        });
        self.request_body_mode = if default_request_body(&route.method).is_empty() {
            RequestBodyMode::None
        } else {
            RequestBodyMode::Raw
        };
        self.pre_request_script
            .update(cx, |input, cx| input.set_text("", cx));
        self.pre_request_status = "idle".to_string();
        self.last_pre_request_actions.clear();
        self.request_assertions = assertion_rows_from_assertions(cx, &[]);
        self.last_assertion_results.clear();
        self.set_response(
            "Route selected",
            route.summary,
            ResponseTone::Neutral,
            pretty_json(&route.mock_body),
        );
        cx.notify();
    }

    fn send_request(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        let build = match self.current_request_build(cx) {
            Ok(build) => build,
            Err(error) => {
                self.pre_request_status = pre_request_error_label(&error.to_string());
                self.last_pre_request_actions.clear();
                self.set_response(
                    "Request build failed",
                    "",
                    ResponseTone::Error,
                    error.to_string(),
                );
                cx.notify();
                return;
            }
        };
        self.pre_request_status = pre_request_status_label(build.pre_request_actions);
        self.last_pre_request_actions = build.pre_request_action_labels;
        let request = build.request;
        if request.url.is_empty() {
            self.set_response(
                "Request needs a URL",
                "",
                ResponseTone::Error,
                "Enter a request URL or select an imported route first.",
            );
            cx.notify();
            return;
        }
        let assertions = match self.current_response_assertions(cx) {
            Ok(assertions) => assertions,
            Err(error) => {
                self.set_response("Tests invalid", "", ResponseTone::Error, error.to_string());
                cx.notify();
                return;
            }
        };

        let history_request =
            history_request_from_body(&request.method, &request.url, &request.body);
        let method = request.method.clone();
        let url = request.url.clone();
        let headers = request.headers.clone();
        let query_params = request.query_params.clone();
        let body = request.body.clone();
        let runtime = self.runtime.clone();
        let (tx, rx) = oneshot::channel();

        self.busy = true;
        self.last_assertion_results.clear();
        self.set_response(
            "Sending",
            "",
            ResponseTone::Busy,
            self.response_body.clone(),
        );
        cx.notify();

        runtime.spawn(async move {
            let _ = tx.send(
                client::send_request_with_body(&method, &url, &headers, &query_params, body).await,
            );
        });

        cx.spawn(async move |app, cx| {
            if let Ok(result) = rx.await {
                app.update(cx, |app, cx| {
                    match result {
                        Ok(response) => {
                            let response_status = format!("HTTP {}", response.status);
                            let assertion_results =
                                evaluate_response_assertions(&response, &assertions);
                            let mut response_meta =
                                format_response_meta(response.elapsed_ms, response.body_bytes);
                            if let Some(test_meta) = assertion_meta(&assertion_results) {
                                response_meta = format!("{response_meta} | {test_meta}");
                            }
                            let history_response = HistoryResponse {
                                status: response_status.clone(),
                                meta: response_meta.clone(),
                                body_preview: preview_text(&response.body),
                            };
                            let headers = format_headers(&response.headers);
                            let tone = if assertion_results.iter().any(|result| !result.passed) {
                                ResponseTone::Error
                            } else {
                                response_tone(response.status)
                            };
                            app.record_history(history_request.clone(), history_response);
                            app.last_assertion_results = assertion_results;
                            app.set_http_response(
                                response_status,
                                response_meta,
                                tone,
                                response.body,
                                response.raw_body,
                                headers,
                            );
                        }
                        Err(error) => {
                            let error = error.to_string();
                            app.record_history(
                                history_request.clone(),
                                HistoryResponse {
                                    status: "Request failed".to_string(),
                                    meta: String::new(),
                                    body_preview: preview_text(&error),
                                },
                            );
                            app.last_assertion_results.clear();
                            app.set_response("Request failed", "", ResponseTone::Error, error);
                        }
                    }
                    app.busy = false;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn connect_websocket(&mut self, cx: &mut Context<Self>) {
        if self.websocket_running {
            return;
        }

        let url = self.websocket_url.read(cx).text();
        if url.trim().is_empty() {
            self.websocket_status = "URL required".to_string();
            self.set_response(
                "WebSocket needs a URL",
                "",
                ResponseTone::Error,
                "Enter a ws:// or wss:// URL before sending a WebSocket message.",
            );
            cx.notify();
            return;
        }

        let options = client::WebSocketSessionOptions {
            headers: read_key_value_rows(&self.websocket_headers, cx),
            protocols: websocket_protocol_list(&self.websocket_protocols.read(cx).text()),
        };
        let runtime = self.runtime.clone();
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        self.websocket_running = true;
        self.websocket_command_tx = Some(command_tx);
        self.websocket_status = "connecting".to_string();
        self.set_response(
            "WebSocket active",
            "",
            ResponseTone::Busy,
            self.response_body.clone(),
        );
        cx.notify();

        runtime.spawn(async move {
            client::run_websocket_session_with_options(url, options, command_rx, event_tx).await;
        });

        cx.spawn(async move |app, cx| {
            let mut event_rx = event_rx;
            while let Some(event) = event_rx.recv().await {
                app.update(cx, |app, cx| {
                    app.handle_websocket_event(event);
                    cx.notify();
                })
                .ok();
            }
            app.update(cx, |app, cx| {
                if app.websocket_running {
                    app.websocket_running = false;
                    app.websocket_command_tx = None;
                    app.websocket_status = "closed".to_string();
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn send_websocket_message(&mut self, cx: &mut Context<Self>) {
        let message = self.websocket_message.read(cx).text();
        let Some(command_tx) = self.websocket_command_tx.as_ref() else {
            self.websocket_status = "not connected".to_string();
            self.set_response(
                "WebSocket not connected",
                "",
                ResponseTone::Error,
                "Connect before sending a WebSocket message.",
            );
            cx.notify();
            return;
        };

        let command = match self.websocket_message_mode {
            WebSocketMessageMode::Text => client::WebSocketSessionCommand::SendText(message),
            WebSocketMessageMode::BinaryHex => match websocket_hex_bytes(&message) {
                Ok(bytes) => client::WebSocketSessionCommand::SendBinary(bytes),
                Err(error) => {
                    self.websocket_status = "invalid binary".to_string();
                    self.set_response("WebSocket binary invalid", "", ResponseTone::Error, error);
                    cx.notify();
                    return;
                }
            },
        };

        if command_tx.send(command).is_err() {
            self.websocket_running = false;
            self.websocket_command_tx = None;
            self.websocket_status = "closed".to_string();
        } else {
            self.websocket_status = "sending".to_string();
        }
        cx.notify();
    }

    fn close_websocket(&mut self, cx: &mut Context<Self>) {
        if let Some(command_tx) = self.websocket_command_tx.as_ref() {
            let _ = command_tx.send(client::WebSocketSessionCommand::Close);
            self.websocket_status = "closing".to_string();
        } else {
            self.websocket_running = false;
            self.websocket_status = "closed".to_string();
        }
        cx.notify();
    }

    fn handle_websocket_event(&mut self, event: client::WebSocketSessionEvent) {
        match event {
            client::WebSocketSessionEvent::Connected { url } => {
                self.websocket_running = true;
                self.websocket_status = "connected".to_string();
                self.set_response(
                    "WebSocket connected",
                    "",
                    ResponseTone::Success,
                    format!("connected: {url}"),
                );
            }
            client::WebSocketSessionEvent::Sent(message) => {
                self.push_websocket_log(WebSocketLogEntry {
                    direction: WebSocketDirection::Sent,
                    kind: websocket_message_kind_label(&message.kind).to_string(),
                    data: message.data,
                });
                self.websocket_status = "sent".to_string();
            }
            client::WebSocketSessionEvent::Received(message) => {
                let kind = websocket_message_kind_label(&message.kind).to_string();
                let data = message.data;
                self.push_websocket_log(WebSocketLogEntry {
                    direction: WebSocketDirection::Received,
                    kind: kind.clone(),
                    data: data.clone(),
                });
                self.websocket_status = format!("received {kind}");
                self.set_response("WebSocket message", kind, ResponseTone::Success, data);
            }
            client::WebSocketSessionEvent::Closed(reason) => {
                self.websocket_running = false;
                self.websocket_command_tx = None;
                self.websocket_status = format!("closed: {}", preview_text(&reason));
                self.set_response("WebSocket closed", "", ResponseTone::Neutral, reason);
            }
            client::WebSocketSessionEvent::Error(error) => {
                self.websocket_running = false;
                self.websocket_command_tx = None;
                self.websocket_status = format!("error: {}", preview_text(&error));
                self.set_response("WebSocket failed", "", ResponseTone::Error, error);
            }
        }
    }

    fn push_websocket_log(&mut self, entry: WebSocketLogEntry) {
        self.websocket_messages.push(entry);
        let overflow = self
            .websocket_messages
            .len()
            .saturating_sub(WEBSOCKET_LOG_LIMIT);
        if overflow > 0 {
            self.websocket_messages.drain(0..overflow);
        }
    }

    fn fetch_sse_events(&mut self, cx: &mut Context<Self>) {
        if self.sse_running {
            return;
        }

        let url = self.sse_url.read(cx).text();
        if url.trim().is_empty() {
            self.sse_status = "URL required".to_string();
            self.set_response(
                "SSE needs a URL",
                "",
                ResponseTone::Error,
                "Enter an http:// or https:// SSE URL before fetching events.",
            );
            cx.notify();
            return;
        }

        let runtime = self.runtime.clone();
        let (tx, rx) = oneshot::channel();
        self.sse_running = true;
        self.sse_status = "connecting".to_string();
        self.set_response(
            "SSE active",
            "",
            ResponseTone::Busy,
            self.response_body.clone(),
        );
        cx.notify();

        runtime.spawn(async move {
            let _ = tx.send(client::collect_sse_events(&url, SSE_EVENT_FETCH_LIMIT).await);
        });

        cx.spawn(async move |app, cx| {
            if let Ok(result) = rx.await {
                app.update(cx, |app, cx| {
                    match result {
                        Ok(exchange) => {
                            let entries = sse_log_entries(&exchange);
                            for entry in entries {
                                app.push_sse_log(entry);
                            }
                            let meta = format!(
                                "{} events | {}ms",
                                exchange.events.len(),
                                exchange.elapsed_ms
                            );
                            app.sse_status = meta.clone();
                            app.set_response(
                                "SSE OK",
                                meta,
                                ResponseTone::Success,
                                sse_exchange_text(&exchange),
                            );
                        }
                        Err(error) => {
                            let error = error.to_string();
                            app.sse_status = format!("error: {}", preview_text(&error));
                            app.set_response("SSE failed", "", ResponseTone::Error, error);
                        }
                    }
                    app.sse_running = false;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn subscribe_sse_events(&mut self, cx: &mut Context<Self>) {
        if self.sse_running {
            return;
        }

        let url = self.sse_url.read(cx).text();
        if url.trim().is_empty() {
            self.sse_status = "URL required".to_string();
            self.set_response(
                "SSE needs a URL",
                "",
                ResponseTone::Error,
                "Enter an http:// or https:// SSE URL before subscribing.",
            );
            cx.notify();
            return;
        }

        let runtime = self.runtime.clone();
        let last_event_id = self.sse_last_event_id.clone();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let handle = runtime.spawn(client::run_sse_subscription(url, last_event_id, event_tx));
        self.sse_subscription = Some(handle);
        self.sse_running = true;
        self.sse_status = "subscribing".to_string();
        self.set_response(
            "SSE subscribing",
            "",
            ResponseTone::Busy,
            self.response_body.clone(),
        );
        cx.notify();

        cx.spawn(async move |app, cx| {
            let mut event_rx = event_rx;
            while let Some(event) = event_rx.recv().await {
                app.update(cx, |app, cx| {
                    app.handle_sse_stream_event(event);
                    cx.notify();
                })
                .ok();
            }
            app.update(cx, |app, cx| {
                if app.sse_subscription.is_some() {
                    app.sse_subscription = None;
                    app.sse_running = false;
                    app.sse_status = "closed".to_string();
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn stop_sse_subscription(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.sse_subscription.take() {
            handle.abort();
            self.sse_running = false;
            self.sse_status = "stopped".to_string();
            self.set_response(
                "SSE stopped",
                "",
                ResponseTone::Neutral,
                "SSE subscription was stopped.",
            );
        }
        cx.notify();
    }

    fn handle_sse_stream_event(&mut self, event: client::SseStreamEvent) {
        match event {
            client::SseStreamEvent::Connected { url } => {
                self.sse_running = true;
                self.sse_status = "subscribed".to_string();
                self.set_response(
                    "SSE subscribed",
                    "",
                    ResponseTone::Success,
                    format!("subscribed: {url}"),
                );
            }
            client::SseStreamEvent::Event(event) => {
                let label = sse_event_label(&event).to_string();
                let data = event.data.clone();
                self.push_sse_log(sse_log_entry(&event));
                self.sse_status = format!("event {label}");
                self.set_response("SSE event", label, ResponseTone::Success, data);
            }
            client::SseStreamEvent::Closed(reason) => {
                self.sse_subscription = None;
                self.sse_running = false;
                self.sse_status = format!("closed: {}", preview_text(&reason));
                self.set_response("SSE closed", "", ResponseTone::Neutral, reason);
            }
            client::SseStreamEvent::Error(error) => {
                self.sse_subscription = None;
                self.sse_running = false;
                self.sse_status = format!("error: {}", preview_text(&error));
                self.set_response("SSE failed", "", ResponseTone::Error, error);
            }
        }
    }

    fn push_sse_log(&mut self, entry: SseLogEntry) {
        if let Some(id) = &entry.id {
            self.sse_last_event_id = Some(id.clone());
        }
        self.sse_events.push(entry);
        let overflow = self.sse_events.len().saturating_sub(SSE_LOG_LIMIT);
        if overflow > 0 {
            self.sse_events.drain(0..overflow);
        }
    }

    fn run_collection_runner(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.runner_running {
            return;
        }

        let total = collection_item_count(&self.collection.items);
        if total == 0 {
            self.runner_status = "No collection requests".to_string();
            self.set_response(
                "Runner needs requests",
                "",
                ResponseTone::Error,
                "Add or import collection requests before running the collection.",
            );
            cx.notify();
            return;
        }

        let collection = self.collection.clone();
        let variables = self.variable_store(cx);
        let active_environment = self.active_environment.clone();
        let options = RunnerOptions {
            delay_ms: 0,
            failure_strategy: if self.runner_stop_on_failure {
                FailureStrategy::StopOnFailure
            } else {
                FailureStrategy::Continue
            },
        };
        let runtime = self.runtime.clone();
        let (tx, rx) = oneshot::channel();

        self.busy = true;
        self.runner_running = true;
        self.runner_results.clear();
        self.runner_status = format!("Running {total} requests");
        self.set_response(
            "Runner active",
            "",
            ResponseTone::Busy,
            "Collection runner is executing requests.",
        );
        cx.notify();

        runtime.spawn(async move {
            let summary = collection_runner::run_collection(
                &collection,
                &variables,
                active_environment.as_deref(),
                options,
            )
            .await;
            let _ = tx.send(summary);
        });

        cx.spawn(async move |app, cx| {
            if let Ok(summary) = rx.await {
                app.update(cx, |app, cx| {
                    app.apply_collection_run_summary(summary, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    fn apply_collection_run_summary(
        &mut self,
        summary: CollectionRunSummary,
        cx: &mut Context<Self>,
    ) {
        let tone = if summary.failed == 0 {
            ResponseTone::Success
        } else {
            ResponseTone::Error
        };
        let status = if summary.failed == 0 {
            "Collection passed"
        } else if summary.stopped_early {
            "Collection stopped"
        } else {
            "Collection failed"
        };
        let body = runner_summary_text(&summary);

        self.runner_running = false;
        self.busy = false;
        self.runner_status = runner_status_text(&summary);
        self.runner_results = summary.results.clone();
        self.set_response(status, self.runner_status.clone(), tone, body);
        cx.notify();
    }

    fn toggle_mock_server(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        if let Some(server) = self.server.take() {
            self.busy = true;
            self.server_running = false;
            self.server_status = "Stopping mock".to_string();
            let runtime = self.runtime.clone();
            let (tx, rx) = oneshot::channel();

            runtime.spawn(async move {
                server.stop().await;
                let _ = tx.send(());
            });

            cx.spawn(async move |app, cx| {
                if rx.await.is_ok() {
                    app.update(cx, |app, cx| {
                        app.busy = false;
                        app.server_running = false;
                        app.server_status = "Mock stopped".to_string();
                        cx.notify();
                    })
                    .ok();
                }
            })
            .detach();
            cx.notify();
            return;
        }

        if self.routes.is_empty() {
            self.set_response(
                "Mock needs routes",
                "",
                ResponseTone::Error,
                "Import an OpenAPI file before starting the mock server.",
            );
            self.server_status = "Import routes first".to_string();
            cx.notify();
            return;
        }

        let routes = self.routes.clone();
        let runtime = self.runtime.clone();
        let (tx, rx) = oneshot::channel();
        let (log_tx, mut log_rx) = mpsc::unbounded_channel();
        self.busy = true;
        self.server_status = "Starting mock".to_string();
        self.mock_logs.clear();
        cx.notify();

        runtime.spawn(async move {
            let _ = tx.send(MockServer::start_with_logs(routes, 8080, log_tx).await);
        });

        cx.spawn(async move |app, cx| {
            while let Some(entry) = log_rx.recv().await {
                if app
                    .update(cx, |app, cx| {
                        app.record_mock_log(entry);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        cx.spawn(async move |app, cx| {
            if let Ok(result) = rx.await {
                app.update(cx, |app, cx| {
                    match result {
                        Ok(server) => {
                            app.server_status = server.addr().to_string();
                            app.server_running = true;
                            app.server = Some(server);
                        }
                        Err(error) => {
                            app.server_running = false;
                            app.server_status = "Mock failed".to_string();
                            app.set_response(
                                "Mock server failed",
                                "",
                                ResponseTone::Error,
                                error.to_string(),
                            );
                        }
                    }
                    app.busy = false;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn set_response(
        &mut self,
        status: impl Into<String>,
        meta: impl Into<String>,
        tone: ResponseTone,
        body: impl Into<String>,
    ) {
        self.response_status = status.into();
        self.response_meta = meta.into();
        self.response_tone = tone;
        let body = body.into();
        self.response_body = body.clone();
        self.response_raw_body = body;
        self.response_headers.clear();
        self.response_view = ResponseView::Pretty;
        self.response_pretty_collapsed = false;
        self.graphql_schema_summary.clear();
        self.graphql_schema_browser.clear();
        self.graphql_query_templates.clear();
    }

    fn set_http_response(
        &mut self,
        status: impl Into<String>,
        meta: impl Into<String>,
        tone: ResponseTone,
        pretty_body: impl Into<String>,
        raw_body: impl Into<String>,
        headers: impl Into<String>,
    ) {
        self.response_status = status.into();
        self.response_meta = meta.into();
        self.response_tone = tone;
        self.response_body = pretty_body.into();
        self.response_raw_body = raw_body.into();
        self.graphql_schema_summary =
            graphql_schema_summary(&self.response_raw_body).unwrap_or_default();
        self.graphql_schema_browser =
            graphql_schema_browser(&self.response_raw_body).unwrap_or_default();
        self.graphql_query_templates =
            graphql_query_templates(&self.response_raw_body).unwrap_or_default();
        self.response_headers = headers.into();
        self.response_pretty_collapsed = false;
    }

    fn record_mock_log(&mut self, entry: MockRequestLog) {
        const MAX_LOGS: usize = 50;

        self.mock_logs.push(entry);
        let overflow = self.mock_logs.len().saturating_sub(MAX_LOGS);
        if overflow > 0 {
            self.mock_logs.drain(0..overflow);
        }
    }

    fn record_history(&mut self, request: HistoryRequest, response: HistoryResponse) {
        const MAX_HISTORY: usize = 100;

        self.history.record(request, response);
        while self.history.entries().len() > MAX_HISTORY {
            if let Some(id) = self.history.entries().last().map(|entry| entry.id) {
                self.history.remove(id);
            } else {
                break;
            }
        }
    }

    fn add_environment(&mut self, cx: &mut Context<Self>) {
        let name = self.environment_name_input.read(cx).text();
        let name = normalized_environment_name(&name);
        if name.is_empty() {
            self.set_response(
                "Environment name needed",
                "",
                ResponseTone::Error,
                "Enter an environment name before creating it.",
            );
            cx.notify();
            return;
        }

        if self
            .environments
            .iter()
            .any(|environment| environment.name == name)
        {
            self.active_environment = Some(name.clone());
            self.environment_name_input
                .update(cx, |input, cx| input.set_text("", cx));
            self.set_response(
                "Environment selected",
                name,
                ResponseTone::Neutral,
                "Existing environment is now active.",
            );
            cx.notify();
            return;
        }

        self.environments.push(environment_config(
            cx,
            &name,
            &[("baseUrl", ""), ("token", ""), ("", "")],
        ));
        self.active_environment = Some(name.clone());
        self.environment_name_input
            .update(cx, |input, cx| input.set_text("", cx));
        self.set_response(
            "Environment created",
            name,
            ResponseTone::Success,
            "New environment is ready for variables.",
        );
        cx.notify();
    }

    fn delete_active_environment(&mut self, cx: &mut Context<Self>) {
        let Some(active_environment) = self.active_environment.clone() else {
            return;
        };

        if let Some(index) = self
            .environments
            .iter()
            .position(|environment| environment.name == active_environment)
        {
            self.environments.remove(index);
            self.active_environment = None;
            self.set_response(
                "Environment deleted",
                active_environment,
                ResponseTone::Success,
                "Environment variables were removed from the active session.",
            );
            cx.notify();
        }
    }

    fn copy_headers_bulk(&mut self, cx: &mut Context<Self>) {
        let headers = read_key_value_rows(&self.request_headers, cx);
        if headers.is_empty() {
            self.set_response(
                "No headers",
                "",
                ResponseTone::Neutral,
                "There are no request headers to copy.",
            );
            cx.notify();
            return;
        }

        cx.write_to_clipboard(ClipboardItem::new_string(format_header_bulk(&headers)));
        self.set_response(
            "Headers copied",
            headers.len().to_string(),
            ResponseTone::Success,
            "Request headers were copied as bulk text.",
        );
        cx.notify();
    }

    fn paste_headers_bulk(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.set_response(
                "Clipboard empty",
                "",
                ResponseTone::Error,
                "Copy header lines before using bulk paste.",
            );
            cx.notify();
            return;
        };
        let headers = parse_header_bulk(&text);
        if headers.is_empty() {
            self.set_response(
                "No headers parsed",
                "",
                ResponseTone::Error,
                "Use one header per line, for example: Accept: application/json.",
            );
            cx.notify();
            return;
        }

        set_key_value_pairs(&mut self.request_headers, headers.clone(), cx);
        self.set_response(
            "Headers pasted",
            headers.len().to_string(),
            ResponseTone::Success,
            "Bulk headers were applied to the request.",
        );
        cx.notify();
    }

    fn restore_history_entry(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(entry) = self.history.find(id).cloned() else {
            return;
        };

        self.method = entry.request.method;
        self.url
            .update(cx, |input, cx| input.set_text(entry.request.url, cx));
        self.request_body.update(cx, |input, cx| {
            input.set_text(entry.request.body_preview.clone(), cx)
        });
        self.request_body_mode = if entry.request.body_preview.is_empty() {
            RequestBodyMode::None
        } else {
            RequestBodyMode::Raw
        };
        self.set_response(
            entry.response.status,
            entry.response.meta,
            ResponseTone::Neutral,
            entry.response.body_preview,
        );
        cx.notify();
    }

    fn auth_pairs(&self, cx: &mut Context<Self>) -> (Vec<(String, String)>, Vec<(String, String)>) {
        match self.auth_mode {
            AuthMode::None => (Vec::new(), Vec::new()),
            AuthMode::Bearer => {
                let token = self.bearer_token.read(cx).text();
                let headers = bearer_auth_pair(&token).into_iter().collect();
                (headers, Vec::new())
            }
            AuthMode::Basic => {
                let username = self.basic_username.read(cx).text();
                let password = self.basic_password.read(cx).text();
                let headers = basic_auth_pair(&username, &password).into_iter().collect();
                (headers, Vec::new())
            }
            AuthMode::Jwt => {
                let token = self.jwt_token.read(cx).text();
                let headers = jwt_auth_pair(&token).into_iter().collect();
                (headers, Vec::new())
            }
            AuthMode::ApiKey => {
                let name = self.api_key_name.read(cx).text();
                let value = self.api_key_value.read(cx).text();
                let Some(pair) = api_key_pair(&name, &value) else {
                    return (Vec::new(), Vec::new());
                };

                match self.api_key_placement {
                    ApiKeyPlacement::Header => (vec![pair], Vec::new()),
                    ApiKeyPlacement::Query => (Vec::new(), vec![pair]),
                }
            }
        }
    }

    fn current_raw_codegen_request(&self, cx: &mut Context<Self>) -> CodegenRequest {
        let mut headers = read_key_value_rows(&self.request_headers, cx);
        let mut query_params = read_key_value_rows(&self.query_params, cx);
        let (auth_headers, auth_query_params) = self.auth_pairs(cx);
        headers.extend(auth_headers);
        query_params.extend(auth_query_params);

        CodegenRequest {
            method: self.method.clone(),
            url: self.url.read(cx).text(),
            headers,
            query_params,
            body: self.request_body_for_send(cx),
        }
    }

    fn current_request_build(&self, cx: &mut Context<Self>) -> Result<RequestBuild> {
        let variable_store = self.variable_store(cx);
        let active_environment = self.active_environment.as_deref();
        let raw_request = self.current_raw_codegen_request(cx);
        let execution = execute_pre_request_actions(
            &self.pre_request_script.read(cx).text(),
            raw_request,
            variable_store,
            active_environment,
        )?;

        let request = resolve_codegen_request_templates(
            execution.request,
            &execution.variables,
            active_environment,
        )?;

        Ok(RequestBuild {
            request,
            pre_request_actions: execution.actions_applied,
            pre_request_action_labels: pre_request_action_labels(&execution.actions),
        })
    }

    fn current_codegen_request(&self, cx: &mut Context<Self>) -> Result<CodegenRequest> {
        Ok(self.current_request_build(cx)?.request)
    }

    fn variable_store(&self, cx: &mut Context<Self>) -> VariableStore {
        let active_environment = self.active_environment.as_deref();
        let environment_variables = self
            .active_environment_variables()
            .map(|variables| read_key_value_rows(variables, cx))
            .unwrap_or_default();

        variable_store_from_pairs(
            read_key_value_rows(&self.global_variables, cx),
            active_environment,
            environment_variables,
        )
    }

    fn active_environment_variables(&self) -> Option<&[KeyValueRow]> {
        let active_environment = self.active_environment.as_deref()?;
        self.environments
            .iter()
            .find(|environment| environment.name == active_environment)
            .map(|environment| environment.variables.as_slice())
    }

    fn request_body_for_send(&self, cx: &mut Context<Self>) -> RequestBody {
        match self.request_body_mode {
            RequestBodyMode::None => RequestBody::None,
            RequestBodyMode::FormData => {
                RequestBody::Multipart(read_key_value_rows(&self.form_data_body, cx))
            }
            RequestBodyMode::UrlEncoded => {
                RequestBody::FormUrlEncoded(read_key_value_rows(&self.urlencoded_body, cx))
            }
            RequestBodyMode::Raw => RequestBody::Raw {
                content_type: Some(self.raw_body_format.content_type().to_string()),
                body: self.request_body.read(cx).text(),
            },
            RequestBodyMode::GraphQL => RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: graphql_body(
                    &self.graphql_query.read(cx).text(),
                    &self.graphql_variables.read(cx).text(),
                ),
            },
            RequestBodyMode::Binary => RequestBody::BinaryFile {
                path: self.binary_body_path.read(cx).text(),
                content_type: Some("application/octet-stream".to_string()),
            },
        }
    }

    fn current_response_assertions(
        &self,
        cx: &mut Context<Self>,
    ) -> Result<Vec<ResponseAssertion>> {
        self.request_assertions
            .iter()
            .filter_map(|row| {
                let name = row.name.read(cx).text();
                let target = row.target.read(cx).text();
                let expected = row.expected.read(cx).text();
                match response_assertion_from_fields(row.kind, &name, &target, &expected) {
                    Ok(Some(assertion)) => Some(Ok(assertion)),
                    Ok(None) => None,
                    Err(error) => Some(Err(error)),
                }
            })
            .collect()
    }

    fn method_button(&self, method: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.method == method;
        let enabled = !self.busy;
        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(32.))
            .w(px(74.))
            .rounded(px(5.))
            .border_1()
            .border_color(if active {
                method_color(method)
            } else {
                ui_border_strong()
            })
            .bg(if active {
                ui_surface_muted()
            } else {
                ui_surface()
            })
            .text_size(px(12.))
            .font_weight(FontWeight::BOLD)
            .text_color(method_color(method))
            .opacity(if enabled { 1.0 } else { 0.55 })
            .cursor_pointer()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    if !app.busy {
                        app.method = method.to_string();
                        cx.notify();
                    }
                }),
            )
            .child(method)
    }

    fn action_button(
        &self,
        label: impl Into<SharedString>,
        enabled: bool,
        tone: ButtonTone,
        on_click: impl Fn(&mut Self, &MouseUpEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let enabled = enabled && !self.busy;
        let colors = tone.colors(enabled);

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(34.))
            .w(px(112.))
            .rounded(px(6.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_size(px(13.))
            .font_weight(FontWeight::BOLD)
            .text_color(colors.text)
            .opacity(if enabled { 1.0 } else { 0.62 })
            .when(enabled, |button| button.cursor_pointer())
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, event, window, cx| {
                    if enabled {
                        on_click(app, event, window, cx);
                    }
                }),
            )
            .child(label.into())
    }

    fn render_top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let can_toggle_mock = self.server_running || !self.routes.is_empty();
        div()
            .flex()
            .items_center()
            .h(px(48.))
            .w_full()
            .border_b_1()
            .border_color(ui_border())
            .bg(ui_surface_muted())
            .px_3()
            .gap_3()
            .child(
                div()
                    .w(px(230.))
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(15.))
                    .text_color(ui_text_primary())
                    .child("ZenAPI"),
            )
            .child(div().flex_1().child(self.import_path.clone()))
            .child(self.action_button(
                "Import",
                true,
                ButtonTone::Neutral,
                |app, _event, _window, cx| app.import_openapi(cx),
                cx,
            ))
            .child(
                div()
                    .w(px(124.))
                    .truncate()
                    .text_size(px(12.))
                    .text_color(ui_text_secondary())
                    .child(self.spec_label.clone()),
            )
            .child(
                div()
                    .w(px(132.))
                    .truncate()
                    .text_size(px(12.))
                    .text_color(if self.server_running {
                        ResponseTone::Success.color()
                    } else {
                        ui_text_secondary()
                    })
                    .child(self.server_status.clone()),
            )
            .child(self.action_button(
                if self.server_running {
                    "Stop Mock"
                } else {
                    "Start Mock"
                },
                can_toggle_mock,
                if self.server_running {
                    ButtonTone::Warning
                } else {
                    ButtonTone::Primary
                },
                |app, _event, _window, cx| app.toggle_mock_server(cx),
                cx,
            ))
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self
            .visible_routes
            .iter()
            .enumerate()
            .map(|(index, route)| {
                self.render_route_row(
                    index,
                    route.method.clone(),
                    route.path.clone(),
                    route.summary.clone(),
                    cx,
                )
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .w(px(320.))
            .h_full()
            .border_r_1()
            .border_color(rgb(0xe5e7eb))
            .bg(rgb(0xf9fafb))
            .p_3()
            .gap_3()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .h(px(24.))
                    .text_size(px(13.))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0x111827))
                            .child("Endpoints"),
                    )
                    .child(
                        div()
                            .text_color(rgb(0x6b7280))
                            .child(if self.routes.is_empty() {
                                String::new()
                            } else {
                                format!("{}/{}", self.visible_routes.len(), self.routes.len())
                            }),
                    ),
            )
            .child(self.route_filter.clone())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap_1()
                    .children(rows)
                    .when(self.visible_routes.is_empty(), |list| {
                        list.child(
                            div()
                                .h(px(34.))
                                .flex()
                                .items_center()
                                .text_color(rgb(0x9ca3af))
                                .text_size(px(13.))
                                .child(if self.routes.is_empty() {
                                    "No imported routes"
                                } else {
                                    "No matching routes"
                                }),
                        )
                    }),
            )
            .child(self.render_collection_section(cx))
            .child(self.render_history_section(cx))
    }

    fn render_collection_section(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows = vec![collection_root_row(
            self.collection.name.clone(),
            collection_item_count(&self.collection.items),
            self.expanded_collection_nodes
                .iter()
                .any(|node| node == "collection"),
            cx,
        )];

        if self
            .expanded_collection_nodes
            .iter()
            .any(|node| node == "collection")
        {
            append_collection_rows(
                &mut rows,
                &self.collection.items,
                "collection",
                1,
                &self.expanded_collection_nodes,
                cx,
            );
        }

        div()
            .flex()
            .flex_col()
            .gap_2()
            .pt_2()
            .border_t_1()
            .border_color(rgb(0xe5e7eb))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .h(px(24.))
                    .text_size(px(13.))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0x111827))
                            .child("Collections"),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_color(rgb(0x6b7280))
                            .child(self.collection_status.clone()),
                    ),
            )
            .child(self.collection_path.clone())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.sidebar_action_button(
                        "Import",
                        58.,
                        true,
                        ButtonTone::Neutral,
                        |app, _event, _window, cx| app.import_collection(cx),
                        cx,
                    ))
                    .child(self.sidebar_action_button(
                        "Save",
                        52.,
                        true,
                        ButtonTone::Primary,
                        |app, _event, _window, cx| app.save_current_request_to_collection(cx),
                        cx,
                    ))
                    .child(self.sidebar_action_button(
                        "Export",
                        58.,
                        true,
                        ButtonTone::Neutral,
                        |app, _event, _window, cx| app.export_collection(false, cx),
                        cx,
                    ))
                    .child(self.sidebar_action_button(
                        "Postman",
                        70.,
                        true,
                        ButtonTone::Neutral,
                        |app, _event, _window, cx| app.export_collection(true, cx),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(rgb(0xe5e7eb))
                    .bg(rgb(0xffffff))
                    .p_1()
                    .children(rows)
                    .when(self.collection.items.is_empty(), |list| {
                        list.child(
                            div()
                                .h(px(30.))
                                .flex()
                                .items_center()
                                .px_2()
                                .text_color(rgb(0x9ca3af))
                                .text_size(px(12.))
                                .child("No collection requests"),
                        )
                    }),
            )
            .when(self.collection_context_menu.is_some(), |section| {
                let menu = self
                    .collection_context_menu
                    .clone()
                    .expect("checked collection context menu");
                section.child(self.render_collection_context_menu(menu, cx))
            })
    }

    fn render_collection_context_menu(
        &self,
        menu: CollectionContextMenu,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let can_mutate_item = menu.kind != CollectionNodeKind::Root;
        let new_request_target = menu.node_id.clone();
        let new_folder_target = menu.node_id.clone();
        let copy_target = menu.node_id.clone();
        let delete_target = menu.node_id.clone();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .rounded(px(5.))
            .border_1()
            .border_color(rgb(0xd1d5db))
            .bg(rgb(0xffffff))
            .p_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0x374151))
                            .child(menu.label),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .h(px(22.))
                            .w(px(44.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(rgb(0xd1d5db))
                            .bg(rgb(0xf9fafb))
                            .text_size(px(11.))
                            .text_color(rgb(0x6b7280))
                            .cursor_pointer()
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|app, _event: &MouseUpEvent, _window, cx| {
                                    app.close_collection_menu(cx);
                                }),
                            )
                            .child("Close"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.sidebar_action_button(
                        "New Req",
                        72.,
                        true,
                        ButtonTone::Neutral,
                        move |app, _event, _window, cx| {
                            app.add_collection_request(new_request_target.clone(), cx);
                        },
                        cx,
                    ))
                    .child(self.sidebar_action_button(
                        "New Dir",
                        72.,
                        true,
                        ButtonTone::Neutral,
                        move |app, _event, _window, cx| {
                            app.add_collection_folder(new_folder_target.clone(), cx);
                        },
                        cx,
                    ))
                    .child(self.sidebar_action_button(
                        "Copy",
                        52.,
                        can_mutate_item,
                        ButtonTone::Neutral,
                        move |app, _event, _window, cx| {
                            app.copy_collection_target(copy_target.clone(), cx);
                        },
                        cx,
                    ))
                    .child(self.sidebar_action_button(
                        "Delete",
                        58.,
                        can_mutate_item,
                        ButtonTone::Warning,
                        move |app, _event, _window, cx| {
                            app.delete_collection_target(delete_target.clone(), cx);
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(self.collection_rename_input.clone()))
                    .child(self.sidebar_action_button(
                        "Rename",
                        70.,
                        true,
                        ButtonTone::Primary,
                        |app, _event, _window, cx| app.rename_collection_target(cx),
                        cx,
                    )),
            )
    }

    fn sidebar_action_button(
        &self,
        label: &'static str,
        width: f32,
        enabled: bool,
        tone: ButtonTone,
        on_click: impl Fn(&mut Self, &MouseUpEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let enabled = enabled && !self.busy;
        let colors = tone.colors(enabled);

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(26.))
            .w(px(width))
            .rounded(px(5.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_size(px(11.))
            .font_weight(FontWeight::BOLD)
            .text_color(colors.text)
            .opacity(if enabled { 1.0 } else { 0.62 })
            .when(enabled, |button| button.cursor_pointer())
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, event, window, cx| {
                    if enabled {
                        on_click(app, event, window, cx);
                    }
                }),
            )
            .child(label)
    }

    fn render_history_section(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered_entries = self.history.filtered(&self.history_query);
        let has_history = !self.history.entries().is_empty();
        let has_matches = !filtered_entries.is_empty();
        let rows = filtered_entries
            .into_iter()
            .take(8)
            .map(|entry| {
                history_row(
                    entry.id,
                    entry.request.method.clone(),
                    entry.request.url.clone(),
                    entry.response.status.clone(),
                    cx,
                )
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .pt_2()
            .border_t_1()
            .border_color(rgb(0xe5e7eb))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .h(px(24.))
                    .text_size(px(13.))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0x111827))
                            .child("History"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(rgb(0x6b7280))
                                    .child(self.history.entries().len().to_string()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(px(24.))
                                    .w(px(58.))
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(rgb(0xd1d5db))
                                    .bg(rgb(0xffffff))
                                    .text_size(px(11.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x6b7280))
                                    .cursor_pointer()
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(|app, _event: &MouseUpEvent, _window, cx| {
                                            app.history.clear();
                                            cx.notify();
                                        }),
                                    )
                                    .child("Clear"),
                            ),
                    ),
            )
            .child(self.history_filter.clone())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(rows)
                    .when(!has_history, |list| {
                        list.child(
                            div()
                                .h(px(34.))
                                .flex()
                                .items_center()
                                .text_color(rgb(0x9ca3af))
                                .text_size(px(13.))
                                .child("No request history"),
                        )
                    })
                    .when(has_history && !has_matches, |list| {
                        list.child(
                            div()
                                .h(px(34.))
                                .flex()
                                .items_center()
                                .text_color(rgb(0x9ca3af))
                                .text_size(px(13.))
                                .child("No matching history"),
                        )
                    }),
            )
    }

    fn render_route_row(
        &self,
        index: usize,
        method: String,
        path: String,
        summary: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + 'static + use<> {
        let selected = self.selected_route == Some(index);
        div()
            .id(("route", index))
            .flex()
            .flex_col()
            .h(px(48.))
            .rounded(px(4.))
            .border_l(px(3.))
            .border_color(if selected {
                ui_accent()
            } else {
                ui_surface_muted()
            })
            .bg(if selected {
                ui_accent_surface()
            } else {
                ui_surface_muted()
            })
            .px_2()
            .py_1()
            .cursor_pointer()
            .hover(|row| if selected { row } else { row.bg(ui_hover()) })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.select_route(index, cx);
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(HTTP_METHOD_LABEL_WIDTH))
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(method_color(&method))
                            .child(method),
                    )
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(13.))
                            .text_color(ui_text_primary())
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .child(path),
                    ),
            )
            .child(
                div()
                    .ml(px(66.))
                    .truncate()
                    .text_size(px(12.))
                    .text_color(ui_text_secondary())
                    .child(summary),
            )
    }

    fn render_workspace(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .bg(ui_surface())
            .child(self.render_request_bar(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .child(self.render_request_panel(cx))
                    .child(self.render_response_panel(cx)),
            )
    }

    fn render_request_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .h(px(54.))
            .border_b_1()
            .border_color(ui_border())
            .px_3()
            .gap_2()
            .child(self.method_button("GET", cx))
            .child(self.method_button("POST", cx))
            .child(self.method_button("PUT", cx))
            .child(self.method_button("PATCH", cx))
            .child(self.method_button("DELETE", cx))
            .child(self.method_button("OPTIONS", cx))
            .child(self.method_button("HEAD", cx))
            .child(div().flex_1().child(self.url.clone()))
            .child(self.action_button(
                "Send",
                true,
                ButtonTone::Primary,
                |app, _event, _window, cx| app.send_request(cx),
                cx,
            ))
    }

    fn render_request_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .border_r_1()
            .border_color(ui_border())
            .child(panel_header("Request", None, ResponseTone::Neutral))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .p_3()
                    .gap_3()
                    .child(self.render_variables_panel(cx))
                    .child(key_value_editor("Query Params", &self.query_params))
                    .child(self.render_headers_editor(cx))
                    .child(self.render_auth_panel(cx))
                    .child(self.render_pre_request_panel())
                    .child(self.render_body_panel(cx))
                    .child(self.render_websocket_panel(cx))
                    .child(self.render_sse_panel(cx))
                    .child(self.render_tests_panel(cx))
                    .child(self.render_codegen_panel(cx))
                    .child(self.render_collection_runner(cx))
                    .child(self.render_mock_log()),
            )
    }

    fn render_websocket_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self
            .websocket_messages
            .iter()
            .rev()
            .take(8)
            .cloned()
            .map(websocket_log_row)
            .collect::<Vec<_>>();
        let input_hint = match self.websocket_message_mode {
            WebSocketMessageMode::Text => "message text",
            WebSocketMessageMode::BinaryHex => "hex bytes, e.g. 00 ff 7a",
        };

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_secondary())
                            .child("WebSocket"),
                    )
                    .child(
                        div()
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .text_size(px(12.))
                            .text_color(if self.websocket_running {
                                ResponseTone::Busy.color()
                            } else {
                                ui_text_secondary()
                            })
                            .child(self.websocket_status.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(self.websocket_url.clone()))
                    .child(self.action_button(
                        "Connect",
                        !self.websocket_running,
                        ButtonTone::Primary,
                        |app, _event, _window, cx| app.connect_websocket(cx),
                        cx,
                    ))
                    .child(self.action_button(
                        "Send",
                        self.websocket_running,
                        ButtonTone::Neutral,
                        |app, _event, _window, cx| app.send_websocket_message(cx),
                        cx,
                    ))
                    .child(self.action_button(
                        "Close",
                        self.websocket_running,
                        ButtonTone::Warning,
                        |app, _event, _window, cx| app.close_websocket(cx),
                        cx,
                    )),
            )
            .child(self.websocket_protocols.clone())
            .child(key_value_editor(
                "WebSocket Headers",
                &self.websocket_headers,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.websocket_message_mode_button(
                        "Text",
                        WebSocketMessageMode::Text,
                        cx,
                    ))
                    .child(self.websocket_message_mode_button(
                        "Binary Hex",
                        WebSocketMessageMode::BinaryHex,
                        cx,
                    ))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(ui_text_secondary())
                            .child(input_hint),
                    ),
            )
            .child(self.websocket_message.clone())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .children(rows)
                    .when(self.websocket_messages.is_empty(), |list| {
                        list.child(
                            div()
                                .h(px(34.))
                                .flex()
                                .items_center()
                                .px_2()
                                .text_color(ui_text_muted())
                                .text_size(px(13.))
                                .child("No WebSocket messages"),
                        )
                    }),
            )
    }

    fn render_sse_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self
            .sse_events
            .iter()
            .rev()
            .take(8)
            .cloned()
            .map(sse_log_row)
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_secondary())
                            .child("SSE"),
                    )
                    .child(
                        div()
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .text_size(px(12.))
                            .text_color(if self.sse_running {
                                ResponseTone::Busy.color()
                            } else {
                                ui_text_secondary()
                            })
                            .child(self.sse_status.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(self.sse_url.clone()))
                    .child(self.action_button(
                        "Fetch Events",
                        !self.sse_running,
                        ButtonTone::Primary,
                        |app, _event, _window, cx| app.fetch_sse_events(cx),
                        cx,
                    ))
                    .child(self.action_button(
                        "Subscribe",
                        !self.sse_running,
                        ButtonTone::Neutral,
                        |app, _event, _window, cx| app.subscribe_sse_events(cx),
                        cx,
                    ))
                    .child(self.action_button(
                        "Stop",
                        self.sse_subscription.is_some(),
                        ButtonTone::Warning,
                        |app, _event, _window, cx| app.stop_sse_subscription(cx),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .children(rows)
                    .when(self.sse_events.is_empty(), |list| {
                        list.child(
                            div()
                                .h(px(34.))
                                .flex()
                                .items_center()
                                .px_2()
                                .text_color(ui_text_muted())
                                .text_size(px(13.))
                                .child("No SSE events"),
                        )
                    }),
            )
    }

    fn render_tests_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let result_rows = self
            .last_assertion_results
            .iter()
            .cloned()
            .map(assertion_result_row)
            .collect::<Vec<_>>();
        let editor_rows = self
            .request_assertions
            .iter()
            .enumerate()
            .map(|(index, row)| assertion_editor_row(index, row, cx))
            .collect::<Vec<_>>();
        let meta = assertion_meta(&self.last_assertion_results).unwrap_or_else(|| {
            format!(
                "{} configured",
                configured_assertion_count(&self.request_assertions, cx)
            )
        });

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_secondary())
                            .child("Tests"),
                    )
                    .child(
                        div()
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .text_size(px(12.))
                            .text_color(ui_text_secondary())
                            .child(meta),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.action_button(
                        "Add Test",
                        true,
                        ButtonTone::Neutral,
                        |app, _event, _window, cx| app.add_response_assertion_row(cx),
                        cx,
                    ))
                    .child(self.action_button(
                        "Clear Results",
                        !self.last_assertion_results.is_empty(),
                        ButtonTone::Neutral,
                        |app, _event, _window, cx| app.clear_response_assertion_results(cx),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .text_size(px(11.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_muted())
                    .child(div().w(px(TEST_ASSERTION_NAME_COLUMN_WIDTH)).child("Name"))
                    .child(div().w(px(TEST_ASSERTION_KIND_COLUMN_WIDTH)).child("Kind"))
                    .child(div().flex_1().child("Target"))
                    .child(div().flex_1().child("Expected")),
            )
            .child(div().flex().flex_col().gap_1().children(editor_rows))
            .when(!self.last_assertion_results.is_empty(), |panel| {
                panel.child(
                    div()
                        .flex()
                        .flex_col()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(ui_border())
                        .bg(ui_surface())
                        .children(result_rows),
                )
            })
    }

    fn render_collection_runner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let total = collection_item_count(&self.collection.items);
        let rows = self
            .runner_results
            .iter()
            .rev()
            .take(6)
            .cloned()
            .map(runner_result_row)
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_secondary())
                            .child("Runner"),
                    )
                    .child(
                        div()
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .text_size(px(12.))
                            .text_color(if self.runner_running {
                                ResponseTone::Busy.color()
                            } else {
                                ui_text_secondary()
                            })
                            .child(self.runner_status.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        compact_toggle("Stop on fail", self.runner_stop_on_failure)
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|app, _event: &MouseUpEvent, _window, cx| {
                                    if !app.runner_running && !app.busy {
                                        app.runner_stop_on_failure = !app.runner_stop_on_failure;
                                        cx.notify();
                                    }
                                }),
                            )
                            .child("Stop on fail"),
                    )
                    .child(self.action_button(
                        "Run All",
                        total > 0 && !self.runner_running,
                        ButtonTone::Primary,
                        |app, _event, _window, cx| app.run_collection_runner(cx),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .children(rows)
                    .when(self.runner_results.is_empty(), |list| {
                        list.child(
                            div()
                                .h(px(34.))
                                .flex()
                                .items_center()
                                .px_2()
                                .text_color(ui_text_muted())
                                .text_size(px(13.))
                                .child(if total == 0 {
                                    "No collection requests"
                                } else {
                                    "No runner results"
                                }),
                        )
                    }),
            )
    }

    fn render_mock_log(&self) -> impl IntoElement {
        let rows = self
            .mock_logs
            .iter()
            .rev()
            .take(8)
            .map(|entry| mock_log_row(entry.method.clone(), entry.path.clone(), entry.status))
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .gap_2()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_secondary())
                    .child("Mock Log"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .children(rows)
                    .when(self.mock_logs.is_empty(), |list| {
                        list.child(
                            div()
                                .h(px(34.))
                                .flex()
                                .items_center()
                                .px_2()
                                .text_color(ui_text_muted())
                                .text_size(px(13.))
                                .child("No mock requests"),
                        )
                    }),
            )
    }

    fn render_headers_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(key_value_editor("Headers", &self.request_headers))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.action_button(
                        "Copy Bulk",
                        true,
                        ButtonTone::Neutral,
                        |app, _event, _window, cx| app.copy_headers_bulk(cx),
                        cx,
                    ))
                    .child(self.action_button(
                        "Paste Bulk",
                        true,
                        ButtonTone::Primary,
                        |app, _event, _window, cx| app.paste_headers_bulk(cx),
                        cx,
                    ))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(ui_text_secondary())
                            .child("Key: Value per line"),
                    ),
            )
    }

    fn render_variables_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let environment_buttons = self
            .environments
            .iter()
            .map(|environment| self.environment_button(environment.name.clone(), cx))
            .collect::<Vec<_>>();
        let active_environment = self.active_environment.as_deref().unwrap_or("none");

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_secondary())
                    .child("Variables"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.environment_button("No Env".to_string(), cx))
                    .children(environment_buttons)
                    .child(
                        div()
                            .ml_2()
                            .truncate()
                            .text_size(px(12.))
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .text_color(ui_text_secondary())
                            .child(format!("active: {active_environment}")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(self.environment_name_input.clone()))
                    .child(self.sidebar_action_button(
                        "Add Env",
                        72.,
                        true,
                        ButtonTone::Neutral,
                        |app, _event, _window, cx| app.add_environment(cx),
                        cx,
                    ))
                    .child(self.sidebar_action_button(
                        "Delete",
                        62.,
                        self.active_environment.is_some(),
                        ButtonTone::Warning,
                        |app, _event, _window, cx| app.delete_active_environment(cx),
                        cx,
                    )),
            )
            .child(key_value_editor("Global Variables", &self.global_variables))
            .when_some(self.active_environment_variables(), |panel, variables| {
                panel.child(key_value_editor("Environment Variables", variables))
            })
    }

    fn environment_button(&self, label: String, cx: &mut Context<Self>) -> gpui::Div {
        let environment = if label == "No Env" {
            None
        } else {
            Some(label.clone())
        };
        let active = self.active_environment == environment;
        compact_toggle(&label, active)
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.active_environment = environment.clone();
                    cx.notify();
                }),
            )
            .child(label)
    }

    fn render_auth_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_secondary())
                    .child("Authorization"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.auth_mode_button("None", AuthMode::None, cx))
                    .child(self.auth_mode_button("Bearer", AuthMode::Bearer, cx))
                    .child(self.auth_mode_button("Basic", AuthMode::Basic, cx))
                    .child(self.auth_mode_button("JWT", AuthMode::Jwt, cx))
                    .child(self.auth_mode_button("API Key", AuthMode::ApiKey, cx)),
            )
            .when(self.auth_mode == AuthMode::Bearer, |panel| {
                panel.child(self.bearer_token.clone())
            })
            .when(self.auth_mode == AuthMode::Basic, |panel| {
                panel.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(KEY_VALUE_KEY_COLUMN_WIDTH))
                                .child(self.basic_username.clone()),
                        )
                        .child(div().flex_1().child(self.basic_password.clone())),
                )
            })
            .when(self.auth_mode == AuthMode::Jwt, |panel| {
                panel.child(self.jwt_token.clone())
            })
            .when(self.auth_mode == AuthMode::ApiKey, |panel| {
                panel
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(KEY_VALUE_KEY_COLUMN_WIDTH))
                                    .child(self.api_key_name.clone()),
                            )
                            .child(div().flex_1().child(self.api_key_value.clone())),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(self.api_key_placement_button(
                                "Header",
                                ApiKeyPlacement::Header,
                                cx,
                            ))
                            .child(self.api_key_placement_button(
                                "Query",
                                ApiKeyPlacement::Query,
                                cx,
                            )),
                    )
            })
    }

    fn render_pre_request_panel(&self) -> impl IntoElement {
        let action_rows = self
            .last_pre_request_actions
            .iter()
            .cloned()
            .map(pre_request_action_row)
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_secondary())
                            .child("Pre-request"),
                    )
                    .child(
                        div()
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .text_size(px(12.))
                            .text_color(if self.pre_request_status.starts_with("error") {
                                ResponseTone::Error.color()
                            } else {
                                ui_text_muted()
                            })
                            .child(self.pre_request_status.clone()),
                    ),
            )
            .child(self.pre_request_script.clone())
            .when(!self.last_pre_request_actions.is_empty(), |panel| {
                panel.child(
                    div()
                        .flex()
                        .flex_col()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(ui_border())
                        .bg(ui_surface())
                        .children(action_rows),
                )
            })
    }

    fn render_body_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_secondary())
                    .child("Body"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.body_mode_button("None", RequestBodyMode::None, cx))
                    .child(self.body_mode_button("form-data", RequestBodyMode::FormData, cx))
                    .child(self.body_mode_button(
                        "x-www-form-urlencoded",
                        RequestBodyMode::UrlEncoded,
                        cx,
                    ))
                    .child(self.body_mode_button("raw", RequestBodyMode::Raw, cx))
                    .child(self.body_mode_button("GraphQL", RequestBodyMode::GraphQL, cx))
                    .child(self.body_mode_button("binary", RequestBodyMode::Binary, cx)),
            )
            .when(
                self.request_body_mode == RequestBodyMode::FormData,
                |panel| panel.child(key_value_editor("Form Data", &self.form_data_body)),
            )
            .when(
                self.request_body_mode == RequestBodyMode::UrlEncoded,
                |panel| {
                    panel.child(key_value_editor(
                        "x-www-form-urlencoded",
                        &self.urlencoded_body,
                    ))
                },
            )
            .when(self.request_body_mode == RequestBodyMode::Raw, |panel| {
                panel
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(self.raw_format_button("JSON", RawBodyFormat::Json, cx))
                            .child(self.raw_format_button("XML", RawBodyFormat::Xml, cx))
                            .child(self.raw_format_button("Text", RawBodyFormat::Text, cx))
                            .child(self.raw_format_button("HTML", RawBodyFormat::Html, cx)),
                    )
                    .child(self.request_body.clone())
                    .child(self.render_raw_body_preview(cx))
            })
            .when(
                self.request_body_mode == RequestBodyMode::GraphQL,
                |panel| {
                    panel
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(ui_text_secondary())
                                        .child("GraphQL"),
                                )
                                .child(self.action_button(
                                    "Introspect",
                                    true,
                                    ButtonTone::Neutral,
                                    |app, _event, _window, cx| {
                                        app.load_graphql_introspection_query(cx)
                                    },
                                    cx,
                                )),
                        )
                        .child(self.graphql_query.clone())
                        .child(self.graphql_variables.clone())
                        .child(self.render_graphql_preview(cx))
                        .when(!self.graphql_schema_summary.is_empty(), |panel| {
                            panel.child(self.render_graphql_schema_summary())
                        })
                        .when(!self.graphql_schema_browser.is_empty(), |panel| {
                            panel.child(self.render_graphql_schema_browser())
                        })
                        .when(!self.graphql_query_templates.is_empty(), |panel| {
                            panel.child(self.render_graphql_query_assistant(cx))
                        })
                },
            )
            .when(self.request_body_mode == RequestBodyMode::Binary, |panel| {
                panel.child(self.binary_body_path.clone())
            })
    }

    fn render_graphql_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let preview = preview_text(&graphql_body(
            &self.graphql_query.read(cx).text(),
            &self.graphql_variables.read(cx).text(),
        ));
        let highlights = syntax_highlights_for_gpui(&preview, RawBodyFormat::Json);

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_secondary())
                    .child("GraphQL JSON"),
            )
            .child(
                div()
                    .min_h(px(72.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface_muted())
                    .p_2()
                    .font_family(PLATFORM_MONOSPACE_FONT)
                    .line_height(px(18.))
                    .text_size(px(12.))
                    .text_color(ui_text_body())
                    .whitespace_normal()
                    .child(StyledText::new(preview).with_highlights(highlights)),
            )
    }

    fn render_graphql_schema_summary(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_secondary())
                    .child("GraphQL Schema"),
            )
            .child(
                div()
                    .min_h(px(72.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .p_2()
                    .font_family(PLATFORM_MONOSPACE_FONT)
                    .line_height(px(18.))
                    .text_size(px(12.))
                    .text_color(ui_text_body())
                    .whitespace_normal()
                    .child(self.graphql_schema_summary.clone()),
            )
    }

    fn render_graphql_schema_browser(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_secondary())
                    .child("Schema Browser"),
            )
            .child(
                div()
                    .min_h(px(112.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface_muted())
                    .p_2()
                    .font_family(PLATFORM_MONOSPACE_FONT)
                    .line_height(px(18.))
                    .text_size(px(12.))
                    .text_color(ui_text_body())
                    .whitespace_normal()
                    .child(self.graphql_schema_browser.clone()),
            )
    }

    fn render_graphql_query_assistant(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows = Vec::new();
        for template in self
            .graphql_query_templates
            .iter()
            .take(GRAPHQL_QUERY_TEMPLATE_LIMIT)
        {
            let operation = template.operation.clone();
            let variables = template.variables.clone();
            let operation_preview = preview_text(&template.operation);
            let variables_preview = preview_text(&template.variables);
            let has_variables = template.variables.trim() != "{}";

            rows.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .p_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .font_family(PLATFORM_MONOSPACE_FONT)
                                    .text_size(px(12.))
                                    .text_color(ui_text_primary())
                                    .child(template.field_name.clone()),
                            )
                            .child(self.action_button(
                                "Use",
                                true,
                                ButtonTone::Neutral,
                                move |app, _event, _window, cx| {
                                    app.apply_graphql_query_template(
                                        operation.clone(),
                                        variables.clone(),
                                        cx,
                                    )
                                },
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .line_height(px(18.))
                            .text_size(px(12.))
                            .text_color(ui_text_body())
                            .whitespace_normal()
                            .child(operation_preview),
                    )
                    .when(has_variables, |row| {
                        row.child(
                            div()
                                .font_family(PLATFORM_MONOSPACE_FONT)
                                .line_height(px(18.))
                                .text_size(px(12.))
                                .text_color(ui_text_secondary())
                                .whitespace_normal()
                                .child(format!("variables {variables_preview}")),
                        )
                    }),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_secondary())
                    .child("Query Assistant"),
            )
            .children(rows)
    }

    fn render_raw_body_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.request_body.read(cx).text();
        let preview = preview_text(&body);
        let highlights = syntax_highlights_for_gpui(&preview, self.raw_body_format);

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_secondary())
                    .child("Syntax Preview"),
            )
            .child(
                div()
                    .min_h(px(72.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface_muted())
                    .p_2()
                    .font_family(PLATFORM_MONOSPACE_FONT)
                    .line_height(px(18.))
                    .text_size(px(12.))
                    .text_color(ui_text_body())
                    .whitespace_normal()
                    .child(
                        StyledText::new(if preview.is_empty() {
                            "No raw body".to_string()
                        } else {
                            preview
                        })
                        .with_highlights(highlights),
                    ),
            )
    }

    fn render_codegen_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let snippet = self.codegen_snippet(cx);
        let snippet_for_copy = snippet.clone();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_secondary())
                            .child("Code"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(self.codegen_language_selector(cx))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(px(26.))
                                    .w(px(72.))
                                    .rounded(px(5.))
                                    .border_1()
                                    .border_color(ui_border_strong())
                                    .bg(ui_surface())
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(ui_text_body())
                                    .cursor_pointer()
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(
                                            move |app, _event: &MouseUpEvent, _window, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    snippet_for_copy.clone(),
                                                ));
                                                app.codegen_menu_open = false;
                                                cx.notify();
                                            },
                                        ),
                                    )
                                    .child("Copy"),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(180.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .p_3()
                    .font_family(PLATFORM_MONOSPACE_FONT)
                    .line_height(px(18.))
                    .text_size(px(12.))
                    .text_color(ui_text_primary())
                    .whitespace_normal()
                    .child(snippet),
            )
    }

    fn codegen_language_selector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                compact_toggle(snippet_language_label(self.codegen_language), true)
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|app, _event: &MouseUpEvent, _window, cx| {
                            app.codegen_menu_open = !app.codegen_menu_open;
                            cx.notify();
                        }),
                    )
                    .child(snippet_language_label(self.codegen_language)),
            )
            .when(self.codegen_menu_open, |menu| {
                menu.child(
                    div()
                        .flex()
                        .flex_col()
                        .rounded(px(5.))
                        .border_1()
                        .border_color(ui_border_strong())
                        .bg(ui_surface())
                        .children(vec![
                            self.codegen_language_menu_item(SnippetLanguage::Curl, cx),
                            self.codegen_language_menu_item(SnippetLanguage::PythonRequests, cx),
                            self.codegen_language_menu_item(SnippetLanguage::JavaScriptFetch, cx),
                            self.codegen_language_menu_item(SnippetLanguage::RustReqwest, cx),
                            self.codegen_language_menu_item(SnippetLanguage::GoNetHttp, cx),
                        ]),
                )
            })
    }

    fn codegen_language_menu_item(
        &self,
        language: SnippetLanguage,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + 'static + use<> {
        let active = self.codegen_language == language;
        div()
            .flex()
            .items_center()
            .h(px(26.))
            .w(px(156.))
            .px_2()
            .text_size(px(12.))
            .font_weight(if active {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(if active { ui_accent() } else { ui_text_body() })
            .hover(|row| row.bg(ui_hover()))
            .cursor_pointer()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.codegen_language = language;
                    app.codegen_menu_open = false;
                    cx.notify();
                }),
            )
            .child(snippet_language_label(language))
    }

    fn codegen_snippet(&self, cx: &mut Context<Self>) -> String {
        match self.current_codegen_request(cx) {
            Ok(request) if request.url.is_empty() => "Enter a request URL".to_string(),
            Ok(request) => generate_snippet(&request, self.codegen_language),
            Err(error) => format!("Request build failed: {error}"),
        }
    }

    fn body_mode_button(
        &self,
        label: &'static str,
        mode: RequestBodyMode,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.request_body_mode == mode;
        compact_toggle(label, active)
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.request_body_mode = mode;
                    cx.notify();
                }),
            )
            .child(label)
    }

    fn raw_format_button(
        &self,
        label: &'static str,
        format: RawBodyFormat,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.raw_body_format == format;
        compact_toggle(label, active)
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.raw_body_format = format;
                    cx.notify();
                }),
            )
            .child(label)
    }

    fn websocket_message_mode_button(
        &self,
        label: &'static str,
        mode: WebSocketMessageMode,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.websocket_message_mode == mode;
        compact_toggle(label, active)
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.websocket_message_mode = mode;
                    cx.notify();
                }),
            )
            .child(label)
    }

    fn auth_mode_button(
        &self,
        label: &'static str,
        mode: AuthMode,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.auth_mode == mode;
        compact_toggle(label, active)
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.auth_mode = mode;
                    cx.notify();
                }),
            )
            .child(label)
    }

    fn api_key_placement_button(
        &self,
        label: &'static str,
        placement: ApiKeyPlacement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.api_key_placement == placement;
        compact_toggle(label, active)
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.api_key_placement = placement;
                    cx.notify();
                }),
            )
            .child(label)
    }

    fn render_response_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let meta = if self.response_meta.is_empty() {
            None
        } else {
            Some(self.response_meta.as_str())
        };
        let body = self.response_body_for_view();
        self.response_body_viewer.update(cx, |viewer, _cx| {
            viewer.set_text_from_parent(body);
        });

        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .child(panel_header(
                &self.response_status,
                meta,
                self.response_tone,
            ))
            .child(self.render_response_tabs(cx))
            .child(
                div()
                    .flex_1()
                    .p_3()
                    .font_family(PLATFORM_MONOSPACE_FONT)
                    .line_height(px(20.))
                    .text_size(px(13.))
                    .text_color(ui_text_primary())
                    .whitespace_normal()
                    .child(self.response_body_viewer.clone()),
            )
    }

    fn render_response_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .h(px(36.))
            .border_b_1()
            .border_color(ui_border())
            .bg(ui_surface_muted())
            .px_3()
            .gap_2()
            .child(self.response_tab("Pretty", ResponseView::Pretty, cx))
            .child(self.response_tab("Raw", ResponseView::Raw, cx))
            .child(self.response_tab("Headers", ResponseView::Headers, cx))
            .when(self.response_view == ResponseView::Pretty, |tabs| {
                tabs.child(self.response_fold_button(cx))
            })
    }

    fn response_fold_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let label = if self.response_pretty_collapsed {
            "Expand"
        } else {
            "Collapse"
        };

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(26.))
            .w(px(86.))
            .rounded(px(5.))
            .border_1()
            .border_color(ui_border_strong())
            .bg(ui_surface())
            .text_size(px(12.))
            .font_weight(FontWeight::BOLD)
            .text_color(ui_text_secondary())
            .cursor_pointer()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _event: &MouseUpEvent, _window, cx| {
                    app.response_pretty_collapsed = !app.response_pretty_collapsed;
                    cx.notify();
                }),
            )
            .child(label)
    }

    fn response_tab(
        &self,
        label: &'static str,
        view: ResponseView,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.response_view == view;
        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(26.))
            .w(px(84.))
            .rounded(px(5.))
            .border_1()
            .border_color(if active {
                ui_accent()
            } else {
                ui_border_strong()
            })
            .bg(if active {
                ui_surface()
            } else {
                ui_surface_muted()
            })
            .text_size(px(12.))
            .font_weight(FontWeight::BOLD)
            .text_color(if active {
                ui_accent()
            } else {
                ui_text_secondary()
            })
            .cursor_pointer()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.response_view = view;
                    cx.notify();
                }),
            )
            .child(label)
    }

    fn response_body_for_view(&self) -> String {
        match self.response_view {
            ResponseView::Pretty => {
                if self.response_pretty_collapsed {
                    collapsed_json_preview(&self.response_raw_body)
                        .unwrap_or_else(|| self.response_body.clone())
                } else {
                    self.response_body.clone()
                }
            }
            ResponseView::Raw => self.response_raw_body.clone(),
            ResponseView::Headers => {
                if self.response_headers.is_empty() {
                    "No response headers".to_string()
                } else {
                    self.response_headers.clone()
                }
            }
        }
    }
}

impl Render for ZenApiApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .font_family(PLATFORM_UI_FONT)
            .text_size(px(13.))
            .text_color(ui_text_primary())
            .bg(ui_surface())
            .child(self.render_top_bar(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .child(self.render_sidebar(cx))
                    .child(self.render_workspace(cx)),
            )
            .child(self.render_status_bar())
    }
}

impl Render for CollectionDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .h(px(28.))
            .max_w(px(220.))
            .rounded(px(5.))
            .border_1()
            .border_color(ui_accent())
            .bg(collection_drag_over_background())
            .px_2()
            .text_size(px(12.))
            .font_weight(FontWeight::BOLD)
            .text_color(ui_accent_text())
            .child(self.label.clone())
    }
}

impl ZenApiApp {
    fn render_status_bar(&self) -> impl IntoElement {
        let route_status = if self.routes.is_empty() {
            "No routes".to_string()
        } else {
            format!(
                "{} routes, {} visible",
                self.routes.len(),
                self.visible_routes.len()
            )
        };
        let busy_status = if self.busy { "Busy" } else { "Ready" };
        let mock_status = if self.server_running {
            format!("Mock {}", self.server_status)
        } else {
            self.server_status.clone()
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(32.))
            .w_full()
            .border_t_1()
            .border_color(ui_border())
            .bg(ui_surface_muted())
            .px_3()
            .text_size(px(12.))
            .text_color(ui_text_secondary())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(route_status)
                    .child(mock_status),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .text_color(self.response_tone.color())
                            .child(self.response_status.clone()),
                    )
                    .child(busy_status),
            )
    }
}

#[derive(Clone, Copy)]
enum ButtonTone {
    Neutral,
    Primary,
    Warning,
}

struct ButtonColors {
    background: Hsla,
    border: Hsla,
    text: Hsla,
}

impl ButtonTone {
    fn colors(self, enabled: bool) -> ButtonColors {
        if !enabled {
            return ButtonColors {
                background: ui_hover(),
                border: ui_border_strong(),
                text: ui_text_muted(),
            };
        }

        match self {
            Self::Neutral => ButtonColors {
                background: ui_surface(),
                border: ui_border_strong(),
                text: ui_text_body(),
            },
            Self::Primary => ButtonColors {
                background: ui_accent(),
                border: ui_accent_text(),
                text: ui_surface(),
            },
            Self::Warning => ButtonColors {
                background: rgb(0xb45309).into(),
                border: rgb(0x92400e).into(),
                text: ui_surface(),
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResponseView {
    Pretty,
    Raw,
    Headers,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AuthMode {
    None,
    Bearer,
    Basic,
    Jwt,
    ApiKey,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ApiKeyPlacement {
    Header,
    Query,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RequestBodyMode {
    None,
    FormData,
    UrlEncoded,
    Raw,
    GraphQL,
    Binary,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RawBodyFormat {
    Json,
    Xml,
    Text,
    Html,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestAssertionKind {
    StatusEquals,
    StatusInRange,
    HeaderExists,
    HeaderEquals,
    BodyContains,
    JsonPathEquals,
    BodyBytesLessThan,
    ElapsedLessThan,
}

impl TestAssertionKind {
    fn label(self) -> &'static str {
        match self {
            Self::StatusEquals => "Status =",
            Self::StatusInRange => "Status range",
            Self::HeaderExists => "Header exists",
            Self::HeaderEquals => "Header =",
            Self::BodyContains => "Body contains",
            Self::JsonPathEquals => "JSON path =",
            Self::BodyBytesLessThan => "Size <",
            Self::ElapsedLessThan => "Time <",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::StatusEquals => Self::StatusInRange,
            Self::StatusInRange => Self::HeaderExists,
            Self::HeaderExists => Self::HeaderEquals,
            Self::HeaderEquals => Self::BodyContains,
            Self::BodyContains => Self::JsonPathEquals,
            Self::JsonPathEquals => Self::BodyBytesLessThan,
            Self::BodyBytesLessThan => Self::ElapsedLessThan,
            Self::ElapsedLessThan => Self::StatusEquals,
        }
    }
}

impl RawBodyFormat {
    fn content_type(self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::Xml => "application/xml",
            Self::Text => "text/plain",
            Self::Html => "text/html",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyntaxTokenKind {
    String,
    Number,
    Keyword,
    Punctuation,
    Tag,
    Attribute,
}

#[derive(Clone, Copy)]
enum ResponseTone {
    Neutral,
    Busy,
    Success,
    Error,
}

impl ResponseTone {
    fn color(self) -> Hsla {
        match self {
            Self::Neutral => ui_text_secondary(),
            Self::Busy => rgb(0xd97706).into(),
            Self::Success => rgb(0x059669).into(),
            Self::Error => rgb(0xdc2626).into(),
        }
    }
}

fn key_value_rows(cx: &mut Context<ZenApiApp>, specs: &[(&str, &str)]) -> Vec<KeyValueRow> {
    specs
        .iter()
        .map(|(key_placeholder, value_placeholder)| {
            key_value_row_entity(cx, *key_placeholder, *value_placeholder)
        })
        .collect()
}

fn key_value_row_entity(
    cx: &mut Context<ZenApiApp>,
    key_placeholder: impl Into<SharedString>,
    value_placeholder: impl Into<SharedString>,
) -> KeyValueRow {
    let key_placeholder = key_placeholder.into();
    let value_placeholder = value_placeholder.into();
    KeyValueRow {
        key: cx.new(|cx| TextInput::new(cx, key_placeholder, true)),
        value: cx.new(|cx| TextInput::new(cx, value_placeholder, true)),
    }
}

fn environment_config(
    cx: &mut Context<ZenApiApp>,
    name: impl Into<String>,
    specs: &[(&str, &str)],
) -> EnvironmentConfig {
    EnvironmentConfig {
        name: name.into(),
        variables: key_value_rows(cx, specs),
    }
}

fn read_key_value_rows(rows: &[KeyValueRow], cx: &mut Context<ZenApiApp>) -> Vec<(String, String)> {
    rows.iter()
        .filter_map(|row| {
            let key = row.key.read(cx).text().trim().to_string();
            if key.is_empty() {
                return None;
            }

            Some((key, row.value.read(cx).text().trim().to_string()))
        })
        .collect()
}

fn set_key_value_rows(rows: &[KeyValueRow], values: Vec<NameValue>, cx: &mut Context<ZenApiApp>) {
    for (index, row) in rows.iter().enumerate() {
        let name = values
            .get(index)
            .map(|pair| pair.name.clone())
            .unwrap_or_default();
        let value = values
            .get(index)
            .map(|pair| pair.value.clone())
            .unwrap_or_default();

        row.key.update(cx, |input, cx| input.set_text(name, cx));
        row.value.update(cx, |input, cx| input.set_text(value, cx));
    }
}

fn assertion_rows_from_assertions(
    cx: &mut Context<ZenApiApp>,
    assertions: &[ResponseAssertion],
) -> Vec<TestAssertionRow> {
    let mut rows = assertions
        .iter()
        .map(|assertion| assertion_row_from_assertion(cx, assertion))
        .collect::<Vec<_>>();

    if rows.is_empty() {
        rows.push(blank_assertion_row(cx));
        rows.push(blank_assertion_row(cx));
    } else {
        rows.push(blank_assertion_row(cx));
    }

    rows
}

fn blank_assertion_row(cx: &mut Context<ZenApiApp>) -> TestAssertionRow {
    assertion_row_entity(cx, "", TestAssertionKind::StatusEquals, "", "")
}

fn assertion_row_from_assertion(
    cx: &mut Context<ZenApiApp>,
    assertion: &ResponseAssertion,
) -> TestAssertionRow {
    let (kind, target, expected) = assertion_fields(assertion);
    assertion_row_entity(cx, &assertion.name, kind, &target, &expected)
}

fn assertion_row_entity(
    cx: &mut Context<ZenApiApp>,
    name: impl Into<SharedString>,
    kind: TestAssertionKind,
    target: impl Into<SharedString>,
    expected: impl Into<SharedString>,
) -> TestAssertionRow {
    let row = TestAssertionRow {
        name: cx.new(|cx| TextInput::new(cx, "Test name", false)),
        kind,
        target: cx.new(|cx| TextInput::new(cx, "Target/path/header/status", true)),
        expected: cx.new(|cx| TextInput::new(cx, "Expected/value/max", true)),
    };
    let name = name.into();
    let target = target.into();
    let expected = expected.into();
    row.name.update(cx, |input, cx| input.set_text(name, cx));
    row.target
        .update(cx, |input, cx| input.set_text(target, cx));
    row.expected
        .update(cx, |input, cx| input.set_text(expected, cx));
    row
}

fn assertion_fields(assertion: &ResponseAssertion) -> (TestAssertionKind, String, String) {
    match &assertion.kind {
        ResponseAssertionKind::StatusEquals { status } => (
            TestAssertionKind::StatusEquals,
            status.to_string(),
            String::new(),
        ),
        ResponseAssertionKind::StatusInRange { min, max } => (
            TestAssertionKind::StatusInRange,
            min.to_string(),
            max.to_string(),
        ),
        ResponseAssertionKind::HeaderExists { name } => {
            (TestAssertionKind::HeaderExists, name.clone(), String::new())
        }
        ResponseAssertionKind::HeaderEquals { name, value } => {
            (TestAssertionKind::HeaderEquals, name.clone(), value.clone())
        }
        ResponseAssertionKind::BodyContains { text } => {
            (TestAssertionKind::BodyContains, text.clone(), String::new())
        }
        ResponseAssertionKind::JsonPathEquals { path, value } => (
            TestAssertionKind::JsonPathEquals,
            path.clone(),
            value.to_string(),
        ),
        ResponseAssertionKind::BodyBytesLessThan { max } => (
            TestAssertionKind::BodyBytesLessThan,
            max.to_string(),
            String::new(),
        ),
        ResponseAssertionKind::ElapsedLessThan { max_ms } => (
            TestAssertionKind::ElapsedLessThan,
            max_ms.to_string(),
            String::new(),
        ),
    }
}

fn response_assertion_from_fields(
    kind: TestAssertionKind,
    name: &str,
    target: &str,
    expected: &str,
) -> Result<Option<ResponseAssertion>> {
    let name = name.trim();
    let target = target.trim();
    let expected = expected.trim();
    if name.is_empty() && target.is_empty() && expected.is_empty() {
        return Ok(None);
    }
    if target.is_empty() {
        return Err(anyhow!("test target is required for {}", kind.label()));
    }

    let assertion_name = if name.is_empty() {
        format!("{} {target}", kind.label())
    } else {
        name.to_string()
    };
    let kind = match kind {
        TestAssertionKind::StatusEquals => ResponseAssertionKind::StatusEquals {
            status: parse_u16_field(target, "status")?,
        },
        TestAssertionKind::StatusInRange => ResponseAssertionKind::StatusInRange {
            min: parse_u16_field(target, "minimum status")?,
            max: parse_u16_field(expected, "maximum status")?,
        },
        TestAssertionKind::HeaderExists => ResponseAssertionKind::HeaderExists {
            name: target.to_string(),
        },
        TestAssertionKind::HeaderEquals => ResponseAssertionKind::HeaderEquals {
            name: target.to_string(),
            value: expected.to_string(),
        },
        TestAssertionKind::BodyContains => ResponseAssertionKind::BodyContains {
            text: target.to_string(),
        },
        TestAssertionKind::JsonPathEquals => ResponseAssertionKind::JsonPathEquals {
            path: target.to_string(),
            value: parse_json_value_field(expected)?,
        },
        TestAssertionKind::BodyBytesLessThan => ResponseAssertionKind::BodyBytesLessThan {
            max: parse_usize_field(target, "body size")?,
        },
        TestAssertionKind::ElapsedLessThan => ResponseAssertionKind::ElapsedLessThan {
            max_ms: parse_u128_field(target, "elapsed time")?,
        },
    };

    if let ResponseAssertionKind::StatusInRange { min, max } = &kind {
        if min > max {
            return Err(anyhow!("minimum status must be <= maximum status"));
        }
    }

    Ok(Some(ResponseAssertion {
        name: assertion_name,
        kind,
    }))
}

fn parse_u16_field(input: &str, label: &str) -> Result<u16> {
    input
        .parse::<u16>()
        .map_err(|error| anyhow!("invalid {label}: {error}"))
}

fn parse_usize_field(input: &str, label: &str) -> Result<usize> {
    input
        .parse::<usize>()
        .map_err(|error| anyhow!("invalid {label}: {error}"))
}

fn parse_u128_field(input: &str, label: &str) -> Result<u128> {
    input
        .parse::<u128>()
        .map_err(|error| anyhow!("invalid {label}: {error}"))
}

fn parse_json_value_field(input: &str) -> Result<serde_json::Value> {
    if input.trim().is_empty() {
        return Err(anyhow!("expected JSON value is required"));
    }

    serde_json::from_str(input).or_else(|_| Ok(serde_json::Value::String(input.to_string())))
}

fn configured_assertion_count(rows: &[TestAssertionRow], cx: &mut Context<ZenApiApp>) -> usize {
    rows.iter()
        .filter(|row| {
            !row.name.read(cx).text().trim().is_empty()
                || !row.target.read(cx).text().trim().is_empty()
                || !row.expected.read(cx).text().trim().is_empty()
        })
        .count()
}

fn assertion_meta(results: &[ResponseAssertionResult]) -> Option<String> {
    if results.is_empty() {
        return None;
    }

    let passed = results.iter().filter(|result| result.passed).count();
    Some(format!("{passed}/{} tests", results.len()))
}

fn pre_request_status_label(actions: usize) -> String {
    match actions {
        0 => "idle".to_string(),
        1 => "1 action".to_string(),
        count => format!("{count} actions"),
    }
}

fn pre_request_error_label(error: &str) -> String {
    format!("error: {}", preview_text(error))
}

fn set_key_value_pairs(
    rows: &mut Vec<KeyValueRow>,
    values: Vec<(String, String)>,
    cx: &mut Context<ZenApiApp>,
) {
    while rows.len() < values.len() {
        rows.push(key_value_row_entity(cx, "", ""));
    }

    for (index, row) in rows.iter().enumerate() {
        let (name, value) = values
            .get(index)
            .cloned()
            .unwrap_or_else(|| (String::new(), String::new()));
        row.key.update(cx, |input, cx| input.set_text(name, cx));
        row.value.update(cx, |input, cx| input.set_text(value, cx));
    }
}

fn parse_header_bulk(input: &str) -> Vec<(String, String)> {
    input.lines().filter_map(parse_header_bulk_line).collect()
}

fn parse_header_bulk_line(line: &str) -> Option<(String, String)> {
    let line = normalize_header_bulk_line(line)?;
    let (name, value) = line.split_once(':').or_else(|| line.split_once('='))?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    Some((name.to_string(), value.trim().to_string()))
}

fn normalize_header_bulk_line(line: &str) -> Option<&str> {
    let mut line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    if let Some(rest) = line.strip_prefix("-H ") {
        line = rest.trim();
    } else if let Some(rest) = line.strip_prefix("--header ") {
        line = rest.trim();
    }

    line = line
        .strip_prefix('\'')
        .and_then(|line| line.strip_suffix('\''))
        .or_else(|| {
            line.strip_prefix('"')
                .and_then(|line| line.strip_suffix('"'))
        })
        .unwrap_or(line)
        .trim();

    (!line.is_empty()).then_some(line)
}

fn format_header_bulk(headers: &[(String, String)]) -> String {
    headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn variable_store_from_pairs(
    global_variables: Vec<(String, String)>,
    active_environment: Option<&str>,
    environment_variables: Vec<(String, String)>,
) -> VariableStore {
    let mut store = VariableStore::new();

    for (name, value) in global_variables {
        store.upsert(Variable::global(name, value));
    }

    if let Some(environment) = active_environment {
        for (name, value) in environment_variables {
            store.upsert(Variable::environment(environment, name, value));
        }
    }

    store
}

fn normalized_environment_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join("-")
}

#[cfg(test)]
fn resolve_template(
    input: &str,
    store: &VariableStore,
    active_environment: Option<&str>,
) -> Result<String> {
    replace_variables(input, store, active_environment)
}

#[cfg(test)]
fn resolve_key_value_pairs(
    pairs: Vec<(String, String)>,
    store: &VariableStore,
    active_environment: Option<&str>,
) -> Result<Vec<(String, String)>> {
    pairs
        .into_iter()
        .map(|(name, value)| {
            Ok((
                resolve_template(&name, store, active_environment)?,
                resolve_template(&value, store, active_environment)?,
            ))
        })
        .collect()
}

#[cfg(test)]
fn resolve_request_body(
    body: RequestBody,
    store: &VariableStore,
    active_environment: Option<&str>,
) -> Result<RequestBody> {
    Ok(match body {
        RequestBody::None => RequestBody::None,
        RequestBody::Raw { content_type, body } => RequestBody::Raw {
            content_type,
            body: resolve_template(&body, store, active_environment)?,
        },
        RequestBody::FormUrlEncoded(fields) => {
            RequestBody::FormUrlEncoded(resolve_key_value_pairs(fields, store, active_environment)?)
        }
        RequestBody::Multipart(fields) => {
            RequestBody::Multipart(resolve_key_value_pairs(fields, store, active_environment)?)
        }
        RequestBody::BinaryFile { path, content_type } => RequestBody::BinaryFile {
            path: resolve_template(&path, store, active_environment)?,
            content_type,
        },
    })
}

fn history_request_from_body(method: &str, url: &str, body: &RequestBody) -> HistoryRequest {
    let (body_kind, body_preview) = match body {
        RequestBody::None => ("none", String::new()),
        RequestBody::Raw { body, .. } => ("raw", preview_text(body)),
        RequestBody::FormUrlEncoded(fields) => ("x-www-form-urlencoded", preview_pairs(fields)),
        RequestBody::Multipart(fields) => ("form-data", preview_pairs(fields)),
        RequestBody::BinaryFile { path, .. } => ("binary", path.clone()),
    };

    HistoryRequest {
        method: method.to_string(),
        url: url.to_string(),
        body_kind: body_kind.to_string(),
        body_preview,
    }
}

fn collection_request_from_codegen(request: &CodegenRequest) -> CollectionRequest {
    CollectionRequest {
        name: collection_request_name(&request.method, &request.url),
        method: request.method.clone(),
        url: request.url.clone(),
        headers: name_values_from_pairs(&request.headers),
        query_params: name_values_from_pairs(&request.query_params),
        body: collection_body_from_request_body(&request.body),
        pre_request_script: String::new(),
        tests: Vec::new(),
    }
}

fn collection_request_for_save(
    request: &CodegenRequest,
    pre_request_script: String,
    tests: Vec<ResponseAssertion>,
) -> CollectionRequest {
    let mut collection_request = collection_request_from_codegen(request);
    collection_request.pre_request_script = pre_request_script;
    collection_request.tests = tests;
    collection_request
}

fn collection_request_name(method: &str, url: &str) -> String {
    let path = url.split('?').next().unwrap_or(url).trim_end_matches('/');
    let tail = path
        .rsplit('/')
        .find(|segment| !segment.trim().is_empty())
        .unwrap_or("request");
    format!("{} {}", method.to_ascii_uppercase(), tail)
}

fn name_values_from_pairs(pairs: &[(String, String)]) -> Vec<NameValue> {
    pairs
        .iter()
        .filter(|(name, _value)| !name.trim().is_empty())
        .map(|(name, value)| NameValue {
            name: name.trim().to_string(),
            value: value.trim().to_string(),
        })
        .collect()
}

fn collection_body_from_request_body(body: &RequestBody) -> CollectionBody {
    match body {
        RequestBody::None => CollectionBody::None,
        RequestBody::Raw { content_type, body } => CollectionBody::Raw {
            content_type: content_type
                .clone()
                .unwrap_or_else(|| "text/plain".to_string()),
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

fn raw_format_from_content_type(content_type: &str) -> RawBodyFormat {
    let content_type = content_type.to_ascii_lowercase();
    if content_type.contains("json") {
        RawBodyFormat::Json
    } else if content_type.contains("xml") {
        RawBodyFormat::Xml
    } else if content_type.contains("html") {
        RawBodyFormat::Html
    } else {
        RawBodyFormat::Text
    }
}

fn syntax_highlights_for_gpui(
    input: &str,
    format: RawBodyFormat,
) -> Vec<(Range<usize>, HighlightStyle)> {
    syntax_highlights(input, format)
        .into_iter()
        .map(|(range, kind)| (range, syntax_highlight_style(kind)))
        .collect()
}

fn syntax_highlights(input: &str, format: RawBodyFormat) -> Vec<(Range<usize>, SyntaxTokenKind)> {
    match format {
        RawBodyFormat::Json => json_syntax_highlights(input),
        RawBodyFormat::Xml | RawBodyFormat::Html => markup_syntax_highlights(input),
        RawBodyFormat::Text => Vec::new(),
    }
}

fn syntax_highlight_style(kind: SyntaxTokenKind) -> HighlightStyle {
    let color = match kind {
        SyntaxTokenKind::String => rgb(0x047857).into(),
        SyntaxTokenKind::Number => rgb(0x7c3aed).into(),
        SyntaxTokenKind::Keyword => rgb(0x2563eb).into(),
        SyntaxTokenKind::Punctuation => rgb(0x6b7280).into(),
        SyntaxTokenKind::Tag => rgb(0xb45309).into(),
        SyntaxTokenKind::Attribute => rgb(0x0891b2).into(),
    };

    HighlightStyle {
        color: Some(color),
        font_weight: matches!(kind, SyntaxTokenKind::Keyword | SyntaxTokenKind::Tag)
            .then_some(FontWeight::BOLD),
        ..Default::default()
    }
}

fn json_syntax_highlights(input: &str) -> Vec<(Range<usize>, SyntaxTokenKind)> {
    let bytes = input.as_bytes();
    let mut highlights = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let end = string_literal_end(bytes, index);
                highlights.push((index..end, SyntaxTokenKind::String));
                index = end;
            }
            b'-' | b'0'..=b'9' => {
                let end = json_number_end(bytes, index);
                if end > index {
                    highlights.push((index..end, SyntaxTokenKind::Number));
                    index = end;
                } else {
                    index += 1;
                }
            }
            b't' | b'f' | b'n' => {
                if let Some(end) = json_keyword_end(input, index) {
                    highlights.push((index..end, SyntaxTokenKind::Keyword));
                    index = end;
                } else {
                    index += 1;
                }
            }
            b'{' | b'}' | b'[' | b']' | b':' | b',' => {
                highlights.push((index..index + 1, SyntaxTokenKind::Punctuation));
                index += 1;
            }
            _ => index += 1,
        }
    }

    highlights
}

fn markup_syntax_highlights(input: &str) -> Vec<(Range<usize>, SyntaxTokenKind)> {
    let bytes = input.as_bytes();
    let mut highlights = Vec::new();
    let mut index = 0;

    while let Some(relative_start) = input[index..].find('<') {
        let start = index + relative_start;
        let Some(relative_end) = input[start..].find('>') else {
            break;
        };
        let end = start + relative_end + 1;
        highlights.push((start..start + 1, SyntaxTokenKind::Punctuation));
        if end > start + 1 {
            highlights.push((end - 1..end, SyntaxTokenKind::Punctuation));
        }

        let mut cursor = start + 1;
        while cursor < end
            && matches!(
                bytes[cursor],
                b'/' | b'!' | b'?' | b'-' | b' ' | b'\t' | b'\n'
            )
        {
            if !bytes[cursor].is_ascii_whitespace() {
                highlights.push((cursor..cursor + 1, SyntaxTokenKind::Punctuation));
            }
            cursor += 1;
        }

        if cursor < end && matches!(bytes[cursor], b'a'..=b'z' | b'A'..=b'Z' | b'_' | b':') {
            let name_start = cursor;
            cursor += 1;
            while cursor < end
                && matches!(
                    bytes[cursor],
                    b'a'..=b'z'
                        | b'A'..=b'Z'
                        | b'0'..=b'9'
                        | b'_'
                        | b'-'
                        | b':'
                        | b'.'
                )
            {
                cursor += 1;
            }
            highlights.push((name_start..cursor, SyntaxTokenKind::Tag));
        }

        while cursor < end {
            match bytes[cursor] {
                b'"' | b'\'' => {
                    let quote = bytes[cursor];
                    let value_start = cursor;
                    cursor += 1;
                    while cursor < end && bytes[cursor] != quote {
                        cursor += 1;
                    }
                    if cursor < end {
                        cursor += 1;
                    }
                    highlights.push((value_start..cursor, SyntaxTokenKind::String));
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'_' | b':' => {
                    let name_start = cursor;
                    cursor += 1;
                    while cursor < end
                        && matches!(
                            bytes[cursor],
                            b'a'..=b'z'
                                | b'A'..=b'Z'
                                | b'0'..=b'9'
                                | b'_'
                                | b'-'
                                | b':'
                                | b'.'
                        )
                    {
                        cursor += 1;
                    }
                    if input[cursor..end].trim_start().starts_with('=') {
                        highlights.push((name_start..cursor, SyntaxTokenKind::Attribute));
                    }
                }
                b'/' | b'?' | b'!' | b'=' => {
                    highlights.push((cursor..cursor + 1, SyntaxTokenKind::Punctuation));
                    cursor += 1;
                }
                _ => cursor += 1,
            }
        }

        index = end;
    }

    highlights
}

fn string_literal_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    let mut escaped = false;
    while index < bytes.len() {
        if escaped {
            escaped = false;
        } else if bytes[index] == b'\\' {
            escaped = true;
        } else if bytes[index] == b'"' {
            return index + 1;
        }
        index += 1;
    }
    bytes.len()
}

fn json_number_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    if bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    let digits_start = index;
    while matches!(bytes.get(index), Some(b'0'..=b'9')) {
        index += 1;
    }
    if index == digits_start {
        return start;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        let exponent = index;
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exponent_digits = index;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }
        if index == exponent_digits {
            return exponent;
        }
    }
    index
}

fn json_keyword_end(input: &str, start: usize) -> Option<usize> {
    ["true", "false", "null"].iter().find_map(|keyword| {
        input[start..]
            .starts_with(keyword)
            .then(|| start + keyword.len())
    })
}

fn blank_collection_request() -> CollectionRequest {
    CollectionRequest {
        name: "New Request".to_string(),
        method: "GET".to_string(),
        url: "https://api.example.com/request".to_string(),
        headers: Vec::new(),
        query_params: Vec::new(),
        body: CollectionBody::None,
        pre_request_script: String::new(),
        tests: Vec::new(),
    }
}

fn insert_collection_item(
    items: &mut Vec<CollectionItem>,
    target_id: &str,
    item: CollectionItem,
) -> bool {
    let Some(indices) = collection_node_indices(target_id) else {
        return false;
    };
    let Some(target_items) = collection_insertion_items_mut(items, &indices) else {
        return false;
    };
    target_items.push(item);
    true
}

fn rename_collection_node(collection: &mut ApiCollection, node_id: &str, name: &str) -> bool {
    if node_id == "collection" {
        collection.name = name.to_string();
        return true;
    }

    let Some(indices) = collection_node_indices(node_id) else {
        return false;
    };
    let Some(item) = collection_item_mut(&mut collection.items, &indices) else {
        return false;
    };

    match item {
        CollectionItem::Folder(folder) => folder.name = name.to_string(),
        CollectionItem::Request(request) => request.name = name.to_string(),
    }
    true
}

fn remove_collection_item(
    items: &mut Vec<CollectionItem>,
    node_id: &str,
) -> Option<CollectionItem> {
    let indices = collection_node_indices(node_id)?;
    remove_collection_item_by_indices(items, &indices)
}

fn remove_collection_item_by_indices(
    items: &mut Vec<CollectionItem>,
    indices: &[usize],
) -> Option<CollectionItem> {
    if indices.is_empty() {
        return None;
    }

    let index = *indices.last()?;
    let parent = collection_parent_items_mut(items, &indices)?;
    (index < parent.len()).then(|| parent.remove(index))
}

fn duplicate_collection_item(items: &mut Vec<CollectionItem>, node_id: &str) -> bool {
    let Some(indices) = collection_node_indices(node_id) else {
        return false;
    };
    if indices.is_empty() {
        return false;
    }

    let Some(item) = collection_item_ref(items, &indices).cloned() else {
        return false;
    };
    let item = collection_item_copy(item);
    let Some(index) = indices.last().copied() else {
        return false;
    };
    let Some(parent) = collection_parent_items_mut(items, &indices) else {
        return false;
    };

    parent.insert(index + 1, item);
    true
}

fn move_collection_item(items: &mut Vec<CollectionItem>, source_id: &str, target_id: &str) -> bool {
    let Some(source_indices) = collection_node_indices(source_id) else {
        return false;
    };
    let Some(mut target_indices) = collection_node_indices(target_id) else {
        return false;
    };
    if source_indices.is_empty()
        || collection_path_contains(&source_indices, &target_indices)
        || (!target_indices.is_empty() && collection_item_ref(items, &target_indices).is_none())
    {
        return false;
    }

    let Some(item) = remove_collection_item_by_indices(items, &source_indices) else {
        return false;
    };
    adjust_collection_indices_after_removal(&source_indices, &mut target_indices);

    if insert_collection_item_for_drop(items, &target_indices, item) {
        true
    } else {
        false
    }
}

fn collection_path_contains(source: &[usize], target: &[usize]) -> bool {
    target.len() >= source.len() && target[..source.len()] == *source
}

fn adjust_collection_indices_after_removal(source: &[usize], target: &mut [usize]) {
    if source.is_empty() || target.len() < source.len() {
        return;
    }

    let source_parent_len = source.len() - 1;
    if target[..source_parent_len] == source[..source_parent_len]
        && target[source_parent_len] > source[source_parent_len]
    {
        target[source_parent_len] -= 1;
    }
}

fn insert_collection_item_for_drop(
    items: &mut Vec<CollectionItem>,
    target_indices: &[usize],
    item: CollectionItem,
) -> bool {
    if target_indices.is_empty() {
        items.push(item);
        return true;
    }

    let target_is_folder = matches!(
        collection_item_ref(items, target_indices),
        Some(CollectionItem::Folder(_))
    );
    if target_is_folder {
        let Some(CollectionItem::Folder(folder)) = collection_item_mut(items, target_indices)
        else {
            return false;
        };
        folder.items.push(item);
        return true;
    }

    let Some(index) = target_indices.last().copied() else {
        return false;
    };
    let Some(parent) = collection_parent_items_mut(items, target_indices) else {
        return false;
    };
    let insert_at = (index + 1).min(parent.len());
    parent.insert(insert_at, item);
    true
}

fn collection_item_copy(mut item: CollectionItem) -> CollectionItem {
    match &mut item {
        CollectionItem::Folder(folder) => folder.name = format!("{} Copy", folder.name),
        CollectionItem::Request(request) => request.name = format!("{} Copy", request.name),
    }
    item
}

fn collection_node_indices(node_id: &str) -> Option<Vec<usize>> {
    if node_id == "collection" {
        return Some(Vec::new());
    }

    let path = node_id.strip_prefix("collection/")?;
    path.split('/')
        .map(|segment| segment.parse::<usize>().ok())
        .collect()
}

fn collection_item_ref<'a>(
    items: &'a [CollectionItem],
    indices: &[usize],
) -> Option<&'a CollectionItem> {
    let (index, rest) = indices.split_first()?;
    let item = items.get(*index)?;
    if rest.is_empty() {
        return Some(item);
    }

    match item {
        CollectionItem::Folder(folder) => collection_item_ref(&folder.items, rest),
        CollectionItem::Request(_) => None,
    }
}

fn collection_item_mut<'a>(
    items: &'a mut Vec<CollectionItem>,
    indices: &[usize],
) -> Option<&'a mut CollectionItem> {
    let (index, rest) = indices.split_first()?;
    let item = items.get_mut(*index)?;
    if rest.is_empty() {
        return Some(item);
    }

    match item {
        CollectionItem::Folder(folder) => collection_item_mut(&mut folder.items, rest),
        CollectionItem::Request(_) => None,
    }
}

fn collection_parent_items_mut<'a>(
    items: &'a mut Vec<CollectionItem>,
    indices: &[usize],
) -> Option<&'a mut Vec<CollectionItem>> {
    if indices.is_empty() || indices.len() == 1 {
        return Some(items);
    }

    let index = indices[0];
    match items.get_mut(index)? {
        CollectionItem::Folder(folder) => {
            collection_parent_items_mut(&mut folder.items, &indices[1..])
        }
        CollectionItem::Request(_) => None,
    }
}

fn collection_insertion_items_mut<'a>(
    items: &'a mut Vec<CollectionItem>,
    indices: &[usize],
) -> Option<&'a mut Vec<CollectionItem>> {
    if indices.is_empty() {
        return Some(items);
    }

    if indices.len() == 1 {
        let index = indices[0];
        let target_is_folder = matches!(items.get(index)?, CollectionItem::Folder(_));
        if !target_is_folder {
            return Some(items);
        }

        return match items.get_mut(index)? {
            CollectionItem::Folder(folder) => Some(&mut folder.items),
            CollectionItem::Request(_) => None,
        };
    }

    let index = indices[0];
    match items.get_mut(index)? {
        CollectionItem::Folder(folder) => {
            collection_insertion_items_mut(&mut folder.items, &indices[1..])
        }
        CollectionItem::Request(_) => None,
    }
}

fn preview_pairs(pairs: &[(String, String)]) -> String {
    preview_text(
        &pairs
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("&"),
    )
}

#[cfg(test)]
fn websocket_log_entries(exchange: &client::WebSocketExchange) -> Vec<WebSocketLogEntry> {
    let mut entries = vec![WebSocketLogEntry {
        direction: WebSocketDirection::Sent,
        kind: "text".to_string(),
        data: exchange.sent.clone(),
    }];

    entries.extend(exchange.received.iter().map(|message| WebSocketLogEntry {
        direction: WebSocketDirection::Received,
        kind: websocket_message_kind_label(&message.kind).to_string(),
        data: message.data.clone(),
    }));

    entries
}

#[cfg(test)]
fn websocket_exchange_text(exchange: &client::WebSocketExchange) -> String {
    let mut lines = vec![
        format!("url: {}", exchange.url),
        format!("elapsed: {}ms", exchange.elapsed_ms),
        format!("sent text: {}", exchange.sent),
    ];

    for message in &exchange.received {
        lines.push(format!(
            "received {}: {}",
            websocket_message_kind_label(&message.kind),
            message.data
        ));
    }

    lines.join("\n")
}

fn websocket_message_kind_label(kind: &client::WebSocketMessageKind) -> &'static str {
    match kind {
        client::WebSocketMessageKind::Text => "text",
        client::WebSocketMessageKind::Binary => "binary",
        client::WebSocketMessageKind::Ping => "ping",
        client::WebSocketMessageKind::Pong => "pong",
        client::WebSocketMessageKind::Close => "close",
    }
}

fn websocket_hex_bytes(input: &str) -> std::result::Result<Vec<u8>, String> {
    let digits = input
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '_' && *ch != '-')
        .collect::<String>();
    if digits.is_empty() {
        return Err("Enter binary data as hexadecimal bytes.".to_string());
    }
    if digits.len() % 2 != 0 {
        return Err("Hex binary input must have an even number of digits.".to_string());
    }

    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for index in (0..digits.len()).step_by(2) {
        let byte = u8::from_str_radix(&digits[index..index + 2], 16)
            .map_err(|_| format!("Invalid hex byte at offset {index}."))?;
        bytes.push(byte);
    }

    Ok(bytes)
}

fn websocket_protocol_list(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|protocol| !protocol.is_empty())
        .map(str::to_string)
        .collect()
}

fn sse_log_entries(exchange: &client::SseExchange) -> Vec<SseLogEntry> {
    exchange.events.iter().map(sse_log_entry).collect()
}

fn sse_log_entry(event: &client::SseEvent) -> SseLogEntry {
    SseLogEntry {
        event: sse_event_label(event).to_string(),
        data: event.data.clone(),
        id: event.id.clone(),
    }
}

fn sse_exchange_text(exchange: &client::SseExchange) -> String {
    let mut lines = vec![
        format!("url: {}", exchange.url),
        format!("elapsed: {}ms", exchange.elapsed_ms),
        format!("events: {}", exchange.events.len()),
    ];

    for event in &exchange.events {
        let mut line = format!("{}: {}", sse_event_label(event), event.data);
        if let Some(id) = &event.id {
            line.push_str(&format!(" [id {id}]"));
        }
        if let Some(retry) = event.retry {
            line.push_str(&format!(" [retry {retry}]"));
        }
        lines.push(line);
    }

    lines.join("\n")
}

fn sse_event_label(event: &client::SseEvent) -> &str {
    event.event.as_deref().unwrap_or("message")
}

fn graphql_body(query: &str, variables: &str) -> String {
    let variables = graphql_variables_value(variables);
    pretty_json(&serde_json::json!({
        "query": query,
        "variables": variables,
    }))
}

fn graphql_variables_value(input: &str) -> serde_json::Value {
    let input = input.trim();
    if input.is_empty() {
        return serde_json::json!({});
    }

    match serde_json::from_str::<serde_json::Value>(input) {
        Ok(value @ serde_json::Value::Object(_)) => value,
        _ => serde_json::json!({}),
    }
}

fn graphql_fields_from_body(content_type: &str, body: &str) -> Option<(String, String)> {
    if !content_type.to_ascii_lowercase().contains("json") {
        return None;
    }

    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    let object = value.as_object()?;
    let query = object.get("query")?.as_str()?.to_string();
    let variables = object
        .get("variables")
        .and_then(|variables| serde_json::to_string_pretty(variables).ok())
        .unwrap_or_else(|| "{}".to_string());
    Some((query, variables))
}

fn graphql_schema_summary(body: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    let schema = value.get("data")?.get("__schema")?;
    let types = schema.get("types")?.as_array()?;
    let directives = schema
        .get("directives")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    let query_type = schema
        .get("queryType")
        .and_then(|value| value.get("name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");
    let mutation_type = schema
        .get("mutationType")
        .and_then(|value| value.get("name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");
    let subscription_type = schema
        .get("subscriptionType")
        .and_then(|value| value.get("name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");

    let object_count = graphql_type_kind_count(types, "OBJECT");
    let input_count = graphql_type_kind_count(types, "INPUT_OBJECT");
    let enum_count = graphql_type_kind_count(types, "ENUM");
    let scalar_count = graphql_type_kind_count(types, "SCALAR");
    let query_fields = graphql_type_field_count(types, query_type);
    let mutation_fields = graphql_type_field_count(types, mutation_type);
    let subscription_fields = graphql_type_field_count(types, subscription_type);

    Some(
        [
            format!("roots: query {query_type}, mutation {mutation_type}, subscription {subscription_type}"),
            format!(
                "types: {} total, {object_count} object, {input_count} input, {enum_count} enum, {scalar_count} scalar",
                types.len()
            ),
            format!("fields: query {query_fields}, mutation {mutation_fields}, subscription {subscription_fields}"),
            format!("directives: {directives}"),
        ]
        .join("\n"),
    )
}

fn graphql_schema_browser(body: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    let schema = value.get("data")?.get("__schema")?;
    let types = schema.get("types")?.as_array()?;

    let mut sections = Vec::new();
    for (label, key) in [
        ("query fields", "queryType"),
        ("mutation fields", "mutationType"),
        ("subscription fields", "subscriptionType"),
    ] {
        if let Some(type_name) = graphql_schema_root_type_name(schema, key) {
            if let Some(section) = graphql_root_fields_section(types, label, type_name) {
                sections.push(section);
            }
        }
    }

    if let Some(section) = graphql_type_index_section(types) {
        sections.push(section);
    }
    if let Some(section) = graphql_directives_section(schema) {
        sections.push(section);
    }

    (!sections.is_empty()).then(|| sections.join("\n\n"))
}

fn graphql_query_templates(body: &str) -> Option<Vec<GraphqlQueryTemplate>> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    let schema = value.get("data")?.get("__schema")?;
    let types = schema.get("types")?.as_array()?;
    let query_type = graphql_schema_root_type_name(schema, "queryType")?;
    let fields = graphql_type_by_name(types, query_type)?
        .get("fields")?
        .as_array()?;
    let templates = fields
        .iter()
        .filter_map(graphql_query_template_from_field)
        .take(GRAPHQL_QUERY_TEMPLATE_LIMIT)
        .collect::<Vec<_>>();

    (!templates.is_empty()).then_some(templates)
}

fn graphql_query_template_from_field(field: &serde_json::Value) -> Option<GraphqlQueryTemplate> {
    let field_name = field.get("name")?.as_str()?.to_string();
    let args = graphql_operation_args(field);
    let operation_name = format!("{}Query", graphql_pascal_case(&field_name));
    let variable_definitions = graphql_operation_variable_definitions(&args);
    let field_arguments = graphql_operation_field_arguments(&args);
    let selection = if field.get("type").is_some_and(graphql_type_ref_is_leaf) {
        String::new()
    } else {
        " {\n    __typename\n  }".to_string()
    };
    let operation = format!(
        "query {operation_name}{variable_definitions} {{\n  {field_name}{field_arguments}{selection}\n}}"
    );

    Some(GraphqlQueryTemplate {
        field_name,
        operation,
        variables: graphql_operation_variables(&args),
    })
}

fn graphql_operation_args(field: &serde_json::Value) -> Vec<GraphqlOperationArg> {
    field
        .get("args")
        .and_then(serde_json::Value::as_array)
        .map(|args| {
            args.iter()
                .filter_map(graphql_operation_arg)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn graphql_operation_arg(arg: &serde_json::Value) -> Option<GraphqlOperationArg> {
    let name = arg.get("name")?.as_str()?.to_string();
    let type_ref_value = arg.get("type");
    let type_ref = type_ref_value
        .map(graphql_type_ref)
        .unwrap_or_else(|| "?".to_string());
    let default_value = arg
        .get("defaultValue")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let placeholder = type_ref_value
        .map(|value| graphql_variable_placeholder(&name, value))
        .unwrap_or_else(|| serde_json::json!(format!("<{name}>")));

    Some(GraphqlOperationArg {
        name,
        type_ref,
        default_value,
        placeholder,
    })
}

fn graphql_operation_variable_definitions(args: &[GraphqlOperationArg]) -> String {
    if args.is_empty() {
        return String::new();
    }

    format!(
        "({})",
        args.iter()
            .map(|arg| {
                let default_value = arg
                    .default_value
                    .as_ref()
                    .map(|value| format!(" = {value}"))
                    .unwrap_or_default();
                format!("${}: {}{}", arg.name, arg.type_ref, default_value)
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn graphql_operation_field_arguments(args: &[GraphqlOperationArg]) -> String {
    if args.is_empty() {
        return String::new();
    }

    format!(
        "({})",
        args.iter()
            .map(|arg| format!("{}: ${}", arg.name, arg.name))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn graphql_operation_variables(args: &[GraphqlOperationArg]) -> String {
    let variables = args
        .iter()
        .map(|arg| (arg.name.clone(), arg.placeholder.clone()))
        .collect::<serde_json::Map<_, _>>();
    pretty_json(&serde_json::Value::Object(variables))
}

fn graphql_variable_placeholder(name: &str, type_ref: &serde_json::Value) -> serde_json::Value {
    match graphql_type_ref_base_name(type_ref) {
        Some("Int") => serde_json::json!(0),
        Some("Float") => serde_json::json!(0.0),
        Some("Boolean") => serde_json::json!(false),
        _ => serde_json::json!(format!("<{name}>")),
    }
}

fn graphql_type_ref_is_leaf(type_ref: &serde_json::Value) -> bool {
    matches!(
        graphql_type_ref_base_kind(type_ref),
        Some("SCALAR" | "ENUM")
    )
}

fn graphql_type_ref_base_kind(type_ref: &serde_json::Value) -> Option<&str> {
    let kind = type_ref
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .filter(|kind| !kind.is_empty())?;

    match kind {
        "NON_NULL" | "LIST" => type_ref
            .get("ofType")
            .filter(|value| !value.is_null())
            .and_then(graphql_type_ref_base_kind),
        _ => Some(kind),
    }
}

fn graphql_type_ref_base_name(type_ref: &serde_json::Value) -> Option<&str> {
    let kind = type_ref
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .filter(|kind| !kind.is_empty())?;

    match kind {
        "NON_NULL" | "LIST" => type_ref
            .get("ofType")
            .filter(|value| !value.is_null())
            .and_then(graphql_type_ref_base_name),
        _ => type_ref.get("name").and_then(serde_json::Value::as_str),
    }
}

fn graphql_pascal_case(input: &str) -> String {
    let mut output = String::new();
    let mut uppercase_next = true;

    for ch in input.chars() {
        if !ch.is_ascii_alphanumeric() {
            uppercase_next = true;
            continue;
        }

        if uppercase_next {
            output.push(ch.to_ascii_uppercase());
        } else {
            output.push(ch);
        }
        uppercase_next = false;
    }

    if output.is_empty() {
        "Operation".to_string()
    } else {
        output
    }
}

fn graphql_schema_root_type_name<'a>(schema: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    schema
        .get(key)?
        .get("name")?
        .as_str()
        .filter(|name| !name.is_empty())
}

fn graphql_root_fields_section(
    types: &[serde_json::Value],
    label: &str,
    type_name: &str,
) -> Option<String> {
    let fields = graphql_type_by_name(types, type_name)?
        .get("fields")?
        .as_array()?;
    let signatures = fields
        .iter()
        .filter_map(graphql_field_signature)
        .collect::<Vec<_>>();

    let mut lines = vec![format!("{label} ({type_name})")];
    if signatures.is_empty() {
        lines.push("  -".to_string());
    } else {
        lines.extend(
            signatures
                .iter()
                .take(GRAPHQL_SCHEMA_FIELD_LIMIT)
                .map(|signature| format!("  {signature}")),
        );
        if signatures.len() > GRAPHQL_SCHEMA_FIELD_LIMIT {
            lines.push(format!(
                "  ... {} more",
                signatures.len() - GRAPHQL_SCHEMA_FIELD_LIMIT
            ));
        }
    }

    Some(lines.join("\n"))
}

fn graphql_field_signature(field: &serde_json::Value) -> Option<String> {
    let name = field.get("name")?.as_str()?;
    let args = field
        .get("args")
        .and_then(serde_json::Value::as_array)
        .map(|args| {
            args.iter()
                .filter_map(graphql_arg_signature)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let return_type = field
        .get("type")
        .map(graphql_type_ref)
        .unwrap_or_else(|| "?".to_string());
    let call = if args.is_empty() {
        name.to_string()
    } else {
        format!("{name}({}):", args.join(", "))
    };
    let deprecated = field
        .get("isDeprecated")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
        .then_some(" @deprecated")
        .unwrap_or_default();

    if args.is_empty() {
        Some(format!("{call}: {return_type}{deprecated}"))
    } else {
        Some(format!("{call} {return_type}{deprecated}"))
    }
}

fn graphql_arg_signature(arg: &serde_json::Value) -> Option<String> {
    let name = arg.get("name")?.as_str()?;
    let arg_type = arg
        .get("type")
        .map(graphql_type_ref)
        .unwrap_or_else(|| "?".to_string());
    let default_value = arg
        .get("defaultValue")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|value| format!(" = {value}"))
        .unwrap_or_default();

    Some(format!("{name}: {arg_type}{default_value}"))
}

fn graphql_type_ref(type_ref: &serde_json::Value) -> String {
    let kind = type_ref
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let name = type_ref
        .get("name")
        .and_then(serde_json::Value::as_str)
        .filter(|name| !name.is_empty());

    match kind {
        "NON_NULL" => {
            let inner = type_ref
                .get("ofType")
                .filter(|value| !value.is_null())
                .map(graphql_type_ref)
                .unwrap_or_else(|| "?".to_string());
            format!("{inner}!")
        }
        "LIST" => {
            let inner = type_ref
                .get("ofType")
                .filter(|value| !value.is_null())
                .map(graphql_type_ref)
                .unwrap_or_else(|| "?".to_string());
            format!("[{inner}]")
        }
        _ => name.map(str::to_string).unwrap_or_else(|| {
            (!kind.is_empty())
                .then(|| kind.to_string())
                .unwrap_or("?".to_string())
        }),
    }
}

fn graphql_type_index_section(types: &[serde_json::Value]) -> Option<String> {
    let mut lines = vec!["type index".to_string()];
    for (label, kind) in [
        ("objects", "OBJECT"),
        ("inputs", "INPUT_OBJECT"),
        ("enums", "ENUM"),
        ("scalars", "SCALAR"),
    ] {
        let names = graphql_type_names_by_kind(types, kind);
        if !names.is_empty() {
            lines.push(format!(
                "  {label}: {}",
                graphql_limited_list(&names, GRAPHQL_SCHEMA_TYPE_LIMIT)
            ));
        }
    }

    (lines.len() > 1).then(|| lines.join("\n"))
}

fn graphql_type_names_by_kind(types: &[serde_json::Value], kind: &str) -> Vec<String> {
    let mut names = types
        .iter()
        .filter(|value| {
            value
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value == kind)
        })
        .filter_map(|value| value.get("name").and_then(serde_json::Value::as_str))
        .filter(|name| !name.starts_with("__"))
        .map(str::to_string)
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

fn graphql_directives_section(schema: &serde_json::Value) -> Option<String> {
    let mut names = schema
        .get("directives")?
        .as_array()?
        .iter()
        .filter_map(|value| value.get("name").and_then(serde_json::Value::as_str))
        .map(|name| format!("@{name}"))
        .collect::<Vec<_>>();
    names.sort_unstable();

    (!names.is_empty()).then(|| {
        format!(
            "directives\n  {}",
            graphql_limited_list(&names, GRAPHQL_SCHEMA_TYPE_LIMIT)
        )
    })
}

fn graphql_limited_list(names: &[String], limit: usize) -> String {
    let rendered = names
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > limit {
        format!("{rendered} (+{} more)", names.len() - limit)
    } else {
        rendered
    }
}

fn graphql_type_by_name<'a>(
    types: &'a [serde_json::Value],
    name: &str,
) -> Option<&'a serde_json::Value> {
    types.iter().find(|value| {
        value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value == name)
    })
}

fn graphql_type_kind_count(types: &[serde_json::Value], kind: &str) -> usize {
    types
        .iter()
        .filter(|value| {
            value
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value == kind)
        })
        .count()
}

fn graphql_type_field_count(types: &[serde_json::Value], name: &str) -> usize {
    if name == "-" {
        return 0;
    }

    types
        .iter()
        .find(|value| {
            value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value == name)
        })
        .and_then(|value| value.get("fields"))
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len)
}

fn preview_text(text: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 240;

    let mut preview = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= MAX_PREVIEW_CHARS {
            preview.push_str("...");
            break;
        }
        preview.push(ch);
    }
    preview
}

fn key_value_editor(title: &'static str, rows: &[KeyValueRow]) -> impl IntoElement {
    let rendered_rows = rows
        .iter()
        .map(|row| key_value_row(row.key.clone(), row.value.clone()))
        .collect::<Vec<_>>();

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::BOLD)
                .text_color(ui_text_secondary())
                .child(title),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .text_size(px(11.))
                .font_weight(FontWeight::BOLD)
                .text_color(ui_text_muted())
                .child(div().w(px(KEY_VALUE_KEY_COLUMN_WIDTH)).child("Key"))
                .child(div().flex_1().child("Value")),
        )
        .child(div().flex().flex_col().gap_1().children(rendered_rows))
}

fn key_value_row(key: Entity<TextInput>, value: Entity<TextInput>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(div().w(px(KEY_VALUE_KEY_COLUMN_WIDTH)).child(key))
        .child(div().flex_1().child(value))
}

fn assertion_editor_row(
    index: usize,
    row: &TestAssertionRow,
    cx: &mut Context<ZenApiApp>,
) -> gpui::Div {
    let kind = row.kind;
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(TEST_ASSERTION_NAME_COLUMN_WIDTH))
                .child(row.name.clone()),
        )
        .child(
            compact_toggle(kind.label(), true)
                .w(px(TEST_ASSERTION_KIND_COLUMN_WIDTH))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                        if let Some(row) = app.request_assertions.get_mut(index) {
                            row.kind = row.kind.next();
                            cx.notify();
                        }
                    }),
                )
                .child(kind.label()),
        )
        .child(div().flex_1().child(row.target.clone()))
        .child(div().flex_1().child(row.expected.clone()))
}

fn assertion_result_row(result: ResponseAssertionResult) -> gpui::Div {
    let tone = if result.passed {
        ResponseTone::Success
    } else {
        ResponseTone::Error
    };
    div()
        .flex()
        .items_center()
        .h(px(30.))
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(ui_hover())
        .text_size(px(12.))
        .child(
            div()
                .w(px(48.))
                .font_family(PLATFORM_MONOSPACE_FONT)
                .font_weight(FontWeight::BOLD)
                .text_color(tone.color())
                .child(if result.passed { "PASS" } else { "FAIL" }),
        )
        .child(
            div()
                .w(px(140.))
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_color(ui_text_body())
                .child(result.name),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .text_color(ui_text_secondary())
                .child(result.error.unwrap_or_else(|| "ok".to_string())),
        )
}

fn pre_request_action_row(action: String) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .h(px(30.))
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(ui_hover())
        .text_size(px(12.))
        .child(
            div()
                .w(px(48.))
                .font_family(PLATFORM_MONOSPACE_FONT)
                .font_weight(FontWeight::BOLD)
                .text_color(ResponseTone::Success.color())
                .child("PASS"),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .font_family(PLATFORM_MONOSPACE_FONT)
                .text_color(ui_text_secondary())
                .child(action),
        )
}

fn compact_toggle(label: &str, active: bool) -> gpui::Div {
    let width = if label.len() > 12 { px(156.) } else { px(76.) };

    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(26.))
        .w(width)
        .px_2()
        .rounded(px(5.))
        .border_1()
        .border_color(if active {
            ui_accent()
        } else {
            ui_border_strong()
        })
        .bg(if active {
            ui_surface()
        } else {
            ui_surface_muted()
        })
        .text_size(px(12.))
        .font_weight(FontWeight::BOLD)
        .text_color(if active {
            ui_accent()
        } else {
            ui_text_secondary()
        })
        .cursor_pointer()
}

fn bearer_auth_pair(token: &str) -> Option<(String, String)> {
    let token = token.trim();
    (!token.is_empty()).then(|| ("Authorization".to_string(), format!("Bearer {token}")))
}

fn jwt_auth_pair(token: &str) -> Option<(String, String)> {
    bearer_auth_pair(token)
}

fn basic_auth_pair(username: &str, password: &str) -> Option<(String, String)> {
    let username = username.trim();
    if username.is_empty() {
        return None;
    }

    let credentials = format!("{username}:{}", password.trim());
    Some((
        "Authorization".to_string(),
        format!("Basic {}", BASE64_STANDARD.encode(credentials)),
    ))
}

fn api_key_pair(name: &str, value: &str) -> Option<(String, String)> {
    let name = name.trim();
    (!name.is_empty()).then(|| (name.to_string(), value.trim().to_string()))
}

fn snippet_language_label(language: SnippetLanguage) -> &'static str {
    match language {
        SnippetLanguage::Curl => "cURL",
        SnippetLanguage::PythonRequests => "Python",
        SnippetLanguage::JavaScriptFetch => "JavaScript",
        SnippetLanguage::RustReqwest => "Rust",
        SnippetLanguage::GoNetHttp => "Go",
    }
}

fn panel_header(
    title: impl Into<SharedString>,
    meta: Option<&str>,
    tone: ResponseTone,
) -> impl IntoElement {
    let title = title.into();
    div()
        .flex()
        .flex_col()
        .h(px(40.))
        .border_b_1()
        .border_color(ui_border())
        .bg(ui_surface())
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .flex_1()
                .pl_3()
                .pr(px(14.))
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .text_size(px(13.))
                        .text_color(ui_text_primary())
                        .child(title.clone()),
                )
                .child(
                    div()
                        .w(px(260.))
                        .truncate()
                        .text_right()
                        .font_family(PLATFORM_MONOSPACE_FONT)
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tone.color())
                        .child(meta.unwrap_or("").to_string()),
                ),
        )
        .child(div().ml(px(12.)).h(px(2.)).w(px(80.)).bg(ui_accent()))
}

fn runner_status_text(summary: &CollectionRunSummary) -> String {
    let suffix = if summary.stopped_early {
        ", stopped"
    } else {
        ""
    };
    format!(
        "{} passed, {} failed, {} total{suffix}",
        summary.passed, summary.failed, summary.total
    )
}

fn runner_summary_text(summary: &CollectionRunSummary) -> String {
    let mut lines = vec![format!(
        "{}\n{} in {} ms",
        summary.collection_name,
        runner_status_text(summary),
        summary.elapsed_ms
    )];

    for result in &summary.results {
        let status = result
            .status
            .map(|status| status.to_string())
            .unwrap_or_else(|| "ERR".to_string());
        let outcome = if result.success { "PASS" } else { "FAIL" };
        let mut line = format!(
            "[{outcome}] {status} {} {} - {} ms, {}",
            result.method,
            result.url,
            result.elapsed_ms,
            format_bytes(result.body_bytes)
        );
        if let Some(error) = &result.error {
            line.push_str(&format!(" - {error}"));
        }
        lines.push(line);
        for action in &result.pre_request_actions {
            lines.push(format!("  [PASS] pre-request {action}"));
        }
        for assertion in &result.assertions {
            let outcome = if assertion.passed { "PASS" } else { "FAIL" };
            let mut line = format!("  [{outcome}] test {}", assertion.name);
            if let Some(error) = &assertion.error {
                line.push_str(&format!(" - {error}"));
            }
            lines.push(line);
        }
    }

    lines.join("\n")
}

fn runner_result_row(result: CollectionRunResult) -> impl IntoElement {
    let status = result
        .status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "ERR".to_string());
    let tone = if result.success {
        ResponseTone::Success
    } else {
        ResponseTone::Error
    };
    let path = result.path.join(" / ");

    div()
        .flex()
        .items_center()
        .h(px(30.))
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(ui_hover())
        .font_family(PLATFORM_MONOSPACE_FONT)
        .text_size(px(12.))
        .child(
            div()
                .w(px(70.))
                .font_weight(FontWeight::BOLD)
                .text_color(method_color(&result.method))
                .child(result.method),
        )
        .child(div().w(px(42.)).text_color(tone.color()).child(status))
        .child(
            div()
                .flex_1()
                .truncate()
                .text_color(ui_text_body())
                .child(path),
        )
        .child(
            div()
                .w(px(76.))
                .text_right()
                .text_color(ui_text_secondary())
                .child(format_bytes(result.body_bytes)),
        )
        .when(!result.pre_request_actions.is_empty(), |row| {
            row.child(
                div()
                    .w(px(52.))
                    .text_right()
                    .text_color(ResponseTone::Success.color())
                    .child(format!("pre {}", result.pre_request_actions.len())),
            )
        })
        .when(!result.assertions.is_empty(), |row| {
            let failed = result
                .assertions
                .iter()
                .filter(|assertion| !assertion.passed)
                .count();
            row.child(
                div()
                    .w(px(64.))
                    .text_right()
                    .text_color(if failed == 0 {
                        ResponseTone::Success.color()
                    } else {
                        ResponseTone::Error.color()
                    })
                    .child(format!(
                        "{}/{}",
                        result.assertions.len() - failed,
                        result.assertions.len()
                    )),
            )
        })
}

fn mock_log_row(method: String, path: String, status: u16) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .h(px(30.))
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(ui_hover())
        .font_family(PLATFORM_MONOSPACE_FONT)
        .text_size(px(12.))
        .child(
            div()
                .w(px(70.))
                .font_weight(FontWeight::BOLD)
                .text_color(method_color(&method))
                .child(method),
        )
        .child(
            div()
                .w(px(42.))
                .text_color(response_tone(status).color())
                .child(status.to_string()),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .text_color(ui_text_body())
                .child(path),
        )
}

fn websocket_log_row(entry: WebSocketLogEntry) -> impl IntoElement {
    let (direction, direction_color) = match entry.direction {
        WebSocketDirection::Sent => ("sent", ui_accent_text()),
        WebSocketDirection::Received => ("recv", ResponseTone::Success.color()),
    };

    div()
        .flex()
        .items_center()
        .h(px(30.))
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(ui_hover())
        .font_family(PLATFORM_MONOSPACE_FONT)
        .text_size(px(12.))
        .child(
            div()
                .w(px(52.))
                .font_weight(FontWeight::BOLD)
                .text_color(direction_color)
                .child(direction),
        )
        .child(
            div()
                .w(px(52.))
                .text_color(ui_text_secondary())
                .child(entry.kind),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .text_color(ui_text_body())
                .child(entry.data),
        )
}

fn sse_log_row(entry: SseLogEntry) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .h(px(30.))
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(ui_hover())
        .font_family(PLATFORM_MONOSPACE_FONT)
        .text_size(px(12.))
        .child(
            div()
                .w(px(74.))
                .font_weight(FontWeight::BOLD)
                .text_color(ui_accent_text())
                .child(entry.event),
        )
        .child(
            div()
                .w(px(58.))
                .text_color(ui_text_secondary())
                .child(entry.id.unwrap_or_else(|| "-".to_string())),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .text_color(ui_text_body())
                .child(entry.data),
        )
}

fn history_row(
    id: u64,
    method: String,
    url: String,
    status: String,
    cx: &mut Context<ZenApiApp>,
) -> impl IntoElement + 'static + use<> {
    div()
        .id(("history", id))
        .flex()
        .items_center()
        .h(px(46.))
        .rounded(px(4.))
        .px_2()
        .py_1()
        .hover(|row| row.bg(ui_hover()))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .cursor_pointer()
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                        app.restore_history_entry(id, cx);
                    }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(HTTP_METHOD_LABEL_WIDTH))
                                .text_size(px(12.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(method_color(&method))
                                .child(method),
                        )
                        .child(
                            div()
                                .flex_1()
                                .truncate()
                                .font_family(PLATFORM_MONOSPACE_FONT)
                                .text_size(px(12.))
                                .text_color(ui_text_primary())
                                .child(url),
                        ),
                )
                .child(
                    div()
                        .ml(px(66.))
                        .truncate()
                        .font_family(PLATFORM_MONOSPACE_FONT)
                        .text_size(px(11.))
                        .text_color(ui_text_secondary())
                        .child(status),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .h(px(24.))
                .w(px(42.))
                .rounded(px(4.))
                .border_1()
                .border_color(ui_border_strong())
                .bg(ui_surface())
                .text_size(px(11.))
                .text_color(ui_text_secondary())
                .cursor_pointer()
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                        app.history.remove(id);
                        cx.notify();
                    }),
                )
                .child("Del"),
        )
}

fn append_collection_rows(
    rows: &mut Vec<gpui::AnyElement>,
    items: &[CollectionItem],
    parent_id: &str,
    depth: usize,
    expanded_nodes: &[String],
    cx: &mut Context<ZenApiApp>,
) {
    for (index, item) in items.iter().enumerate() {
        let id = format!("{parent_id}/{index}");
        match item {
            CollectionItem::Folder(folder) => {
                let expanded = expanded_nodes.iter().any(|node| node == &id);
                rows.push(collection_folder_row(
                    &id,
                    folder,
                    depth,
                    collection_item_count(&folder.items),
                    expanded,
                    cx,
                ));
                if expanded {
                    append_collection_rows(rows, &folder.items, &id, depth + 1, expanded_nodes, cx);
                }
            }
            CollectionItem::Request(request) => {
                rows.push(collection_request_row(&id, request.clone(), depth, cx));
            }
        }
    }
}

fn collection_tree_indent(depth: usize) -> f32 {
    COLLECTION_TREE_INDENT_BASE + depth as f32 * COLLECTION_TREE_INDENT_STEP
}

fn collection_tree_row_height(kind: CollectionNodeKind) -> f32 {
    match kind {
        CollectionNodeKind::Root => COLLECTION_TREE_ROOT_ROW_HEIGHT,
        CollectionNodeKind::Folder => COLLECTION_TREE_FOLDER_ROW_HEIGHT,
        CollectionNodeKind::Request => COLLECTION_TREE_REQUEST_ROW_HEIGHT,
    }
}

fn collection_drag_over_background() -> Hsla {
    ui_accent_surface()
}

fn collection_root_row(
    name: String,
    item_count: usize,
    expanded: bool,
    cx: &mut Context<ZenApiApp>,
) -> gpui::AnyElement {
    let marker = if expanded { "v" } else { ">" };
    let menu_label = name.clone();

    div()
        .flex()
        .items_center()
        .h(px(collection_tree_row_height(CollectionNodeKind::Root)))
        .rounded(px(4.))
        .px_2()
        .gap_2()
        .text_size(px(12.))
        .cursor_pointer()
        .hover(|row| row.bg(ui_hover()))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|app, _event: &MouseUpEvent, _window, cx| {
                app.toggle_collection_node("collection".to_string(), cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, _event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                app.open_collection_menu(
                    CollectionContextMenu {
                        node_id: "collection".to_string(),
                        label: menu_label.clone(),
                        kind: CollectionNodeKind::Root,
                    },
                    cx,
                );
            }),
        )
        .drag_over::<DraggedCollectionNode>(|row, _dragged, _window, _cx| {
            row.bg(collection_drag_over_background())
        })
        .on_drop(
            cx.listener(|app, dragged: &DraggedCollectionNode, _window, cx| {
                app.move_collection_target(dragged.node_id.clone(), "collection".to_string(), cx);
            }),
        )
        .child(
            div()
                .w(px(COLLECTION_TREE_MARKER_WIDTH))
                .font_family(PLATFORM_MONOSPACE_FONT)
                .text_color(ui_text_secondary())
                .child(marker),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_color(ui_text_primary())
                .child(name),
        )
        .child(
            div()
                .font_family(PLATFORM_MONOSPACE_FONT)
                .text_color(ui_text_muted())
                .child(item_count.to_string()),
        )
        .into_any()
}

fn collection_folder_row(
    id: &str,
    folder: &CollectionFolder,
    depth: usize,
    item_count: usize,
    expanded: bool,
    cx: &mut Context<ZenApiApp>,
) -> gpui::AnyElement {
    let id = id.to_string();
    let marker = if expanded { "v" } else { ">" };
    let element_id = format!("collection-folder:{id}");
    let toggle_id = id.clone();
    let menu_id = id.clone();
    let menu_label = folder.name.clone();
    let drop_id = id.clone();
    let drag_value = DraggedCollectionNode {
        node_id: id,
        label: folder.name.clone(),
    };

    div()
        .id(element_id)
        .flex()
        .items_center()
        .h(px(collection_tree_row_height(CollectionNodeKind::Folder)))
        .rounded(px(4.))
        .pl(px(collection_tree_indent(depth)))
        .pr_2()
        .gap_2()
        .text_size(px(12.))
        .cursor_pointer()
        .hover(|row| row.bg(ui_hover()))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                app.toggle_collection_node(toggle_id.clone(), cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, _event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                app.open_collection_menu(
                    CollectionContextMenu {
                        node_id: menu_id.clone(),
                        label: menu_label.clone(),
                        kind: CollectionNodeKind::Folder,
                    },
                    cx,
                );
            }),
        )
        .on_drag(drag_value, |dragged, _offset, _window, cx| {
            cx.new(|_| CollectionDragPreview {
                label: dragged.label.clone(),
            })
        })
        .drag_over::<DraggedCollectionNode>(|row, _dragged, _window, _cx| {
            row.bg(collection_drag_over_background())
        })
        .on_drop(
            cx.listener(move |app, dragged: &DraggedCollectionNode, _window, cx| {
                app.move_collection_target(dragged.node_id.clone(), drop_id.clone(), cx);
            }),
        )
        .child(
            div()
                .w(px(COLLECTION_TREE_MARKER_WIDTH))
                .font_family(PLATFORM_MONOSPACE_FONT)
                .text_color(ui_text_secondary())
                .child(marker),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_color(ui_text_body())
                .child(folder.name.clone()),
        )
        .child(
            div()
                .font_family(PLATFORM_MONOSPACE_FONT)
                .text_color(ui_text_muted())
                .child(item_count.to_string()),
        )
        .into_any()
}

fn collection_request_row(
    id: &str,
    request: CollectionRequest,
    depth: usize,
    cx: &mut Context<ZenApiApp>,
) -> gpui::AnyElement {
    let menu_id = id.to_string();
    let element_id = format!("collection-request:{id}");
    let method = request.method.clone();
    let name = request.name.clone();
    let url = request.url.clone();
    let menu_label = request.name.clone();
    let drop_id = menu_id.clone();
    let drag_value = DraggedCollectionNode {
        node_id: menu_id.clone(),
        label: request.name.clone(),
    };
    let restore_request = request.clone();

    div()
        .id(element_id)
        .flex()
        .items_center()
        .h(px(collection_tree_row_height(CollectionNodeKind::Request)))
        .rounded(px(4.))
        .pl(px(collection_tree_indent(depth)))
        .pr_2()
        .gap_2()
        .cursor_pointer()
        .hover(|row| row.bg(ui_hover()))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                app.restore_collection_request(restore_request.clone(), cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, _event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                app.open_collection_menu(
                    CollectionContextMenu {
                        node_id: menu_id.clone(),
                        label: menu_label.clone(),
                        kind: CollectionNodeKind::Request,
                    },
                    cx,
                );
            }),
        )
        .on_drag(drag_value, |dragged, _offset, _window, cx| {
            cx.new(|_| CollectionDragPreview {
                label: dragged.label.clone(),
            })
        })
        .drag_over::<DraggedCollectionNode>(|row, _dragged, _window, _cx| {
            row.bg(collection_drag_over_background())
        })
        .on_drop(
            cx.listener(move |app, dragged: &DraggedCollectionNode, _window, cx| {
                app.move_collection_target(dragged.node_id.clone(), drop_id.clone(), cx);
            }),
        )
        .child(
            div()
                .w(px(HTTP_METHOD_LABEL_WIDTH))
                .text_size(px(11.))
                .font_weight(FontWeight::BOLD)
                .text_color(method_color(&method))
                .child(method),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.))
                        .text_color(ui_text_primary())
                        .child(name),
                )
                .child(
                    div()
                        .truncate()
                        .font_family(PLATFORM_MONOSPACE_FONT)
                        .text_size(px(11.))
                        .text_color(ui_text_secondary())
                        .child(url),
                ),
        )
        .into_any()
}

fn collection_item_count(items: &[CollectionItem]) -> usize {
    items
        .iter()
        .map(|item| match item {
            CollectionItem::Folder(folder) => collection_item_count(&folder.items),
            CollectionItem::Request(_) => 1,
        })
        .sum()
}

fn method_color(method: &str) -> Hsla {
    match method {
        "GET" => rgb(0x059669).into(),
        "POST" => rgb(0xd97706).into(),
        "PUT" => rgb(0x2563eb).into(),
        "PATCH" => rgb(0x7c3aed).into(),
        "DELETE" => rgb(0xdc2626).into(),
        "OPTIONS" => rgb(0x0891b2).into(),
        "HEAD" => rgb(0x4b5563).into(),
        _ => rgb(0x6b7280).into(),
    }
}

fn response_tone(status: u16) -> ResponseTone {
    if (200..400).contains(&status) {
        ResponseTone::Success
    } else if status >= 400 {
        ResponseTone::Error
    } else {
        ResponseTone::Neutral
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

fn default_request_body(method: &str) -> &'static str {
    match method {
        "POST" | "PUT" | "PATCH" => "{}",
        _ => "",
    }
}

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn collapsed_json_preview(input: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(input).ok()?;
    Some(collapsed_json_value(&value, 0))
}

fn collapsed_json_value(value: &serde_json::Value, depth: usize) -> String {
    const MAX_CHILDREN: usize = 24;

    match value {
        serde_json::Value::Object(object) => {
            if object.is_empty() {
                return "{}".to_string();
            }

            let indent = "  ".repeat(depth);
            let child_indent = "  ".repeat(depth + 1);
            let mut lines = vec![format!("{{ // {} keys", object.len())];
            for (index, (key, value)) in object.iter().enumerate() {
                if index >= MAX_CHILDREN {
                    lines.push(format!(
                        "{child_indent}... // {} more",
                        object.len() - MAX_CHILDREN
                    ));
                    break;
                }
                lines.push(format!(
                    "{child_indent}\"{key}\": {}",
                    collapsed_json_summary(value)
                ));
            }
            lines.push(format!("{indent}}}"));
            lines.join("\n")
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                return "[]".to_string();
            }

            let indent = "  ".repeat(depth);
            let child_indent = "  ".repeat(depth + 1);
            let mut lines = vec![format!("[ // {} items", items.len())];
            for (index, value) in items.iter().enumerate().take(MAX_CHILDREN) {
                lines.push(format!(
                    "{child_indent}[{index}] {}",
                    collapsed_json_summary(value)
                ));
            }
            if items.len() > MAX_CHILDREN {
                lines.push(format!(
                    "{child_indent}... // {} more",
                    items.len() - MAX_CHILDREN
                ));
            }
            lines.push(format!("{indent}]"));
            lines.join("\n")
        }
        _ => collapsed_json_summary(value),
    }
}

fn collapsed_json_summary(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(object) => format!("{{...}} // {} keys", object.len()),
        serde_json::Value::Array(items) => format!("[...] // {} items", items.len()),
        serde_json::Value::String(value) => format!("{value:?}"),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Null => "null".to_string(),
    }
}

fn format_response_meta(elapsed_ms: u128, body_bytes: usize) -> String {
    format!("{} ms | {}", elapsed_ms, format_bytes(body_bytes))
}

fn format_headers(headers: &[(String, String)]) -> String {
    headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_bytes(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = 1024 * KB;

    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
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
    fn empty_route_filter_returns_all_routes() {
        let routes = vec![
            route("GET", "/users", "List accounts"),
            route("POST", "/sessions", "Create login session"),
        ];

        assert_eq!(filter_routes(&routes, "   "), routes);
    }

    #[test]
    fn maps_http_status_to_response_tone() {
        assert!(matches!(response_tone(200), ResponseTone::Success));
        assert!(matches!(response_tone(302), ResponseTone::Success));
        assert!(matches!(response_tone(100), ResponseTone::Neutral));
        assert!(matches!(response_tone(404), ResponseTone::Error));
        assert!(matches!(response_tone(500), ResponseTone::Error));
    }

    #[test]
    fn formats_response_meta_with_elapsed_time_and_size() {
        assert_eq!(format_response_meta(42, 17), "42 ms | 17 B");
        assert_eq!(format_response_meta(5, 2048), "5 ms | 2.0 KB");
    }

    #[test]
    fn runner_summary_includes_pre_request_action_log() {
        let summary = CollectionRunSummary {
            collection_name: "Demo".to_string(),
            total: 1,
            passed: 1,
            failed: 0,
            stopped_early: false,
            elapsed_ms: 7,
            results: vec![CollectionRunResult {
                index: 0,
                path: vec!["Demo".to_string(), "Health".to_string()],
                name: "Health".to_string(),
                method: "GET".to_string(),
                url: "http://localhost/health".to_string(),
                status: Some(200),
                success: true,
                elapsed_ms: 3,
                body_bytes: 2,
                pre_request_actions: vec![
                    "set_var token".to_string(),
                    "set_header Authorization".to_string(),
                ],
                assertions: Vec::new(),
                error: None,
            }],
        };

        let text = runner_summary_text(&summary);

        assert!(text.contains("[PASS] pre-request set_var token"));
        assert!(text.contains("[PASS] pre-request set_header Authorization"));
    }

    #[test]
    fn formats_response_headers() {
        let headers = vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("x-request-id".to_string(), "abc".to_string()),
        ];

        assert_eq!(
            format_headers(&headers),
            "content-type: application/json\nx-request-id: abc"
        );
    }

    #[test]
    fn collapses_json_response_preview() {
        let collapsed = collapsed_json_preview(
            r#"{"users":[{"id":1,"name":"Zen"}],"ok":true,"meta":{"page":1}}"#,
        )
        .expect("json");

        assert!(collapsed.contains("{ // 3 keys"));
        assert!(collapsed.contains("\"users\": [...] // 1 items"));
        assert!(collapsed.contains("\"ok\": true"));
        assert!(collapsed.contains("\"meta\": {...} // 1 keys"));
    }

    #[test]
    fn collapsed_json_preview_rejects_non_json() {
        assert_eq!(collapsed_json_preview("not-json"), None);
    }

    #[test]
    fn parses_bulk_header_text() {
        let headers = parse_header_bulk(
            r#"
Accept: application/json
Authorization=Bearer token
-H 'X-Trace-Id: abc'
--header "X-Mode: test"
# ignored
Cookie: a=b; c=d
"#,
        );

        assert_eq!(
            headers,
            vec![
                ("Accept".to_string(), "application/json".to_string()),
                ("Authorization".to_string(), "Bearer token".to_string()),
                ("X-Trace-Id".to_string(), "abc".to_string()),
                ("X-Mode".to_string(), "test".to_string()),
                ("Cookie".to_string(), "a=b; c=d".to_string()),
            ]
        );
    }

    #[test]
    fn formats_bulk_headers_for_clipboard() {
        let headers = vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("X-Trace-Id".to_string(), "abc".to_string()),
        ];

        assert_eq!(
            format_header_bulk(&headers),
            "Accept: application/json\nX-Trace-Id: abc"
        );
    }

    #[test]
    fn builds_response_assertions_from_editor_fields() {
        let assertion =
            response_assertion_from_fields(TestAssertionKind::StatusInRange, "2xx", "200", "299")
                .expect("assertion")
                .expect("configured");

        assert_eq!(assertion.name, "2xx");
        assert!(matches!(
            assertion.kind,
            ResponseAssertionKind::StatusInRange { min: 200, max: 299 }
        ));

        let json =
            response_assertion_from_fields(TestAssertionKind::JsonPathEquals, "", "ok", "true")
                .expect("json assertion")
                .expect("configured");

        assert_eq!(json.name, "JSON path = ok");
        assert_eq!(
            json.kind,
            ResponseAssertionKind::JsonPathEquals {
                path: "ok".to_string(),
                value: serde_json::Value::Bool(true),
            }
        );
    }

    #[test]
    fn rejects_invalid_response_assertion_fields() {
        let invalid_status =
            response_assertion_from_fields(TestAssertionKind::StatusEquals, "", "abc", "")
                .expect_err("invalid status");
        assert!(invalid_status.to_string().contains("invalid status"));

        let invalid_range =
            response_assertion_from_fields(TestAssertionKind::StatusInRange, "", "500", "200")
                .expect_err("invalid range");
        assert!(
            invalid_range
                .to_string()
                .contains("minimum status must be <= maximum status")
        );

        assert!(
            response_assertion_from_fields(TestAssertionKind::HeaderExists, "", "", "")
                .expect("empty row")
                .is_none()
        );
    }

    #[test]
    fn formats_pre_request_status_labels() {
        assert_eq!(pre_request_status_label(0), "idle");
        assert_eq!(pre_request_status_label(1), "1 action");
        assert_eq!(pre_request_status_label(3), "3 actions");
        assert_eq!(
            pre_request_error_label("set_header expects name=value"),
            "error: set_header expects name=value"
        );
    }

    #[test]
    fn pre_request_action_labels_do_not_include_values() {
        let execution = execute_pre_request_actions(
            "set_var token=secret; set_header Authorization=Bearer {{token}}",
            CodegenRequest {
                method: "GET".to_string(),
                url: "https://api.example.com".to_string(),
                headers: Vec::new(),
                query_params: Vec::new(),
                body: RequestBody::None,
            },
            VariableStore::new(),
            None,
        )
        .expect("pre-request execution");

        let labels = pre_request_action_labels(&execution.actions);

        assert_eq!(
            labels,
            vec![
                "set_var token".to_string(),
                "set_header Authorization".to_string(),
            ]
        );
        assert!(!labels.join("\n").contains("secret"));
    }

    #[test]
    fn builds_bearer_basic_and_api_key_pairs() {
        assert_eq!(
            bearer_auth_pair(" token "),
            Some(("Authorization".to_string(), "Bearer token".to_string()))
        );
        assert_eq!(
            basic_auth_pair("dev", "secret"),
            Some((
                "Authorization".to_string(),
                "Basic ZGV2OnNlY3JldA==".to_string()
            ))
        );
        assert_eq!(
            jwt_auth_pair(" ey.jwt.token "),
            Some((
                "Authorization".to_string(),
                "Bearer ey.jwt.token".to_string()
            ))
        );
        assert_eq!(
            api_key_pair("X-API-Key", " key "),
            Some(("X-API-Key".to_string(), "key".to_string()))
        );
        assert_eq!(bearer_auth_pair(" "), None);
        assert_eq!(jwt_auth_pair(" "), None);
        assert_eq!(basic_auth_pair("", "secret"), None);
        assert_eq!(api_key_pair("", "key"), None);
    }

    #[test]
    fn builds_variable_store_and_resolves_request_templates() {
        let store = variable_store_from_pairs(
            vec![
                (
                    "baseUrl".to_string(),
                    "https://prod.example.com".to_string(),
                ),
                ("token".to_string(), "prod-token".to_string()),
            ],
            Some("dev"),
            vec![
                ("baseUrl".to_string(), "http://localhost:8080".to_string()),
                ("token".to_string(), "dev-token".to_string()),
            ],
        );

        assert_eq!(
            resolve_template("{{baseUrl}}/users", &store, Some("dev")).expect("url"),
            "http://localhost:8080/users"
        );
        assert_eq!(
            resolve_key_value_pairs(
                vec![("Authorization".to_string(), "Bearer {{token}}".to_string())],
                &store,
                Some("dev"),
            )
            .expect("headers"),
            vec![("Authorization".to_string(), "Bearer dev-token".to_string())]
        );
    }

    #[test]
    fn normalizes_custom_environment_names() {
        assert_eq!(normalized_environment_name(" staging "), "staging");
        assert_eq!(
            normalized_environment_name(" qa team  west "),
            "qa-team-west"
        );
        assert_eq!(normalized_environment_name("   "), "");
    }

    #[test]
    fn resolves_variables_for_custom_environments() {
        let store = variable_store_from_pairs(
            vec![("baseUrl".to_string(), "https://api.example.com".to_string())],
            Some("qa-team-west"),
            vec![(
                "baseUrl".to_string(),
                "https://qa-west.example.com".to_string(),
            )],
        );

        assert_eq!(
            resolve_template("{{baseUrl}}/health", &store, Some("qa-team-west"))
                .expect("custom environment"),
            "https://qa-west.example.com/health"
        );
    }

    #[test]
    fn resolves_variables_in_all_request_body_modes() {
        let store = variable_store_from_pairs(
            vec![
                ("name".to_string(), "Zen".to_string()),
                ("file".to_string(), "/tmp/upload.bin".to_string()),
            ],
            None,
            Vec::new(),
        );

        assert_eq!(
            resolve_request_body(
                RequestBody::Raw {
                    content_type: Some("application/json".to_string()),
                    body: "{\"name\":\"{{name}}\"}".to_string(),
                },
                &store,
                None,
            )
            .expect("raw"),
            RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"name\":\"Zen\"}".to_string(),
            }
        );
        assert_eq!(
            resolve_request_body(
                RequestBody::FormUrlEncoded(vec![("name".to_string(), "{{name}}".to_string(),)]),
                &store,
                None,
            )
            .expect("urlencoded"),
            RequestBody::FormUrlEncoded(vec![("name".to_string(), "Zen".to_string())])
        );
        assert_eq!(
            resolve_request_body(
                RequestBody::Multipart(vec![("file".to_string(), "@{{file}}".to_string(),)]),
                &store,
                None,
            )
            .expect("multipart"),
            RequestBody::Multipart(vec![("file".to_string(), "@/tmp/upload.bin".to_string())])
        );
        assert_eq!(
            resolve_request_body(
                RequestBody::BinaryFile {
                    path: "{{file}}".to_string(),
                    content_type: Some("application/octet-stream".to_string()),
                },
                &store,
                None,
            )
            .expect("binary"),
            RequestBody::BinaryFile {
                path: "/tmp/upload.bin".to_string(),
                content_type: Some("application/octet-stream".to_string()),
            }
        );
    }

    #[test]
    fn builds_graphql_body_and_extracts_saved_graphql_fields() {
        let body = graphql_body(
            "query User($id: ID!) { user(id: $id) { name } }",
            r#"{"id":"42"}"#,
        );
        let value = serde_json::from_str::<serde_json::Value>(&body).expect("graphql json");

        assert_eq!(
            value["query"],
            "query User($id: ID!) { user(id: $id) { name } }"
        );
        assert_eq!(value["variables"]["id"], "42");

        let (query, variables) =
            graphql_fields_from_body("application/json", &body).expect("graphql fields");
        assert_eq!(query, "query User($id: ID!) { user(id: $id) { name } }");
        assert!(variables.contains("\"id\": \"42\""));

        let empty_variables = graphql_body("{ viewer { login } }", "not-json");
        let value = serde_json::from_str::<serde_json::Value>(&empty_variables).expect("json");
        assert_eq!(value["variables"], serde_json::json!({}));
    }

    #[test]
    fn graphql_introspection_query_builds_schema_request_body() {
        let body = graphql_body(GRAPHQL_INTROSPECTION_QUERY, "{}");
        let value = serde_json::from_str::<serde_json::Value>(&body).expect("introspection json");
        let query = value["query"].as_str().expect("query");

        assert!(query.contains("__schema"));
        assert!(query.contains("queryType"));
        assert!(query.contains("directives"));
        assert_eq!(value["variables"], serde_json::json!({}));
    }

    #[test]
    fn summarizes_graphql_introspection_response() {
        let response = serde_json::json!({
            "data": {
                "__schema": {
                    "queryType": { "name": "Query" },
                    "mutationType": { "name": "Mutation" },
                    "subscriptionType": null,
                    "types": [
                        { "kind": "OBJECT", "name": "Query", "fields": [
                            { "name": "viewer" },
                            { "name": "node" }
                        ] },
                        { "kind": "OBJECT", "name": "Mutation", "fields": [
                            { "name": "createUser" }
                        ] },
                        { "kind": "SCALAR", "name": "String" },
                        { "kind": "ENUM", "name": "Role" },
                        { "kind": "INPUT_OBJECT", "name": "UserInput" }
                    ],
                    "directives": [
                        { "name": "include" },
                        { "name": "skip" }
                    ]
                }
            }
        })
        .to_string();

        let summary = graphql_schema_summary(&response).expect("schema summary");

        assert!(summary.contains("roots: query Query, mutation Mutation, subscription -"));
        assert!(summary.contains("5 total, 2 object, 1 input, 1 enum, 1 scalar"));
        assert!(summary.contains("fields: query 2, mutation 1, subscription 0"));
        assert!(summary.contains("directives: 2"));
        assert_eq!(graphql_schema_summary(r#"{"data":{"ok":true}}"#), None);
    }

    #[test]
    fn browses_graphql_introspection_response() {
        let response = serde_json::json!({
            "data": {
                "__schema": {
                    "queryType": { "name": "Query" },
                    "mutationType": { "name": "Mutation" },
                    "subscriptionType": null,
                    "types": [
                        { "kind": "OBJECT", "name": "Query", "fields": [
                            {
                                "name": "viewer",
                                "args": [],
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "User" }
                                }
                            },
                            {
                                "name": "search",
                                "args": [
                                    {
                                        "name": "term",
                                        "type": {
                                            "kind": "NON_NULL",
                                            "name": null,
                                            "ofType": { "kind": "SCALAR", "name": "String" }
                                        }
                                    },
                                    {
                                        "name": "limit",
                                        "type": { "kind": "SCALAR", "name": "Int" },
                                        "defaultValue": "10"
                                    }
                                ],
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "User" }
                                }
                            },
                            {
                                "name": "legacy",
                                "args": [],
                                "type": { "kind": "SCALAR", "name": "String" },
                                "isDeprecated": true
                            }
                        ] },
                        { "kind": "OBJECT", "name": "Mutation", "fields": [
                            {
                                "name": "createUser",
                                "args": [],
                                "type": { "kind": "OBJECT", "name": "User" }
                            }
                        ] },
                        { "kind": "OBJECT", "name": "User", "fields": [] },
                        { "kind": "OBJECT", "name": "__Schema", "fields": [] },
                        { "kind": "SCALAR", "name": "String" },
                        { "kind": "SCALAR", "name": "Int" },
                        { "kind": "ENUM", "name": "Role" },
                        { "kind": "INPUT_OBJECT", "name": "UserInput" }
                    ],
                    "directives": [
                        { "name": "skip" },
                        { "name": "include" }
                    ]
                }
            }
        })
        .to_string();

        let browser = graphql_schema_browser(&response).expect("schema browser");

        assert!(browser.contains("query fields (Query)"));
        assert!(browser.contains("viewer: User!"));
        assert!(browser.contains("search(term: String!, limit: Int = 10): [User]"));
        assert!(browser.contains("legacy: String @deprecated"));
        assert!(browser.contains("mutation fields (Mutation)"));
        assert!(browser.contains("createUser: User"));
        assert!(browser.contains("objects: Mutation, Query, User"));
        assert!(browser.contains("inputs: UserInput"));
        assert!(browser.contains("enums: Role"));
        assert!(browser.contains("scalars: Int, String"));
        assert!(browser.contains("directives\n  @include, @skip"));
        assert!(!browser.contains("__Schema"));
        assert_eq!(graphql_schema_browser(r#"{"data":{"ok":true}}"#), None);
    }

    #[test]
    fn generates_graphql_query_templates_from_introspection_response() {
        let response = serde_json::json!({
            "data": {
                "__schema": {
                    "queryType": { "name": "Query" },
                    "mutationType": null,
                    "subscriptionType": null,
                    "types": [
                        { "kind": "OBJECT", "name": "Query", "fields": [
                            {
                                "name": "viewer",
                                "args": [
                                    {
                                        "name": "id",
                                        "type": {
                                            "kind": "NON_NULL",
                                            "name": null,
                                            "ofType": { "kind": "SCALAR", "name": "ID" }
                                        }
                                    }
                                ],
                                "type": { "kind": "OBJECT", "name": "User" }
                            },
                            {
                                "name": "search",
                                "args": [
                                    {
                                        "name": "term",
                                        "type": {
                                            "kind": "NON_NULL",
                                            "name": null,
                                            "ofType": { "kind": "SCALAR", "name": "String" }
                                        }
                                    },
                                    {
                                        "name": "limit",
                                        "type": { "kind": "SCALAR", "name": "Int" },
                                        "defaultValue": "10"
                                    },
                                    {
                                        "name": "active",
                                        "type": { "kind": "SCALAR", "name": "Boolean" }
                                    }
                                ],
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "User" }
                                }
                            },
                            {
                                "name": "version",
                                "args": [],
                                "type": { "kind": "SCALAR", "name": "String" }
                            }
                        ] },
                        { "kind": "OBJECT", "name": "User", "fields": [] },
                        { "kind": "SCALAR", "name": "ID" },
                        { "kind": "SCALAR", "name": "String" },
                        { "kind": "SCALAR", "name": "Int" },
                        { "kind": "SCALAR", "name": "Boolean" }
                    ],
                    "directives": []
                }
            }
        })
        .to_string();

        let templates = graphql_query_templates(&response).expect("query templates");

        assert_eq!(templates.len(), 3);
        assert_eq!(templates[0].field_name, "viewer");
        assert!(
            templates[0]
                .operation
                .contains("query ViewerQuery($id: ID!)")
        );
        assert!(templates[0].operation.contains("viewer(id: $id) {"));
        assert!(templates[0].operation.contains("__typename"));

        let viewer_variables =
            serde_json::from_str::<serde_json::Value>(&templates[0].variables).expect("variables");
        assert_eq!(viewer_variables["id"], "<id>");

        assert_eq!(templates[1].field_name, "search");
        assert!(
            templates[1]
                .operation
                .contains("$term: String!, $limit: Int = 10, $active: Boolean")
        );
        assert!(
            templates[1]
                .operation
                .contains("search(term: $term, limit: $limit, active: $active)")
        );
        let search_variables =
            serde_json::from_str::<serde_json::Value>(&templates[1].variables).expect("variables");
        assert_eq!(search_variables["term"], "<term>");
        assert_eq!(search_variables["limit"], 0);
        assert_eq!(search_variables["active"], false);

        assert_eq!(templates[2].field_name, "version");
        assert!(templates[2].operation.contains("query VersionQuery"));
        assert!(templates[2].operation.contains("version"));
        assert!(!templates[2].operation.contains("__typename"));
        assert_eq!(templates[2].variables, "{}");
        assert_eq!(graphql_query_templates(r#"{"data":{"ok":true}}"#), None);
    }

    #[test]
    fn builds_websocket_log_entries_and_response_text() {
        let exchange = client::WebSocketExchange {
            url: "ws://localhost/socket".to_string(),
            sent: "hello".to_string(),
            received: vec![
                client::WebSocketMessage {
                    kind: client::WebSocketMessageKind::Text,
                    data: "echo:hello".to_string(),
                },
                client::WebSocketMessage {
                    kind: client::WebSocketMessageKind::Pong,
                    data: "0 bytes".to_string(),
                },
            ],
            elapsed_ms: 12,
        };

        let entries = websocket_log_entries(&exchange);
        assert_eq!(
            entries,
            vec![
                WebSocketLogEntry {
                    direction: WebSocketDirection::Sent,
                    kind: "text".to_string(),
                    data: "hello".to_string(),
                },
                WebSocketLogEntry {
                    direction: WebSocketDirection::Received,
                    kind: "text".to_string(),
                    data: "echo:hello".to_string(),
                },
                WebSocketLogEntry {
                    direction: WebSocketDirection::Received,
                    kind: "pong".to_string(),
                    data: "0 bytes".to_string(),
                },
            ]
        );

        let text = websocket_exchange_text(&exchange);
        assert!(text.contains("url: ws://localhost/socket"));
        assert!(text.contains("elapsed: 12ms"));
        assert!(text.contains("sent text: hello"));
        assert!(text.contains("received text: echo:hello"));
        assert!(text.contains("received pong: 0 bytes"));
    }

    #[test]
    fn parses_websocket_binary_hex_input() {
        assert_eq!(
            websocket_hex_bytes("00 ff 7A").expect("hex"),
            vec![0, 255, 122]
        );
        assert_eq!(
            websocket_hex_bytes("00-01_ff").expect("hex"),
            vec![0, 1, 255]
        );

        assert!(
            websocket_hex_bytes("")
                .expect_err("empty")
                .contains("hexadecimal")
        );
        assert!(websocket_hex_bytes("0").expect_err("odd").contains("even"));
        assert!(
            websocket_hex_bytes("zz")
                .expect_err("invalid")
                .contains("offset 0")
        );
    }

    #[test]
    fn parses_websocket_subprotocol_list() {
        assert_eq!(
            websocket_protocol_list("chat, superchat, , json.v2 "),
            vec![
                "chat".to_string(),
                "superchat".to_string(),
                "json.v2".to_string()
            ]
        );
        assert!(websocket_protocol_list(" , ").is_empty());
    }

    #[test]
    fn builds_sse_log_entries_and_response_text() {
        let exchange = client::SseExchange {
            url: "http://localhost/events".to_string(),
            events: vec![
                client::SseEvent {
                    event: Some("ready".to_string()),
                    data: "connected".to_string(),
                    id: Some("1".to_string()),
                    retry: Some(3000),
                },
                client::SseEvent {
                    event: None,
                    data: "plain".to_string(),
                    id: None,
                    retry: None,
                },
            ],
            elapsed_ms: 9,
        };

        assert_eq!(
            sse_log_entries(&exchange),
            vec![
                SseLogEntry {
                    event: "ready".to_string(),
                    data: "connected".to_string(),
                    id: Some("1".to_string()),
                },
                SseLogEntry {
                    event: "message".to_string(),
                    data: "plain".to_string(),
                    id: None,
                },
            ]
        );

        let text = sse_exchange_text(&exchange);
        assert!(text.contains("url: http://localhost/events"));
        assert!(text.contains("elapsed: 9ms"));
        assert!(text.contains("events: 2"));
        assert!(text.contains("ready: connected [id 1] [retry 3000]"));
        assert!(text.contains("message: plain"));
    }

    #[test]
    fn resolves_variables_inside_graphql_body_json() {
        let store =
            variable_store_from_pairs(vec![("id".to_string(), "42".to_string())], None, Vec::new());
        let body = RequestBody::Raw {
            content_type: Some("application/json".to_string()),
            body: graphql_body(
                "query User { user(id: \"{{id}}\") { name } }",
                r#"{"id":"{{id}}"}"#,
            ),
        };

        let resolved = resolve_request_body(body, &store, None).expect("graphql variables");

        let RequestBody::Raw { body, .. } = resolved else {
            panic!("expected raw GraphQL JSON body");
        };
        let value = serde_json::from_str::<serde_json::Value>(&body).expect("graphql json");

        assert_eq!(value["query"], "query User { user(id: \"42\") { name } }");
        assert_eq!(value["variables"]["id"], "42");
    }

    #[test]
    fn builds_history_request_summaries_from_body_modes() {
        let raw = history_request_from_body(
            "POST",
            "https://api.example.com/users",
            &RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"name\":\"Zen\"}".to_string(),
            },
        );
        assert_eq!(raw.method, "POST");
        assert_eq!(raw.body_kind, "raw");
        assert_eq!(raw.body_preview, "{\"name\":\"Zen\"}");

        let form = history_request_from_body(
            "POST",
            "https://api.example.com/login",
            &RequestBody::FormUrlEncoded(vec![("username".to_string(), "dev".to_string())]),
        );
        assert_eq!(form.body_kind, "x-www-form-urlencoded");
        assert_eq!(form.body_preview, "username=dev");

        let binary = history_request_from_body(
            "POST",
            "https://api.example.com/upload",
            &RequestBody::BinaryFile {
                path: "/tmp/upload.bin".to_string(),
                content_type: Some("application/octet-stream".to_string()),
            },
        );
        assert_eq!(binary.body_kind, "binary");
        assert_eq!(binary.body_preview, "/tmp/upload.bin");
    }

    #[derive(Debug, PartialEq, Eq)]
    struct VisibleHistoryRow {
        id: u64,
        method: String,
        url: String,
        status: String,
    }

    fn visible_history_rows(history: &RequestHistory, query: &str) -> Vec<VisibleHistoryRow> {
        history
            .filtered(query)
            .into_iter()
            .take(8)
            .map(|entry| VisibleHistoryRow {
                id: entry.id,
                method: entry.request.method.clone(),
                url: entry.request.url.clone(),
                status: entry.response.status.clone(),
            })
            .collect()
    }

    #[test]
    fn history_sidebar_visible_rows_follow_filter_and_limit() {
        let mut history = RequestHistory::new();
        for index in 0..10 {
            history.record_at(
                index,
                HistoryRequest {
                    method: if index % 2 == 0 { "GET" } else { "POST" }.to_string(),
                    url: format!("https://api.example.com/items/{index}"),
                    body_kind: "none".to_string(),
                    body_preview: String::new(),
                },
                HistoryResponse {
                    status: if index == 9 { "HTTP 201" } else { "HTTP 200" }.to_string(),
                    meta: format!("{index} ms | 2 B"),
                    body_preview: "{}".to_string(),
                },
            );
        }

        let rows = visible_history_rows(&history, "");
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[0].url, "https://api.example.com/items/9");
        assert_eq!(rows[0].status, "HTTP 201");

        let filtered = visible_history_rows(&history, "items/4");
        assert_eq!(
            filtered,
            vec![VisibleHistoryRow {
                id: 4,
                method: "GET".to_string(),
                url: "https://api.example.com/items/4".to_string(),
                status: "HTTP 200".to_string(),
            }]
        );

        assert!(visible_history_rows(&history, "missing").is_empty());
    }

    #[test]
    fn converts_codegen_request_to_collection_request() {
        let request = CodegenRequest {
            method: "POST".to_string(),
            url: "https://api.example.com/users?debug=true".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            query_params: vec![("debug".to_string(), "true".to_string())],
            body: RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"name\":\"Zen\"}".to_string(),
            },
        };

        let collection_request = collection_request_from_codegen(&request);

        assert_eq!(collection_request.name, "POST users");
        assert_eq!(collection_request.method, "POST");
        assert_eq!(collection_request.headers[0].name, "Content-Type");
        assert_eq!(collection_request.query_params[0].value, "true");
        assert!(matches!(
            collection_request.body,
            CollectionBody::Raw {
                ref content_type,
                ref body
            } if content_type == "application/json" && body == "{\"name\":\"Zen\"}"
        ));
    }

    #[test]
    fn collection_save_preserves_raw_request_and_pre_request_script() {
        let request = CodegenRequest {
            method: "GET".to_string(),
            url: "{{baseUrl}}/users".to_string(),
            headers: vec![("Accept".to_string(), "application/json".to_string())],
            query_params: Vec::new(),
            body: RequestBody::None,
        };
        let script =
            "set_method POST; set_header Authorization=Bearer {{token}}; set_query debug=true"
                .to_string();
        let tests = vec![ResponseAssertion {
            name: "status".to_string(),
            kind: ResponseAssertionKind::StatusEquals { status: 201 },
        }];

        let collection_request = collection_request_for_save(&request, script.clone(), tests);

        assert_eq!(collection_request.method, "GET");
        assert_eq!(collection_request.url, "{{baseUrl}}/users");
        assert_eq!(collection_request.headers.len(), 1);
        assert_eq!(collection_request.pre_request_script, script);
        assert_eq!(collection_request.tests.len(), 1);
    }

    #[test]
    fn maps_collection_content_types_to_raw_body_format() {
        assert!(matches!(
            raw_format_from_content_type("application/vnd.api+json"),
            RawBodyFormat::Json
        ));
        assert!(matches!(
            raw_format_from_content_type("application/xml"),
            RawBodyFormat::Xml
        ));
        assert!(matches!(
            raw_format_from_content_type("text/html; charset=utf-8"),
            RawBodyFormat::Html
        ));
        assert!(matches!(
            raw_format_from_content_type("text/plain"),
            RawBodyFormat::Text
        ));
    }

    #[test]
    fn highlights_json_raw_body_tokens() {
        let input = r#"{"name":"Zen","active":true,"count":42,"empty":null}"#;
        let highlights = syntax_highlights(input, RawBodyFormat::Json);

        assert!(highlights.contains(&(0..1, SyntaxTokenKind::Punctuation)));
        assert!(highlights.contains(&(1..7, SyntaxTokenKind::String)));
        assert!(highlights.contains(&(23..27, SyntaxTokenKind::Keyword)));
        assert!(highlights.contains(&(36..38, SyntaxTokenKind::Number)));
        assert!(highlights.contains(&(47..51, SyntaxTokenKind::Keyword)));
    }

    #[test]
    fn highlights_markup_raw_body_tokens() {
        let input = r#"<user id="42" active='true'>Zen</user>"#;
        let highlights = syntax_highlights(input, RawBodyFormat::Html);

        assert!(highlights.contains(&(0..1, SyntaxTokenKind::Punctuation)));
        assert!(highlights.contains(&(1..5, SyntaxTokenKind::Tag)));
        assert!(highlights.contains(&(6..8, SyntaxTokenKind::Attribute)));
        assert!(highlights.contains(&(9..13, SyntaxTokenKind::String)));
        assert!(highlights.contains(&(14..20, SyntaxTokenKind::Attribute)));
        assert!(highlights.contains(&(21..27, SyntaxTokenKind::String)));
        assert!(highlights.contains(&(33..37, SyntaxTokenKind::Tag)));
    }

    #[test]
    fn counts_collection_requests_recursively() {
        let items = vec![CollectionItem::Folder(CollectionFolder {
            name: "Users".to_string(),
            description: String::new(),
            items: vec![
                CollectionItem::Request(CollectionRequest {
                    name: "List".to_string(),
                    method: "GET".to_string(),
                    url: "https://api.example.com/users".to_string(),
                    headers: Vec::new(),
                    query_params: Vec::new(),
                    body: CollectionBody::None,
                    pre_request_script: String::new(),
                    tests: Vec::new(),
                }),
                CollectionItem::Request(CollectionRequest {
                    name: "Create".to_string(),
                    method: "POST".to_string(),
                    url: "https://api.example.com/users".to_string(),
                    headers: Vec::new(),
                    query_params: Vec::new(),
                    body: CollectionBody::None,
                    pre_request_script: String::new(),
                    tests: Vec::new(),
                }),
            ],
        })];

        assert_eq!(collection_item_count(&items), 2);
    }

    #[test]
    fn parses_collection_node_ids() {
        assert_eq!(collection_node_indices("collection"), Some(Vec::new()));
        assert_eq!(collection_node_indices("collection/0/2"), Some(vec![0, 2]));
        assert_eq!(collection_node_indices("routes/0"), None);
    }

    #[test]
    fn mutates_collection_items_for_context_menu_actions() {
        let mut collection = ApiCollection {
            name: "Demo".to_string(),
            description: String::new(),
            items: vec![CollectionItem::Folder(CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: Vec::new(),
            })],
        };

        assert!(insert_collection_item(
            &mut collection.items,
            "collection/0",
            CollectionItem::Request(blank_collection_request())
        ));
        assert_eq!(collection_item_count(&collection.items), 1);

        assert!(rename_collection_node(
            &mut collection,
            "collection/0/0",
            "List users"
        ));
        let CollectionItem::Folder(folder) = &collection.items[0] else {
            panic!("expected folder");
        };
        let CollectionItem::Request(request) = &folder.items[0] else {
            panic!("expected request");
        };
        assert_eq!(request.name, "List users");

        assert!(duplicate_collection_item(
            &mut collection.items,
            "collection/0/0"
        ));
        let CollectionItem::Folder(folder) = &collection.items[0] else {
            panic!("expected folder");
        };
        assert_eq!(folder.items.len(), 2);

        assert!(remove_collection_item(&mut collection.items, "collection/0/1").is_some());
        assert_eq!(collection_item_count(&collection.items), 1);
    }

    #[test]
    fn moves_collection_items_for_drag_and_drop() {
        let mut items = vec![
            CollectionItem::Folder(CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: Vec::new(),
            }),
            CollectionItem::Request(CollectionRequest {
                name: "List users".to_string(),
                method: "GET".to_string(),
                url: "https://api.example.com/users".to_string(),
                headers: Vec::new(),
                query_params: Vec::new(),
                body: CollectionBody::None,
                pre_request_script: String::new(),
                tests: Vec::new(),
            }),
            CollectionItem::Request(CollectionRequest {
                name: "Create user".to_string(),
                method: "POST".to_string(),
                url: "https://api.example.com/users".to_string(),
                headers: Vec::new(),
                query_params: Vec::new(),
                body: CollectionBody::None,
                pre_request_script: String::new(),
                tests: Vec::new(),
            }),
        ];

        assert!(move_collection_item(
            &mut items,
            "collection/1",
            "collection/0"
        ));
        let CollectionItem::Folder(folder) = &items[0] else {
            panic!("expected folder");
        };
        assert_eq!(folder.items.len(), 1);

        assert!(move_collection_item(
            &mut items,
            "collection/1",
            "collection/0/0"
        ));
        let CollectionItem::Folder(folder) = &items[0] else {
            panic!("expected folder");
        };
        assert_eq!(folder.items.len(), 2);
        let CollectionItem::Request(request) = &folder.items[1] else {
            panic!("expected request");
        };
        assert_eq!(request.name, "Create user");
    }

    #[test]
    fn rejects_moving_collection_folder_into_itself() {
        let mut items = vec![CollectionItem::Folder(CollectionFolder {
            name: "Users".to_string(),
            description: String::new(),
            items: vec![CollectionItem::Folder(CollectionFolder {
                name: "Nested".to_string(),
                description: String::new(),
                items: Vec::new(),
            })],
        })];

        assert!(!move_collection_item(
            &mut items,
            "collection/0",
            "collection/0/0"
        ));
        assert_eq!(collection_item_count(&items), 0);
    }

    #[test]
    fn ui_metrics_keep_collection_tree_and_editors_aligned() {
        assert_eq!(collection_tree_indent(0), 8.);
        assert_eq!(collection_tree_indent(1), 22.);
        assert_eq!(collection_tree_indent(2), 36.);
        assert_eq!(
            collection_tree_row_height(CollectionNodeKind::Root),
            collection_tree_row_height(CollectionNodeKind::Folder)
        );
        assert!(
            collection_tree_row_height(CollectionNodeKind::Request)
                > collection_tree_row_height(CollectionNodeKind::Folder)
        );
        assert_eq!(COLLECTION_TREE_MARKER_WIDTH, 14.);
        assert_eq!(HTTP_METHOD_LABEL_WIDTH, 58.);
        assert_eq!(KEY_VALUE_KEY_COLUMN_WIDTH, 150.);
        assert_eq!(
            TEST_ASSERTION_NAME_COLUMN_WIDTH,
            TEST_ASSERTION_KIND_COLUMN_WIDTH
        );
        assert_ne!(UI_COLOR_SURFACE, UI_COLOR_SURFACE_MUTED);
        assert_ne!(UI_COLOR_BORDER, UI_COLOR_BORDER_STRONG);
        assert_ne!(UI_COLOR_ACCENT, UI_COLOR_ACCENT_SURFACE);
    }

    #[derive(Debug, PartialEq, Eq)]
    struct VisibleCollectionRow {
        depth: usize,
        kind: &'static str,
        label: String,
        method: Option<String>,
    }

    fn visible_collection_rows(
        collection: &ApiCollection,
        expanded_nodes: &[String],
    ) -> Vec<VisibleCollectionRow> {
        let mut rows = vec![VisibleCollectionRow {
            depth: 0,
            kind: "collection",
            label: collection.name.clone(),
            method: None,
        }];

        if expanded_nodes.iter().any(|node| node == "collection") {
            append_visible_collection_rows(
                &mut rows,
                &collection.items,
                "collection",
                1,
                expanded_nodes,
            );
        }

        rows
    }

    fn append_visible_collection_rows(
        rows: &mut Vec<VisibleCollectionRow>,
        items: &[CollectionItem],
        parent_id: &str,
        depth: usize,
        expanded_nodes: &[String],
    ) {
        for (index, item) in items.iter().enumerate() {
            let id = format!("{parent_id}/{index}");
            match item {
                CollectionItem::Folder(folder) => {
                    rows.push(VisibleCollectionRow {
                        depth,
                        kind: "folder",
                        label: folder.name.clone(),
                        method: None,
                    });
                    if expanded_nodes.iter().any(|node| node == &id) {
                        append_visible_collection_rows(
                            rows,
                            &folder.items,
                            &id,
                            depth + 1,
                            expanded_nodes,
                        );
                    }
                }
                CollectionItem::Request(request) => rows.push(VisibleCollectionRow {
                    depth,
                    kind: "request",
                    label: request.name.clone(),
                    method: Some(request.method.clone()),
                }),
            }
        }
    }

    #[test]
    fn postman_collection_import_projects_to_visible_tree_rows() {
        let input = r#"
{
  "info": {
    "name": "Postman Demo",
    "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
  },
  "item": [
    {
      "name": "Users",
      "item": [
        {
          "name": "List users",
          "request": {
            "method": "GET",
            "url": "https://api.example.com/users"
          }
        },
        {
          "name": "Create user",
          "request": {
            "method": "POST",
            "url": "https://api.example.com/users"
          }
        }
      ]
    }
  ]
}
"#;
        let collection = ApiCollection::from_postman_json(input).expect("postman collection");

        let rows = visible_collection_rows(
            &collection,
            &["collection".to_string(), "collection/0".to_string()],
        );

        assert_eq!(
            rows,
            vec![
                VisibleCollectionRow {
                    depth: 0,
                    kind: "collection",
                    label: "Postman Demo".to_string(),
                    method: None,
                },
                VisibleCollectionRow {
                    depth: 1,
                    kind: "folder",
                    label: "Users".to_string(),
                    method: None,
                },
                VisibleCollectionRow {
                    depth: 2,
                    kind: "request",
                    label: "List users".to_string(),
                    method: Some("GET".to_string()),
                },
                VisibleCollectionRow {
                    depth: 2,
                    kind: "request",
                    label: "Create user".to_string(),
                    method: Some("POST".to_string()),
                },
            ]
        );
    }
}

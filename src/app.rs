mod input;

use std::{path::Path, sync::Arc};

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, Entity, FontWeight, Hsla, MouseButton, MouseDownEvent,
    MouseUpEvent, Render, SharedString, Window, WindowBounds, WindowOptions, div, px, rgb, size,
};
use tokio::{
    runtime::Runtime,
    sync::{mpsc, oneshot},
};
use zenapi::{
    client::{self, RequestBody},
    codegen::{CodegenRequest, SnippetLanguage, generate_snippet},
    collections::{
        ApiCollection, CollectionBody, CollectionFolder, CollectionItem, CollectionRequest,
        NameValue,
    },
    history::{HistoryRequest, HistoryResponse, RequestHistory},
    mock_server::{MockRequestLog, MockServer},
    openapi::{ApiRoute, ApiSpec, load_openapi_file},
    variables::{Variable, VariableStore, replace_variables},
};

use self::input::{TextAccepted, TextChanged, TextInput, bind_text_input_keys};

pub fn run() -> Result<()> {
    let runtime = Arc::new(Runtime::new()?);

    gpui_platform::application().run(move |cx: &mut App| {
        bind_text_input_keys(cx);

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
    active_environment: EnvironmentSelection,
    global_variables: Vec<KeyValueRow>,
    environment_variables: Vec<KeyValueRow>,
    query_params: Vec<KeyValueRow>,
    request_headers: Vec<KeyValueRow>,
    auth_mode: AuthMode,
    bearer_token: Entity<TextInput>,
    basic_username: Entity<TextInput>,
    basic_password: Entity<TextInput>,
    api_key_name: Entity<TextInput>,
    api_key_value: Entity<TextInput>,
    api_key_placement: ApiKeyPlacement,
    request_body_mode: RequestBodyMode,
    raw_body_format: RawBodyFormat,
    request_body: Entity<TextInput>,
    form_data_body: Vec<KeyValueRow>,
    urlencoded_body: Vec<KeyValueRow>,
    binary_body_path: Entity<TextInput>,
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
    codegen_language: SnippetLanguage,
    codegen_menu_open: bool,
    server: Option<MockServer>,
    server_running: bool,
    server_status: String,
    mock_logs: Vec<MockRequestLog>,
    history: RequestHistory,
    history_query: String,
    busy: bool,
}

struct KeyValueRow {
    key: Entity<TextInput>,
    value: Entity<TextInput>,
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

impl ZenApiApp {
    fn new(runtime: Arc<Runtime>, cx: &mut Context<Self>) -> Self {
        let import_path = cx.new(|cx| TextInput::new(cx, "OpenAPI / Swagger file path", true));
        let collection_path = cx.new(|cx| TextInput::new(cx, "Collection JSON path", true));
        let collection_rename_input = cx.new(|cx| TextInput::new(cx, "Collection item name", true));
        let route_filter =
            cx.new(|cx| TextInput::new(cx, "Filter method, path, or summary", false));
        let history_filter = cx.new(|cx| TextInput::new(cx, "Filter history", false));
        let url = cx.new(|cx| TextInput::new(cx, "Request URL", true));
        let global_variables = key_value_rows(
            cx,
            &[
                ("baseUrl", "https://api.example.com"),
                ("token", "secret"),
                ("", ""),
            ],
        );
        let environment_variables = key_value_rows(
            cx,
            &[
                ("baseUrl", "http://localhost:8080"),
                ("token", "dev-token"),
                ("", ""),
            ],
        );
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
        let api_key_name = cx.new(|cx| TextInput::new(cx, "X-API-Key", true));
        let api_key_value = cx.new(|cx| TextInput::new(cx, "API key value", true));
        let request_body = cx.new(|cx| TextInput::new(cx, "JSON body", true));
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

        Self {
            runtime,
            import_path,
            collection_path,
            collection_rename_input,
            route_filter,
            history_filter,
            url,
            active_environment: EnvironmentSelection::None,
            global_variables,
            environment_variables,
            query_params,
            request_headers,
            auth_mode: AuthMode::None,
            bearer_token,
            basic_username,
            basic_password,
            api_key_name,
            api_key_value,
            api_key_placement: ApiKeyPlacement::Header,
            request_body_mode: RequestBodyMode::None,
            raw_body_format: RawBodyFormat::Json,
            request_body,
            form_data_body,
            urlencoded_body,
            binary_body_path,
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
            response_body: "Import an OpenAPI or Swagger document to begin.".to_string(),
            response_raw_body: "Import an OpenAPI or Swagger document to begin.".to_string(),
            response_headers: String::new(),
            response_view: ResponseView::Pretty,
            codegen_language: SnippetLanguage::Curl,
            codegen_menu_open: false,
            server: None,
            server_running: false,
            server_status: "Mock stopped".to_string(),
            mock_logs: Vec::new(),
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
        let request = match self.current_codegen_request(cx) {
            Ok(request) if !request.url.is_empty() => request,
            Ok(_) => {
                self.set_response(
                    "Save needs URL",
                    "",
                    ResponseTone::Error,
                    "Enter a request URL before saving to the collection.",
                );
                cx.notify();
                return;
            }
            Err(error) => {
                self.set_response("Save failed", "", ResponseTone::Error, error.to_string());
                cx.notify();
                return;
            }
        };

        let collection_request = collection_request_from_codegen(&request);
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
                self.request_body_mode = RequestBodyMode::Raw;
                self.raw_body_format = raw_format_from_content_type(&content_type);
                self.request_body
                    .update(cx, |input, cx| input.set_text(body, cx));
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

        let request = match self.current_codegen_request(cx) {
            Ok(request) => request,
            Err(error) => {
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
                            let response_meta =
                                format_response_meta(response.elapsed_ms, response.body_bytes);
                            let history_response = HistoryResponse {
                                status: response_status.clone(),
                                meta: response_meta.clone(),
                                body_preview: preview_text(&response.body),
                            };
                            let headers = format_headers(&response.headers);
                            app.record_history(history_request.clone(), history_response);
                            app.set_http_response(
                                response_status,
                                response_meta,
                                response_tone(response.status),
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
        self.response_headers = headers.into();
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

    fn current_codegen_request(&self, cx: &mut Context<Self>) -> Result<CodegenRequest> {
        let variable_store = self.variable_store(cx);
        let active_environment = self.active_environment.name();
        let mut headers = resolve_key_value_pairs(
            read_key_value_rows(&self.request_headers, cx),
            &variable_store,
            active_environment,
        )?;
        let mut query_params = resolve_key_value_pairs(
            read_key_value_rows(&self.query_params, cx),
            &variable_store,
            active_environment,
        )?;
        let (auth_headers, auth_query_params) = self.auth_pairs(cx);
        headers.extend(resolve_key_value_pairs(
            auth_headers,
            &variable_store,
            active_environment,
        )?);
        query_params.extend(resolve_key_value_pairs(
            auth_query_params,
            &variable_store,
            active_environment,
        )?);

        Ok(CodegenRequest {
            method: self.method.clone(),
            url: resolve_template(
                &self.url.read(cx).text(),
                &variable_store,
                active_environment,
            )?
            .trim()
            .to_string(),
            headers,
            query_params,
            body: resolve_request_body(
                self.request_body_for_send(cx),
                &variable_store,
                active_environment,
            )?,
        })
    }

    fn variable_store(&self, cx: &mut Context<Self>) -> VariableStore {
        variable_store_from_pairs(
            read_key_value_rows(&self.global_variables, cx),
            self.active_environment.name(),
            read_key_value_rows(&self.environment_variables, cx),
        )
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
            RequestBodyMode::Binary => RequestBody::BinaryFile {
                path: self.binary_body_path.read(cx).text(),
                content_type: Some("application/octet-stream".to_string()),
            },
        }
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
                rgb(0xd1d5db).into()
            })
            .bg(if active { rgb(0xf9fafb) } else { rgb(0xffffff) })
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
            .border_color(rgb(0xe5e7eb))
            .bg(rgb(0xf9fafb))
            .px_3()
            .gap_3()
            .child(
                div()
                    .w(px(230.))
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(15.))
                    .text_color(rgb(0x111827))
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
                    .text_color(rgb(0x6b7280))
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
                        rgb(0x6b7280).into()
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
        let rows = self
            .history
            .filtered(&self.history_query)
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
            .child(div().flex().flex_col().gap_1().children(rows).when(
                self.history.entries().is_empty(),
                |list| {
                    list.child(
                        div()
                            .h(px(34.))
                            .flex()
                            .items_center()
                            .text_color(rgb(0x9ca3af))
                            .text_size(px(13.))
                            .child("No request history"),
                    )
                },
            ))
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
                rgb(0x2563eb)
            } else {
                rgb(0xf9fafb)
            })
            .bg(if selected {
                rgb(0xeff6ff)
            } else {
                rgb(0xf9fafb)
            })
            .px_2()
            .py_1()
            .cursor_pointer()
            .hover(|row| if selected { row } else { row.bg(rgb(0xf3f4f6)) })
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
                            .w(px(58.))
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
                            .text_color(rgb(0x111827))
                            .font_family("monospace")
                            .child(path),
                    ),
            )
            .child(
                div()
                    .ml(px(66.))
                    .truncate()
                    .text_size(px(12.))
                    .text_color(rgb(0x6b7280))
                    .child(summary),
            )
    }

    fn render_workspace(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .bg(rgb(0xffffff))
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
            .border_color(rgb(0xe5e7eb))
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
            .border_color(rgb(0xe5e7eb))
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
                    .child(key_value_editor("Headers", &self.request_headers))
                    .child(self.render_auth_panel(cx))
                    .child(self.render_body_panel(cx))
                    .child(self.render_codegen_panel(cx))
                    .child(self.render_mock_log()),
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
                    .text_color(rgb(0x6b7280))
                    .child("Mock Log"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(rgb(0xe5e7eb))
                    .bg(rgb(0xffffff))
                    .children(rows)
                    .when(self.mock_logs.is_empty(), |list| {
                        list.child(
                            div()
                                .h(px(34.))
                                .flex()
                                .items_center()
                                .px_2()
                                .text_color(rgb(0x9ca3af))
                                .text_size(px(13.))
                                .child("No mock requests"),
                        )
                    }),
            )
    }

    fn render_variables_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0x6b7280))
                    .child("Variables"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.environment_button("No Env", EnvironmentSelection::None, cx))
                    .child(self.environment_button("dev", EnvironmentSelection::Dev, cx))
                    .child(self.environment_button("test", EnvironmentSelection::Test, cx))
                    .child(self.environment_button("prod", EnvironmentSelection::Prod, cx))
                    .child(
                        div()
                            .ml_2()
                            .truncate()
                            .text_size(px(12.))
                            .font_family("monospace")
                            .text_color(rgb(0x6b7280))
                            .child(format!("active: {}", self.active_environment.label())),
                    ),
            )
            .child(key_value_editor("Global Variables", &self.global_variables))
            .when(
                self.active_environment != EnvironmentSelection::None,
                |panel| {
                    panel.child(key_value_editor(
                        "Environment Variables",
                        &self.environment_variables,
                    ))
                },
            )
    }

    fn environment_button(
        &self,
        label: &'static str,
        environment: EnvironmentSelection,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.active_environment == environment;
        compact_toggle(label, active)
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.active_environment = environment;
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
                    .text_color(rgb(0x6b7280))
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
                        .child(div().w(px(150.)).child(self.basic_username.clone()))
                        .child(div().flex_1().child(self.basic_password.clone())),
                )
            })
            .when(self.auth_mode == AuthMode::ApiKey, |panel| {
                panel
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().w(px(150.)).child(self.api_key_name.clone()))
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

    fn render_body_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0x6b7280))
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
            })
            .when(self.request_body_mode == RequestBodyMode::Binary, |panel| {
                panel.child(self.binary_body_path.clone())
            })
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
                            .text_color(rgb(0x6b7280))
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
                                    .border_color(rgb(0xd1d5db))
                                    .bg(rgb(0xffffff))
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x374151))
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
                    .border_color(rgb(0xe5e7eb))
                    .bg(rgb(0xffffff))
                    .p_3()
                    .font_family("monospace")
                    .line_height(px(18.))
                    .text_size(px(12.))
                    .text_color(rgb(0x111827))
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
                        .border_color(rgb(0xd1d5db))
                        .bg(rgb(0xffffff))
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
            .text_color(if active { rgb(0x2563eb) } else { rgb(0x374151) })
            .hover(|row| row.bg(rgb(0xf3f4f6)))
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
                    .font_family("monospace")
                    .line_height(px(20.))
                    .text_size(px(13.))
                    .text_color(rgb(0x111827))
                    .whitespace_normal()
                    .child(body),
            )
    }

    fn render_response_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .h(px(36.))
            .border_b_1()
            .border_color(rgb(0xe5e7eb))
            .bg(rgb(0xf9fafb))
            .px_3()
            .gap_2()
            .child(self.response_tab("Pretty", ResponseView::Pretty, cx))
            .child(self.response_tab("Raw", ResponseView::Raw, cx))
            .child(self.response_tab("Headers", ResponseView::Headers, cx))
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
            .border_color(if active { rgb(0x2563eb) } else { rgb(0xd1d5db) })
            .bg(if active { rgb(0xffffff) } else { rgb(0xf9fafb) })
            .text_size(px(12.))
            .font_weight(FontWeight::BOLD)
            .text_color(if active { rgb(0x2563eb) } else { rgb(0x6b7280) })
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
            ResponseView::Pretty => self.response_body.clone(),
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
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .text_color(rgb(0x111827))
            .bg(rgb(0xffffff))
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
            .border_color(rgb(0x2563eb))
            .bg(rgb(0xeff6ff))
            .px_2()
            .text_size(px(12.))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(0x1d4ed8))
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
            .border_color(rgb(0xe5e7eb))
            .bg(rgb(0xf9fafb))
            .px_3()
            .text_size(px(12.))
            .text_color(rgb(0x6b7280))
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
                            .font_family("monospace")
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
                background: rgb(0xf3f4f6).into(),
                border: rgb(0xd1d5db).into(),
                text: rgb(0x9ca3af).into(),
            };
        }

        match self {
            Self::Neutral => ButtonColors {
                background: rgb(0xffffff).into(),
                border: rgb(0xd1d5db).into(),
                text: rgb(0x374151).into(),
            },
            Self::Primary => ButtonColors {
                background: rgb(0x2563eb).into(),
                border: rgb(0x1d4ed8).into(),
                text: rgb(0xffffff).into(),
            },
            Self::Warning => ButtonColors {
                background: rgb(0xb45309).into(),
                border: rgb(0x92400e).into(),
                text: rgb(0xffffff).into(),
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
    ApiKey,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ApiKeyPlacement {
    Header,
    Query,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EnvironmentSelection {
    None,
    Dev,
    Test,
    Prod,
}

impl EnvironmentSelection {
    fn name(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Dev => Some("dev"),
            Self::Test => Some("test"),
            Self::Prod => Some("prod"),
        }
    }

    fn label(self) -> &'static str {
        self.name().unwrap_or("none")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RequestBodyMode {
    None,
    FormData,
    UrlEncoded,
    Raw,
    Binary,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RawBodyFormat {
    Json,
    Xml,
    Text,
    Html,
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
            Self::Neutral => rgb(0x6b7280).into(),
            Self::Busy => rgb(0xd97706).into(),
            Self::Success => rgb(0x059669).into(),
            Self::Error => rgb(0xdc2626).into(),
        }
    }
}

fn key_value_rows(cx: &mut Context<ZenApiApp>, specs: &[(&str, &str)]) -> Vec<KeyValueRow> {
    specs
        .iter()
        .map(|(key_placeholder, value_placeholder)| KeyValueRow {
            key: cx.new(|cx| TextInput::new(cx, *key_placeholder, true)),
            value: cx.new(|cx| TextInput::new(cx, *value_placeholder, true)),
        })
        .collect()
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

fn resolve_template(
    input: &str,
    store: &VariableStore,
    active_environment: Option<&str>,
) -> Result<String> {
    replace_variables(input, store, active_environment)
}

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
    }
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

fn blank_collection_request() -> CollectionRequest {
    CollectionRequest {
        name: "New Request".to_string(),
        method: "GET".to_string(),
        url: "https://api.example.com/request".to_string(),
        headers: Vec::new(),
        query_params: Vec::new(),
        body: CollectionBody::None,
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
                .text_color(rgb(0x6b7280))
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
                .text_color(rgb(0x9ca3af))
                .child(div().w(px(150.)).child("Key"))
                .child(div().flex_1().child("Value")),
        )
        .child(div().flex().flex_col().gap_1().children(rendered_rows))
}

fn key_value_row(key: Entity<TextInput>, value: Entity<TextInput>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(div().w(px(150.)).child(key))
        .child(div().flex_1().child(value))
}

fn compact_toggle(label: &'static str, active: bool) -> gpui::Div {
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
        .border_color(if active { rgb(0x2563eb) } else { rgb(0xd1d5db) })
        .bg(if active { rgb(0xffffff) } else { rgb(0xf9fafb) })
        .text_size(px(12.))
        .font_weight(FontWeight::BOLD)
        .text_color(if active { rgb(0x2563eb) } else { rgb(0x6b7280) })
        .cursor_pointer()
}

fn bearer_auth_pair(token: &str) -> Option<(String, String)> {
    let token = token.trim();
    (!token.is_empty()).then(|| ("Authorization".to_string(), format!("Bearer {token}")))
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
        .border_color(rgb(0xe5e7eb))
        .bg(rgb(0xffffff))
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
                        .text_color(rgb(0x111827))
                        .child(title.clone()),
                )
                .child(
                    div()
                        .w(px(260.))
                        .truncate()
                        .text_right()
                        .font_family("monospace")
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tone.color())
                        .child(meta.unwrap_or("").to_string()),
                ),
        )
        .child(div().ml(px(12.)).h(px(2.)).w(px(80.)).bg(rgb(0x2563eb)))
}

fn mock_log_row(method: String, path: String, status: u16) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .h(px(30.))
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(rgb(0xf3f4f6))
        .font_family("monospace")
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
                .text_color(rgb(0x374151))
                .child(path),
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
        .hover(|row| row.bg(rgb(0xf3f4f6)))
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
                                .w(px(58.))
                                .text_size(px(12.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(method_color(&method))
                                .child(method),
                        )
                        .child(
                            div()
                                .flex_1()
                                .truncate()
                                .font_family("monospace")
                                .text_size(px(12.))
                                .text_color(rgb(0x111827))
                                .child(url),
                        ),
                )
                .child(
                    div()
                        .ml(px(66.))
                        .truncate()
                        .font_family("monospace")
                        .text_size(px(11.))
                        .text_color(rgb(0x6b7280))
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
                .border_color(rgb(0xd1d5db))
                .bg(rgb(0xffffff))
                .text_size(px(11.))
                .text_color(rgb(0x6b7280))
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
        .h(px(30.))
        .rounded(px(4.))
        .px_2()
        .gap_2()
        .text_size(px(12.))
        .cursor_pointer()
        .hover(|row| row.bg(rgb(0xf3f4f6)))
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
        .drag_over::<DraggedCollectionNode>(|row, _dragged, _window, _cx| row.bg(rgb(0xeff6ff)))
        .on_drop(
            cx.listener(|app, dragged: &DraggedCollectionNode, _window, cx| {
                app.move_collection_target(dragged.node_id.clone(), "collection".to_string(), cx);
            }),
        )
        .child(
            div()
                .w(px(14.))
                .font_family("monospace")
                .text_color(rgb(0x6b7280))
                .child(marker),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(0x111827))
                .child(name),
        )
        .child(
            div()
                .font_family("monospace")
                .text_color(rgb(0x9ca3af))
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
        .h(px(30.))
        .rounded(px(4.))
        .pl(px(8. + depth as f32 * 14.))
        .pr_2()
        .gap_2()
        .text_size(px(12.))
        .cursor_pointer()
        .hover(|row| row.bg(rgb(0xf3f4f6)))
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
        .drag_over::<DraggedCollectionNode>(|row, _dragged, _window, _cx| row.bg(rgb(0xeff6ff)))
        .on_drop(
            cx.listener(move |app, dragged: &DraggedCollectionNode, _window, cx| {
                app.move_collection_target(dragged.node_id.clone(), drop_id.clone(), cx);
            }),
        )
        .child(
            div()
                .w(px(14.))
                .font_family("monospace")
                .text_color(rgb(0x6b7280))
                .child(marker),
        )
        .child(
            div()
                .flex_1()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(0x374151))
                .child(folder.name.clone()),
        )
        .child(
            div()
                .font_family("monospace")
                .text_color(rgb(0x9ca3af))
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
        .h(px(36.))
        .rounded(px(4.))
        .pl(px(8. + depth as f32 * 14.))
        .pr_2()
        .gap_2()
        .cursor_pointer()
        .hover(|row| row.bg(rgb(0xf3f4f6)))
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
        .drag_over::<DraggedCollectionNode>(|row, _dragged, _window, _cx| row.bg(rgb(0xeff6ff)))
        .on_drop(
            cx.listener(move |app, dragged: &DraggedCollectionNode, _window, cx| {
                app.move_collection_target(dragged.node_id.clone(), drop_id.clone(), cx);
            }),
        )
        .child(
            div()
                .w(px(58.))
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
                        .text_color(rgb(0x111827))
                        .child(name),
                )
                .child(
                    div()
                        .truncate()
                        .font_family("monospace")
                        .text_size(px(11.))
                        .text_color(rgb(0x6b7280))
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
            api_key_pair("X-API-Key", " key "),
            Some(("X-API-Key".to_string(), "key".to_string()))
        );
        assert_eq!(bearer_auth_pair(" "), None);
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
                }),
                CollectionItem::Request(CollectionRequest {
                    name: "Create".to_string(),
                    method: "POST".to_string(),
                    url: "https://api.example.com/users".to_string(),
                    headers: Vec::new(),
                    query_params: Vec::new(),
                    body: CollectionBody::None,
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
            }),
            CollectionItem::Request(CollectionRequest {
                name: "Create user".to_string(),
                method: "POST".to_string(),
                url: "https://api.example.com/users".to_string(),
                headers: Vec::new(),
                query_params: Vec::new(),
                body: CollectionBody::None,
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
}

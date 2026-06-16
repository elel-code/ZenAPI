use anyhow::{Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use zenapi::{
    client::{self, RequestBody},
    codegen::{CodegenRequest, SnippetLanguage, generate_snippet},
    mock_server::MockServer,
    openapi::{ApiRoute, ApiSpec, load_openapi_file},
    variables::{Variable, VariableStore, replace_variables},
};

use crate::ui::{AppWindow, RouteRow};

pub fn run() -> Result<()> {
    let runtime = Arc::new(Runtime::new()?);
    let state = Arc::new(Mutex::new(AppState::default()));
    let app = AppWindow::new().map_err(|err| anyhow!(err.to_string()))?;

    wire_import(&app, runtime.clone(), state.clone());
    wire_route_filter(&app, state.clone());
    wire_route_selection(&app, state.clone());
    wire_request_sender(&app, runtime.clone());
    wire_codegen(&app);
    wire_mock_server(&app, runtime, state);

    app.run().map_err(|err| anyhow!(err.to_string()))
}

#[derive(Default)]
struct AppState {
    routes: Vec<ApiRoute>,
    visible_routes: Vec<ApiRoute>,
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

impl AppState {
    fn next_server_action(&mut self) -> ServerAction {
        if let Some(server) = self.server.take() {
            ServerAction::Stop(server)
        } else {
            ServerAction::Start(self.routes.clone())
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

fn wire_request_sender(app: &AppWindow, runtime: Arc<Runtime>) {
    let weak_app = app.as_weak();
    app.on_send_request(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let request = match build_codegen_request_projection(&RequestProjectionInput {
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
        }) {
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
                        set_response(
                            &app,
                            &format!("HTTP {}", response.status),
                            &format!("{} ms / {} B", response.elapsed_ms, response.body_bytes),
                            response_tone(response.status),
                            &response.body,
                        );
                        app.set_response_headers(response_headers.into());
                    }
                    Err(error) => {
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

        let request = match build_codegen_request_projection(&RequestProjectionInput {
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
        }) {
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
}

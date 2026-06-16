use anyhow::{Result, anyhow};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use zenapi::{
    client,
    mock_server::MockServer,
    openapi::{ApiRoute, ApiSpec, load_openapi_file},
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

        let method = app.get_method().to_string();
        let url = app.get_url().trim().to_string();
        let body = app.get_request_body().to_string();
        if url.is_empty() {
            set_response(
                &app,
                "Request needs a URL",
                "",
                "error",
                "Enter a request URL or select an imported route first.",
            );
            return;
        }

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
            let result = client::send_request(&method, &url, &body).await;
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak_app.upgrade() else {
                    return;
                };
                match result {
                    Ok(response) => {
                        set_response(
                            &app,
                            &format!("HTTP {}", response.status),
                            &format!("{} ms / {} B", response.elapsed_ms, response.body_bytes),
                            response_tone(response.status),
                            &response.body,
                        );
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
}

use super::state::{AppState, ServerAction};
use crate::ui::{AppWindow, RouteRow};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use zenapi::{
    client::{self, pretty_body},
    mock_server::MockServer,
    openapi::{ApiRoute, ApiSpec, load_openapi_file},
};

pub(super) fn wire_import(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    app.on_import_openapi(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        let path = path.trim();
        if path.is_empty() {
            app.set_response_status("Import needs a file path".into());
            app.set_response_meta("".into());
            app.set_response_body("Enter a local OpenAPI or Swagger JSON/YAML file path.".into());
            return;
        }

        app.set_busy(true);
        match load_openapi_file(path) {
            Ok(spec) => {
                let routes = spec.routes.clone();
                app.set_routes(route_model(&routes));
                app.set_selected_route(-1);
                app.set_route_filter("".into());
                app.set_total_route_count(routes.len() as i32);
                app.set_response_status(format!("Imported {}", display_spec_name(&spec)).into());
                app.set_response_meta(format!("{} routes", routes.len()).into());
                app.set_response_body(format!("Ready: {} routes parsed.", routes.len()).into());
                app.set_server_status(if routes.is_empty() {
                    "No mock routes in imported spec".into()
                } else {
                    "Mock server ready".into()
                });

                if let Ok(mut state) = state.lock() {
                    state.routes = routes.clone();
                    state.visible_routes = routes;
                }
            }
            Err(error) => {
                app.set_response_status("Import failed".into());
                app.set_response_meta("".into());
                app.set_response_body(error.to_string().into());
            }
        }
        app.set_busy(false);
    });
}

pub(super) fn wire_route_filter(app: &AppWindow, state: Arc<Mutex<AppState>>) {
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
        app.set_response_status(if query.trim().is_empty() {
            "Filter cleared".into()
        } else {
            "Routes filtered".into()
        });
        app.set_response_meta(format!("{} visible", filtered.len()).into());
    });
}

pub(super) fn wire_route_selection(app: &AppWindow, state: Arc<Mutex<AppState>>) {
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
                "http://localhost:8080{}",
                route.path
            )));
            app.set_request_body(default_request_body(&app.get_method()).into());
            app.set_response_status("Route selected".into());
            app.set_response_meta(route.summary.into());
            app.set_response_body(pretty_json(&route.mock_body).into());
        }
    });
}

pub(super) fn wire_request_sender(app: &AppWindow, runtime: Arc<Runtime>) {
    let weak_app = app.as_weak();
    app.on_send_request(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        let method = app.get_method().to_string();
        let url = app.get_url().trim().to_string();
        let body = app.get_request_body().to_string();
        if url.is_empty() {
            app.set_response_status("Request needs a URL".into());
            app.set_response_meta("".into());
            app.set_response_body("Enter a request URL or select an imported route first.".into());
            return;
        }

        app.set_busy(true);
        app.set_response_status("Sending".into());
        app.set_response_meta("".into());

        let weak_app = app.as_weak();
        runtime.spawn(async move {
            let result = client::send_request(&method, &url, &body).await;
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak_app.upgrade() else {
                    return;
                };
                match result {
                    Ok(response) => {
                        app.set_response_status(format!("HTTP {}", response.status).into());
                        app.set_response_meta(format!("{} ms", response.elapsed_ms).into());
                        app.set_response_body(pretty_body(&response.body).into());
                    }
                    Err(error) => {
                        app.set_response_status("Request failed".into());
                        app.set_response_meta("".into());
                        app.set_response_body(error.to_string().into());
                    }
                }
                app.set_busy(false);
            });
        });
    });
}

pub(super) fn wire_mock_server(
    app: &AppWindow,
    runtime: Arc<Runtime>,
    state: Arc<Mutex<AppState>>,
) {
    let weak_app = app.as_weak();
    app.on_toggle_server(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        app.set_busy(true);
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
                            app.set_server_status("Mock server stopped".into());
                            app.set_busy(false);
                        }
                    });
                }
                ServerAction::Start(routes) => {
                    if routes.is_empty() {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = weak_app.upgrade() {
                                app.set_server_running(false);
                                app.set_server_status("Import routes before starting mock".into());
                                app.set_response_status("Mock needs routes".into());
                                app.set_response_meta("".into());
                                app.set_response_body(
                                    "Import an OpenAPI file before starting the mock server."
                                        .into(),
                                );
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
                                app.set_server_status(
                                    format!("Mock server at http://{addr}").into(),
                                );
                            }
                            Err(error) => {
                                app.set_server_running(false);
                                app.set_server_status("Mock server failed".into());
                                app.set_response_status("Mock server failed".into());
                                app.set_response_body(error.to_string().into());
                            }
                        }
                        app.set_busy(false);
                    });
                }
            }
        });
    });
}

fn display_spec_name(spec: &ApiSpec) -> String {
    if spec.version.is_empty() {
        spec.title.clone()
    } else {
        format!("{} {}", spec.title, spec.version)
    }
}

fn route_model(routes: &[ApiRoute]) -> ModelRc<RouteRow> {
    let rows = routes.iter().map(|route| RouteRow {
        method: route.method.clone().into(),
        path: route.path.clone().into(),
        summary: route.summary.clone().into(),
    });

    ModelRc::new(VecModel::from_iter(rows))
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
        "POST" | "PUT" | "PATCH" => "{\n  \n}",
        _ => "",
    }
}

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

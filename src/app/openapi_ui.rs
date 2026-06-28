use slint::ComponentHandle;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::runtime::Runtime;

use zenapi::openapi::{ApiRoute, ApiSpec, load_openapi_file};

use crate::ui::AppWindow;

use super::{
    AppState,
    mock_ui::{clear_selected_mock_route, route_model, set_selected_mock_route},
    request_editor_ui::{
        default_body_mode, default_request_body, refresh_body_field_rows, refresh_header_rows,
        refresh_query_param_rows,
    },
    response_format::pretty_json,
    set_response,
};

pub(super) fn display_spec_name(spec: &ApiSpec) -> String {
    if spec.version.is_empty() {
        spec.title.clone()
    } else {
        format!("{} {}", spec.title, spec.version)
    }
}

pub(super) fn display_spec_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

pub(super) fn filter_routes(routes: &[ApiRoute], query: &str) -> Vec<ApiRoute> {
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

pub(super) fn import_openapi_path(
    app: &AppWindow,
    runtime: Arc<Runtime>,
    state: Arc<Mutex<AppState>>,
    path: String,
) {
    if app.get_busy() {
        return;
    }

    let path = path.trim().to_string();
    if path.is_empty() {
        set_response(
            app,
            "Import needs a file path",
            "",
            "error",
            "Enter a local OpenAPI or Swagger JSON/YAML file path.",
        );
        return;
    }

    app.set_busy(true);
    app.set_activity("Importing OpenAPI spec".into());

    match load_openapi_file(&path) {
        Ok(spec) => {
            let routes = spec.routes.clone();
            let stopped_server = state.lock().ok().and_then(|mut state| {
                let stopped_server = state.server.take();
                state.routes = routes.clone();
                state.visible_routes = routes.clone();
                stopped_server
            });

            let spec_name = display_spec_name(&spec);
            let spec_label = display_spec_label(&path);
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
            set_response(app, "Import failed", "", "error", &error.to_string());
            app.set_activity("".into());
            app.set_busy(false);
        }
    }
}

pub(super) fn wire_openapi_actions(
    app: &AppWindow,
    runtime: Arc<Runtime>,
    state: Arc<Mutex<AppState>>,
) {
    wire_import(app, runtime, state.clone());
    wire_route_filter(app, state.clone());
    wire_route_selection(app, state);
}

fn wire_import(app: &AppWindow, runtime: Arc<Runtime>, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    app.on_import_openapi(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        import_openapi_path(&app, runtime.clone(), state.clone(), path.to_string());
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
            app.set_method(route.method.clone().into());
            app.set_url(format!("http://127.0.0.1:8080{}", route.path).into());
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

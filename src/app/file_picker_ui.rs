use slint::ComponentHandle;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

use crate::ui::AppWindow;

use super::{
    AppState,
    collection_file_ui::{
        export_postman_collection_path, import_collection_path, save_collection_path,
    },
    file_dialog::{default_dialog_file_name, pick_file_path, pick_save_path},
    openapi_ui::import_openapi_path,
    runner::normalize_runner_report_format,
};

pub(super) fn wire_file_pickers(
    app: &AppWindow,
    runtime: Arc<Runtime>,
    state: Arc<Mutex<AppState>>,
) {
    let weak_app = app.as_weak();
    let openapi_runtime = runtime.clone();
    let openapi_state = state.clone();
    app.on_pick_openapi_file(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        if let Some(path) = pick_file_path(
            "Open OpenAPI spec",
            app.get_file_path().as_str(),
            &[("OpenAPI", &["json", "yaml", "yml"])],
        ) {
            app.set_file_path(path.clone().into());
            import_openapi_path(&app, openapi_runtime.clone(), openapi_state.clone(), path);
        }
    });

    let weak_app = app.as_weak();
    let import_collection_state = state.clone();
    app.on_pick_collection_import_file(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        if let Some(path) = pick_file_path(
            "Open collection",
            app.get_collection_path().as_str(),
            &[("Collection JSON", &["json"])],
        ) {
            import_collection_path(&app, import_collection_state.clone(), path);
        }
    });

    let weak_app = app.as_weak();
    let save_collection_state = state.clone();
    app.on_pick_collection_save_path(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let current_path = app.get_collection_path().to_string();
        let default_name = default_dialog_file_name(&current_path, "zenapi-collection.json");
        if let Some(path) = pick_save_path(
            "Save collection",
            &current_path,
            &default_name,
            &[("ZenAPI collection", &["json"])],
        ) {
            save_collection_path(&app, save_collection_state.clone(), path);
        }
    });

    let weak_app = app.as_weak();
    let postman_collection_state = state.clone();
    app.on_pick_postman_collection_path(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let current_path = app.get_collection_path().to_string();
        let default_name = default_dialog_file_name(&current_path, "postman-collection.json");
        if let Some(path) = pick_save_path(
            "Export Postman collection",
            &current_path,
            &default_name,
            &[("Postman collection", &["json"])],
        ) {
            export_postman_collection_path(&app, postman_collection_state.clone(), path);
        }
    });

    let weak_app = app.as_weak();
    app.on_pick_query_param_import_file(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        if let Some(path) = pick_file_path(
            "Open query parameter file",
            app.get_query_param_import_path().as_str(),
            &[("Text", &["txt", "csv", "env"])],
        ) {
            app.set_query_param_import_path(path.into());
        }
    });

    let weak_app = app.as_weak();
    app.on_pick_header_import_file(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        if let Some(path) = pick_file_path(
            "Open header file",
            app.get_header_import_path().as_str(),
            &[("Text", &["txt", "csv", "env"])],
        ) {
            app.set_header_import_path(path.into());
        }
    });

    let weak_app = app.as_weak();
    app.on_pick_form_file(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        if let Some(path) = pick_file_path(
            "Attach multipart file",
            app.get_form_file_path().as_str(),
            &[],
        ) {
            app.set_form_file_path(path.into());
        }
    });

    let weak_app = app.as_weak();
    app.on_pick_grpc_descriptor_file(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() || app.get_grpc_streaming() {
            return;
        }

        if let Some(path) = pick_file_path(
            "Open gRPC descriptor or proto",
            app.get_grpc_descriptor_path().as_str(),
            &[("gRPC schema", &["protoset", "bin", "proto"])],
        ) {
            app.set_grpc_descriptor_path(path.into());
        }
    });

    let weak_app = app.as_weak();
    app.on_pick_codegen_path(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let current_path = app.get_codegen_path().to_string();
        let default_name = default_dialog_file_name(&current_path, "zenapi-snippet.txt");
        if let Some(path) = pick_save_path(
            "Save code snippet",
            &current_path,
            &default_name,
            &[("Text", &["txt"])],
        ) {
            app.set_codegen_path(path.into());
        }
    });

    let weak_app = app.as_weak();
    app.on_pick_runner_report_path(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() || app.get_runner_active() {
            return;
        }

        let current_path = app.get_runner_report_path().to_string();
        let fallback =
            if normalize_runner_report_format(app.get_runner_report_format().as_str()) == "json" {
                "zenapi-runner-report.json"
            } else {
                "zenapi-runner-report.txt"
            };
        let default_name = default_dialog_file_name(&current_path, fallback);
        let filters: &[(&str, &[&str])] = if fallback.ends_with(".json") {
            &[("JSON", &["json"])]
        } else {
            &[("Text", &["txt"])]
        };
        if let Some(path) =
            pick_save_path("Save runner report", &current_path, &default_name, filters)
        {
            app.set_runner_report_path(path.into());
        }
    });

    let weak_app = app.as_weak();
    app.on_pick_mock_log_path(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let current_path = app.get_mock_log_path().to_string();
        let default_name = default_dialog_file_name(&current_path, "mock-logs.json");
        if let Some(path) = pick_save_path(
            "Save mock logs",
            &current_path,
            &default_name,
            &[("JSON", &["json"])],
        ) {
            app.set_mock_log_path(path.into());
        }
    });
}

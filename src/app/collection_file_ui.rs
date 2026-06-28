use std::sync::{Arc, Mutex};

use slint::ComponentHandle;
use zenapi::collections::ApiCollection;

use crate::ui::AppWindow;

use super::{
    AppState,
    collection_tree::{collection_model, count_collection_requests},
    set_response,
};

pub(super) fn import_collection_path(app: &AppWindow, state: Arc<Mutex<AppState>>, path: String) {
    if app.get_busy() {
        return;
    }

    let path = path.trim().to_string();
    if path.is_empty() {
        app.set_collection_status("Collection path required".into());
        set_response(
            app,
            "Collection import failed",
            "",
            "error",
            "Enter a local native or Postman collection JSON file path.",
        );
        return;
    }

    match ApiCollection::load_file(&path) {
        Ok(collection) => {
            let name = collection.name.clone();
            let request_count = count_collection_requests(&collection.items);
            let rows = collection_model(&collection);
            if let Ok(mut state) = state.lock() {
                state.collection = collection;
            }
            app.set_collection_path(path.clone().into());
            app.set_collection_name(name.clone().into());
            app.set_collection_rows(rows);
            app.set_collection_status(format!("Loaded {request_count} requests").into());
            app.set_selected_collection_request(-1);
            app.set_selected_collection_folder("".into());
            app.set_collection_move_target_label(name.clone().into());
            app.set_collection_request_name("".into());
            set_response(
                app,
                "Collection loaded",
                &path,
                "success",
                &format!("{name}\n{request_count} requests"),
            );
        }
        Err(error) => {
            app.set_collection_status("Load failed".into());
            set_response(
                app,
                "Collection import failed",
                &path,
                "error",
                &error.to_string(),
            );
        }
    }
}

pub(super) fn wire_collection_file_actions(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    let import_state = state.clone();
    app.on_import_collection(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        import_collection_path(&app, import_state.clone(), path.to_string());
    });

    let weak_app = app.as_weak();
    let save_state = state.clone();
    app.on_save_collection(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        save_collection_path(&app, save_state.clone(), path.to_string());
    });

    let weak_app = app.as_weak();
    let export_state = state.clone();
    app.on_export_postman_collection(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        export_postman_collection_path(&app, export_state.clone(), path.to_string());
    });
}

pub(super) fn save_collection_path(app: &AppWindow, state: Arc<Mutex<AppState>>, path: String) {
    if app.get_busy() {
        return;
    }

    let path = path.trim().to_string();
    if path.is_empty() {
        app.set_collection_status("Collection path required".into());
        set_response(
            app,
            "Collection save failed",
            "",
            "error",
            "Enter a target native collection JSON file path.",
        );
        return;
    }

    let Some(collection) = state.lock().ok().map(|state| state.collection.clone()) else {
        return;
    };
    match collection.save_file(&path) {
        Ok(()) => {
            app.set_collection_path(path.clone().into());
            app.set_collection_status("Saved native JSON".into());
            set_response(app, "Collection saved", &path, "success", &collection.name);
        }
        Err(error) => {
            app.set_collection_status("Save failed".into());
            set_response(
                app,
                "Collection save failed",
                &path,
                "error",
                &error.to_string(),
            );
        }
    }
}

pub(super) fn export_postman_collection_path(
    app: &AppWindow,
    state: Arc<Mutex<AppState>>,
    path: String,
) {
    if app.get_busy() {
        return;
    }

    let path = path.trim().to_string();
    if path.is_empty() {
        app.set_collection_status("Collection path required".into());
        set_response(
            app,
            "Postman export failed",
            "",
            "error",
            "Enter a target Postman collection JSON file path.",
        );
        return;
    }

    let Some(collection) = state.lock().ok().map(|state| state.collection.clone()) else {
        return;
    };
    match collection.save_postman_file(&path) {
        Ok(()) => {
            app.set_collection_path(path.clone().into());
            app.set_collection_status("Exported Postman JSON".into());
            set_response(
                app,
                "Postman collection exported",
                &path,
                "success",
                &collection.name,
            );
        }
        Err(error) => {
            app.set_collection_status("Export failed".into());
            set_response(
                app,
                "Postman export failed",
                &path,
                "error",
                &error.to_string(),
            );
        }
    }
}

use slint::ComponentHandle;
use std::sync::{Arc, Mutex};

use crate::ui::AppWindow;

use super::super::{
    AppState,
    collection_tree::{
        add_collection_request_in, collection_folder_label, collection_model,
        collection_request_at, count_collection_requests, duplicate_collection_request_at,
        move_collection_request_to_folder, remove_collection_request_at,
        rename_collection_request_at, reorder_collection_request_at,
    },
    request_editor_ui::request_projection_input,
    response_assertion_parser::parse_response_assertions,
    set_response,
};
use super::editor::{collection_request_from_editor, restore_collection_request};

pub(in crate::app) fn wire_collection_request_actions(
    app: &AppWindow,
    state: Arc<Mutex<AppState>>,
) {
    let weak_app = app.as_weak();
    let reorder_request_state = state.clone();
    app.on_reorder_collection_request(move |id, delta| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() || id < 0 {
            return;
        }

        let Some((request, new_id, request_count, rows)) =
            reorder_request_state.lock().ok().and_then(|mut state| {
                let (request, new_id) =
                    reorder_collection_request_at(&mut state.collection, id as usize, delta)?;
                let request_count = count_collection_requests(&state.collection.items);
                let rows = collection_model(&state.collection);
                Some((request, new_id, request_count, rows))
            })
        else {
            app.set_collection_status("Request reorder failed".into());
            set_response(
                &app,
                "Collection request reorder failed",
                "",
                "error",
                "Select a saved request that can move in that direction.",
            );
            return;
        };

        app.set_collection_rows(rows);
        app.set_selected_collection_request(new_id);
        app.set_collection_request_name(request.name.clone().into());
        app.set_collection_status(format!("Reordered {request_count} requests").into());
        set_response(
            &app,
            "Collection request reordered",
            &request.name,
            "success",
            &request.url,
        );
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
        let selected_folder = app.get_selected_collection_folder().to_string();

        let Some((collection_name, target_label, request_count, selected_id, rows)) =
            add_state.lock().ok().and_then(|mut state| {
                let selected_id = add_collection_request_in(
                    &mut state.collection,
                    &selected_folder,
                    collection_request,
                )?;
                let target_label = collection_folder_label(&selected_folder);
                let request_count = count_collection_requests(&state.collection.items);
                let rows = collection_model(&state.collection);
                Some((
                    state.collection.name.clone(),
                    target_label,
                    request_count,
                    selected_id,
                    rows,
                ))
            })
        else {
            app.set_collection_status("Add failed".into());
            set_response(
                &app,
                "Collection add failed",
                "",
                "error",
                "Select the collection root or a valid folder before saving.",
            );
            return;
        };

        app.set_collection_name(collection_name.into());
        app.set_collection_rows(rows);
        app.set_collection_status(format!("Saved {request_count} requests").into());
        app.set_selected_collection_request(selected_id);
        app.set_collection_request_name(request_name.clone().into());
        set_response(
            &app,
            "Request saved to collection",
            &request_name,
            "success",
            &format!("{request_url}\nSaved under {target_label}."),
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
    let move_state = state.clone();
    app.on_move_collection_request(move |id, folder_path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() || id < 0 {
            return;
        }

        let Some((moved, target_label, request_count, rows)) =
            move_state.lock().ok().and_then(|mut state| {
                move_collection_request_to_folder(
                    &mut state.collection,
                    id as usize,
                    folder_path.as_str(),
                )
                .map(|request| {
                    let target_label = collection_folder_label(folder_path.as_str());
                    let request_count = count_collection_requests(&state.collection.items);
                    let rows = collection_model(&state.collection);
                    (request, target_label, request_count, rows)
                })
            })
        else {
            app.set_collection_status("Move failed".into());
            set_response(
                &app,
                "Collection move failed",
                "",
                "error",
                "Select a saved request and a valid target folder.",
            );
            return;
        };

        app.set_collection_rows(rows);
        app.set_collection_status(format!("Moved {request_count} requests").into());
        app.set_selected_collection_request(-1);
        app.set_collection_request_name("".into());
        set_response(
            &app,
            "Collection request moved",
            &moved.name,
            "success",
            &format!("Moved to {target_label}."),
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

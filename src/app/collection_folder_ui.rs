use std::sync::{Arc, Mutex};

use slint::ComponentHandle;

use crate::ui::AppWindow;

use super::{
    AppState,
    collection_tree::{
        add_collection_folder_in, collection_folder_label, collection_model,
        count_collection_requests, remove_collection_folder_at, rename_collection_folder_at,
        reorder_collection_folder_at,
    },
    set_response,
};

pub(super) fn wire_collection_folder_actions(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    let folder_state = state.clone();
    app.on_add_collection_folder(move |name| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let selected_folder = app.get_selected_collection_folder().to_string();
        let Some((folder_name, parent_label, request_count, rows)) =
            folder_state.lock().ok().and_then(|mut state| {
                let folder_name = add_collection_folder_in(
                    &mut state.collection,
                    &selected_folder,
                    name.as_str(),
                )?;
                let parent_label = collection_folder_label(&selected_folder);
                let request_count = count_collection_requests(&state.collection.items);
                let rows = collection_model(&state.collection);
                Some((folder_name, parent_label, request_count, rows))
            })
        else {
            app.set_collection_status("Folder add failed".into());
            set_response(
                &app,
                "Collection folder add failed",
                "",
                "error",
                "Enter a folder name before adding it.",
            );
            return;
        };

        app.set_collection_rows(rows);
        app.set_collection_folder_name("".into());
        app.set_collection_status(format!("Added folder / {request_count} requests").into());
        set_response(
            &app,
            "Collection folder added",
            &folder_name,
            "success",
            &format!("Folder created under {parent_label}."),
        );
    });

    let weak_app = app.as_weak();
    let rename_folder_state = state.clone();
    app.on_rename_collection_folder(move |folder_path, name| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let Some((renamed_name, new_path, request_count, rows)) =
            rename_folder_state.lock().ok().and_then(|mut state| {
                let (renamed_name, new_path) = rename_collection_folder_at(
                    &mut state.collection,
                    folder_path.as_str(),
                    name.as_str(),
                )?;
                let request_count = count_collection_requests(&state.collection.items);
                let rows = collection_model(&state.collection);
                Some((renamed_name, new_path, request_count, rows))
            })
        else {
            app.set_collection_status("Folder rename failed".into());
            set_response(
                &app,
                "Collection folder rename failed",
                "",
                "error",
                "Select a folder and enter a new folder name.",
            );
            return;
        };

        app.set_collection_rows(rows);
        app.set_selected_collection_folder(new_path.clone().into());
        app.set_collection_move_target_label(collection_folder_label(&new_path).into());
        app.set_collection_folder_name("".into());
        app.set_collection_status(format!("Renamed folder / {request_count} requests").into());
        set_response(
            &app,
            "Collection folder renamed",
            &renamed_name,
            "success",
            &format!("Folder path: {}", collection_folder_label(&new_path)),
        );
    });

    let weak_app = app.as_weak();
    let delete_folder_state = state.clone();
    app.on_delete_collection_folder(move |folder_path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let Some((removed, removed_count, request_count, rows)) =
            delete_folder_state.lock().ok().and_then(|mut state| {
                let removed =
                    remove_collection_folder_at(&mut state.collection, folder_path.as_str())?;
                let removed_count = count_collection_requests(&removed.items);
                let request_count = count_collection_requests(&state.collection.items);
                let rows = collection_model(&state.collection);
                Some((removed, removed_count, request_count, rows))
            })
        else {
            app.set_collection_status("Folder delete failed".into());
            set_response(
                &app,
                "Collection folder delete failed",
                "",
                "error",
                "Select a folder before deleting it.",
            );
            return;
        };

        let root_label = if app.get_collection_name().is_empty() {
            "Collection".to_string()
        } else {
            app.get_collection_name().to_string()
        };
        app.set_collection_rows(rows);
        app.set_selected_collection_folder("".into());
        app.set_collection_move_target_label(root_label.into());
        app.set_selected_collection_request(-1);
        app.set_collection_request_name("".into());
        app.set_collection_folder_name("".into());
        app.set_collection_status(format!("Deleted folder / {request_count} requests").into());
        set_response(
            &app,
            "Collection folder deleted",
            &removed.name,
            "neutral",
            &format!("Removed {removed_count} saved requests from this folder."),
        );
    });

    let weak_app = app.as_weak();
    let reorder_folder_state = state.clone();
    app.on_reorder_collection_folder(move |folder_path, delta| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let Some((folder_name, new_path, request_count, rows)) =
            reorder_folder_state.lock().ok().and_then(|mut state| {
                let (folder_name, new_path) = reorder_collection_folder_at(
                    &mut state.collection,
                    folder_path.as_str(),
                    delta,
                )?;
                let request_count = count_collection_requests(&state.collection.items);
                let rows = collection_model(&state.collection);
                Some((folder_name, new_path, request_count, rows))
            })
        else {
            app.set_collection_status("Folder reorder failed".into());
            set_response(
                &app,
                "Collection folder reorder failed",
                "",
                "error",
                "Select a folder that can move in that direction.",
            );
            return;
        };

        app.set_collection_rows(rows);
        app.set_selected_collection_folder(new_path.clone().into());
        app.set_collection_move_target_label(collection_folder_label(&new_path).into());
        app.set_collection_status(format!("Reordered folder / {request_count} requests").into());
        set_response(
            &app,
            "Collection folder reordered",
            &folder_name,
            "success",
            &format!("Folder path: {}", collection_folder_label(&new_path)),
        );
    });
}

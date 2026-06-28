use slint::ComponentHandle;

use crate::ui::AppWindow;

use super::super::super::{
    clipboard_ui::read_text_from_clipboard, request_editor_ui::refresh_query_param_rows,
    set_response,
};
use super::super::{
    add_key_value_text, delete_key_value_text, merge_key_value_file, merge_key_value_text,
    update_key_value_text,
};

pub(in crate::app) fn wire_query_param_actions(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_paste_query_params(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let result = read_text_from_clipboard().and_then(|clipboard| {
            merge_key_value_text(
                app.get_query_params().as_str(),
                clipboard.as_str(),
                "query param",
                false,
                false,
            )
        });
        match result {
            Ok((params, count)) => {
                app.set_query_params(params.into());
                refresh_query_param_rows(&app);
                set_response(
                    &app,
                    "Params pasted",
                    "",
                    "success",
                    &format!("{count} query parameter rows imported from clipboard."),
                );
            }
            Err(error) => {
                set_response(&app, "Params paste failed", "", "error", &error.to_string())
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_import_query_params(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match merge_key_value_file(
            app.get_query_params().as_str(),
            path.as_str(),
            "query param",
            false,
            false,
        ) {
            Ok((params, count)) => {
                app.set_query_params(params.into());
                refresh_query_param_rows(&app);
                set_response(
                    &app,
                    "Params imported",
                    path.as_str(),
                    "success",
                    &format!("{count} query parameter rows imported."),
                );
            }
            Err(error) => set_response(
                &app,
                "Params import failed",
                path.as_str(),
                "error",
                &error.to_string(),
            ),
        }
    });

    let weak_app = app.as_weak();
    app.on_add_query_param(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = add_key_value_text(app.get_query_params().as_str(), "param");
        app.set_query_params(updated.into());
        refresh_query_param_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_update_query_param_row(move |row_id, key, value| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = update_key_value_text(
            app.get_query_params().as_str(),
            row_id,
            key.as_str(),
            value.as_str(),
        );
        app.set_query_params(updated.into());
        refresh_query_param_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_delete_query_param_row(move |row_id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = delete_key_value_text(app.get_query_params().as_str(), row_id);
        app.set_query_params(updated.into());
        refresh_query_param_rows(&app);
    });
}

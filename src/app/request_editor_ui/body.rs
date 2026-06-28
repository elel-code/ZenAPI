use slint::ComponentHandle;

use crate::ui::AppWindow;

use super::super::{
    key_value_ui::{
        add_form_file_field_text, add_key_value_text, delete_key_value_text, unique_key_value_name,
        update_key_value_text,
    },
    set_response,
};
use super::refresh::refresh_body_field_rows;

pub(in crate::app) fn wire_body_field_actions(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_body_mode_changed(move |_mode| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        refresh_body_field_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_add_body_field(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = add_key_value_text(app.get_request_body().as_str(), "field");
        app.set_request_body(updated.into());
        refresh_body_field_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_add_body_file_field(move |field, path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match add_form_file_field_text(
            app.get_request_body().as_str(),
            field.as_str(),
            path.as_str(),
        ) {
            Ok(updated) => {
                app.set_request_body(updated.into());
                app.set_form_file_field(
                    unique_key_value_name(app.get_request_body().as_str(), "file").into(),
                );
                app.set_form_file_path("".into());
                refresh_body_field_rows(&app);
                set_response(
                    &app,
                    "Form file added",
                    field.as_str(),
                    "success",
                    "Multipart file field appended.",
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Form file failed",
                    field.as_str(),
                    "error",
                    &error.to_string(),
                );
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_update_body_field_row(move |row_id, key, value| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = update_key_value_text(
            app.get_request_body().as_str(),
            row_id,
            key.as_str(),
            value.as_str(),
        );
        app.set_request_body(updated.into());
        refresh_body_field_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_delete_body_field_row(move |row_id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = delete_key_value_text(app.get_request_body().as_str(), row_id);
        app.set_request_body(updated.into());
        refresh_body_field_rows(&app);
    });
}

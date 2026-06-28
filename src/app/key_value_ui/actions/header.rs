use slint::ComponentHandle;

use crate::ui::AppWindow;

use super::super::super::{
    clipboard_ui::{copy_text_to_clipboard, read_text_from_clipboard},
    request_editor_ui::refresh_header_rows,
    request_projection::apply_header_preset,
    set_response,
};
use super::super::{
    add_key_value_text, delete_key_value_text, merge_key_value_file, merge_key_value_text,
    update_key_value_text,
};

pub(in crate::app) fn wire_header_helpers(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_apply_header_preset(move |preset| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match apply_header_preset(&app.get_request_headers(), preset.as_str()) {
            Ok(headers) => {
                app.set_request_headers(headers.into());
                refresh_header_rows(&app);
                set_response(
                    &app,
                    "Header preset applied",
                    preset.as_str(),
                    "success",
                    "Request headers updated.",
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Header preset failed",
                    preset.as_str(),
                    "error",
                    &error.to_string(),
                );
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_copy_headers(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        match copy_text_to_clipboard(app.get_request_headers().as_str()) {
            Ok(()) => app.set_activity("Copied request headers".into()),
            Err(error) => app.set_activity(format!("Copy headers failed: {error}").into()),
        }
    });

    let weak_app = app.as_weak();
    app.on_paste_headers(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let result = read_text_from_clipboard().and_then(|clipboard| {
            merge_key_value_text(
                app.get_request_headers().as_str(),
                clipboard.as_str(),
                "header",
                true,
                true,
            )
        });
        match result {
            Ok((headers, count)) => {
                app.set_request_headers(headers.into());
                refresh_header_rows(&app);
                set_response(
                    &app,
                    "Headers pasted",
                    "",
                    "success",
                    &format!("{count} header rows imported from clipboard."),
                );
            }
            Err(error) => {
                set_response(&app, "Header paste failed", "", "error", &error.to_string())
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_import_headers(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match merge_key_value_file(
            app.get_request_headers().as_str(),
            path.as_str(),
            "header",
            true,
            true,
        ) {
            Ok((headers, count)) => {
                app.set_request_headers(headers.into());
                refresh_header_rows(&app);
                set_response(
                    &app,
                    "Headers imported",
                    path.as_str(),
                    "success",
                    &format!("{count} header rows imported."),
                );
            }
            Err(error) => set_response(
                &app,
                "Header import failed",
                path.as_str(),
                "error",
                &error.to_string(),
            ),
        }
    });

    let weak_app = app.as_weak();
    app.on_add_header(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = add_key_value_text(app.get_request_headers().as_str(), "Header");
        app.set_request_headers(updated.into());
        refresh_header_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_update_header_row(move |row_id, key, value| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = update_key_value_text(
            app.get_request_headers().as_str(),
            row_id,
            key.as_str(),
            value.as_str(),
        );
        app.set_request_headers(updated.into());
        refresh_header_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_delete_header_row(move |row_id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = delete_key_value_text(app.get_request_headers().as_str(), row_id);
        app.set_request_headers(updated.into());
        refresh_header_rows(&app);
    });
}

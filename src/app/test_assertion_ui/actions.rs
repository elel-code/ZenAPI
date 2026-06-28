use slint::ComponentHandle;

use crate::ui::AppWindow;

use super::super::{request_editor_ui::refresh_test_assertion_rows, set_response};
use super::text::{
    add_custom_test_assertion_text, add_test_assertion_template_text, add_test_assertion_text,
    delete_test_assertion_text, next_test_assertion_template, update_test_assertion_text,
};

pub(in crate::app) fn wire_test_assertion_actions(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_add_test_assertion(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = add_test_assertion_text(app.get_request_tests().as_str());
        app.set_request_tests(updated.into());
        refresh_test_assertion_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_add_test_assertion_template(move |template| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match add_test_assertion_template_text(app.get_request_tests().as_str(), template.as_str())
        {
            Ok(updated) => {
                app.set_request_tests(updated.into());
                refresh_test_assertion_rows(&app);
                set_response(
                    &app,
                    "Test template added",
                    template.as_str(),
                    "success",
                    "Assertion row appended.",
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Test template failed",
                    "",
                    "error",
                    &error.to_string(),
                );
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_add_custom_test_assertion(move |kind, target, expected| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match add_custom_test_assertion_text(
            app.get_request_tests().as_str(),
            kind.as_str(),
            target.as_str(),
            expected.as_str(),
        ) {
            Ok(updated) => {
                app.set_request_tests(updated.into());
                refresh_test_assertion_rows(&app);
                set_response(
                    &app,
                    "Test assertion added",
                    kind.as_str(),
                    "success",
                    "Custom assertion row appended.",
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Test assertion failed",
                    "",
                    "error",
                    &error.to_string(),
                );
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_update_test_assertion_row(move |row_id, kind, target, expected| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = update_test_assertion_text(
            app.get_request_tests().as_str(),
            row_id,
            kind.as_str(),
            target.as_str(),
            expected.as_str(),
        );
        app.set_request_tests(updated.into());
        refresh_test_assertion_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_cycle_test_assertion_kind(move |row_id, kind| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let (next_kind, target, expected) = next_test_assertion_template(kind.as_str());
        let updated = update_test_assertion_text(
            app.get_request_tests().as_str(),
            row_id,
            next_kind,
            target,
            expected,
        );
        app.set_request_tests(updated.into());
        refresh_test_assertion_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_delete_test_assertion_row(move |row_id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = delete_test_assertion_text(app.get_request_tests().as_str(), row_id);
        app.set_request_tests(updated.into());
        refresh_test_assertion_rows(&app);
    });
}

use slint::ComponentHandle;
use std::sync::Arc;
use tokio::runtime::Runtime;

use crate::{
    auth::{fetch_oauth2_token_config, format_basic_auth_config},
    ui::AppWindow,
};

use super::super::{
    key_value_ui::{add_key_value_text, delete_key_value_text, update_key_value_text},
    set_response,
};
use super::refresh::{refresh_auth_key_rows, refresh_basic_auth_fields};

pub(in crate::app) fn wire_auth_key_actions(app: &AppWindow, runtime: Arc<Runtime>) {
    let weak_app = app.as_weak();
    app.on_auth_mode_changed(move |mode| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if mode.as_str() == "basic" {
            refresh_basic_auth_fields(&app);
        }
        refresh_auth_key_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_update_basic_auth(move |username, password| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        app.set_auth_config(format_basic_auth_config(username.as_str(), password.as_str()).into());
        refresh_auth_key_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_add_auth_key(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let base_key = if app.get_auth_mode().as_str() == "api-query" {
            "api_key"
        } else {
            "x-api-key"
        };
        let updated = add_key_value_text(app.get_auth_config().as_str(), base_key);
        app.set_auth_config(updated.into());
        refresh_auth_key_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_update_auth_key_row(move |row_id, key, value| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = update_key_value_text(
            app.get_auth_config().as_str(),
            row_id,
            key.as_str(),
            value.as_str(),
        );
        app.set_auth_config(updated.into());
        refresh_auth_key_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_delete_auth_key_row(move |row_id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let updated = delete_key_value_text(app.get_auth_config().as_str(), row_id);
        app.set_auth_config(updated.into());
        refresh_auth_key_rows(&app);
    });

    let weak_app = app.as_weak();
    app.on_fetch_oauth2_token(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }
        if app.get_auth_mode().as_str() != "oauth2" {
            app.set_activity("Switch to OAuth2 before fetching a token.".into());
            return;
        }

        let config = app.get_auth_config().to_string();
        app.set_busy(true);
        app.set_activity("Fetching OAuth2 token".into());
        set_response(
            &app,
            "OAuth2 token request",
            "",
            "busy",
            "Waiting for token endpoint.",
        );

        let weak_app = app.as_weak();
        runtime.spawn(async move {
            let result = fetch_oauth2_token_config(&config).await;
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak_app.upgrade() else {
                    return;
                };
                match result {
                    Ok(updated) => {
                        app.set_auth_config(updated.into());
                        set_response(
                            &app,
                            "OAuth2 token fetched",
                            "",
                            "success",
                            "Access token stored in the OAuth2 auth config.",
                        );
                    }
                    Err(error) => {
                        set_response(
                            &app,
                            "OAuth2 token fetch failed",
                            "",
                            "error",
                            &error.to_string(),
                        );
                    }
                }
                app.set_activity("".into());
                app.set_busy(false);
            });
        });
    });
}

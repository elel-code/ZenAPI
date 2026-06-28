use anyhow::anyhow;
use slint::ComponentHandle;
use std::sync::{Arc, Mutex};

use crate::ui::AppWindow;

use super::super::super::{AppState, response_format::pretty_json, set_response};
use super::{
    model::{
        clear_selected_mock_rule, refresh_mock_rule_rows, set_selected_mock_route,
        set_selected_mock_rule,
    },
    state::{
        add_selected_mock_rule, delete_selected_mock_rule, save_selected_mock_rule, selected_route,
        update_selected_mock_response,
    },
};

pub(in crate::app) fn wire_mock_response_actions(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    app.on_save_mock_response(move |body| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let selected_route = app.get_selected_route();
        let result = state
            .lock()
            .map_err(|_| anyhow!("mock route state is unavailable"))
            .and_then(|mut state| {
                update_selected_mock_response(&mut state, selected_route, body.as_str())
            });

        match result {
            Ok(route) => {
                set_selected_mock_route(&app, &route);
                set_response(
                    &app,
                    "Mock response saved",
                    route.path.as_str(),
                    "success",
                    &pretty_json(&route.mock_body),
                );
                app.set_activity(if app.get_server_running() {
                    "Mock response saved; restart server to apply".into()
                } else {
                    "Mock response saved".into()
                });
            }
            Err(error) => {
                set_response(
                    &app,
                    "Mock response save failed",
                    "",
                    "error",
                    &error.to_string(),
                );
            }
        }
    });
}

pub(in crate::app) fn wire_mock_rule_actions(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    let state_for_select = state.clone();
    app.on_select_mock_rule(move |row_id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        let rule = state_for_select.lock().ok().and_then(|state| {
            selected_route(&state, app.get_selected_route())
                .ok()
                .and_then(|route| route.mock_rules.get(row_id as usize).cloned())
        });

        if let Some(rule) = rule {
            set_selected_mock_rule(&app, row_id, &rule);
        } else {
            clear_selected_mock_rule(&app);
        }
    });

    let weak_app = app.as_weak();
    let state_for_add = state.clone();
    app.on_add_mock_rule(move |source| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let result = state_for_add
            .lock()
            .map_err(|_| anyhow!("mock route state is unavailable"))
            .and_then(|mut state| {
                add_selected_mock_rule(&mut state, app.get_selected_route(), source.as_str())
            });

        match result {
            Ok((route, row_id)) => {
                refresh_mock_rule_rows(&app, &route);
                if let Some(rule) = route.mock_rules.get(row_id as usize) {
                    set_selected_mock_rule(&app, row_id, rule);
                }
                app.set_activity(
                    "Mock rule added; restart server to apply if it is running.".into(),
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Mock rule add failed",
                    "",
                    "error",
                    &error.to_string(),
                );
            }
        }
    });

    let weak_app = app.as_weak();
    let state_for_save = state.clone();
    app.on_save_mock_rule(move |row_id, source, name, value, body| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let result = state_for_save
            .lock()
            .map_err(|_| anyhow!("mock route state is unavailable"))
            .and_then(|mut state| {
                save_selected_mock_rule(
                    &mut state,
                    app.get_selected_route(),
                    row_id,
                    source.as_str(),
                    name.as_str(),
                    value.as_str(),
                    body.as_str(),
                )
            });

        match result {
            Ok(route) => {
                refresh_mock_rule_rows(&app, &route);
                if let Some(rule) = route.mock_rules.get(row_id as usize) {
                    set_selected_mock_rule(&app, row_id, rule);
                }
                app.set_activity(
                    "Mock rule saved; restart server to apply if it is running.".into(),
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Mock rule save failed",
                    "",
                    "error",
                    &error.to_string(),
                );
            }
        }
    });

    let weak_app = app.as_weak();
    app.on_delete_mock_rule(move |row_id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let result = state
            .lock()
            .map_err(|_| anyhow!("mock route state is unavailable"))
            .and_then(|mut state| {
                delete_selected_mock_rule(&mut state, app.get_selected_route(), row_id)
            });

        match result {
            Ok((route, next_row)) => {
                refresh_mock_rule_rows(&app, &route);
                if let Some(row_id) = next_row {
                    if let Some(rule) = route.mock_rules.get(row_id as usize) {
                        set_selected_mock_rule(&app, row_id, rule);
                    }
                } else {
                    clear_selected_mock_rule(&app);
                }
                app.set_activity(
                    "Mock rule deleted; restart server to apply if it is running.".into(),
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Mock rule delete failed",
                    "",
                    "error",
                    &error.to_string(),
                );
            }
        }
    });
}

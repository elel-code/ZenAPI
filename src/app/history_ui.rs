use slint::{ComponentHandle, ModelRc, VecModel};
use std::sync::{Arc, Mutex};
use zenapi::{
    client::{ClientResponse, RequestBody},
    codegen::CodegenRequest,
    history::{HistoryRequest, HistoryResponse, RequestHistory},
};

use crate::ui::{AppWindow, HistoryRow};

use super::{
    AppState,
    key_value_ui::format_key_value_preview,
    request_editor_ui::{
        refresh_auth_key_rows, refresh_basic_auth_fields, refresh_body_field_rows,
        refresh_header_rows, refresh_query_param_rows, refresh_test_assertion_rows,
    },
    request_projection::{
        RequestProjectionInput, normalize_raw_body_subtype, raw_body_subtype_from_content_type,
    },
    response_format::truncate_preview,
    set_response,
};

pub(super) fn filtered_history_model(history: &RequestHistory, query: &str) -> ModelRc<HistoryRow> {
    let entries = history.filtered(query);
    history_model_from_entries(entries.into_iter())
}

pub(super) fn delete_history_entry(
    history: &mut RequestHistory,
    id: u64,
    query: &str,
) -> Option<ModelRc<HistoryRow>> {
    if !history.remove(id) {
        return None;
    }
    Some(filtered_history_model(history, query))
}

pub(super) fn empty_history_model() -> ModelRc<HistoryRow> {
    history_model_from_entries(std::iter::empty())
}

fn history_model_from_entries<'a>(
    entries: impl Iterator<Item = &'a zenapi::history::HistoryEntry>,
) -> ModelRc<HistoryRow> {
    ModelRc::new(VecModel::from_iter(entries.map(|entry| HistoryRow {
        id: entry.id as i32,
        method: entry.request.method.clone().into(),
        url: entry.request.url.clone().into(),
        status: entry.response.status.clone().into(),
    })))
}

pub(super) fn history_request(
    request: &CodegenRequest,
    input: &RequestProjectionInput,
    request_tests: &str,
) -> HistoryRequest {
    let (body_kind, raw_body_subtype, body_preview) = request_body_preview(&request.body);
    HistoryRequest {
        method: request.method.clone(),
        url: request.url.clone(),
        query_params: request.query_params.clone(),
        headers: request.headers.clone(),
        auth_mode: input.auth_mode.clone(),
        auth_config: input.auth_config.clone(),
        body_kind,
        raw_body_subtype,
        body_preview,
        pre_request_script: input.pre_request_script.clone(),
        request_tests: request_tests.to_string(),
    }
}

pub(super) fn normalized_history_auth_mode(mode: &str) -> &str {
    let mode = mode.trim();
    if mode.is_empty() { "none" } else { mode }
}

pub(super) fn history_body_to_slint(request: &HistoryRequest) -> (String, String) {
    let body_mode = request.body_kind.trim();
    let body_mode = if body_mode.is_empty() {
        "none"
    } else {
        body_mode
    };
    let raw_body_subtype = if body_mode == "raw" {
        normalize_raw_body_subtype(&request.raw_body_subtype)
    } else {
        "json"
    };
    (body_mode.to_string(), raw_body_subtype.to_string())
}

pub(super) fn success_history_response(response: &ClientResponse) -> HistoryResponse {
    HistoryResponse {
        status: format!("HTTP {}", response.status),
        meta: format!("{} ms / {} B", response.elapsed_ms, response.body_bytes),
        body_preview: truncate_preview(&response.body, 1200),
    }
}

pub(super) fn request_body_preview(body: &RequestBody) -> (String, String, String) {
    match body {
        RequestBody::None => ("none".to_string(), "json".to_string(), String::new()),
        RequestBody::Raw { body, content_type } => (
            "raw".to_string(),
            content_type
                .as_deref()
                .map(raw_body_subtype_from_content_type)
                .unwrap_or_else(|| "json".to_string()),
            body.clone(),
        ),
        RequestBody::FormUrlEncoded(fields) => (
            "urlenc".to_string(),
            "json".to_string(),
            format_key_value_preview(fields),
        ),
        RequestBody::Multipart(fields) => (
            "form".to_string(),
            "json".to_string(),
            format_key_value_preview(fields),
        ),
        RequestBody::BinaryFile { path, .. } => {
            ("binary".to_string(), "json".to_string(), path.clone())
        }
    }
}

pub(super) fn wire_history_selection(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    app.on_select_history(move |id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }
        if id < 0 {
            return;
        }

        let entry = state
            .lock()
            .ok()
            .and_then(|state| state.history.find(id as u64).cloned());

        if let Some(entry) = entry {
            let (body_mode, raw_body_subtype) = history_body_to_slint(&entry.request);
            app.set_method(entry.request.method.into());
            app.set_url(entry.request.url.into());
            app.set_query_params(format_key_value_preview(&entry.request.query_params).into());
            refresh_query_param_rows(&app);
            app.set_request_headers(format_key_value_preview(&entry.request.headers).into());
            refresh_header_rows(&app);
            app.set_auth_mode(normalized_history_auth_mode(&entry.request.auth_mode).into());
            app.set_auth_config(entry.request.auth_config.into());
            refresh_auth_key_rows(&app);
            refresh_basic_auth_fields(&app);
            app.set_body_mode(body_mode.into());
            app.set_raw_body_subtype(raw_body_subtype.into());
            app.set_request_body(entry.request.body_preview.into());
            refresh_body_field_rows(&app);
            app.set_graphql_variables("{}".into());
            app.set_pre_request_script(entry.request.pre_request_script.into());
            app.set_request_tests(entry.request.request_tests.into());
            refresh_test_assertion_rows(&app);
            set_response(
                &app,
                "History restored",
                &entry.response.status,
                "neutral",
                &entry.response.body_preview,
            );
        }
    });
}

pub(super) fn wire_history_actions(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    let filter_state = state.clone();
    app.on_filter_history(move |query| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let model = filter_state
            .lock()
            .ok()
            .map(|state| filtered_history_model(&state.history, query.as_str()))
            .unwrap_or_else(empty_history_model);
        app.set_history_rows(model);
    });

    let weak_app = app.as_weak();
    let delete_state = state.clone();
    app.on_delete_history(move |id| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() || id < 0 {
            return;
        }

        let result = delete_state.lock().ok().and_then(|mut state| {
            let model = delete_history_entry(
                &mut state.history,
                id as u64,
                app.get_history_filter().as_str(),
            );
            if model.is_some() {
                state.save_history_to_disk();
            }
            model
        });

        if let Some(model) = result {
            app.set_history_rows(model);
            set_response(
                &app,
                "History deleted",
                "",
                "neutral",
                "The selected request was removed from history.",
            );
        }
    });

    let weak_app = app.as_weak();
    app.on_clear_history(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }
        if let Ok(mut state) = state.lock() {
            state.history.clear();
            state.save_history_to_disk();
        }
        app.set_history_filter("".into());
        app.set_history_rows(empty_history_model());
        set_response(
            &app,
            "History cleared",
            "",
            "neutral",
            "Request history is empty.",
        );
    });
}

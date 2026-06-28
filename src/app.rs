use anyhow::{Result, anyhow};
use slint::ComponentHandle;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::runtime::Runtime;
use zenapi::{
    collections::ApiCollection,
    history::RequestHistory,
    mock_server::{MockRequestLog, MockServer},
    openapi::ApiRoute,
};

use crate::ui::AppWindow;

mod api_key_ui;
mod clipboard_ui;
mod codegen_ui;
mod collection_file_ui;
mod collection_folder_ui;
mod collection_request_ui;
mod collection_tree;
mod environment_ui;
mod file_dialog;
mod file_io;
mod file_picker_ui;
mod graphql;
mod grpc_ui;
mod history_ui;
mod key_value_ui;
mod mock_ui;
mod openapi_ui;
mod realtime;
mod request_editor_ui;
mod request_projection;
mod request_sender_ui;
mod response_assertion_parser;
mod response_format;
mod response_ui;
mod runner;
mod test_assertion_ui;
mod ui_smoke;
mod variable_ui;

use self::api_key_ui::wire_api_key_page_actions;
use self::codegen_ui::wire_codegen;
#[cfg(test)]
use self::codegen_ui::{
    codegen_language_label, codegen_metadata, save_codegen_snippet, snippet_language,
};
use self::collection_file_ui::wire_collection_file_actions;
use self::collection_folder_ui::wire_collection_folder_actions;
use self::collection_request_ui::wire_collection_request_actions;
#[cfg(test)]
use self::collection_request_ui::{collection_body_to_slint, collection_request_from_editor};
#[cfg(test)]
use self::collection_tree::{
    add_collection_folder_in, add_collection_request_in, collection_folder_label, collection_model,
    collection_request_at, collection_request_from_codegen, count_collection_requests,
    duplicate_collection_request_at, format_name_values, move_collection_request_to_folder,
    remove_collection_folder_at, remove_collection_request_at, rename_collection_folder_at,
    rename_collection_request_at, reorder_collection_folder_at, reorder_collection_request_at,
};
use self::environment_ui::{
    ENVIRONMENT_FILE_NAME, apply_environment_workspace, load_environment_workspace,
    wire_environment_actions,
};
#[cfg(test)]
use self::environment_ui::{
    EnvironmentProfiles, EnvironmentWorkspace, environment_rows_model, save_environment_workspace,
};
use self::file_picker_ui::wire_file_pickers;
use self::graphql::wire_graphql_helpers;
#[cfg(test)]
use self::graphql::{graphql_template, summarize_graphql_schema_response};
#[cfg(test)]
use self::grpc_ui::format_grpc_draft;
#[cfg(test)]
use self::grpc_ui::format_grpcurl_command;
use self::grpc_ui::wire_grpc_draft;
#[cfg(test)]
use self::history_ui::delete_history_entry;
#[cfg(test)]
use self::history_ui::history_request;
#[cfg(test)]
use self::history_ui::request_body_preview;
use self::history_ui::{filtered_history_model, wire_history_actions, wire_history_selection};
#[cfg(test)]
use self::key_value_ui::key_value_table_model;
#[cfg(test)]
use self::key_value_ui::{
    add_form_file_field_text, add_key_value_text, delete_key_value_text, update_key_value_text,
};
#[cfg(test)]
use self::key_value_ui::{merge_key_value_file, merge_key_value_text};
use self::key_value_ui::{wire_header_helpers, wire_query_param_actions};
#[cfg(test)]
use self::mock_ui::{
    add_selected_mock_rule, clear_mock_logs, delete_selected_mock_rule, filtered_mock_log_model,
    push_mock_log, save_mock_logs, save_selected_mock_rule, update_selected_mock_response,
};
use self::mock_ui::{
    wire_mock_log_filter, wire_mock_response_actions, wire_mock_rule_actions, wire_mock_server,
};
#[cfg(test)]
use self::openapi_ui::filter_routes;
use self::openapi_ui::wire_openapi_actions;
use self::realtime::wire_realtime_actions;
#[cfg(test)]
use self::realtime::{
    MAX_SSE_STREAM_EVENTS, format_sse_stream_events, parse_websocket_binary_message,
    push_bounded_sse_stream_event, sse_stream_event_done, sse_stream_event_last_id,
    sse_stream_meta, sse_stream_status, sse_stream_tone,
};
#[cfg(test)]
use self::realtime::{
    format_sse_exchange, format_websocket_exchange, format_websocket_session_events,
    latest_sse_event_id, parse_positive_usize, parse_websocket_protocols,
    websocket_session_command, websocket_session_status,
};
use self::request_editor_ui::{
    refresh_auth_key_rows, refresh_basic_auth_fields, refresh_body_field_rows, refresh_header_rows,
    refresh_query_param_rows, refresh_test_assertion_rows, refresh_variable_table,
    wire_auth_key_actions, wire_body_field_actions,
};
#[cfg(test)]
use self::request_projection::apply_header_preset;
#[cfg(test)]
use self::request_projection::{
    RequestProjectionInput, binary_body_content_type, build_codegen_request_projection,
    build_request_body, build_variable_store, parse_key_value_lines, resolve_pairs, resolve_text,
    upsert_pair,
};
use self::request_sender_ui::wire_request_sender;
#[cfg(test)]
use self::response_assertion_parser::format_response_assertions;
#[cfg(test)]
use self::response_assertion_parser::parse_response_assertions;
#[cfg(test)]
use self::response_format::{fold_json_response_text, response_tone};
#[cfg(test)]
use self::response_format::{folded_response_view_text, split_response_meta};
#[cfg(test)]
use self::response_format::{
    format_cookies, format_headers, response_body_with_assertions, response_status_with_assertions,
    response_tone_with_assertions, truncate_preview,
};
#[cfg(test)]
use self::response_format::{format_json_response_text, response_copy_text};
use self::response_ui::{set_response, set_response_payload, wire_response_actions};
use self::runner::wire_collection_runner;
#[cfg(test)]
use self::runner::{
    format_runner_result, format_runner_summary, normalize_runner_report_format, runner_options,
    runner_response_status, runner_response_tone, runner_result_detail, runner_result_status,
    runner_summary_line, save_runner_report,
};
#[cfg(test)]
use self::test_assertion_ui::test_assertion_table_model;
#[cfg(test)]
use self::test_assertion_ui::test_assertion_template;
use self::test_assertion_ui::wire_test_assertion_actions;
#[cfg(test)]
use self::test_assertion_ui::{
    add_custom_test_assertion_text, add_test_assertion_template_text, add_test_assertion_text,
    delete_test_assertion_text, next_test_assertion_template, update_test_assertion_text,
};
pub use self::ui_smoke::run_ui_frame_latency_smoke;
#[cfg(test)]
use self::variable_ui::{
    add_variable_text, delete_variable_text, update_variable_text, variable_table_model,
    variables_json_preview,
};
#[cfg(test)]
use crate::auth::build_auth_entries;
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use std::fs;
#[cfg(test)]
use zenapi::assertions::{ResponseAssertion, ResponseAssertionKind, ResponseAssertionResult};
#[cfg(test)]
use zenapi::collection_runner::{
    CollectionRunResult, CollectionRunSummary, FailureStrategy, RunnerOptions,
};
#[cfg(test)]
use zenapi::collections::{
    CollectionBody, CollectionFolder, CollectionItem, CollectionRequest, NameValue,
};
#[cfg(test)]
use zenapi::grpc::build_grpc_request_draft;
#[cfg(test)]
use zenapi::history::{HistoryRequest, HistoryResponse};
#[cfg(test)]
use zenapi::openapi::MockRuleSource;
#[cfg(test)]
use zenapi::{
    client::{self, RequestBody},
    codegen::{CodegenRequest, SnippetLanguage},
};

const HISTORY_FILE_NAME: &str = ".zenapi-history.json";

pub fn run() -> Result<()> {
    let runtime = Arc::new(Runtime::new()?);
    let mut initial_state = AppState::default();
    let history_load_error = initial_state.load_history_from_disk().err();
    let mut environment_load_error = None;
    let environment_workspace = match load_environment_workspace(Path::new(ENVIRONMENT_FILE_NAME)) {
        Ok(workspace) => workspace,
        Err(error) => {
            environment_load_error = Some(error);
            None
        }
    };
    let app = AppWindow::new().map_err(|err| anyhow!(err.to_string()))?;
    if let Some(workspace) = &environment_workspace {
        apply_environment_workspace(&app, workspace);
    }
    refresh_variable_table(&app);
    refresh_query_param_rows(&app);
    refresh_header_rows(&app);
    refresh_auth_key_rows(&app);
    refresh_basic_auth_fields(&app);
    refresh_body_field_rows(&app);
    refresh_test_assertion_rows(&app);
    app.set_history_rows(filtered_history_model(&initial_state.history, ""));
    if let Some(error) = history_load_error {
        set_response(
            &app,
            "History load failed",
            HISTORY_FILE_NAME,
            "error",
            &error.to_string(),
        );
    }
    if let Some(error) = environment_load_error {
        set_response(
            &app,
            "Environment load failed",
            ENVIRONMENT_FILE_NAME,
            "error",
            &error.to_string(),
        );
    }

    let state = Arc::new(Mutex::new(initial_state));

    wire_openapi_actions(&app, runtime.clone(), state.clone());
    wire_file_pickers(&app, runtime.clone(), state.clone());
    wire_history_selection(&app, state.clone());
    wire_history_actions(&app, state.clone());
    wire_collection_actions(&app, state.clone());
    wire_mock_log_filter(&app, state.clone());
    wire_mock_response_actions(&app, state.clone());
    wire_mock_rule_actions(&app, state.clone());
    wire_header_helpers(&app);
    wire_query_param_actions(&app);
    wire_auth_key_actions(&app, runtime.clone());
    wire_body_field_actions(&app);
    wire_test_assertion_actions(&app);
    wire_environment_actions(&app, environment_workspace);
    wire_request_sender(&app, runtime.clone(), state.clone());
    wire_response_actions(&app);
    wire_graphql_helpers(&app);
    wire_codegen(&app);
    wire_api_key_page_actions(&app);
    wire_collection_runner(&app, runtime.clone(), state.clone());
    wire_realtime_actions(&app, runtime.clone());
    wire_grpc_draft(&app, runtime.clone());
    wire_mock_server(&app, runtime, state);

    app.run().map_err(|err| anyhow!(err.to_string()))
}

struct AppState {
    routes: Vec<ApiRoute>,
    visible_routes: Vec<ApiRoute>,
    collection: ApiCollection,
    history: RequestHistory,
    history_path: PathBuf,
    mock_logs: Vec<MockRequestLog>,
    server: Option<MockServer>,
}

enum ServerAction {
    Start(Vec<ApiRoute>),
    Stop(MockServer),
}

impl AppState {
    fn load_history_from_disk(&mut self) -> Result<()> {
        if !self.history_path.exists() {
            return Ok(());
        }
        self.history = RequestHistory::load_file(&self.history_path)?;
        Ok(())
    }

    fn save_history_to_disk(&self) {
        let _ = self.history.save_file(&self.history_path);
    }

    fn next_server_action(&mut self) -> ServerAction {
        if let Some(server) = self.server.take() {
            ServerAction::Stop(server)
        } else {
            ServerAction::Start(self.routes.clone())
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            routes: Vec::new(),
            visible_routes: Vec::new(),
            collection: ApiCollection::new("Collection"),
            history: RequestHistory::default(),
            history_path: PathBuf::from(HISTORY_FILE_NAME),
            mock_logs: Vec::new(),
            server: None,
        }
    }
}

fn wire_collection_actions(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    wire_collection_file_actions(app, state.clone());
    wire_collection_folder_actions(app, state.clone());
    wire_collection_request_actions(app, state);
}

#[cfg(test)]
mod tests;

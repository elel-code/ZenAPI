use anyhow::{Result, anyhow};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::sync::{Arc, Mutex};
use zenapi::mock_server::MockRequestLog;

use crate::ui::{AppWindow, MockLogRow};

use super::super::{AppState, file_io::write_text_file, set_response};

pub(in crate::app) fn mock_log_model(logs: &[MockRequestLog]) -> ModelRc<MockLogRow> {
    mock_log_model_from_iter(logs.iter())
}

pub(in crate::app) fn filtered_mock_log_model(
    logs: &[MockRequestLog],
    query: &str,
) -> ModelRc<MockLogRow> {
    mock_log_model_from_iter(filtered_mock_logs(logs, query).into_iter())
}

fn filtered_mock_logs<'a>(logs: &'a [MockRequestLog], query: &str) -> Vec<&'a MockRequestLog> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return logs.iter().collect();
    }

    logs.iter()
        .filter(|log| {
            log.method.to_lowercase().contains(&query)
                || log.path.to_lowercase().contains(&query)
                || log.status.to_string().contains(&query)
        })
        .collect()
}

fn mock_log_model_from_iter<'a>(
    logs: impl Iterator<Item = &'a MockRequestLog>,
) -> ModelRc<MockLogRow> {
    ModelRc::new(VecModel::from_iter(logs.map(|log| MockLogRow {
        method: log.method.clone().into(),
        path: log.path.clone().into(),
        status: log.status.to_string().into(),
    })))
}

pub(in crate::app) fn push_mock_log(
    logs: &mut Vec<MockRequestLog>,
    log: MockRequestLog,
    limit: usize,
) {
    logs.insert(0, log);
    logs.truncate(limit);
}

pub(in crate::app) fn clear_mock_logs(logs: &mut Vec<MockRequestLog>) -> usize {
    let cleared = logs.len();
    logs.clear();
    cleared
}

pub(in crate::app) fn save_mock_logs(
    path: &str,
    logs: &[MockRequestLog],
    query: &str,
) -> Result<usize> {
    let exported = filtered_mock_logs(logs, query);
    let body = serde_json::to_string_pretty(&exported)
        .map_err(|err| anyhow!("serialize mock log export: {err}"))?;
    write_text_file(path, &body, "mock log export")?;
    Ok(exported.len())
}

pub(in crate::app) fn wire_mock_log_filter(app: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak_app = app.as_weak();
    let filter_state = state.clone();
    app.on_filter_mock_logs(move |query| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let model = filter_state
            .lock()
            .ok()
            .map(|state| filtered_mock_log_model(&state.mock_logs, query.as_str()))
            .unwrap_or_else(|| mock_log_model(&[]));
        app.set_mock_logs(model);
    });

    let weak_app = app.as_weak();
    let clear_state = state.clone();
    app.on_clear_mock_logs(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let Some(cleared) = clear_state.lock().ok().map(|mut state| {
            let cleared = clear_mock_logs(&mut state.mock_logs);
            cleared
        }) else {
            return;
        };

        app.set_mock_log_filter("".into());
        app.set_mock_logs(mock_log_model(&[]));
        set_response(
            &app,
            "Mock logs cleared",
            "",
            "neutral",
            &format!("{cleared} mock log entries removed."),
        );
    });

    let weak_app = app.as_weak();
    app.on_save_mock_logs(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let filter = app.get_mock_log_filter();
        let result = state
            .lock()
            .map_err(|_| anyhow!("mock log state is unavailable"))
            .and_then(|state| save_mock_logs(path.as_str(), &state.mock_logs, filter.as_str()));

        match result {
            Ok(count) => {
                set_response(
                    &app,
                    "Mock logs saved",
                    path.as_str(),
                    "success",
                    &format!("{count} mock log entries exported."),
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Mock log save failed",
                    path.as_str(),
                    "error",
                    &error.to_string(),
                );
            }
        }
    });
}

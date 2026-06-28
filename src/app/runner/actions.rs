use slint::ComponentHandle;
use std::sync::{Arc, Mutex};
use tokio::{runtime::Runtime, task::JoinHandle};
use zenapi::collection_runner::{CollectionRunSummary, run_collection};

use crate::ui::AppWindow;

use super::super::{
    AppState, collection_tree::count_collection_requests, request_projection::build_variable_store,
    set_response,
};
use super::format::{
    empty_runner_model, format_runner_summary, normalize_runner_report_format, runner_model,
    runner_options, runner_response_status, runner_response_tone, runner_summary_line,
    save_runner_report,
};

pub(in crate::app) fn wire_collection_runner(
    app: &AppWindow,
    runtime: Arc<Runtime>,
    state: Arc<Mutex<AppState>>,
) {
    let active_run = Arc::new(Mutex::new(None::<JoinHandle<()>>));
    let last_summary = Arc::new(Mutex::new(None::<CollectionRunSummary>));

    let weak_app = app.as_weak();
    let cancel_run = active_run.clone();
    let cancel_summary = last_summary.clone();
    app.on_cancel_collection_run(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        let Some(handle) = cancel_run.lock().ok().and_then(|mut run| run.take()) else {
            set_response(
                &app,
                "Runner idle",
                "",
                "neutral",
                "No collection run is active.",
            );
            return;
        };

        handle.abort();
        app.set_runner_active(false);
        app.set_busy(false);
        app.set_activity("".into());
        app.set_runner_summary("Cancelled".into());
        if let Ok(mut summary) = cancel_summary.lock() {
            *summary = None;
        }
        set_response(
            &app,
            "Runner cancelled",
            "",
            "neutral",
            "The active collection run was cancelled.",
        );
    });

    let weak_app = app.as_weak();
    let save_summary = last_summary.clone();
    app.on_save_runner_report(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() || app.get_runner_active() {
            return;
        }

        let Some(summary) = save_summary.lock().ok().and_then(|summary| summary.clone()) else {
            set_response(
                &app,
                "Runner report failed",
                "",
                "error",
                "Run a collection before saving a report.",
            );
            return;
        };

        let format = app.get_runner_report_format().to_string();
        match save_runner_report(path.as_str(), &summary, &format) {
            Ok(()) => set_response(
                &app,
                "Runner report saved",
                path.as_str(),
                "success",
                &format!(
                    "Collection runner {} report exported.",
                    normalize_runner_report_format(&format)
                ),
            ),
            Err(error) => set_response(
                &app,
                "Runner report failed",
                path.as_str(),
                "error",
                &error.to_string(),
            ),
        }
    });

    let weak_app = app.as_weak();
    let active_run_for_start = active_run.clone();
    let summary_for_start = last_summary.clone();
    app.on_run_collection(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }
        if active_run_for_start
            .lock()
            .ok()
            .and_then(|run| run.as_ref().map(|_| ()))
            .is_some()
        {
            set_response(
                &app,
                "Runner already running",
                "",
                "neutral",
                "Cancel the active collection run before starting another.",
            );
            return;
        }

        let options =
            match runner_options(&app.get_runner_delay_ms(), app.get_runner_stop_on_failure()) {
                Ok(options) => options,
                Err(error) => {
                    set_response(&app, "Runner failed", "", "error", &error.to_string());
                    return;
                }
            };
        let (variables, active_environment) = match build_variable_store(
            &app.get_global_variables(),
            &app.get_environment_name(),
            &app.get_environment_variables(),
        ) {
            Ok(variables) => variables,
            Err(error) => {
                set_response(&app, "Runner failed", "", "error", &error.to_string());
                return;
            }
        };

        let Some(collection) = state.lock().ok().map(|state| state.collection.clone()) else {
            return;
        };
        let request_count = count_collection_requests(&collection.items);
        if request_count == 0 {
            if let Ok(mut summary) = summary_for_start.lock() {
                *summary = None;
            }
            app.set_runner_summary("No requests to run".into());
            app.set_runner_rows(empty_runner_model());
            set_response(
                &app,
                "Runner idle",
                &collection.name,
                "neutral",
                "Save or load collection requests before running.",
            );
            return;
        }

        app.set_busy(true);
        app.set_runner_active(true);
        app.set_activity("Running collection".into());
        app.set_runner_summary(format!("Running {request_count} requests").into());
        app.set_runner_rows(empty_runner_model());
        if let Ok(mut summary) = summary_for_start.lock() {
            *summary = None;
        }
        set_response(
            &app,
            "Runner running",
            &collection.name,
            "busy",
            &format!("Running {request_count} requests"),
        );

        let weak_app = app.as_weak();
        let active_run = active_run_for_start.clone();
        let last_summary = summary_for_start.clone();
        let handle = runtime.spawn(async move {
            let summary = run_collection(
                &collection,
                &variables,
                active_environment.as_deref(),
                options,
            )
            .await;
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak_app.upgrade() else {
                    return;
                };

                let response_tone = runner_response_tone(&summary);
                let response_status = runner_response_status(&summary);
                let response_meta = format!("{} ms", summary.elapsed_ms);
                if let Ok(mut last_summary) = last_summary.lock() {
                    *last_summary = Some(summary.clone());
                }
                app.set_runner_rows(runner_model(&summary.results));
                app.set_runner_summary(runner_summary_line(&summary).into());
                set_response(
                    &app,
                    &response_status,
                    &response_meta,
                    response_tone,
                    &format_runner_summary(&summary),
                );
                app.set_activity("".into());
                app.set_runner_active(false);
                app.set_busy(false);
                if let Ok(mut run) = active_run.lock() {
                    *run = None;
                }
            });
        });
        if let Ok(mut run) = active_run_for_start.lock() {
            *run = Some(handle);
        }
    });
}

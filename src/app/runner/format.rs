use anyhow::{Result, anyhow};
use slint::{ModelRc, VecModel};
use zenapi::collection_runner::{
    CollectionRunResult, CollectionRunSummary, FailureStrategy, RunnerOptions,
};

use crate::ui::RunnerRow;

use super::super::file_io::write_text_file;

pub(in crate::app) fn empty_runner_model() -> ModelRc<RunnerRow> {
    ModelRc::new(VecModel::from_iter(Vec::<RunnerRow>::new()))
}

pub(in crate::app) fn runner_model(results: &[CollectionRunResult]) -> ModelRc<RunnerRow> {
    ModelRc::new(VecModel::from_iter(results.iter().map(|result| {
        RunnerRow {
            method: result.method.clone().into(),
            name: result.path.join(" / ").into(),
            status: runner_result_status(result).into(),
            detail: runner_result_detail(result).into(),
            tone: if result.success { "success" } else { "error" }.into(),
        }
    })))
}

pub(in crate::app) fn runner_options(
    delay_ms: &str,
    stop_on_failure: bool,
) -> Result<RunnerOptions> {
    let delay_ms = delay_ms.trim();
    let delay_ms = if delay_ms.is_empty() {
        0
    } else {
        delay_ms
            .parse::<u64>()
            .map_err(|_| anyhow!("runner delay must be a non-negative integer"))?
    };
    Ok(RunnerOptions {
        delay_ms,
        failure_strategy: if stop_on_failure {
            FailureStrategy::StopOnFailure
        } else {
            FailureStrategy::Continue
        },
    })
}

pub(in crate::app) fn runner_response_tone(summary: &CollectionRunSummary) -> &'static str {
    if summary.total == 0 {
        "neutral"
    } else if summary.failed == 0 {
        "success"
    } else {
        "error"
    }
}

pub(in crate::app) fn runner_response_status(summary: &CollectionRunSummary) -> String {
    if summary.failed == 0 {
        "Runner passed".to_string()
    } else {
        "Runner failed".to_string()
    }
}

pub(in crate::app) fn runner_summary_line(summary: &CollectionRunSummary) -> String {
    let stop = if summary.stopped_early {
        " / stopped"
    } else {
        ""
    };
    format!(
        "{}: {} passed, {} failed, {} total / {} ms{stop}",
        summary.collection_name, summary.passed, summary.failed, summary.total, summary.elapsed_ms
    )
}

pub(in crate::app) fn format_runner_summary(summary: &CollectionRunSummary) -> String {
    let mut lines = vec![runner_summary_line(summary)];
    for result in &summary.results {
        lines.push(format_runner_result(result));
    }
    lines.join("\n")
}

pub(in crate::app) fn normalize_runner_report_format(format: &str) -> &'static str {
    match format.trim().to_ascii_lowercase().as_str() {
        "json" => "json",
        _ => "text",
    }
}

fn format_runner_report(summary: &CollectionRunSummary, format: &str) -> Result<String> {
    match normalize_runner_report_format(format) {
        "json" => Ok(serde_json::to_string_pretty(summary)?),
        _ => Ok(format_runner_summary(summary)),
    }
}

pub(in crate::app) fn save_runner_report(
    path: &str,
    summary: &CollectionRunSummary,
    format: &str,
) -> Result<()> {
    let report = format_runner_report(summary, format)?;
    write_text_file(path, &report, "runner report")
}

pub(in crate::app) fn format_runner_result(result: &CollectionRunResult) -> String {
    let path = result.path.join(" / ");
    let mut line = format!(
        "[{}] {} {} {} ({path})",
        runner_result_status(result),
        result_status_label(result),
        result.method,
        result.url
    );
    if let Some(error) = &result.error {
        line.push_str(&format!(" - {error}"));
    }
    if !result.pre_request_actions.is_empty() {
        line.push_str(&format!(
            " - pre-request {}",
            result.pre_request_actions.len()
        ));
    }
    if !result.assertions.is_empty() {
        let passed = result
            .assertions
            .iter()
            .filter(|assertion| assertion.passed)
            .count();
        line.push_str(&format!(" - tests {passed}/{}", result.assertions.len()));
    }
    line
}

pub(in crate::app) fn runner_result_status(result: &CollectionRunResult) -> &'static str {
    if result.success { "PASS" } else { "FAIL" }
}

pub(in crate::app) fn runner_result_detail(result: &CollectionRunResult) -> String {
    let mut parts = vec![
        result_status_label(result),
        format!("{} ms", result.elapsed_ms),
        format!("{} B", result.body_bytes),
    ];
    if !result.assertions.is_empty() {
        let passed = result
            .assertions
            .iter()
            .filter(|assertion| assertion.passed)
            .count();
        parts.push(format!("tests {passed}/{}", result.assertions.len()));
    }
    if !result.pre_request_actions.is_empty() {
        parts.push(format!("pre {}", result.pre_request_actions.len()));
    }
    if let Some(error) = &result.error {
        parts.push(error.clone());
    }
    parts.join(" / ")
}

fn result_status_label(result: &CollectionRunResult) -> String {
    result
        .status
        .map(|status| format!("HTTP {status}"))
        .unwrap_or_else(|| "ERR".to_string())
}

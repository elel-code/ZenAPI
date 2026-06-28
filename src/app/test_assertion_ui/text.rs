use anyhow::{Result, anyhow};
use slint::{ModelRc, VecModel};
use zenapi::assertions::{ResponseAssertion, ResponseAssertionKind};

use crate::ui::TestAssertionRow;

use super::super::response_assertion_parser::parse_response_assertion_line;

pub(in crate::app) fn test_assertion_table_model(input: &str) -> ModelRc<TestAssertionRow> {
    ModelRc::new(VecModel::from_iter(
        test_assertion_ui_entries(input)
            .into_iter()
            .enumerate()
            .map(|(row_id, (_, kind, target, expected))| TestAssertionRow {
                row_id: row_id as i32,
                kind: kind.into(),
                target: target.into(),
                expected: expected.into(),
            }),
    ))
}

fn test_assertion_ui_entries(input: &str) -> Vec<(usize, String, String, String)> {
    input
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                return None;
            }
            if let Ok(assertion) = parse_response_assertion_line(line) {
                let (kind, target, expected) = response_assertion_ui_fields(&assertion);
                return Some((line_index, kind, target, expected));
            }
            let (kind, args) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let args = args.trim();
            let (target, expected) = args.split_once(char::is_whitespace).unwrap_or((args, ""));
            Some((
                line_index,
                kind.trim().to_string(),
                target.trim().to_string(),
                expected.trim().to_string(),
            ))
        })
        .collect()
}

fn response_assertion_ui_fields(assertion: &ResponseAssertion) -> (String, String, String) {
    match &assertion.kind {
        ResponseAssertionKind::StatusEquals { status } => (
            "status_equals".to_string(),
            status.to_string(),
            String::new(),
        ),
        ResponseAssertionKind::StatusInRange { min, max } => (
            "status_in_range".to_string(),
            min.to_string(),
            max.to_string(),
        ),
        ResponseAssertionKind::ResponseTimeBelow { max_ms } => (
            "response_time_below".to_string(),
            max_ms.to_string(),
            String::new(),
        ),
        ResponseAssertionKind::ResponseSizeBelow { max_bytes } => (
            "response_size_below".to_string(),
            max_bytes.to_string(),
            String::new(),
        ),
        ResponseAssertionKind::HeaderExists { name } => {
            ("header_exists".to_string(), name.clone(), String::new())
        }
        ResponseAssertionKind::HeaderNotExists { name } => {
            ("header_not_exists".to_string(), name.clone(), String::new())
        }
        ResponseAssertionKind::HeaderEquals { name, value } => {
            ("header_equals".to_string(), name.clone(), value.clone())
        }
        ResponseAssertionKind::BodyEquals { text } => {
            ("body_equals".to_string(), text.clone(), String::new())
        }
        ResponseAssertionKind::BodyContains { text } => {
            ("body_contains".to_string(), text.clone(), String::new())
        }
        ResponseAssertionKind::BodyNotContains { text } => {
            ("body_not_contains".to_string(), text.clone(), String::new())
        }
        ResponseAssertionKind::JsonPathExists { path } => (
            "json_path_exists".to_string(),
            json_path_ui_value(path),
            String::new(),
        ),
        ResponseAssertionKind::JsonPathNotExists { path } => (
            "json_path_not_exists".to_string(),
            json_path_ui_value(path),
            String::new(),
        ),
        ResponseAssertionKind::JsonPathType { path, value_type } => (
            "json_path_type".to_string(),
            json_path_ui_value(path),
            value_type.clone(),
        ),
        ResponseAssertionKind::JsonPathLength { path, length } => (
            "json_path_length".to_string(),
            json_path_ui_value(path),
            length.to_string(),
        ),
        ResponseAssertionKind::JsonPathContains { path, value } => (
            "json_path_contains".to_string(),
            json_path_ui_value(path),
            value.to_string(),
        ),
        ResponseAssertionKind::JsonPathNotContains { path, value } => (
            "json_path_not_contains".to_string(),
            json_path_ui_value(path),
            value.to_string(),
        ),
        ResponseAssertionKind::JsonPathEquals { path, value } => (
            "json_path_equals".to_string(),
            json_path_ui_value(path),
            value.to_string(),
        ),
        ResponseAssertionKind::JsonPathNotEquals { path, value } => (
            "json_path_not_equals".to_string(),
            json_path_ui_value(path),
            value.to_string(),
        ),
    }
}

pub(in crate::app) fn json_path_ui_value(path: &str) -> String {
    if path.trim().is_empty() {
        "$".to_string()
    } else {
        path.to_string()
    }
}

pub(in crate::app) fn update_test_assertion_text(
    input: &str,
    row_id: i32,
    kind: &str,
    target: &str,
    expected: &str,
) -> String {
    let mut lines = input.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = test_assertion_ui_entries(input);
    let new_line = format_test_assertion_line(kind, target, expected);

    if let Some((line_index, _, _, _)) = row_id
        .try_into()
        .ok()
        .and_then(|row_id: usize| entries.get(row_id))
    {
        lines[*line_index] = new_line;
    } else {
        lines.push(new_line);
    }

    lines.join("\n")
}

pub(in crate::app) fn add_test_assertion_text(input: &str) -> String {
    let (kind, target, expected) = test_assertion_template("status").expect("status template");
    append_test_assertion_line(input, &format_test_assertion_line(kind, target, expected))
}

pub(in crate::app) fn add_test_assertion_template_text(
    input: &str,
    template: &str,
) -> Result<String> {
    let (kind, target, expected) = test_assertion_template(template)
        .ok_or_else(|| anyhow!("unknown test assertion template: {template}"))?;

    Ok(append_test_assertion_line(
        input,
        &format_test_assertion_line(kind, target, expected),
    ))
}

pub(in crate::app) fn add_custom_test_assertion_text(
    input: &str,
    kind: &str,
    target: &str,
    expected: &str,
) -> Result<String> {
    let line = format_test_assertion_line(kind, target, expected);
    parse_response_assertion_line(&line)?;
    Ok(append_test_assertion_line(input, &line))
}

fn append_test_assertion_line(input: &str, new_line: &str) -> String {
    if input.trim().is_empty() {
        new_line.to_string()
    } else {
        format!("{}\n{new_line}", input.trim_end())
    }
}

pub(in crate::app) fn test_assertion_template(
    template: &str,
) -> Option<(&'static str, &'static str, &'static str)> {
    match template.trim().to_ascii_lowercase().as_str() {
        "status" | "status_equals" => Some(("status_equals", "200", "")),
        "range" | "status_in_range" => Some(("status_in_range", "200", "299")),
        "time" | "response_time" | "response_time_below" => {
            Some(("response_time_below", "500", ""))
        }
        "size" | "response_size" | "response_size_below" => {
            Some(("response_size_below", "65536", ""))
        }
        "header" | "header_equals" => Some(("header_equals", "content-type", "application/json")),
        "header_absent" | "header_not_exists" => Some(("header_not_exists", "x-debug", "")),
        "body_equals" | "body_exact" => Some(("body_equals", r#"{"ok":true}"#, "")),
        "body" | "body_contains" => Some(("body_contains", "ok", "")),
        "body_not" | "body_not_contains" => Some(("body_not_contains", "error", "")),
        "json_exists" | "json_path_exists" => Some(("json_path_exists", "data.id", "")),
        "json_not_exists" | "json_path_not_exists" => Some(("json_path_not_exists", "error", "")),
        "json_type" | "json_path_type" => Some(("json_path_type", "data.items", "array")),
        "json_length" | "json_path_length" => Some(("json_path_length", "data.items", "2")),
        "json_contains" | "json_path_contains" => {
            Some(("json_path_contains", "data.items", r#"{"id":1}"#))
        }
        "json_not_contains" | "json_path_not_contains" => {
            Some(("json_path_not_contains", "data.items", r#"{"id":999}"#))
        }
        "json" | "json_path" | "json_path_equals" => Some(("json_path_equals", "data.id", "1")),
        "json_not_equals" | "json_path_not_equals" => {
            Some(("json_path_not_equals", "data.id", "0"))
        }
        _ => None,
    }
}

pub(in crate::app) fn next_test_assertion_template(
    kind: &str,
) -> (&'static str, &'static str, &'static str) {
    match kind.trim().to_ascii_lowercase().as_str() {
        "status" | "status_equals" | "status=" => ("status_in_range", "200", "299"),
        "range" | "status_in_range" => ("response_time_below", "500", ""),
        "time" | "response_time" | "response_time_below" => ("response_size_below", "65536", ""),
        "size" | "response_size" | "response_size_below" => ("header_exists", "content-type", ""),
        "header" | "header_exists" | "header?" => {
            ("header_equals", "content-type", "application/json")
        }
        "header_equals" | "header=" => ("header_not_exists", "x-debug", ""),
        "header_absent" | "header_not_exists" | "header!" => ("body_equals", r#"{"ok":true}"#, ""),
        "body_equals" | "body_exact" => ("body_contains", "ok", ""),
        "body" | "body_contains" | "body?" => ("body_not_contains", "error", ""),
        "body_not" | "body_not_contains" | "body!" => ("json_path_exists", "data.id", ""),
        "json_exists" | "json_path_exists" | "json?" => ("json_path_not_exists", "error", ""),
        "json_not_exists" | "json_path_not_exists" | "json!" => {
            ("json_path_type", "data.items", "array")
        }
        "json_type" | "json_path_type" => ("json_path_length", "data.items", "2"),
        "json_length" | "json_path_length" => ("json_path_contains", "data.items", r#"{"id":1}"#),
        "json_contains" | "json_path_contains" => {
            ("json_path_not_contains", "data.items", r#"{"id":999}"#)
        }
        "json_not_contains" | "json_path_not_contains" => ("json_path_equals", "data.id", "1"),
        "json" | "json_path_equals" | "json=" => ("json_path_not_equals", "data.id", "0"),
        "json_not_equals" | "json_path_not_equals" | "json!=" => ("status_equals", "200", ""),
        _ => ("status_equals", "200", ""),
    }
}

pub(in crate::app) fn delete_test_assertion_text(input: &str, row_id: i32) -> String {
    let mut lines = input.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = test_assertion_ui_entries(input);

    if let Some((line_index, _, _, _)) = row_id
        .try_into()
        .ok()
        .and_then(|row_id: usize| entries.get(row_id))
    {
        lines.remove(*line_index);
    }

    lines.join("\n")
}

fn format_test_assertion_line(kind: &str, target: &str, expected: &str) -> String {
    let kind = kind.trim();
    let kind = if kind.is_empty() {
        "status_equals"
    } else {
        kind
    };
    let target = target.trim();
    let expected = expected.trim();
    if expected.is_empty() {
        format!("{kind} {target}")
    } else {
        format!("{kind} {target} {expected}")
    }
}

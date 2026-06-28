use anyhow::{Result, anyhow, bail};
use serde_json::Value;
use zenapi::assertions::ResponseAssertionResult;

pub(super) fn response_copy_text(text: &str, anchor: i32, cursor: i32) -> (&str, bool) {
    let start = anchor.min(cursor);
    let end = anchor.max(cursor);
    if start < 0 || end <= start {
        return (text, false);
    }

    let Ok(start) = usize::try_from(start) else {
        return (text, false);
    };
    let Ok(end) = usize::try_from(end) else {
        return (text, false);
    };
    if end > text.len() || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return (text, false);
    }

    (&text[start..end], true)
}

pub(super) fn format_json_response_text(text: &str) -> Result<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("response is empty");
    }

    let value: Value = serde_json::from_str(trimmed)
        .map_err(|error| anyhow!("response is not valid JSON: {error}"))?;
    serde_json::to_string_pretty(&value).map_err(|error| anyhow!("failed to format JSON: {error}"))
}

pub(super) fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

pub(super) fn truncate_preview(input: &str, max_chars: usize) -> String {
    let mut preview = input.chars().take(max_chars).collect::<String>();
    if input.chars().count() > max_chars {
        preview.push_str("\n...");
    }
    preview
}

pub(super) fn folded_response_view_text(tab: &str, text: &str) -> String {
    match tab {
        "pretty" | "raw" => fold_json_response_text(text)
            .unwrap_or_else(|_| folded_line_summary(response_tab_label(tab), text)),
        "headers" | "cookies" => folded_line_summary(response_tab_label(tab), text),
        _ => folded_line_summary("Response", text),
    }
}

pub(super) fn fold_json_response_text(text: &str) -> Result<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("response is empty");
    }

    let value: Value = serde_json::from_str(trimmed)
        .map_err(|error| anyhow!("response is not valid JSON: {error}"))?;
    Ok(render_folded_json_value(&value))
}

fn render_folded_json_value(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                return "{}".to_string();
            }

            let mut lines = Vec::with_capacity(map.len() + 2);
            lines.push("{".to_string());
            let last = map.len().saturating_sub(1);
            for (index, (key, value)) in map.iter().enumerate() {
                let comma = if index == last { "" } else { "," };
                lines.push(format!(
                    "  {}: {}{}",
                    json_string(key),
                    collapsed_json_value(value),
                    comma
                ));
            }
            lines.push("}".to_string());
            lines.join("\n")
        }
        Value::Array(items) => {
            if items.is_empty() {
                return "[]".to_string();
            }

            let mut lines = Vec::with_capacity(items.len() + 2);
            lines.push("[".to_string());
            let last = items.len().saturating_sub(1);
            for (index, value) in items.iter().enumerate() {
                let comma = if index == last { "" } else { "," };
                lines.push(format!("  {}{}", collapsed_json_value(value), comma));
            }
            lines.push("]".to_string());
            lines.join("\n")
        }
        _ => json_value(value),
    }
}

fn collapsed_json_value(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                "{}".to_string()
            } else {
                "{ ... }".to_string()
            }
        }
        Value::Array(items) => {
            if items.is_empty() {
                "[]".to_string()
            } else {
                "[ ... ]".to_string()
            }
        }
        _ => json_value(value),
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

pub(super) fn json_value(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

pub(super) fn folded_line_summary(label: &str, text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return format!("{label} folded\n0 lines");
    }

    let line_count = trimmed.lines().count();
    let byte_count = trimmed.len();
    let first_line = trimmed.lines().next().unwrap_or_default();
    format!(
        "{label} folded\n{line_count} line(s), {byte_count} byte(s)\n\n{}",
        truncate_summary_line(first_line, 160)
    )
}

pub(super) fn truncate_summary_line(line: &str, max_chars: usize) -> String {
    let mut chars = line.chars();
    let mut truncated = String::new();
    for _ in 0..max_chars {
        let Some(ch) = chars.next() else {
            return line.to_string();
        };
        truncated.push(ch);
    }

    if chars.next().is_some() {
        truncated.push_str("...");
    }
    truncated
}

pub(super) fn response_tab_label(tab: &str) -> &'static str {
    match tab {
        "pretty" => "Pretty",
        "raw" => "Raw",
        "headers" => "Headers",
        "cookies" => "Cookies",
        _ => "active",
    }
}

pub(super) fn split_response_meta(meta: &str) -> (String, String) {
    let meta = meta.trim();
    if meta.is_empty() {
        return (String::new(), String::new());
    }

    if let Some((time, size)) = meta.split_once(" / ") {
        (time.trim().to_string(), size.trim().to_string())
    } else {
        (meta.to_string(), String::new())
    }
}

pub(super) fn format_headers(headers: &[(String, String)]) -> String {
    if headers.is_empty() {
        return "No headers".to_string();
    }

    headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn format_cookies(headers: &[(String, String)]) -> String {
    let cookies = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    if cookies.is_empty() {
        "No cookies".to_string()
    } else {
        cookies
            .iter()
            .enumerate()
            .map(|(index, value)| format!("{} {}", index + 1, value))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub(super) fn response_status_with_assertions(
    status: u16,
    assertion_results: &[ResponseAssertionResult],
) -> String {
    if assertion_results.is_empty() {
        return format!("HTTP {status}");
    }
    let passed = assertion_results
        .iter()
        .filter(|result| result.passed)
        .count();
    format!("HTTP {status} / tests {passed}/{}", assertion_results.len())
}

pub(super) fn response_tone_with_assertions(
    status: u16,
    assertion_results: &[ResponseAssertionResult],
) -> &'static str {
    if assertion_results.iter().any(|result| !result.passed) {
        "error"
    } else {
        response_tone(status)
    }
}

pub(super) fn response_body_with_assertions(
    body: &str,
    assertion_results: &[ResponseAssertionResult],
) -> String {
    if assertion_results.is_empty() {
        return body.to_string();
    }
    format!(
        "{body}\n\nTests\n{}",
        format_assertion_results(assertion_results)
    )
}

fn format_assertion_results(assertion_results: &[ResponseAssertionResult]) -> String {
    assertion_results
        .iter()
        .map(|result| {
            let outcome = if result.passed { "PASS" } else { "FAIL" };
            match &result.error {
                Some(error) => format!("[{outcome}] {} - {error}", result.name),
                None => format!("[{outcome}] {}", result.name),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn response_tone(status: u16) -> &'static str {
    if (200..400).contains(&status) {
        "success"
    } else if status >= 400 {
        "error"
    } else {
        "neutral"
    }
}

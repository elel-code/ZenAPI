use anyhow::{Result, anyhow, bail};
use serde_json::Value;
use zenapi::assertions::{ResponseAssertion, ResponseAssertionKind};

mod js;
mod postman;

use self::postman::{normalize_pm_json_type_name, parse_pm_test_assertion_line};

pub(super) fn parse_response_assertions(input: &str) -> Result<Vec<ResponseAssertion>> {
    input
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            (!line.is_empty() && !line.starts_with('#') && !line.starts_with("//"))
                .then_some((index, line))
        })
        .map(|(index, line)| {
            parse_response_assertion_line(line)
                .map_err(|error| anyhow!("test line {}: {error}", index + 1))
        })
        .collect()
}

pub(super) fn parse_response_assertion_line(line: &str) -> Result<ResponseAssertion> {
    if let Some(assertion) = parse_pm_test_assertion_line(line)? {
        return Ok(assertion);
    }

    let (kind, args) = split_assertion_command(line)?;
    let assertion_kind = match kind {
        "status" | "status_equals" | "status=" => ResponseAssertionKind::StatusEquals {
            status: parse_status(args, kind)?,
        },
        "range" | "status_in_range" => {
            let mut parts = args.split_whitespace();
            let min = parts
                .next()
                .ok_or_else(|| anyhow!("{kind} expects min max"))
                .and_then(|value| parse_status(value, kind))?;
            let max = parts
                .next()
                .ok_or_else(|| anyhow!("{kind} expects min max"))
                .and_then(|value| parse_status(value, kind))?;
            ResponseAssertionKind::StatusInRange { min, max }
        }
        "time" | "response_time" | "response_time_below" | "response_time_under" => {
            ResponseAssertionKind::ResponseTimeBelow {
                max_ms: parse_response_time_ms(args, kind)?,
            }
        }
        "size" | "response_size" | "response_size_below" | "response_size_under" => {
            ResponseAssertionKind::ResponseSizeBelow {
                max_bytes: parse_response_size_bytes(args, kind)?,
            }
        }
        "header" | "header_exists" | "header?" => ResponseAssertionKind::HeaderExists {
            name: require_assertion_arg(args, kind)?,
        },
        "header_absent" | "header_not_exists" | "header!" => {
            ResponseAssertionKind::HeaderNotExists {
                name: require_assertion_arg(args, kind)?,
            }
        }
        "header_equals" | "header=" => {
            let (name, value) = split_first_arg(args, kind)?;
            ResponseAssertionKind::HeaderEquals { name, value }
        }
        "body_equals" | "body_exact" | "body=" => ResponseAssertionKind::BodyEquals {
            text: require_assertion_arg(args, kind)?,
        },
        "body" | "body_contains" | "body?" => ResponseAssertionKind::BodyContains {
            text: require_assertion_arg(args, kind)?,
        },
        "body_not" | "body_not_contains" | "body!" => ResponseAssertionKind::BodyNotContains {
            text: require_assertion_arg(args, kind)?,
        },
        "json_exists" | "json_path_exists" | "json?" => ResponseAssertionKind::JsonPathExists {
            path: require_assertion_arg(args, kind)?,
        },
        "json_not_exists" | "json_path_not_exists" | "json!" => {
            ResponseAssertionKind::JsonPathNotExists {
                path: require_assertion_arg(args, kind)?,
            }
        }
        "json_type" | "json_path_type" | "json_type_is" => {
            let (path, value_type) = split_first_arg(args, kind)?;
            let value_type = normalize_pm_json_type_name(&value_type)
                .ok_or_else(|| anyhow!("{kind} expects a JSON value type"))?;
            ResponseAssertionKind::JsonPathType { path, value_type }
        }
        "json_length" | "json_path_length" | "json_length_equals" => {
            let (path, length) = split_first_arg(args, kind)?;
            ResponseAssertionKind::JsonPathLength {
                path,
                length: parse_json_length(&length, kind)?,
            }
        }
        "json_contains" | "json_path_contains" | "json_includes" => {
            let (path, value) = split_first_arg(args, kind)?;
            ResponseAssertionKind::JsonPathContains {
                path,
                value: parse_json_assertion_value(&value),
            }
        }
        "json_not_contains" | "json_path_not_contains" | "json_excludes" => {
            let (path, value) = split_first_arg(args, kind)?;
            ResponseAssertionKind::JsonPathNotContains {
                path,
                value: parse_json_assertion_value(&value),
            }
        }
        "json" | "json_path_equals" | "json=" => {
            let (path, value) = split_first_arg(args, kind)?;
            ResponseAssertionKind::JsonPathEquals {
                path,
                value: parse_json_assertion_value(&value),
            }
        }
        "json_not_equals" | "json_path_not_equals" | "json!=" => {
            let (path, value) = split_first_arg(args, kind)?;
            ResponseAssertionKind::JsonPathNotEquals {
                path,
                value: parse_json_assertion_value(&value),
            }
        }
        _ => bail!("unknown assertion kind: {kind}"),
    };

    Ok(ResponseAssertion {
        name: format!("{kind} {args}"),
        kind: assertion_kind,
    })
}

fn split_assertion_command(line: &str) -> Result<(&str, &str)> {
    let Some((kind, args)) = line.split_once(char::is_whitespace) else {
        bail!("assertion needs arguments: {line}");
    };
    Ok((kind.trim(), args.trim()))
}

fn require_assertion_arg(args: &str, kind: &str) -> Result<String> {
    let args = args.trim();
    if args.is_empty() {
        bail!("{kind} expects a value");
    }
    Ok(args.to_string())
}

fn split_first_arg(args: &str, kind: &str) -> Result<(String, String)> {
    let Some((first, rest)) = args.trim().split_once(char::is_whitespace) else {
        bail!("{kind} expects target and expected value");
    };
    let first = first.trim();
    let rest = rest.trim();
    if first.is_empty() || rest.is_empty() {
        bail!("{kind} expects target and expected value");
    }
    Ok((first.to_string(), rest.to_string()))
}

fn parse_status(value: &str, kind: &str) -> Result<u16> {
    value
        .trim()
        .parse::<u16>()
        .map_err(|_| anyhow!("{kind} expects an HTTP status code"))
}

fn parse_response_time_ms(value: &str, kind: &str) -> Result<u128> {
    value
        .trim()
        .parse::<u128>()
        .map_err(|_| anyhow!("{kind} expects milliseconds"))
}

fn parse_response_size_bytes(value: &str, kind: &str) -> Result<usize> {
    value
        .trim()
        .parse::<usize>()
        .map_err(|_| anyhow!("{kind} expects bytes"))
}

fn parse_json_length(value: &str, kind: &str) -> Result<usize> {
    value
        .trim()
        .parse::<usize>()
        .map_err(|_| anyhow!("{kind} expects a JSON length"))
}

fn parse_json_assertion_value(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()))
}

fn json_path_ui_value(path: &str) -> String {
    if path.trim().is_empty() {
        "$".to_string()
    } else {
        path.to_string()
    }
}

pub(super) fn format_response_assertions(assertions: &[ResponseAssertion]) -> String {
    assertions
        .iter()
        .map(|assertion| match &assertion.kind {
            ResponseAssertionKind::StatusEquals { status } => format!("status_equals {status}"),
            ResponseAssertionKind::StatusInRange { min, max } => {
                format!("status_in_range {min} {max}")
            }
            ResponseAssertionKind::ResponseTimeBelow { max_ms } => {
                format!("response_time_below {max_ms}")
            }
            ResponseAssertionKind::ResponseSizeBelow { max_bytes } => {
                format!("response_size_below {max_bytes}")
            }
            ResponseAssertionKind::HeaderExists { name } => format!("header_exists {name}"),
            ResponseAssertionKind::HeaderNotExists { name } => {
                format!("header_not_exists {name}")
            }
            ResponseAssertionKind::HeaderEquals { name, value } => {
                format!("header_equals {name} {value}")
            }
            ResponseAssertionKind::BodyEquals { text } => format!("body_equals {text}"),
            ResponseAssertionKind::BodyContains { text } => format!("body_contains {text}"),
            ResponseAssertionKind::BodyNotContains { text } => {
                format!("body_not_contains {text}")
            }
            ResponseAssertionKind::JsonPathExists { path } => {
                format!("json_path_exists {}", json_path_ui_value(path))
            }
            ResponseAssertionKind::JsonPathNotExists { path } => {
                format!("json_path_not_exists {}", json_path_ui_value(path))
            }
            ResponseAssertionKind::JsonPathType { path, value_type } => {
                format!("json_path_type {} {value_type}", json_path_ui_value(path))
            }
            ResponseAssertionKind::JsonPathLength { path, length } => {
                format!("json_path_length {} {length}", json_path_ui_value(path))
            }
            ResponseAssertionKind::JsonPathContains { path, value } => {
                format!("json_path_contains {} {value}", json_path_ui_value(path))
            }
            ResponseAssertionKind::JsonPathNotContains { path, value } => {
                format!(
                    "json_path_not_contains {} {value}",
                    json_path_ui_value(path)
                )
            }
            ResponseAssertionKind::JsonPathEquals { path, value } => {
                format!("json_path_equals {} {value}", json_path_ui_value(path))
            }
            ResponseAssertionKind::JsonPathNotEquals { path, value } => {
                format!("json_path_not_equals {} {value}", json_path_ui_value(path))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

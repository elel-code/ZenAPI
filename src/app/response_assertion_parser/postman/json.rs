use serde_json::Value;
use zenapi::assertions::ResponseAssertionKind;

use super::super::{
    js::{
        call_argument_after_subject, expect_equal_argument_after_subject,
        expect_not_equal_argument_after_subject, is_js_identifier_boundary, join_json_path,
        parse_js_member_path, parse_js_string_literal, split_js_arguments, strip_js_string_value,
        trim_js_subject_suffix,
    },
    parse_json_assertion_value, parse_json_length,
};

pub(super) fn parse_pm_json_assertion(body: &str) -> Option<ResponseAssertionKind> {
    if let Some(kind) = parse_pm_json_property_assertion(body) {
        return Some(kind);
    }
    if let Some(kind) = parse_pm_json_type_assertion(body) {
        return Some(kind);
    }
    if let Some(kind) = parse_pm_json_length_assertion(body) {
        return Some(kind);
    }
    if let Some(kind) = parse_pm_json_contains_assertion(body) {
        return Some(kind);
    }

    let (path, after_path) = parse_pm_json_expect_subject(body)?;
    let chain = trim_js_subject_suffix(after_path);
    if let Some(value) = expect_not_equal_argument_after_subject(after_path) {
        return Some(ResponseAssertionKind::JsonPathNotEquals {
            path,
            value: parse_pm_json_assertion_value(value),
        });
    }

    let value = if let Some(value) = expect_equal_argument_after_subject(after_path) {
        parse_pm_json_assertion_value(value)
    } else if chain.starts_with(".to.be.true") {
        Value::Bool(true)
    } else if chain.starts_with(".to.be.false") {
        Value::Bool(false)
    } else if chain.starts_with(".to.be.null") {
        Value::Null
    } else {
        return parse_pm_json_exists_assertion(path, chain);
    };

    Some(ResponseAssertionKind::JsonPathEquals { path, value })
}

fn parse_pm_json_exists_assertion(path: String, chain: &str) -> Option<ResponseAssertionKind> {
    if chain.starts_with(".to.not.exist") || chain.starts_with(".to.be.undefined") {
        return Some(ResponseAssertionKind::JsonPathNotExists { path });
    }

    (chain.starts_with(".to.exist") || chain.starts_with(".to.not.be.undefined"))
        .then_some(ResponseAssertionKind::JsonPathExists { path })
}

fn parse_pm_json_type_assertion(body: &str) -> Option<ResponseAssertionKind> {
    if let Some(subject) = parse_pm_json_direct_subject(body, true) {
        if let Some(assertion) = parse_pm_json_type_from_subject(subject) {
            return Some(assertion);
        }
    }

    parse_pm_json_alias_subject(body, true).and_then(parse_pm_json_type_from_subject)
}

fn parse_pm_json_type_from_subject(
    (path, after_subject): (String, &str),
) -> Option<ResponseAssertionKind> {
    for marker in [".to.be.an(", ".to.be.a("] {
        if let Some(value) = call_argument_after_subject(after_subject, marker) {
            return normalize_pm_json_type_name(&strip_js_string_value(value))
                .map(|value_type| ResponseAssertionKind::JsonPathType { path, value_type });
        }
    }
    None
}

pub(in crate::app::response_assertion_parser) fn normalize_pm_json_type_name(
    value_type: &str,
) -> Option<String> {
    match value_type.trim().to_ascii_lowercase().as_str() {
        "array" => Some("array".to_string()),
        "bool" | "boolean" => Some("boolean".to_string()),
        "integer" | "int" | "number" | "num" => Some("number".to_string()),
        "null" => Some("null".to_string()),
        "object" => Some("object".to_string()),
        "string" | "str" => Some("string".to_string()),
        _ => None,
    }
}

fn parse_pm_json_length_assertion(body: &str) -> Option<ResponseAssertionKind> {
    if let Some(subject) = parse_pm_json_direct_subject(body, true) {
        if let Some(assertion) = parse_pm_json_length_from_subject(subject) {
            return Some(assertion);
        }
    }

    parse_pm_json_alias_subject(body, true).and_then(parse_pm_json_length_from_subject)
}

fn parse_pm_json_length_from_subject(
    (path, after_subject): (String, &str),
) -> Option<ResponseAssertionKind> {
    for marker in [".to.have.lengthOf(", ".to.have.length("] {
        if let Some(value) = call_argument_after_subject(after_subject, marker) {
            return parse_json_length(value, "pm.expect JSON length")
                .ok()
                .map(|length| ResponseAssertionKind::JsonPathLength { path, length });
        }
    }

    let base_path = path.strip_suffix(".length")?;
    expect_equal_argument_after_subject(after_subject)
        .and_then(|value| parse_json_length(value, "pm.expect JSON length").ok())
        .map(|length| ResponseAssertionKind::JsonPathLength {
            path: base_path.to_string(),
            length,
        })
}

fn parse_pm_json_contains_assertion(body: &str) -> Option<ResponseAssertionKind> {
    if let Some(subject) = parse_pm_json_direct_subject(body, true) {
        if let Some(assertion) = parse_pm_json_contains_from_subject(subject) {
            return Some(assertion);
        }
    }

    parse_pm_json_alias_subject(body, true).and_then(parse_pm_json_contains_from_subject)
}

fn parse_pm_json_contains_from_subject(
    (path, after_subject): (String, &str),
) -> Option<ResponseAssertionKind> {
    for marker in [
        ".to.not.deep.include(",
        ".to.not.deep.contain(",
        ".to.not.include(",
        ".to.not.contain(",
    ] {
        if let Some(value) = call_argument_after_subject(after_subject, marker) {
            return Some(ResponseAssertionKind::JsonPathNotContains {
                path,
                value: parse_pm_json_assertion_value(value),
            });
        }
    }

    for marker in [
        ".to.deep.include(",
        ".to.deep.contain(",
        ".to.include(",
        ".to.contain(",
    ] {
        if let Some(value) = call_argument_after_subject(after_subject, marker) {
            return Some(ResponseAssertionKind::JsonPathContains {
                path,
                value: parse_pm_json_assertion_value(value),
            });
        }
    }

    None
}

fn parse_pm_json_expect_subject(body: &str) -> Option<(String, &str)> {
    parse_pm_json_direct_subject(body, false).or_else(|| parse_pm_json_alias_subject(body, false))
}

fn parse_pm_json_assertion_value(value: &str) -> Value {
    parse_js_string_literal(value.trim())
        .map(|(value, _)| Value::String(value))
        .unwrap_or_else(|| parse_json_assertion_value(value.trim()))
}

fn parse_pm_json_property_assertion(body: &str) -> Option<ResponseAssertionKind> {
    if let Some(subject) = parse_pm_json_direct_subject(body, true) {
        if let Some(assertion) = parse_pm_json_property_from_subject(subject) {
            return Some(assertion);
        }
    }

    parse_pm_json_alias_subject(body, true).and_then(parse_pm_json_property_from_subject)
}

fn parse_pm_json_property_from_subject(
    (base_path, after_subject): (String, &str),
) -> Option<ResponseAssertionKind> {
    let args = call_argument_after_subject(after_subject, ".to.have.property(")?;
    let parts = split_js_arguments(args);
    if parts.is_empty() {
        return None;
    }
    let property = strip_js_string_value(parts[0]);
    let path = join_json_path(&base_path, &property);
    if let Some(value) = parts.get(1) {
        Some(ResponseAssertionKind::JsonPathEquals {
            path,
            value: parse_pm_json_assertion_value(value),
        })
    } else {
        Some(ResponseAssertionKind::JsonPathExists { path })
    }
}

fn parse_pm_json_direct_subject(body: &str, allow_empty_path: bool) -> Option<(String, &str)> {
    let marker = "pm.response.json()";
    let after_json = &body[body.find(marker)? + marker.len()..];
    let (path, consumed) = parse_js_member_path(after_json).unwrap_or_default();
    if path.is_empty() && !allow_empty_path {
        return None;
    }
    Some((path, &after_json[consumed..]))
}

fn parse_pm_json_alias_subject<'a>(
    body: &'a str,
    allow_empty_path: bool,
) -> Option<(String, &'a str)> {
    for alias in parse_pm_json_aliases(body) {
        let mut search_start = 0usize;
        while let Some(relative_start) = body[search_start..].find(alias) {
            let start = search_start + relative_start;
            let after_alias_start = start + alias.len();
            search_start = after_alias_start;

            if !is_js_identifier_boundary(body, start, after_alias_start) {
                continue;
            }

            let after_alias = &body[after_alias_start..];
            let (path, consumed) = parse_js_member_path(after_alias).unwrap_or_default();
            if path.is_empty() && !allow_empty_path {
                continue;
            }

            let after_subject = &after_alias[consumed..];
            if trim_js_subject_suffix(after_subject).starts_with(".to.") {
                return Some((path, after_subject));
            }
        }
    }

    None
}

fn parse_pm_json_aliases(body: &str) -> Vec<&str> {
    let mut aliases = Vec::new();

    for declaration in ["const ", "let ", "var "] {
        let mut search_start = 0usize;
        while let Some(relative_start) = body[search_start..].find(declaration) {
            let start = search_start + relative_start + declaration.len();
            search_start = start;

            let rest = body[start..].trim_start();
            let skipped = body[start..].len() - rest.len();
            let alias_start = start + skipped;
            let alias_len = rest
                .chars()
                .take_while(|character| {
                    character.is_ascii_alphanumeric() || *character == '_' || *character == '$'
                })
                .map(char::len_utf8)
                .sum::<usize>();
            if alias_len == 0 {
                continue;
            }

            let alias = &body[alias_start..alias_start + alias_len];
            let after_alias = body[alias_start + alias_len..].trim_start();
            let Some(after_equals) = after_alias.strip_prefix('=') else {
                continue;
            };
            if after_equals.trim_start().starts_with("pm.response.json()") {
                aliases.push(alias);
            }
        }
    }

    aliases
}

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::ClientResponse;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseAssertion {
    pub name: String,
    #[serde(flatten)]
    pub kind: ResponseAssertionKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResponseAssertionKind {
    StatusEquals { status: u16 },
    StatusInRange { min: u16, max: u16 },
    ResponseTimeBelow { max_ms: u128 },
    ResponseSizeBelow { max_bytes: usize },
    HeaderExists { name: String },
    HeaderNotExists { name: String },
    HeaderEquals { name: String, value: String },
    BodyEquals { text: String },
    BodyContains { text: String },
    BodyNotContains { text: String },
    JsonPathExists { path: String },
    JsonPathNotExists { path: String },
    JsonPathType { path: String, value_type: String },
    JsonPathLength { path: String, length: usize },
    JsonPathContains { path: String, value: Value },
    JsonPathNotContains { path: String, value: Value },
    JsonPathEquals { path: String, value: Value },
    JsonPathNotEquals { path: String, value: Value },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseAssertionResult {
    pub name: String,
    pub passed: bool,
    pub error: Option<String>,
}

pub fn evaluate_response_assertions(
    response: &ClientResponse,
    assertions: &[ResponseAssertion],
) -> Vec<ResponseAssertionResult> {
    assertions
        .iter()
        .map(|assertion| evaluate_response_assertion(response, assertion))
        .collect()
}

pub fn evaluate_response_assertion(
    response: &ClientResponse,
    assertion: &ResponseAssertion,
) -> ResponseAssertionResult {
    let error = match &assertion.kind {
        ResponseAssertionKind::StatusEquals { status } => (response.status != *status)
            .then(|| format!("expected status {status}, got {}", response.status)),
        ResponseAssertionKind::StatusInRange { min, max } => {
            (!(response.status >= *min && response.status <= *max)).then(|| {
                format!(
                    "expected status between {min} and {max}, got {}",
                    response.status
                )
            })
        }
        ResponseAssertionKind::ResponseTimeBelow { max_ms } => (response.elapsed_ms > *max_ms)
            .then(|| {
                format!(
                    "expected response time <= {max_ms} ms, got {} ms",
                    response.elapsed_ms
                )
            }),
        ResponseAssertionKind::ResponseSizeBelow { max_bytes } => {
            (response.body_bytes > *max_bytes).then(|| {
                format!(
                    "expected response size <= {max_bytes} B, got {} B",
                    response.body_bytes
                )
            })
        }
        ResponseAssertionKind::HeaderExists { name } => find_header(response, name)
            .is_none()
            .then(|| format!("missing header {name}")),
        ResponseAssertionKind::HeaderNotExists { name } => find_header(response, name)
            .is_some()
            .then(|| format!("unexpected header {name}")),
        ResponseAssertionKind::HeaderEquals { name, value } => match find_header(response, name) {
            Some(actual) if actual == value => None,
            Some(actual) => Some(format!("expected header {name}={value}, got {actual}")),
            None => Some(format!("missing header {name}")),
        },
        ResponseAssertionKind::BodyEquals { text } => (response.raw_body != *text)
            .then(|| "response body does not equal expected text".to_string()),
        ResponseAssertionKind::BodyContains { text } => (!response.raw_body.contains(text))
            .then(|| format!("response body does not contain {text:?}")),
        ResponseAssertionKind::BodyNotContains { text } => response
            .raw_body
            .contains(text)
            .then(|| format!("response body contains forbidden text {text:?}")),
        ResponseAssertionKind::JsonPathExists { path } => {
            match serde_json::from_str::<Value>(&response.raw_body) {
                Ok(json) => json_path_value(&json, path)
                    .is_none()
                    .then(|| format!("missing JSON path {}", display_json_path(path))),
                Err(error) => Some(format!("failed to parse response JSON: {error}")),
            }
        }
        ResponseAssertionKind::JsonPathNotExists { path } => {
            match serde_json::from_str::<Value>(&response.raw_body) {
                Ok(json) => json_path_value(&json, path)
                    .is_some()
                    .then(|| format!("unexpected JSON path {}", display_json_path(path))),
                Err(error) => Some(format!("failed to parse response JSON: {error}")),
            }
        }
        ResponseAssertionKind::JsonPathType { path, value_type } => {
            match serde_json::from_str::<Value>(&response.raw_body) {
                Ok(json) => match normalize_json_value_type(value_type) {
                    Some(expected) => match json_path_value(&json, path) {
                        Some(actual) if json_value_type(actual) == expected => None,
                        Some(actual) => Some(format!(
                            "expected JSON path {} to be {expected}, got {}",
                            display_json_path(path),
                            json_value_type(actual)
                        )),
                        None => Some(format!("missing JSON path {}", display_json_path(path))),
                    },
                    None => Some(format!("unsupported JSON value type {value_type}")),
                },
                Err(error) => Some(format!("failed to parse response JSON: {error}")),
            }
        }
        ResponseAssertionKind::JsonPathLength { path, length } => {
            match serde_json::from_str::<Value>(&response.raw_body) {
                Ok(json) => match json_path_value(&json, path) {
                    Some(actual) => match json_value_length(actual) {
                        Some(actual_length) if actual_length == *length => None,
                        Some(actual_length) => Some(format!(
                            "expected JSON path {} length to be {length}, got {actual_length}",
                            display_json_path(path)
                        )),
                        None => Some(format!(
                            "JSON path {} has no length",
                            display_json_path(path)
                        )),
                    },
                    None => Some(format!("missing JSON path {}", display_json_path(path))),
                },
                Err(error) => Some(format!("failed to parse response JSON: {error}")),
            }
        }
        ResponseAssertionKind::JsonPathContains { path, value } => {
            match serde_json::from_str::<Value>(&response.raw_body) {
                Ok(json) => match json_path_value(&json, path) {
                    Some(actual) if json_value_contains(actual, value) => None,
                    Some(actual) => Some(format!(
                        "expected JSON path {} to contain {value}, got {actual}",
                        display_json_path(path)
                    )),
                    None => Some(format!("missing JSON path {}", display_json_path(path))),
                },
                Err(error) => Some(format!("failed to parse response JSON: {error}")),
            }
        }
        ResponseAssertionKind::JsonPathNotContains { path, value } => {
            match serde_json::from_str::<Value>(&response.raw_body) {
                Ok(json) => match json_path_value(&json, path) {
                    Some(actual) if json_value_contains(actual, value) => Some(format!(
                        "expected JSON path {} to not contain {value}, got {actual}",
                        display_json_path(path)
                    )),
                    Some(_) => None,
                    None => Some(format!("missing JSON path {}", display_json_path(path))),
                },
                Err(error) => Some(format!("failed to parse response JSON: {error}")),
            }
        }
        ResponseAssertionKind::JsonPathEquals { path, value } => {
            match serde_json::from_str::<Value>(&response.raw_body) {
                Ok(json) => match json_path_value(&json, path) {
                    Some(actual) if actual == value => None,
                    Some(actual) => Some(format!(
                        "expected JSON path {} to equal {value}, got {actual}",
                        display_json_path(path)
                    )),
                    None => Some(format!("missing JSON path {}", display_json_path(path))),
                },
                Err(error) => Some(format!("failed to parse response JSON: {error}")),
            }
        }
        ResponseAssertionKind::JsonPathNotEquals { path, value } => {
            match serde_json::from_str::<Value>(&response.raw_body) {
                Ok(json) => match json_path_value(&json, path) {
                    Some(actual) if actual == value => Some(format!(
                        "expected JSON path {} to not equal {value}",
                        display_json_path(path)
                    )),
                    Some(_) => None,
                    None => Some(format!("missing JSON path {}", display_json_path(path))),
                },
                Err(error) => Some(format!("failed to parse response JSON: {error}")),
            }
        }
    };

    ResponseAssertionResult {
        name: assertion.name.clone(),
        passed: error.is_none(),
        error,
    }
}

fn find_header<'a>(response: &'a ClientResponse, name: &str) -> Option<&'a str> {
    response
        .headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name.trim()))
        .map(|(_, value)| value.as_str())
}

fn json_path_value<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    let path = normalize_json_path(path);
    if path.is_empty() {
        return Some(current);
    }

    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = if let Ok(index) = segment.parse::<usize>() {
            current.as_array()?.get(index)?
        } else {
            current.as_object()?.get(segment)?
        };
    }

    Some(current)
}

fn normalize_json_path(path: &str) -> &str {
    let path = path.trim();
    if path.is_empty() || path == "$" || path == "." {
        return "";
    }
    path.strip_prefix("$.")
        .or_else(|| path.strip_prefix('.'))
        .unwrap_or(path)
}

fn display_json_path(path: &str) -> &str {
    let path = path.trim();
    if path.is_empty() { "$" } else { path }
}

fn normalize_json_value_type(value_type: &str) -> Option<&'static str> {
    match value_type.trim().to_ascii_lowercase().as_str() {
        "array" => Some("array"),
        "bool" | "boolean" => Some("boolean"),
        "integer" | "int" | "number" | "num" => Some("number"),
        "null" => Some("null"),
        "object" => Some("object"),
        "string" | "str" => Some("string"),
        _ => None,
    }
}

fn json_value_type(value: &Value) -> &'static str {
    match value {
        Value::Array(_) => "array",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
        Value::Number(_) => "number",
        Value::Object(_) => "object",
        Value::String(_) => "string",
    }
}

fn json_value_length(value: &Value) -> Option<usize> {
    match value {
        Value::Array(values) => Some(values.len()),
        Value::Object(values) => Some(values.len()),
        Value::String(value) => Some(value.chars().count()),
        _ => None,
    }
}

fn json_value_contains(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Array(values), expected) => values.iter().any(|value| value == expected),
        (Value::Object(actual), Value::Object(expected)) => expected
            .iter()
            .all(|(key, value)| actual.get(key) == Some(value)),
        (Value::Object(actual), Value::String(key)) => actual.contains_key(key),
        (Value::String(actual), Value::String(expected)) => actual.contains(expected),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response() -> ClientResponse {
        ClientResponse {
            status: 200,
            elapsed_ms: 12,
            body_bytes: 30,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            raw_body: r#"{"user":{"name":"Zen"},"items":[{"id":1}]}"#.to_string(),
            body: "{\n  \"user\": {}\n}".to_string(),
        }
    }

    #[test]
    fn evaluates_status_headers_body_assertions() {
        let assertions = vec![
            ResponseAssertion {
                name: "status".to_string(),
                kind: ResponseAssertionKind::StatusEquals { status: 200 },
            },
            ResponseAssertion {
                name: "header".to_string(),
                kind: ResponseAssertionKind::HeaderEquals {
                    name: "Content-Type".to_string(),
                    value: "application/json".to_string(),
                },
            },
            ResponseAssertion {
                name: "header absent".to_string(),
                kind: ResponseAssertionKind::HeaderNotExists {
                    name: "X-Debug".to_string(),
                },
            },
            ResponseAssertion {
                name: "time".to_string(),
                kind: ResponseAssertionKind::ResponseTimeBelow { max_ms: 50 },
            },
            ResponseAssertion {
                name: "size".to_string(),
                kind: ResponseAssertionKind::ResponseSizeBelow { max_bytes: 64 },
            },
            ResponseAssertion {
                name: "body exact".to_string(),
                kind: ResponseAssertionKind::BodyEquals {
                    text: r#"{"user":{"name":"Zen"},"items":[{"id":1}]}"#.to_string(),
                },
            },
            ResponseAssertion {
                name: "body".to_string(),
                kind: ResponseAssertionKind::BodyContains {
                    text: "Zen".to_string(),
                },
            },
            ResponseAssertion {
                name: "body absent".to_string(),
                kind: ResponseAssertionKind::BodyNotContains {
                    text: "error".to_string(),
                },
            },
        ];

        let results = evaluate_response_assertions(&response(), &assertions);

        assert!(results.iter().all(|result| result.passed));
    }

    #[test]
    fn evaluates_json_path_assertions() {
        let exists = ResponseAssertion {
            name: "exists".to_string(),
            kind: ResponseAssertionKind::JsonPathExists {
                path: "user.name".to_string(),
            },
        };
        let passing = ResponseAssertion {
            name: "json".to_string(),
            kind: ResponseAssertionKind::JsonPathEquals {
                path: "items.0.id".to_string(),
                value: Value::from(1),
            },
        };
        let typed = ResponseAssertion {
            name: "type".to_string(),
            kind: ResponseAssertionKind::JsonPathType {
                path: "$.items".to_string(),
                value_type: "array".to_string(),
            },
        };
        let length = ResponseAssertion {
            name: "length".to_string(),
            kind: ResponseAssertionKind::JsonPathLength {
                path: "items".to_string(),
                length: 1,
            },
        };
        let contains = ResponseAssertion {
            name: "contains".to_string(),
            kind: ResponseAssertionKind::JsonPathContains {
                path: "items".to_string(),
                value: serde_json::json!({ "id": 1 }),
            },
        };
        let not_exists = ResponseAssertion {
            name: "not exists".to_string(),
            kind: ResponseAssertionKind::JsonPathNotExists {
                path: "error".to_string(),
            },
        };
        let not_contains = ResponseAssertion {
            name: "not contains".to_string(),
            kind: ResponseAssertionKind::JsonPathNotContains {
                path: "items".to_string(),
                value: serde_json::json!({ "id": 999 }),
            },
        };
        let not_equals = ResponseAssertion {
            name: "not equals".to_string(),
            kind: ResponseAssertionKind::JsonPathNotEquals {
                path: "user.name".to_string(),
                value: Value::from("Other"),
            },
        };
        let failing = ResponseAssertion {
            name: "wrong".to_string(),
            kind: ResponseAssertionKind::JsonPathEquals {
                path: "user.name".to_string(),
                value: Value::from("Other"),
            },
        };

        assert!(evaluate_response_assertion(&response(), &exists).passed);
        assert!(evaluate_response_assertion(&response(), &passing).passed);
        assert!(evaluate_response_assertion(&response(), &typed).passed);
        assert!(evaluate_response_assertion(&response(), &length).passed);
        assert!(evaluate_response_assertion(&response(), &contains).passed);
        assert!(evaluate_response_assertion(&response(), &not_exists).passed);
        assert!(evaluate_response_assertion(&response(), &not_contains).passed);
        assert!(evaluate_response_assertion(&response(), &not_equals).passed);
        let result = evaluate_response_assertion(&response(), &failing);
        assert!(!result.passed);
        assert!(
            result
                .error
                .unwrap_or_default()
                .contains("expected JSON path user.name")
        );
    }

    #[test]
    fn reports_missing_header_and_invalid_json() {
        let missing = ResponseAssertion {
            name: "header".to_string(),
            kind: ResponseAssertionKind::HeaderExists {
                name: "x-token".to_string(),
            },
        };
        assert!(!evaluate_response_assertion(&response(), &missing).passed);

        let invalid_json = ClientResponse {
            raw_body: "not-json".to_string(),
            ..response()
        };
        let json = ResponseAssertion {
            name: "json".to_string(),
            kind: ResponseAssertionKind::JsonPathEquals {
                path: "ok".to_string(),
                value: Value::from(true),
            },
        };
        assert!(!evaluate_response_assertion(&invalid_json, &json).passed);
    }
}

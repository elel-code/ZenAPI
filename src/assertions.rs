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
    HeaderExists { name: String },
    HeaderEquals { name: String, value: String },
    BodyContains { text: String },
    JsonPathEquals { path: String, value: Value },
    BodyBytesLessThan { max: usize },
    ElapsedLessThan { max_ms: u128 },
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
        ResponseAssertionKind::HeaderExists { name } => find_header(response, name)
            .is_none()
            .then(|| format!("missing header {name}")),
        ResponseAssertionKind::HeaderEquals { name, value } => match find_header(response, name) {
            Some(actual) if actual == value => None,
            Some(actual) => Some(format!("expected header {name}={value}, got {actual}")),
            None => Some(format!("missing header {name}")),
        },
        ResponseAssertionKind::BodyContains { text } => (!response.raw_body.contains(text))
            .then(|| format!("response body does not contain {text:?}")),
        ResponseAssertionKind::JsonPathEquals { path, value } => {
            match serde_json::from_str::<Value>(&response.raw_body) {
                Ok(json) => match json_path_value(&json, path) {
                    Some(actual) if actual == value => None,
                    Some(actual) => Some(format!(
                        "expected JSON path {path} to equal {value}, got {actual}"
                    )),
                    None => Some(format!("missing JSON path {path}")),
                },
                Err(error) => Some(format!("failed to parse response JSON: {error}")),
            }
        }
        ResponseAssertionKind::BodyBytesLessThan { max } => {
            (response.body_bytes >= *max).then(|| {
                format!(
                    "expected body size < {max} B, got {} B",
                    response.body_bytes
                )
            })
        }
        ResponseAssertionKind::ElapsedLessThan { max_ms } => {
            (response.elapsed_ms >= *max_ms).then(|| {
                format!(
                    "expected elapsed < {max_ms} ms, got {} ms",
                    response.elapsed_ms
                )
            })
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

    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = if let Ok(index) = segment.parse::<usize>() {
            current.as_array()?.get(index)?
        } else {
            current.as_object()?.get(segment)?
        };
    }

    Some(current)
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
    fn evaluates_status_headers_body_and_timing_assertions() {
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
                name: "body".to_string(),
                kind: ResponseAssertionKind::BodyContains {
                    text: "Zen".to_string(),
                },
            },
            ResponseAssertion {
                name: "elapsed".to_string(),
                kind: ResponseAssertionKind::ElapsedLessThan { max_ms: 50 },
            },
        ];

        let results = evaluate_response_assertions(&response(), &assertions);

        assert!(results.iter().all(|result| result.passed));
    }

    #[test]
    fn evaluates_json_path_assertions() {
        let passing = ResponseAssertion {
            name: "json".to_string(),
            kind: ResponseAssertionKind::JsonPathEquals {
                path: "items.0.id".to_string(),
                value: Value::from(1),
            },
        };
        let failing = ResponseAssertion {
            name: "wrong".to_string(),
            kind: ResponseAssertionKind::JsonPathEquals {
                path: "user.name".to_string(),
                value: Value::from("Other"),
            },
        };

        assert!(evaluate_response_assertion(&response(), &passing).passed);
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

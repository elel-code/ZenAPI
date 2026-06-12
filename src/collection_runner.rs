use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, sleep};

use crate::{
    assertions::{ResponseAssertionResult, evaluate_response_assertions},
    client::{RequestBody, send_request_with_body},
    codegen::CodegenRequest,
    collections::{ApiCollection, CollectionBody, CollectionItem, CollectionRequest, NameValue},
    pre_request::{execute_pre_request_actions, resolve_codegen_request_templates},
    variables::VariableStore,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureStrategy {
    #[default]
    Continue,
    StopOnFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerOptions {
    pub delay_ms: u64,
    pub failure_strategy: FailureStrategy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionRunRequest {
    pub index: usize,
    pub path: Vec<String>,
    pub request: CollectionRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionRunResult {
    pub index: usize,
    pub path: Vec<String>,
    pub name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub success: bool,
    pub elapsed_ms: u128,
    pub body_bytes: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pre_request_actions: Vec<String>,
    pub assertions: Vec<ResponseAssertionResult>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCollectionRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub query_params: Vec<(String, String)>,
    pub body: RequestBody,
    pub pre_request_actions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionRunSummary {
    pub collection_name: String,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub stopped_early: bool,
    pub elapsed_ms: u128,
    pub results: Vec<CollectionRunResult>,
}

pub fn collect_collection_requests(collection: &ApiCollection) -> Vec<CollectionRunRequest> {
    let mut requests = Vec::new();
    let mut path = vec![collection.name.clone()];
    collect_items(&collection.items, &mut path, &mut requests);
    requests
}

pub async fn run_collection(
    collection: &ApiCollection,
    variables: &VariableStore,
    active_environment: Option<&str>,
    options: RunnerOptions,
) -> CollectionRunSummary {
    let started = Instant::now();
    let requests = collect_collection_requests(collection);
    let total = requests.len();
    let mut results = Vec::with_capacity(total);
    let mut stopped_early = false;

    for (position, run_request) in requests.into_iter().enumerate() {
        if position > 0 && options.delay_ms > 0 {
            sleep(Duration::from_millis(options.delay_ms)).await;
        }

        let result = run_collection_request(&run_request, variables, active_environment).await;
        let should_stop =
            !result.success && options.failure_strategy == FailureStrategy::StopOnFailure;
        results.push(result);

        if should_stop {
            stopped_early = results.len() < total;
            break;
        }
    }

    let passed = results.iter().filter(|result| result.success).count();
    let failed = results.len().saturating_sub(passed);

    CollectionRunSummary {
        collection_name: collection.name.clone(),
        total,
        passed,
        failed,
        stopped_early,
        elapsed_ms: started.elapsed().as_millis(),
        results,
    }
}

pub fn collection_request_to_client_request(
    request: &CollectionRequest,
    variables: &VariableStore,
    active_environment: Option<&str>,
) -> Result<(
    String,
    String,
    Vec<(String, String)>,
    Vec<(String, String)>,
    RequestBody,
)> {
    let resolved = resolve_collection_request(request, variables, active_environment)?;
    Ok((
        resolved.method,
        resolved.url,
        resolved.headers,
        resolved.query_params,
        resolved.body,
    ))
}

pub fn resolve_collection_request(
    request: &CollectionRequest,
    variables: &VariableStore,
    active_environment: Option<&str>,
) -> Result<ResolvedCollectionRequest> {
    let raw_request = CodegenRequest {
        method: request.method.clone(),
        url: request.url.clone(),
        headers: name_values_to_pairs(&request.headers),
        query_params: name_values_to_pairs(&request.query_params),
        body: collection_body_to_unresolved_request_body(&request.body),
    };
    let execution = execute_pre_request_actions(
        &request.pre_request_script,
        raw_request,
        variables.clone(),
        active_environment,
    )
    .with_context(|| "pre-request failed")?;
    let resolved = resolve_codegen_request_templates(
        execution.request,
        &execution.variables,
        active_environment,
    )
    .with_context(|| "request template resolution failed")?;

    let method = resolved.method;
    let url = resolved.url;
    if url.trim().is_empty() {
        return Err(anyhow!("collection request URL is empty"));
    }
    Ok(ResolvedCollectionRequest {
        method,
        url,
        headers: resolved.headers,
        query_params: resolved.query_params,
        body: resolved.body,
        pre_request_actions: execution
            .actions
            .iter()
            .map(|action| format!("{} {}", action.action, action.target))
            .collect(),
    })
}

impl Default for RunnerOptions {
    fn default() -> Self {
        Self {
            delay_ms: 0,
            failure_strategy: FailureStrategy::Continue,
        }
    }
}

fn collect_items(
    items: &[CollectionItem],
    path: &mut Vec<String>,
    requests: &mut Vec<CollectionRunRequest>,
) {
    for item in items {
        match item {
            CollectionItem::Folder(folder) => {
                path.push(folder.name.clone());
                collect_items(&folder.items, path, requests);
                path.pop();
            }
            CollectionItem::Request(request) => {
                let mut request_path = path.clone();
                request_path.push(request.name.clone());
                requests.push(CollectionRunRequest {
                    index: requests.len(),
                    path: request_path,
                    request: request.clone(),
                });
            }
        }
    }
}

async fn run_collection_request(
    run_request: &CollectionRunRequest,
    variables: &VariableStore,
    active_environment: Option<&str>,
) -> CollectionRunResult {
    let request_name = run_request.request.name.clone();
    let fallback_method = run_request.request.method.clone();
    let fallback_url = run_request.request.url.clone();

    match resolve_collection_request(&run_request.request, variables, active_environment) {
        Ok(resolved) => {
            match send_request_with_body(
                &resolved.method,
                &resolved.url,
                &resolved.headers,
                &resolved.query_params,
                resolved.body,
            )
            .await
            {
                Ok(response) => {
                    let assertions =
                        evaluate_response_assertions(&response, &run_request.request.tests);
                    let assertion_failed =
                        assertions.iter().filter(|result| !result.passed).count();
                    let success = if assertions.is_empty() {
                        (200..400).contains(&response.status)
                    } else {
                        assertion_failed == 0
                    };
                    CollectionRunResult {
                        index: run_request.index,
                        path: run_request.path.clone(),
                        name: request_name,
                        method: resolved.method,
                        url: resolved.url,
                        status: Some(response.status),
                        success,
                        elapsed_ms: response.elapsed_ms,
                        body_bytes: response.body_bytes,
                        pre_request_actions: resolved.pre_request_actions,
                        assertions,
                        error: (assertion_failed > 0)
                            .then(|| format!("{assertion_failed} assertion(s) failed")),
                    }
                }
                Err(error) => CollectionRunResult {
                    index: run_request.index,
                    path: run_request.path.clone(),
                    name: request_name,
                    method: resolved.method,
                    url: resolved.url,
                    status: None,
                    success: false,
                    elapsed_ms: 0,
                    body_bytes: 0,
                    pre_request_actions: resolved.pre_request_actions,
                    assertions: Vec::new(),
                    error: Some(error.to_string()),
                },
            }
        }
        Err(error) => CollectionRunResult {
            index: run_request.index,
            path: run_request.path.clone(),
            name: request_name,
            method: fallback_method,
            url: fallback_url,
            status: None,
            success: false,
            elapsed_ms: 0,
            body_bytes: 0,
            pre_request_actions: Vec::new(),
            assertions: Vec::new(),
            error: Some(error.to_string()),
        },
    }
}

fn name_values_to_pairs(pairs: &[NameValue]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|pair| (pair.name.clone(), pair.value.clone()))
        .collect()
}

fn collection_body_to_unresolved_request_body(body: &CollectionBody) -> RequestBody {
    match body {
        CollectionBody::None => RequestBody::None,
        CollectionBody::Raw { content_type, body } => RequestBody::Raw {
            content_type: Some(content_type.clone()),
            body: body.clone(),
        },
        CollectionBody::FormData { fields } => RequestBody::Multipart(name_values_to_pairs(fields)),
        CollectionBody::UrlEncoded { fields } => {
            RequestBody::FormUrlEncoded(name_values_to_pairs(fields))
        }
        CollectionBody::Binary { path, content_type } => RequestBody::BinaryFile {
            path: path.clone(),
            content_type: Some(content_type.clone()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assertions::{ResponseAssertion, ResponseAssertionKind};
    use axum::{
        Json, Router,
        http::StatusCode,
        routing::{get, post},
    };
    use serde_json::{Value, json};
    use tokio::net::TcpListener;

    fn request(name: &str, method: &str, url: String) -> CollectionRequest {
        CollectionRequest {
            name: name.to_string(),
            method: method.to_string(),
            url,
            headers: Vec::new(),
            query_params: Vec::new(),
            body: CollectionBody::None,
            pre_request_script: String::new(),
            tests: Vec::new(),
        }
    }

    #[test]
    fn collects_nested_requests_in_depth_first_order() {
        let collection = ApiCollection {
            name: "Demo".to_string(),
            description: String::new(),
            items: vec![
                CollectionItem::Folder(crate::collections::CollectionFolder {
                    name: "Users".to_string(),
                    description: String::new(),
                    items: vec![CollectionItem::Request(request(
                        "List",
                        "GET",
                        "https://example.com/users".to_string(),
                    ))],
                }),
                CollectionItem::Request(request(
                    "Health",
                    "GET",
                    "https://example.com/health".to_string(),
                )),
            ],
        };

        let requests = collect_collection_requests(&collection);

        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].path, vec!["Demo", "Users", "List"]);
        assert_eq!(requests[1].path, vec!["Demo", "Health"]);
        assert_eq!(requests[1].index, 1);
    }

    #[test]
    fn resolves_collection_request_variables_into_client_request() {
        let mut variables = VariableStore::new();
        variables.upsert(crate::variables::Variable::environment(
            "dev",
            "baseUrl",
            "http://localhost:8080",
        ));
        variables.upsert(crate::variables::Variable::global("token", "secret"));
        let request = CollectionRequest {
            name: "Create".to_string(),
            method: "POST".to_string(),
            url: "{{baseUrl}}/users".to_string(),
            headers: vec![NameValue {
                name: "Authorization".to_string(),
                value: "Bearer {{token}}".to_string(),
            }],
            query_params: vec![NameValue {
                name: "debug".to_string(),
                value: "true".to_string(),
            }],
            body: CollectionBody::Raw {
                content_type: "application/json".to_string(),
                body: r#"{"name":"{{token}}"}"#.to_string(),
            },
            pre_request_script: String::new(),
            tests: Vec::new(),
        };

        let (method, url, headers, query_params, body) =
            collection_request_to_client_request(&request, &variables, Some("dev"))
                .expect("client request");

        assert_eq!(method, "POST");
        assert_eq!(url, "http://localhost:8080/users");
        assert_eq!(
            headers,
            vec![("Authorization".to_string(), "Bearer secret".to_string())]
        );
        assert_eq!(
            query_params,
            vec![("debug".to_string(), "true".to_string())]
        );
        assert_eq!(
            body,
            RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: r#"{"name":"secret"}"#.to_string(),
            }
        );
    }

    #[test]
    fn applies_collection_pre_request_actions_before_resolving_request() {
        let mut variables = VariableStore::new();
        variables.upsert(crate::variables::Variable::environment(
            "dev",
            "baseUrl",
            "http://localhost:8080",
        ));
        let mut request = request("Scripted", "GET", "{{baseUrl}}/users".to_string());
        request.pre_request_script = "set_var token=script-token; set_method POST; set_header Authorization=Bearer {{token}}; set_query debug=true".to_string();

        let (method, url, headers, query_params, body) =
            collection_request_to_client_request(&request, &variables, Some("dev"))
                .expect("scripted request");

        assert_eq!(method, "POST");
        assert_eq!(url, "http://localhost:8080/users");
        assert_eq!(
            headers,
            vec![(
                "Authorization".to_string(),
                "Bearer script-token".to_string()
            )]
        );
        assert_eq!(
            query_params,
            vec![("debug".to_string(), "true".to_string())]
        );
        assert_eq!(body, RequestBody::None);

        let resolved =
            resolve_collection_request(&request, &variables, Some("dev")).expect("resolved");
        assert_eq!(
            resolved.pre_request_actions,
            vec![
                "set_var token".to_string(),
                "set_method method".to_string(),
                "set_header Authorization".to_string(),
                "set_query debug".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn runs_collection_and_continues_after_failures() {
        let app = Router::new()
            .route("/ok", get(|| async { Json(json!({"ok": true})) }))
            .route("/fail", get(|| async { StatusCode::INTERNAL_SERVER_ERROR }))
            .route("/echo", post(|| async { Json(json!({"created": true})) }));
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("test server addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test server");
        });
        let collection = ApiCollection {
            name: "Runner".to_string(),
            description: String::new(),
            items: vec![
                CollectionItem::Request(request("OK", "GET", format!("http://{addr}/ok"))),
                CollectionItem::Request(request("Fail", "GET", format!("http://{addr}/fail"))),
                CollectionItem::Request(request("Echo", "POST", format!("http://{addr}/echo"))),
            ],
        };

        let summary = run_collection(
            &collection,
            &VariableStore::new(),
            None,
            RunnerOptions::default(),
        )
        .await;
        server.abort();

        assert_eq!(summary.total, 3);
        assert_eq!(summary.results.len(), 3);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);
        assert!(!summary.stopped_early);
        assert_eq!(summary.results[1].status, Some(500));
        assert!(!summary.results[1].success);
    }

    #[tokio::test]
    async fn stops_on_first_failure_when_configured() {
        let app = Router::new()
            .route("/fail", get(|| async { StatusCode::INTERNAL_SERVER_ERROR }))
            .route("/ok", get(|| async { Json::<Value>(json!({"ok": true})) }));
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("test server addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test server");
        });
        let collection = ApiCollection {
            name: "Stop".to_string(),
            description: String::new(),
            items: vec![
                CollectionItem::Request(request("Fail", "GET", format!("http://{addr}/fail"))),
                CollectionItem::Request(request("Skipped", "GET", format!("http://{addr}/ok"))),
            ],
        };

        let summary = run_collection(
            &collection,
            &VariableStore::new(),
            None,
            RunnerOptions {
                delay_ms: 0,
                failure_strategy: FailureStrategy::StopOnFailure,
            },
        )
        .await;
        server.abort();

        assert_eq!(summary.total, 2);
        assert_eq!(summary.results.len(), 1);
        assert_eq!(summary.failed, 1);
        assert!(summary.stopped_early);
    }

    #[tokio::test]
    async fn response_assertions_drive_runner_success() {
        let app = Router::new()
            .route(
                "/expected-failure",
                get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
            )
            .route("/json", get(|| async { Json(json!({"ok": true})) }));
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("test server addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test server");
        });

        let mut expected_failure = request(
            "Expected failure",
            "GET",
            format!("http://{addr}/expected-failure"),
        );
        expected_failure.tests = vec![ResponseAssertion {
            name: "status is 500".to_string(),
            kind: ResponseAssertionKind::StatusEquals { status: 500 },
        }];
        let mut failing_json = request("JSON", "GET", format!("http://{addr}/json"));
        failing_json.tests = vec![ResponseAssertion {
            name: "ok false".to_string(),
            kind: ResponseAssertionKind::JsonPathEquals {
                path: "ok".to_string(),
                value: Value::from(false),
            },
        }];
        let collection = ApiCollection {
            name: "Assertions".to_string(),
            description: String::new(),
            items: vec![
                CollectionItem::Request(expected_failure),
                CollectionItem::Request(failing_json),
            ],
        };

        let summary = run_collection(
            &collection,
            &VariableStore::new(),
            None,
            RunnerOptions::default(),
        )
        .await;
        server.abort();

        assert_eq!(summary.total, 2);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 1);
        assert!(summary.results[0].success);
        assert_eq!(summary.results[0].status, Some(500));
        assert_eq!(summary.results[0].assertions.len(), 1);
        assert!(!summary.results[1].success);
        assert_eq!(summary.results[1].assertions[0].name, "ok false");
        assert!(
            summary.results[1]
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("assertion")
        );
    }
}

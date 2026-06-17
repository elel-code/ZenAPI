use crate::openapi::{ApiRoute, MockRule, MockRuleSource};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderValue, Method, Request, StatusCode, header},
    response::{IntoResponse, Response},
    routing::any,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc;

#[derive(Clone)]
struct MockRouteResponse {
    default_body: Value,
    rules: Vec<MockRule>,
}

type RouteMap = HashMap<(Method, String), MockRouteResponse>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockRequestLog {
    pub method: String,
    pub path: String,
    pub status: u16,
}

struct MockState {
    routes: RouteMap,
    log_sender: Option<mpsc::UnboundedSender<MockRequestLog>>,
}

pub(super) fn mock_router(
    routes: Vec<ApiRoute>,
    log_sender: Option<mpsc::UnboundedSender<MockRequestLog>>,
) -> Router {
    let state = Arc::new(MockState {
        routes: build_route_map(routes),
        log_sender,
    });
    Router::new().fallback(any(mock_handler)).with_state(state)
}

fn build_route_map(routes: Vec<ApiRoute>) -> RouteMap {
    routes
        .into_iter()
        .filter_map(|route| {
            let method = route.method.parse::<Method>().ok()?;
            Some((
                (method, route.path),
                MockRouteResponse {
                    default_body: route.mock_body,
                    rules: route.mock_rules,
                },
            ))
        })
        .collect()
}

async fn mock_handler(State(state): State<Arc<MockState>>, request: Request<Body>) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    if request.method() == Method::OPTIONS {
        state.record_request(method, path, StatusCode::NO_CONTENT);
        return with_cors(StatusCode::NO_CONTENT.into_response());
    }

    let key = (method.clone(), path.clone());
    let matched = state.routes.get(&key);
    let body = matched
        .map(|route| resolve_mock_body(route, &request))
        .unwrap_or_else(|| {
            json!({
                "error": "mock route not found",
                "method": method.as_str(),
                "path": path,
            })
        });

    let status = if matched.is_some() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    state.record_request(method, key.1, status);

    let response = (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response();
    with_cors(response)
}

fn resolve_mock_body(route: &MockRouteResponse, request: &Request<Body>) -> Value {
    route
        .rules
        .iter()
        .find(|rule| mock_rule_matches(rule, request))
        .map(|rule| rule.mock_body.clone())
        .unwrap_or_else(|| route.default_body.clone())
}

fn mock_rule_matches(rule: &MockRule, request: &Request<Body>) -> bool {
    let name = rule.name.trim();
    let value = rule.value.trim();
    if name.is_empty() || value.is_empty() {
        return false;
    }

    match rule.source {
        MockRuleSource::Header => request
            .headers()
            .get(name)
            .and_then(|header| header.to_str().ok())
            .is_some_and(|header| header == value),
        MockRuleSource::Query => request
            .uri()
            .query()
            .is_some_and(|query| query_param_matches(query, name, value)),
    }
}

fn query_param_matches(query: &str, name: &str, value: &str) -> bool {
    query.split('&').any(|pair| {
        let (key, pair_value) = pair.split_once('=').unwrap_or((pair, ""));
        key == name && pair_value == value
    })
}

impl MockState {
    fn record_request(&self, method: Method, path: String, status: StatusCode) {
        if let Some(sender) = &self.log_sender {
            let _ = sender.send(MockRequestLog {
                method: method.to_string(),
                path,
                status: status.as_u16(),
            });
        }
    }
}

fn with_cors(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET,POST,PUT,PATCH,DELETE,HEAD,OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("*"),
    );
    response
}

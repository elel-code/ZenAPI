use crate::openapi::ApiRoute;
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

type RouteMap = HashMap<(Method, String), Value>;

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
            Some(((method, route.path), route.mock_body))
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
    let body = state.routes.get(&key).cloned().unwrap_or_else(|| {
        json!({
            "error": "mock route not found",
            "method": method.as_str(),
            "path": path,
        })
    });

    let status = if state.routes.contains_key(&key) {
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

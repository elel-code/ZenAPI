use crate::openapi::ApiRoute;
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderValue, Method, Request, StatusCode, header},
    response::{IntoResponse, Response},
    routing::any,
};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};

type RouteMap = HashMap<(Method, String), Value>;

pub(super) fn mock_router(routes: Vec<ApiRoute>) -> Router {
    let route_map = Arc::new(build_route_map(routes));
    Router::new()
        .fallback(any(mock_handler))
        .with_state(route_map)
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

async fn mock_handler(State(routes): State<Arc<RouteMap>>, request: Request<Body>) -> Response {
    if request.method() == Method::OPTIONS {
        return with_cors(StatusCode::NO_CONTENT.into_response());
    }

    let key = (request.method().clone(), request.uri().path().to_string());
    let body = routes.get(&key).cloned().unwrap_or_else(|| {
        json!({
            "error": "mock route not found",
            "method": request.method().as_str(),
            "path": request.uri().path(),
        })
    });

    let status = if routes.contains_key(&key) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };

    let response = (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response();
    with_cors(response)
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

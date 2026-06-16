use super::response::{ClientResponse, pretty_body};
use anyhow::{Context, Result};
use reqwest::{
    Method,
    header::{CONTENT_TYPE, HeaderName, HeaderValue},
    multipart::{Form, Part},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestBody {
    None,
    Raw {
        content_type: Option<String>,
        body: String,
    },
    FormUrlEncoded(Vec<(String, String)>),
    Multipart(Vec<(String, String)>),
    BinaryFile {
        path: String,
        content_type: Option<String>,
    },
}

pub async fn send_request(method: &str, url: &str, body: &str) -> Result<ClientResponse> {
    send_request_with_options(method, url, &[], &[], body).await
}

pub async fn send_request_with_options(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    query_params: &[(String, String)],
    body: &str,
) -> Result<ClientResponse> {
    send_request_with_body(
        method,
        url,
        headers,
        query_params,
        RequestBody::Raw {
            content_type: Some("application/json".to_string()),
            body: body.to_string(),
        },
    )
    .await
}

pub async fn send_request_with_body(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    query_params: &[(String, String)],
    body: RequestBody,
) -> Result<ClientResponse> {
    let method = Method::from_bytes(method.trim().as_bytes())
        .with_context(|| format!("invalid HTTP method: {method}"))?;
    let client = reqwest::Client::builder()
        .user_agent("ZenAPI/0.1")
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client")?;

    let mut request = client.request(method.clone(), url.trim());
    let mut has_content_type = false;

    if !query_params.is_empty() {
        request = request.query(query_params);
    }

    for (name, value) in headers {
        let header_name = HeaderName::from_bytes(name.trim().as_bytes())
            .with_context(|| format!("invalid request header name: {name}"))?;
        let header_value = HeaderValue::from_str(value.trim())
            .with_context(|| format!("invalid request header value for {name}"))?;
        has_content_type |= header_name == CONTENT_TYPE;
        request = request.header(header_name, header_value);
    }

    if method != Method::GET && method != Method::HEAD {
        request = apply_request_body(request, body, has_content_type)?;
    }

    send_prepared_request(request).await
}

fn apply_request_body(
    mut request: reqwest::RequestBuilder,
    body: RequestBody,
    has_content_type: bool,
) -> Result<reqwest::RequestBuilder> {
    match body {
        RequestBody::None => {}
        RequestBody::Raw { content_type, body } => {
            if !body.trim().is_empty() {
                if !has_content_type {
                    if let Some(content_type) = content_type.filter(|value| !value.is_empty()) {
                        request = request.header(CONTENT_TYPE, content_type);
                    }
                }
                request = request.body(body);
            }
        }
        RequestBody::FormUrlEncoded(fields) => {
            if !fields.is_empty() {
                request = request.form(&fields);
            }
        }
        RequestBody::Multipart(fields) => {
            if !fields.is_empty() {
                request = request.multipart(build_multipart_form(fields)?);
            }
        }
        RequestBody::BinaryFile { path, content_type } => {
            let path = path.trim();
            if !path.is_empty() {
                let bytes =
                    fs::read(path).with_context(|| format!("failed to read binary body {path}"))?;
                if !has_content_type {
                    request = request.header(
                        CONTENT_TYPE,
                        content_type
                            .filter(|value| !value.is_empty())
                            .unwrap_or_else(|| "application/octet-stream".to_string()),
                    );
                }
                request = request.body(bytes);
            }
        }
    }

    Ok(request)
}

fn build_multipart_form(fields: Vec<(String, String)>) -> Result<Form> {
    let mut form = Form::new();

    for (name, value) in fields {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }

        let value = value.trim();
        if let Some(path) = value.strip_prefix('@') {
            let path = path.trim();
            let bytes =
                fs::read(path).with_context(|| format!("failed to read multipart file {path}"))?;
            let filename = Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("upload")
                .to_string();
            form = form.part(name.to_string(), Part::bytes(bytes).file_name(filename));
        } else {
            form = form.text(name.to_string(), value.to_string());
        }
    }

    Ok(form)
}

async fn send_prepared_request(request: reqwest::RequestBuilder) -> Result<ClientResponse> {
    let started = Instant::now();
    let response = request.send().await.context("request failed")?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                value.to_str().unwrap_or("<non-utf8>").to_string(),
            )
        })
        .collect::<Vec<_>>();
    let body = response
        .text()
        .await
        .context("failed to read response body")?;
    let body_bytes = body.len();
    let elapsed_ms = started.elapsed().as_millis();

    Ok(ClientResponse {
        status,
        elapsed_ms,
        body_bytes,
        headers,
        raw_body: body.clone(),
        body: pretty_body(&body),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        body::Bytes,
        extract::Query,
        http::HeaderMap,
        routing::{get, post},
    };
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use tokio::net::TcpListener;

    async fn echo_request(
        Query(params): Query<HashMap<String, String>>,
        headers: HeaderMap,
    ) -> Json<Value> {
        Json(json!({
            "token": headers
                .get("x-token")
                .and_then(|value| value.to_str().ok())
                .unwrap_or(""),
            "search": params.get("search").cloned().unwrap_or_default(),
            "limit": params.get("limit").cloned().unwrap_or_default(),
        }))
    }

    async fn echo_body(headers: HeaderMap, body: Bytes) -> Json<Value> {
        Json(json!({
            "content_type": headers
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .unwrap_or(""),
            "body": String::from_utf8_lossy(&body).to_string(),
            "bytes": body.len(),
        }))
    }

    #[tokio::test]
    async fn sends_headers_and_query_params() {
        let app = Router::new().route("/echo", get(echo_request));
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("test server addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test server");
        });

        let response = send_request_with_options(
            "GET",
            &format!("http://{addr}/echo"),
            &[("x-token".to_string(), "secret".to_string())],
            &[
                ("search".to_string(), "rust slint".to_string()),
                ("limit".to_string(), "20".to_string()),
            ],
            "",
        )
        .await
        .expect("send request");
        server.abort();

        assert_eq!(response.status, 200);
        assert!(response.body_bytes > 0);
        assert!(
            response
                .headers
                .iter()
                .any(|(name, value)| name == "content-type" && value.contains("application/json"))
        );
        assert!(response.body.contains('\n'));
        let body = serde_json::from_str::<Value>(&response.raw_body).expect("json body");
        assert_eq!(body["token"], "secret");
        assert_eq!(body["search"], "rust slint");
        assert_eq!(body["limit"], "20");
    }

    #[tokio::test]
    async fn sends_urlencoded_raw_multipart_and_binary_bodies() {
        let app = Router::new().route("/body", post(echo_body));
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("test server addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test server");
        });
        let url = format!("http://{addr}/body");
        let dir = temp_dir::TempDir::new().expect("temp dir");
        let file_path = dir.path().join("upload.txt");
        std::fs::write(&file_path, "file-body").expect("write file");

        let urlencoded = send_request_with_body(
            "POST",
            &url,
            &[],
            &[],
            RequestBody::FormUrlEncoded(vec![
                ("search".to_string(), "rust slint".to_string()),
                ("limit".to_string(), "20".to_string()),
            ]),
        )
        .await
        .expect("send urlencoded");
        let urlencoded_body =
            serde_json::from_str::<Value>(&urlencoded.raw_body).expect("urlencoded json");
        assert!(
            urlencoded_body["content_type"]
                .as_str()
                .unwrap_or_default()
                .starts_with("application/x-www-form-urlencoded")
        );
        assert_eq!(urlencoded_body["body"], "search=rust+slint&limit=20");

        let raw = send_request_with_body(
            "POST",
            &url,
            &[],
            &[],
            RequestBody::Raw {
                content_type: Some("text/plain".to_string()),
                body: "hello".to_string(),
            },
        )
        .await
        .expect("send raw");
        let raw_body = serde_json::from_str::<Value>(&raw.raw_body).expect("raw json");
        assert_eq!(raw_body["content_type"], "text/plain");
        assert_eq!(raw_body["body"], "hello");

        let multipart = send_request_with_body(
            "POST",
            &url,
            &[],
            &[],
            RequestBody::Multipart(vec![
                ("note".to_string(), "hello".to_string()),
                ("upload".to_string(), format!("@{}", file_path.display())),
            ]),
        )
        .await
        .expect("send multipart");
        let multipart_body =
            serde_json::from_str::<Value>(&multipart.raw_body).expect("multipart json");
        assert!(
            multipart_body["content_type"]
                .as_str()
                .unwrap_or_default()
                .starts_with("multipart/form-data")
        );
        let multipart_text = multipart_body["body"].as_str().unwrap_or_default();
        assert!(multipart_text.contains("name=\"note\""));
        assert!(multipart_text.contains("hello"));
        assert!(multipart_text.contains("name=\"upload\""));
        assert!(multipart_text.contains("file-body"));

        let binary = send_request_with_body(
            "POST",
            &url,
            &[],
            &[],
            RequestBody::BinaryFile {
                path: file_path.display().to_string(),
                content_type: Some("application/octet-stream".to_string()),
            },
        )
        .await
        .expect("send binary");
        let binary_body = serde_json::from_str::<Value>(&binary.raw_body).expect("binary json");
        assert_eq!(binary_body["content_type"], "application/octet-stream");
        assert_eq!(binary_body["body"], "file-body");

        server.abort();
    }
}

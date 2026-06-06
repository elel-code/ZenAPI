use super::response::{ClientResponse, pretty_body};
use anyhow::{Context, Result};
use reqwest::Method;
use std::time::{Duration, Instant};

pub async fn send_request(method: &str, url: &str, body: &str) -> Result<ClientResponse> {
    let method = Method::from_bytes(method.trim().as_bytes())
        .with_context(|| format!("invalid HTTP method: {method}"))?;
    let client = reqwest::Client::builder()
        .user_agent("ZenAPI/0.1")
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client")?;

    let mut request = client.request(method.clone(), url.trim());

    if method != Method::GET && method != Method::HEAD && !body.trim().is_empty() {
        request = request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_owned());
    }

    let started = Instant::now();
    let response = request.send().await.context("request failed")?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .context("failed to read response body")?;
    let elapsed_ms = started.elapsed().as_millis();

    Ok(ClientResponse {
        status,
        elapsed_ms,
        body: pretty_body(&body),
    })
}

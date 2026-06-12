mod routing;
mod server;

pub use routing::MockRequestLog;
pub use server::MockServer;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::send_request;
    use crate::openapi::ApiRoute;
    use serde_json::json;

    #[tokio::test]
    async fn serves_mock_route() {
        let server = MockServer::start(
            vec![ApiRoute {
                method: "GET".to_string(),
                path: "/health".to_string(),
                summary: String::new(),
                mock_body: json!({ "ok": true }),
            }],
            0,
        )
        .await
        .expect("start server");

        let response = send_request("GET", &format!("http://{}/health", server.addr()), "")
            .await
            .expect("call mock");

        assert_eq!(response.status, 200);
        assert!(response.body.contains("\"ok\": true"));

        server.stop().await;
    }

    #[tokio::test]
    async fn enables_cors_for_preflight_and_json_responses() {
        let server = MockServer::start(
            vec![ApiRoute {
                method: "GET".to_string(),
                path: "/health".to_string(),
                summary: String::new(),
                mock_body: json!({ "ok": true }),
            }],
            0,
        )
        .await
        .expect("start server");
        let url = format!("http://{}/health", server.addr());
        let client = reqwest::Client::new();

        let preflight = client
            .request(reqwest::Method::OPTIONS, &url)
            .header(reqwest::header::ORIGIN, "http://localhost:3000")
            .header(reqwest::header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .send()
            .await
            .expect("preflight request");

        assert_eq!(preflight.status(), reqwest::StatusCode::NO_CONTENT);
        assert_eq!(
            preflight
                .headers()
                .get(reqwest::header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|value| value.to_str().ok()),
            Some("*")
        );

        let response = client.get(&url).send().await.expect("get request");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(reqwest::header::ACCESS_CONTROL_ALLOW_METHODS)
                .and_then(|value| value.to_str().ok()),
            Some("GET,POST,PUT,PATCH,DELETE,HEAD,OPTIONS")
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn emits_mock_request_logs() {
        let (log_tx, mut log_rx) = tokio::sync::mpsc::unbounded_channel();
        let server = MockServer::start_with_logs(
            vec![ApiRoute {
                method: "GET".to_string(),
                path: "/health".to_string(),
                summary: String::new(),
                mock_body: json!({ "ok": true }),
            }],
            0,
            log_tx,
        )
        .await
        .expect("start server");

        let response = send_request("GET", &format!("http://{}/health", server.addr()), "")
            .await
            .expect("call mock");
        assert_eq!(response.status, 200);

        let entry = tokio::time::timeout(std::time::Duration::from_secs(1), log_rx.recv())
            .await
            .expect("log timeout")
            .expect("log entry");

        assert_eq!(
            entry,
            MockRequestLog {
                method: "GET".to_string(),
                path: "/health".to_string(),
                status: 200,
            }
        );

        server.stop().await;
    }
}

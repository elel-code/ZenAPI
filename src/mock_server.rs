mod routing;
mod server;

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
}

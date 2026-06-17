use super::{
    model::{ApiRoute, ApiSpec, METHODS},
    schema::operation_mock_body,
};
use anyhow::{Context, Result};
use serde_json::Value;

pub(crate) fn parse_json_value(root: &Value) -> Result<ApiSpec> {
    let paths = root
        .get("paths")
        .and_then(Value::as_object)
        .context("missing paths object")?;

    let info = root.get("info").and_then(Value::as_object);
    let title = info
        .and_then(|info| info.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Untitled API")
        .to_string();
    let version = info
        .and_then(|info| info.get("version"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let mut routes = Vec::new();

    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };

        for method in METHODS {
            let Some(operation) = item.get(method).and_then(Value::as_object) else {
                continue;
            };

            let summary = operation
                .get("summary")
                .or_else(|| operation.get("operationId"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();

            routes.push(ApiRoute {
                method: method.to_uppercase(),
                path: path.to_string(),
                summary,
                mock_body: operation_mock_body(operation),
                mock_rules: Vec::new(),
            });
        }
    }

    routes.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.method.cmp(&b.method)));

    Ok(ApiSpec {
        title,
        version,
        routes,
    })
}

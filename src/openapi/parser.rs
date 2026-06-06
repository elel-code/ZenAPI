use super::{json::parse_json_value, model::ApiSpec, yaml::parse_yaml_value};
use anyhow::{Context, Result};
use serde_json::Value;
use std::{fs, path::Path};

pub fn load_openapi_file(path: impl AsRef<Path>) -> Result<ApiSpec> {
    let path = path.as_ref();
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    parse_openapi(&content).with_context(|| format!("failed to parse {}", path.display()))
}

pub fn parse_openapi(content: &str) -> Result<ApiSpec> {
    match serde_json::from_str::<Value>(content) {
        Ok(value) => parse_json_value(&value),
        Err(json_error) => {
            parse_yaml_value(content).map_err(|yaml_error| {
                anyhow::anyhow!(
                    "unsupported OpenAPI document: JSON parse failed ({json_error}); YAML parse failed ({yaml_error})"
                )
            })
        }
    }
}

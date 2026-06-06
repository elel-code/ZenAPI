use super::{json::parse_json_value, model::ApiSpec};
use anyhow::{Context, Result};
use serde_json::Value;

pub(crate) fn parse_yaml_value(content: &str) -> Result<ApiSpec> {
    let value = serde_yaml::from_str::<Value>(content).context("invalid YAML")?;
    parse_json_value(&value)
}

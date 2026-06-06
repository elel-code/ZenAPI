use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiRoute {
    pub method: String,
    pub path: String,
    pub summary: String,
    pub mock_body: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApiSpec {
    pub title: String,
    pub version: String,
    pub routes: Vec<ApiRoute>,
}

pub(crate) const METHODS: [&str; 8] = [
    "get", "post", "put", "patch", "delete", "head", "options", "trace",
];

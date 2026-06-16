use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: u64,
    pub timestamp_ms: u128,
    pub request: HistoryRequest,
    pub response: HistoryResponse,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryRequest {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub query_params: Vec<(String, String)>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub auth_mode: String,
    #[serde(default)]
    pub auth_config: String,
    pub body_kind: String,
    pub body_preview: String,
    #[serde(default)]
    pub pre_request_script: String,
    #[serde(default)]
    pub request_tests: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryResponse {
    pub status: String,
    pub meta: String,
    pub body_preview: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestHistory {
    next_id: u64,
    entries: Vec<HistoryEntry>,
}

impl RequestHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_json(input: &str) -> Result<Self> {
        serde_json::from_str(input).context("failed to parse request history JSON")
    }

    pub fn load_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read request history {}", path.display()))?;
        Self::from_json(&content)
            .with_context(|| format!("failed to parse request history {}", path.display()))
    }

    pub fn save_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create request history directory {}",
                    parent.display()
                )
            })?;
        }
        let content = serde_json::to_string_pretty(self)
            .context("failed to serialize request history JSON")?;
        fs::write(path, content)
            .with_context(|| format!("failed to write request history {}", path.display()))
    }

    pub fn record(&mut self, request: HistoryRequest, response: HistoryResponse) -> u64 {
        self.record_at(now_ms(), request, response)
    }

    pub fn record_at(
        &mut self,
        timestamp_ms: u128,
        request: HistoryRequest,
        response: HistoryResponse,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.insert(
            0,
            HistoryEntry {
                id,
                timestamp_ms,
                request,
                response,
            },
        );
        id
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn find(&self, id: u64) -> Option<&HistoryEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let len = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        self.entries.len() != len
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn filtered(&self, query: &str) -> Vec<&HistoryEntry> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return self.entries.iter().collect();
        }

        self.entries
            .iter()
            .filter(|entry| {
                entry.request.method.to_lowercase().contains(&query)
                    || entry.request.url.to_lowercase().contains(&query)
                    || entry.response.status.to_lowercase().contains(&query)
                    || entry.response.meta.to_lowercase().contains(&query)
            })
            .collect()
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, url: &str) -> HistoryRequest {
        HistoryRequest {
            method: method.to_string(),
            url: url.to_string(),
            query_params: Vec::new(),
            headers: Vec::new(),
            auth_mode: "none".to_string(),
            auth_config: String::new(),
            body_kind: "none".to_string(),
            body_preview: String::new(),
            pre_request_script: String::new(),
            request_tests: String::new(),
        }
    }

    fn response(status: &str) -> HistoryResponse {
        HistoryResponse {
            status: status.to_string(),
            meta: "2/2 tests".to_string(),
            body_preview: "{}".to_string(),
        }
    }

    #[test]
    fn records_entries_in_reverse_chronological_order() {
        let mut history = RequestHistory::new();

        let first = history.record_at(10, request("GET", "/users"), response("HTTP 200"));
        let second = history.record_at(20, request("POST", "/users"), response("HTTP 201"));

        assert_eq!(history.entries()[0].id, second);
        assert_eq!(history.entries()[1].id, first);
    }

    #[test]
    fn filters_by_method_url_status_or_meta() {
        let mut history = RequestHistory::new();
        history.record_at(10, request("GET", "/users"), response("HTTP 200"));
        history.record_at(20, request("DELETE", "/sessions"), response("HTTP 404"));

        assert_eq!(history.filtered("delete")[0].request.method, "DELETE");
        assert_eq!(history.filtered("users")[0].request.url, "/users");
        assert_eq!(history.filtered("404")[0].response.status, "HTTP 404");
        assert_eq!(history.filtered("tests").len(), 2);
    }

    #[test]
    fn removes_and_clears_entries() {
        let mut history = RequestHistory::new();
        let id = history.record_at(10, request("GET", "/users"), response("HTTP 200"));

        assert!(history.remove(id));
        assert!(history.entries().is_empty());

        history.record_at(20, request("GET", "/users"), response("HTTP 200"));
        history.clear();
        assert!(history.entries().is_empty());
    }

    #[test]
    fn saves_and_loads_history_json_file() {
        let path = std::env::temp_dir().join(format!("zenapi-history-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);

        let mut history = RequestHistory::new();
        history.record_at(10, request("GET", "/users"), response("HTTP 200"));

        history.save_file(&path).expect("save history");
        let loaded = RequestHistory::load_file(&path).expect("load history");

        assert_eq!(loaded, history);

        let _ = fs::remove_file(path);
    }
}

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

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
    pub body_kind: String,
    pub body_preview: String,
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
            body_kind: "none".to_string(),
            body_preview: String::new(),
        }
    }

    fn response(status: &str) -> HistoryResponse {
        HistoryResponse {
            status: status.to_string(),
            meta: "12 ms | 2 B".to_string(),
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
        assert_eq!(history.filtered("12 ms").len(), 2);
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
}

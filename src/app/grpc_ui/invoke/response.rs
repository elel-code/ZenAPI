use serde_json::Value;
use zenapi::grpc::{GrpcServerStreamingResponse, GrpcUnaryResponse};

use crate::ui::AppWindow;

use super::super::super::{set_response, set_response_payload};

pub(in crate::app::grpc_ui) fn set_grpc_unary_response(
    app: &AppWindow,
    response: &GrpcUnaryResponse,
) {
    let body = serde_json::to_string_pretty(&response.message)
        .unwrap_or_else(|_| response.message.to_string());
    let metadata = if response.metadata.is_empty() {
        "No metadata".to_string()
    } else {
        response
            .metadata
            .iter()
            .map(|entry| format!("{}: {}", entry.name, entry.value))
            .collect::<Vec<_>>()
            .join("\n")
    };

    set_response(app, "gRPC OK", &response.method, "success", &body);
    set_response_payload(app, &body, &body, &metadata, "No cookies");
}

pub(in crate::app::grpc_ui) fn set_grpc_server_streaming_progress(
    app: &AppWindow,
    response: &GrpcServerStreamingResponse,
    status: &str,
    tone: &str,
) {
    let body = serde_json::to_string_pretty(&response.messages)
        .unwrap_or_else(|_| Value::Array(response.messages.clone()).to_string());
    let metadata = if response.metadata.is_empty() {
        "No metadata".to_string()
    } else {
        response
            .metadata
            .iter()
            .map(|entry| format!("{}: {}", entry.name, entry.value))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let meta = format!("{} messages / {}", response.messages.len(), response.method);

    set_response(app, status, &meta, tone, &body);
    set_response_payload(app, &body, &body, &metadata, "No cookies");
}

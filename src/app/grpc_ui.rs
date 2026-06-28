use slint::ComponentHandle;
use std::sync::Arc;
use tokio::runtime::Runtime;
use zenapi::grpc::{
    GrpcRequestDraft, build_grpc_request_draft, format_grpc_method_catalog,
    load_grpc_file_descriptor_set, load_grpc_proto_file, load_grpc_reflection_descriptors,
};

use crate::ui::AppWindow;

use super::set_response;

mod invoke;

pub(super) fn format_grpc_draft(draft: &GrpcRequestDraft) -> String {
    let metadata = if draft.metadata.is_empty() {
        "No metadata".to_string()
    } else {
        draft
            .metadata
            .iter()
            .map(|entry| format!("{}: {}", entry.name, entry.value))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let descriptor = draft
        .descriptor
        .as_ref()
        .map(|descriptor| {
            format!(
                "Kind: {}\nRequest: {}\nResponse: {}",
                descriptor.kind.label(),
                descriptor.request_type,
                descriptor.response_type
            )
        })
        .unwrap_or_else(|| "No method catalog".to_string());
    let message =
        serde_json::to_string_pretty(&draft.message).unwrap_or_else(|_| draft.message.to_string());
    let command = format_grpcurl_command(draft, &message);

    format!(
        "Endpoint: {}\nMethod: {}\n\nDescriptor\n{}\n\nMetadata\n{}\n\nMessage\n{}\n\nCommand\n{}",
        draft.endpoint,
        draft.method_path(),
        descriptor,
        metadata,
        message,
        command
    )
}

pub(super) fn format_grpcurl_command(draft: &GrpcRequestDraft, message: &str) -> String {
    let (target, plaintext) = grpcurl_target(&draft.endpoint);
    let mut lines = vec!["grpcurl".to_string()];
    if plaintext {
        lines.push("  -plaintext".to_string());
    }
    for entry in &draft.metadata {
        lines.push(format!(
            "  -H {}",
            shell_single_quote(&format!("{}: {}", entry.name, entry.value))
        ));
    }
    lines.push(format!("  -d {}", shell_single_quote(message)));
    lines.push(format!("  {}", shell_single_quote(&target)));
    lines.push(format!(
        "  {}",
        shell_single_quote(draft.method_path().trim_start_matches('/'))
    ));
    lines.join(" \\\n")
}

fn grpcurl_target(endpoint: &str) -> (String, bool) {
    let endpoint = endpoint.trim();
    if let Some(target) = endpoint.strip_prefix("http://") {
        return (target.trim_end_matches('/').to_string(), true);
    }
    if let Some(target) = endpoint.strip_prefix("https://") {
        return (target.trim_end_matches('/').to_string(), false);
    }
    (endpoint.trim_end_matches('/').to_string(), false)
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(super) fn wire_grpc_loader_actions(app: &AppWindow, runtime: Arc<Runtime>) {
    let weak_app = app.as_weak();
    app.on_load_grpc_descriptor_set(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match load_grpc_file_descriptor_set(path.as_str()) {
            Ok(descriptors) => {
                let catalog = format_grpc_method_catalog(&descriptors);
                set_response(
                    &app,
                    "gRPC descriptors loaded",
                    &format!("{} methods", descriptors.len()),
                    "success",
                    &catalog,
                );
                app.set_grpc_method_catalog(catalog.into());
            }
            Err(error) => set_response(
                &app,
                "gRPC descriptor load failed",
                path.as_str(),
                "error",
                &error.to_string(),
            ),
        }
    });

    let weak_app = app.as_weak();
    app.on_load_grpc_proto_file(move |path, protoc_path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let protoc_path = protoc_path.trim();
        let protoc_path = if protoc_path.is_empty() {
            "protoc"
        } else {
            protoc_path
        };
        match load_grpc_proto_file(path.as_str(), &[], protoc_path) {
            Ok(descriptors) => {
                let catalog = format_grpc_method_catalog(&descriptors);
                set_response(
                    &app,
                    "gRPC proto loaded",
                    &format!("{} methods", descriptors.len()),
                    "success",
                    &catalog,
                );
                app.set_grpc_method_catalog(catalog.into());
            }
            Err(error) => set_response(
                &app,
                "gRPC proto load failed",
                path.as_str(),
                "error",
                &error.to_string(),
            ),
        }
    });

    let weak_app = app.as_weak();
    let runtime_for_reflection = runtime.clone();
    app.on_load_grpc_reflection(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        app.set_busy(true);
        app.set_activity("Loading gRPC reflection".into());
        let endpoint = app.get_grpc_endpoint().to_string();
        let weak_app = app.as_weak();
        runtime_for_reflection.spawn(async move {
            let result = load_grpc_reflection_descriptors(&endpoint).await;
            if let Some(app) = weak_app.upgrade() {
                app.set_busy(false);
                match result {
                    Ok(descriptors) => {
                        let catalog = format_grpc_method_catalog(&descriptors);
                        set_response(
                            &app,
                            "gRPC reflection loaded",
                            &format!("{} methods", descriptors.len()),
                            "success",
                            &catalog,
                        );
                        app.set_grpc_method_catalog(catalog.into());
                        app.set_activity("gRPC reflection loaded".into());
                    }
                    Err(error) => {
                        set_response(
                            &app,
                            "gRPC reflection failed",
                            &endpoint,
                            "error",
                            &error.to_string(),
                        );
                        app.set_activity(format!("gRPC reflection failed: {error}").into());
                    }
                }
            }
        });
    });
}

pub(super) fn wire_grpc_draft(app: &AppWindow, runtime: Arc<Runtime>) {
    wire_grpc_loader_actions(app, runtime.clone());
    invoke::wire_grpc_invoke_actions(app, runtime);
    wire_grpc_build_actions(app);
}

pub(super) fn wire_grpc_build_actions(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_build_grpc_draft(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        match build_grpc_request_draft(
            &app.get_grpc_endpoint(),
            &app.get_grpc_method(),
            &app.get_grpc_metadata(),
            &app.get_grpc_message(),
            &app.get_grpc_method_catalog(),
        ) {
            Ok(draft) => set_response(
                &app,
                "gRPC draft ready",
                &draft.method_path(),
                "success",
                &format_grpc_draft(&draft),
            ),
            Err(error) => set_response(&app, "gRPC draft failed", "", "error", &error.to_string()),
        }
    });
}

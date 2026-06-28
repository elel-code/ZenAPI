use slint::ComponentHandle;
use std::sync::{Arc, Mutex};
use tokio::{runtime::Runtime, task::JoinHandle};
use zenapi::grpc::{
    GrpcServerStreamEvent, GrpcServerStreamingResponse, invoke_grpc_unary,
    stream_grpc_server_streaming,
};

use crate::ui::AppWindow;

use super::super::super::set_response;
use super::{
    descriptor::grpc_descriptor_set_for_invoke,
    response::{set_grpc_server_streaming_progress, set_grpc_unary_response},
};

type GrpcStreamState = Arc<Mutex<Option<JoinHandle<()>>>>;

pub(in crate::app::grpc_ui) fn wire_grpc_invoke_actions(app: &AppWindow, runtime: Arc<Runtime>) {
    let grpc_stream: GrpcStreamState = Arc::new(Mutex::new(None));
    let weak_app = app.as_weak();
    let runtime_for_invoke = runtime.clone();
    app.on_invoke_grpc_unary(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        app.set_busy(true);
        app.set_activity("Invoking gRPC unary request".into());
        let endpoint = app.get_grpc_endpoint().to_string();
        let method = app.get_grpc_method().to_string();
        let metadata = app.get_grpc_metadata().to_string();
        let message = app.get_grpc_message().to_string();
        let descriptor_path = app.get_grpc_descriptor_path().to_string();
        let protoc_path = app.get_grpc_protoc_path().to_string();
        let weak_app = app.as_weak();
        runtime_for_invoke.spawn(async move {
            let result = async {
                let descriptor_set =
                    grpc_descriptor_set_for_invoke(&endpoint, &descriptor_path, &protoc_path)
                        .await?;
                invoke_grpc_unary(&endpoint, &method, &metadata, &message, descriptor_set).await
            }
            .await;

            if let Some(app) = weak_app.upgrade() {
                app.set_busy(false);
                match result {
                    Ok(response) => {
                        set_grpc_unary_response(&app, &response);
                        app.set_activity("gRPC unary request complete".into());
                    }
                    Err(error) => {
                        set_response(
                            &app,
                            "gRPC invoke failed",
                            &method,
                            "error",
                            &error.to_string(),
                        );
                        app.set_activity(format!("gRPC invoke failed: {error}").into());
                    }
                }
            }
        });
    });

    let weak_app = app.as_weak();
    let runtime_for_stream = runtime.clone();
    let stream_state_for_start = grpc_stream.clone();
    app.on_invoke_grpc_server_streaming(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() || app.get_grpc_streaming() {
            return;
        }

        let endpoint = app.get_grpc_endpoint().to_string();
        let method = app.get_grpc_method().to_string();
        let metadata = app.get_grpc_metadata().to_string();
        let message = app.get_grpc_message().to_string();
        let descriptor_path = app.get_grpc_descriptor_path().to_string();
        let protoc_path = app.get_grpc_protoc_path().to_string();
        let Ok(mut stream) = stream_state_for_start.lock() else {
            return;
        };
        if stream.is_some() {
            set_response(
                &app,
                "gRPC stream active",
                &method,
                "neutral",
                "Stop the active gRPC stream before starting another.",
            );
            return;
        }

        app.set_grpc_streaming(true);
        app.set_activity("gRPC server stream active".into());
        set_response(
            &app,
            "gRPC streaming",
            &method,
            "busy",
            &format!("Connecting\n\n{endpoint}\n{method}"),
        );

        let weak_app = app.as_weak();
        let stream_state = stream_state_for_start.clone();
        let handle = runtime_for_stream.spawn(async move {
            let mut response_method = method.clone();
            let mut response_metadata = Vec::new();
            let mut response_messages = Vec::new();
            let result = async {
                let descriptor_set =
                    grpc_descriptor_set_for_invoke(&endpoint, &descriptor_path, &protoc_path)
                        .await?;
                stream_grpc_server_streaming(
                    &endpoint,
                    &method,
                    &metadata,
                    &message,
                    descriptor_set,
                    |event| {
                        let (status, tone, activity, closed) = match event {
                            GrpcServerStreamEvent::Started { method, metadata } => {
                                response_method = method;
                                response_metadata = metadata;
                                (
                                    "gRPC Stream Open",
                                    "busy",
                                    "gRPC server stream active",
                                    false,
                                )
                            }
                            GrpcServerStreamEvent::Message(message) => {
                                response_messages.push(message);
                                (
                                    "gRPC Stream Event",
                                    "busy",
                                    "gRPC server stream active",
                                    false,
                                )
                            }
                            GrpcServerStreamEvent::Closed => (
                                "gRPC Stream OK",
                                "success",
                                "gRPC server stream complete",
                                true,
                            ),
                        };
                        let response = GrpcServerStreamingResponse {
                            method: response_method.clone(),
                            metadata: response_metadata.clone(),
                            messages: response_messages.clone(),
                        };
                        let weak_app = weak_app.clone();
                        let stream_state = stream_state.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = weak_app.upgrade() {
                                set_grpc_server_streaming_progress(&app, &response, status, tone);
                                app.set_activity(activity.into());
                                if closed {
                                    app.set_grpc_streaming(false);
                                    if let Ok(mut stream) = stream_state.lock() {
                                        *stream = None;
                                    }
                                }
                            }
                        });
                    },
                )
                .await
            }
            .await;

            if let Err(error) = result {
                if let Some(app) = weak_app.upgrade() {
                    app.set_grpc_streaming(false);
                    if let Ok(mut stream) = stream_state.lock() {
                        *stream = None;
                    }
                    set_response(
                        &app,
                        "gRPC stream failed",
                        &method,
                        "error",
                        &error.to_string(),
                    );
                    app.set_activity(format!("gRPC stream failed: {error}").into());
                } else if let Ok(mut stream) = stream_state.lock() {
                    *stream = None;
                }
            } else if let Some(app) = weak_app.upgrade()
                && app.get_grpc_streaming()
            {
                app.set_grpc_streaming(false);
                if let Ok(mut stream) = stream_state.lock() {
                    *stream = None;
                }
                if response_messages.is_empty() {
                    set_response(
                        &app,
                        "gRPC Stream OK",
                        &method,
                        "success",
                        "gRPC server stream closed without messages.",
                    );
                }
            }
        });
        *stream = Some(handle);
    });

    let weak_app = app.as_weak();
    let stream_state_for_stop = grpc_stream;
    app.on_stop_grpc_stream(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let Some(handle) = stream_state_for_stop
            .lock()
            .ok()
            .and_then(|mut stream| stream.take())
        else {
            app.set_grpc_streaming(false);
            set_response(
                &app,
                "gRPC stream idle",
                &app.get_grpc_method(),
                "neutral",
                "No gRPC stream is active.",
            );
            return;
        };

        handle.abort();
        app.set_grpc_streaming(false);
        app.set_activity("".into());
        set_response(
            &app,
            "gRPC stream stopped",
            &app.get_grpc_method(),
            "neutral",
            "gRPC server stream stopped.",
        );
    });
}

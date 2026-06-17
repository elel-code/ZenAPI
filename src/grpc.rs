use anyhow::{Result, anyhow, bail};
use prost::Message;
use prost_types::FileDescriptorSet;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrpcMethodKind {
    Unary,
    ServerStreaming,
    ClientStreaming,
    BidiStreaming,
}

impl GrpcMethodKind {
    pub fn label(&self) -> &'static str {
        match self {
            GrpcMethodKind::Unary => "unary",
            GrpcMethodKind::ServerStreaming => "server streaming",
            GrpcMethodKind::ClientStreaming => "client streaming",
            GrpcMethodKind::BidiStreaming => "bidirectional streaming",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrpcMethodDescriptor {
    pub package: String,
    pub service: String,
    pub method: String,
    pub request_type: String,
    pub response_type: String,
    pub kind: GrpcMethodKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrpcMetadata {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GrpcRequestDraft {
    pub endpoint: String,
    pub service: String,
    pub method: String,
    pub metadata: Vec<GrpcMetadata>,
    pub message: Value,
    pub descriptor: Option<GrpcMethodDescriptor>,
}

impl GrpcMethodDescriptor {
    pub fn full_method_name(&self) -> String {
        let service = if self.package.trim().is_empty() {
            self.service.clone()
        } else {
            format!("{}.{}", self.package, self.service)
        };
        format!("{service}/{}", self.method)
    }
}

impl GrpcRequestDraft {
    pub fn method_path(&self) -> String {
        format!("/{}/{}", self.service, self.method)
    }
}

pub fn build_grpc_request_draft(
    endpoint: &str,
    full_method: &str,
    metadata_lines: &str,
    message_json: &str,
    method_catalog: &str,
) -> Result<GrpcRequestDraft> {
    let endpoint = parse_grpc_endpoint(endpoint)?;
    let (service, method) = parse_grpc_method_path(full_method)?;
    let metadata = parse_grpc_metadata_lines(metadata_lines)?;
    let message = parse_grpc_message_json(message_json)?;
    let descriptors = parse_grpc_method_catalog(method_catalog)?;
    let descriptor = resolve_grpc_method_descriptor(&service, &method, &descriptors)?;

    Ok(GrpcRequestDraft {
        endpoint,
        service,
        method,
        metadata,
        message,
        descriptor,
    })
}

pub fn parse_grpc_endpoint(endpoint: &str) -> Result<String> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        bail!("gRPC endpoint is empty");
    }
    if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
        bail!("gRPC endpoint must start with http:// or https://");
    }
    Ok(endpoint.to_string())
}

pub fn parse_grpc_method_path(full_method: &str) -> Result<(String, String)> {
    let full_method = full_method.trim().trim_start_matches('/');
    let Some((service, method)) = full_method.rsplit_once('/') else {
        bail!("gRPC method must use service/method");
    };
    let service = service.trim();
    let method = method.trim();
    if service.is_empty() || method.is_empty() {
        bail!("gRPC method must include service and method names");
    }
    Ok((service.to_string(), method.to_string()))
}

pub fn parse_grpc_metadata_lines(input: &str) -> Result<Vec<GrpcMetadata>> {
    input
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            (!line.is_empty() && !line.starts_with('#') && !line.starts_with("//"))
                .then_some((index, line))
        })
        .map(|(index, line)| {
            let Some((name, value)) = split_metadata_line(line) else {
                return Err(anyhow!(
                    "metadata line {} must use name=value or name: value",
                    index + 1
                ));
            };
            let name = name.trim();
            if name.is_empty() {
                bail!("metadata line {} has an empty name", index + 1);
            }
            Ok(GrpcMetadata {
                name: name.to_string(),
                value: value.trim().to_string(),
            })
        })
        .collect()
}

pub fn parse_grpc_message_json(input: &str) -> Result<Value> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Value::Object(Default::default()));
    }

    let value = serde_json::from_str::<Value>(input)
        .map_err(|error| anyhow!("gRPC message JSON is invalid: {error}"))?;
    if !value.is_object() {
        bail!("gRPC message JSON must be an object");
    }
    Ok(value)
}

fn split_metadata_line(line: &str) -> Option<(&str, &str)> {
    let separator = match (line.find('='), line.find(':')) {
        (Some(eq), Some(colon)) => eq.min(colon),
        (Some(eq), None) => eq,
        (None, Some(colon)) => colon,
        (None, None) => return None,
    };

    Some((&line[..separator], &line[separator + 1..]))
}

pub fn parse_grpc_method_catalog(input: &str) -> Result<Vec<GrpcMethodDescriptor>> {
    input
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            (!line.is_empty() && !line.starts_with('#') && !line.starts_with("//"))
                .then_some((index, line))
        })
        .map(|(index, line)| parse_grpc_method_descriptor_line(index + 1, line))
        .collect()
}

pub fn decode_grpc_file_descriptor_set(input: &[u8]) -> Result<Vec<GrpcMethodDescriptor>> {
    let descriptor_set = FileDescriptorSet::decode(input)
        .map_err(|error| anyhow!("failed to decode gRPC descriptor set: {error}"))?;
    grpc_method_descriptors_from_file_descriptor_set(&descriptor_set)
}

pub fn load_grpc_file_descriptor_set(path: impl AsRef<Path>) -> Result<Vec<GrpcMethodDescriptor>> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|error| {
        anyhow!(
            "failed to read gRPC descriptor set {}: {error}",
            path.display()
        )
    })?;
    decode_grpc_file_descriptor_set(&bytes)
}

pub fn grpc_method_descriptors_from_file_descriptor_set(
    descriptor_set: &FileDescriptorSet,
) -> Result<Vec<GrpcMethodDescriptor>> {
    let mut descriptors = Vec::new();

    for file in &descriptor_set.file {
        let package = file.package.as_deref().unwrap_or("").trim();
        for service in &file.service {
            let service_name = service.name.as_deref().unwrap_or("").trim();
            if service_name.is_empty() {
                bail!("gRPC descriptor set contains a service without a name");
            }

            for method in &service.method {
                let method_name = method.name.as_deref().unwrap_or("").trim();
                let request_type = method.input_type.as_deref().unwrap_or("").trim();
                let response_type = method.output_type.as_deref().unwrap_or("").trim();
                if method_name.is_empty() {
                    bail!("gRPC descriptor set contains a method without a name");
                }
                if request_type.is_empty() || response_type.is_empty() {
                    bail!(
                        "gRPC descriptor {}.{}/{} is missing request or response type",
                        package,
                        service_name,
                        method_name
                    );
                }

                descriptors.push(GrpcMethodDescriptor {
                    package: package.to_string(),
                    service: service_name.to_string(),
                    method: method_name.to_string(),
                    request_type: normalize_grpc_type_name(request_type),
                    response_type: normalize_grpc_type_name(response_type),
                    kind: grpc_method_kind_from_streaming(
                        method.client_streaming.unwrap_or(false),
                        method.server_streaming.unwrap_or(false),
                    ),
                });
            }
        }
    }

    Ok(descriptors)
}

pub fn format_grpc_method_catalog(descriptors: &[GrpcMethodDescriptor]) -> String {
    descriptors
        .iter()
        .map(|descriptor| {
            format!(
                "{} {} {} {}",
                grpc_method_kind_catalog_token(descriptor.kind),
                descriptor.full_method_name(),
                descriptor.request_type,
                descriptor.response_type
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_grpc_method_descriptor_line(
    line_number: usize,
    line: &str,
) -> Result<GrpcMethodDescriptor> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 4 {
        bail!(
            "gRPC method catalog line {line_number} must use kind service/method request_type response_type"
        );
    }

    let kind = parse_grpc_method_kind(parts[0], line_number)?;
    let (service_path, method) = parse_grpc_method_path(parts[1])
        .map_err(|error| anyhow!("gRPC method catalog line {line_number}: {error}"))?;
    let request_type = parts[2].trim();
    let response_type = parts[3].trim();
    if request_type.is_empty() || response_type.is_empty() {
        bail!("gRPC method catalog line {line_number} must include request and response types");
    }

    let (package, service) = split_grpc_package_service(&service_path);
    Ok(GrpcMethodDescriptor {
        package,
        service,
        method,
        request_type: request_type.to_string(),
        response_type: response_type.to_string(),
        kind,
    })
}

fn parse_grpc_method_kind(input: &str, line_number: usize) -> Result<GrpcMethodKind> {
    match input.trim().to_ascii_lowercase().as_str() {
        "unary" => Ok(GrpcMethodKind::Unary),
        "server-streaming" | "server_streaming" | "server" => Ok(GrpcMethodKind::ServerStreaming),
        "client-streaming" | "client_streaming" | "client" => Ok(GrpcMethodKind::ClientStreaming),
        "bidi-streaming" | "bidi_streaming" | "bidi" | "bidirectional" => {
            Ok(GrpcMethodKind::BidiStreaming)
        }
        _ => bail!("gRPC method catalog line {line_number} has unknown method kind"),
    }
}

fn grpc_method_kind_from_streaming(
    client_streaming: bool,
    server_streaming: bool,
) -> GrpcMethodKind {
    match (client_streaming, server_streaming) {
        (false, false) => GrpcMethodKind::Unary,
        (false, true) => GrpcMethodKind::ServerStreaming,
        (true, false) => GrpcMethodKind::ClientStreaming,
        (true, true) => GrpcMethodKind::BidiStreaming,
    }
}

fn grpc_method_kind_catalog_token(kind: GrpcMethodKind) -> &'static str {
    match kind {
        GrpcMethodKind::Unary => "unary",
        GrpcMethodKind::ServerStreaming => "server-streaming",
        GrpcMethodKind::ClientStreaming => "client-streaming",
        GrpcMethodKind::BidiStreaming => "bidi",
    }
}

fn normalize_grpc_type_name(input: &str) -> String {
    input.trim_start_matches('.').to_string()
}

fn split_grpc_package_service(service_path: &str) -> (String, String) {
    service_path
        .rsplit_once('.')
        .map(|(package, service)| (package.to_string(), service.to_string()))
        .unwrap_or_else(|| ("".to_string(), service_path.to_string()))
}

fn resolve_grpc_method_descriptor(
    service: &str,
    method: &str,
    descriptors: &[GrpcMethodDescriptor],
) -> Result<Option<GrpcMethodDescriptor>> {
    if descriptors.is_empty() {
        return Ok(None);
    }

    let requested = format!("{service}/{method}");
    descriptors
        .iter()
        .find(|descriptor| descriptor.full_method_name() == requested)
        .cloned()
        .map(Some)
        .ok_or_else(|| anyhow!("gRPC method {requested} was not found in the method catalog"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use prost_types::{FileDescriptorProto, MethodDescriptorProto, ServiceDescriptorProto};
    use serde_json::json;
    use temp_dir::TempDir;

    #[test]
    fn builds_grpc_request_draft_from_editor_fields() {
        let draft = build_grpc_request_draft(
            " http://localhost:50051 ",
            "/demo.Users/GetUser",
            "authorization=Bearer token\nx-trace-id: abc",
            r#"{"id":"u_123"}"#,
            "unary demo.Users/GetUser demo.GetUserRequest demo.GetUserResponse",
        )
        .expect("draft");

        assert_eq!(draft.endpoint, "http://localhost:50051");
        assert_eq!(draft.service, "demo.Users");
        assert_eq!(draft.method, "GetUser");
        assert_eq!(draft.method_path(), "/demo.Users/GetUser");
        assert_eq!(
            draft.metadata,
            vec![
                GrpcMetadata {
                    name: "authorization".to_string(),
                    value: "Bearer token".to_string(),
                },
                GrpcMetadata {
                    name: "x-trace-id".to_string(),
                    value: "abc".to_string(),
                },
            ]
        );
        assert_eq!(draft.message, json!({ "id": "u_123" }));
        assert_eq!(
            draft.descriptor.expect("descriptor").request_type,
            "demo.GetUserRequest"
        );
    }

    #[test]
    fn validates_grpc_endpoint_method_and_message() {
        assert!(
            parse_grpc_endpoint("localhost:50051")
                .expect_err("endpoint scheme")
                .to_string()
                .contains("http:// or https://")
        );
        assert!(
            parse_grpc_method_path("Users")
                .expect_err("method path")
                .to_string()
                .contains("service/method")
        );
        assert!(
            parse_grpc_message_json("[]")
                .expect_err("message shape")
                .to_string()
                .contains("must be an object")
        );
        assert!(
            build_grpc_request_draft(
                "http://localhost:50051",
                "demo.Users/Missing",
                "",
                "{}",
                "unary demo.Users/GetUser demo.GetUserRequest demo.GetUserResponse",
            )
            .expect_err("catalog miss")
            .to_string()
            .contains("not found in the method catalog")
        );
    }

    #[test]
    fn describes_full_grpc_method_names() {
        let descriptor = GrpcMethodDescriptor {
            package: "demo.v1".to_string(),
            service: "Users".to_string(),
            method: "List".to_string(),
            request_type: "demo.v1.ListUsersRequest".to_string(),
            response_type: "demo.v1.ListUsersResponse".to_string(),
            kind: GrpcMethodKind::Unary,
        };

        assert_eq!(descriptor.full_method_name(), "demo.v1.Users/List");
    }

    #[test]
    fn parses_grpc_method_catalog() {
        let descriptors = parse_grpc_method_catalog(
            "\
# kind service/method request response
unary demo.v1.Users/List demo.v1.ListUsersRequest demo.v1.ListUsersResponse
server-streaming demo.v1.Events/Subscribe demo.v1.SubscribeRequest demo.v1.Event
client-streaming demo.v1.Upload/Send demo.v1.Chunk demo.v1.UploadResult
bidi demo.v1.Chat/Stream demo.v1.ChatMessage demo.v1.ChatMessage
",
        )
        .expect("catalog");

        assert_eq!(descriptors.len(), 4);
        assert_eq!(descriptors[0].package, "demo.v1");
        assert_eq!(descriptors[0].service, "Users");
        assert_eq!(descriptors[0].full_method_name(), "demo.v1.Users/List");
        assert_eq!(descriptors[0].kind, GrpcMethodKind::Unary);
        assert_eq!(descriptors[1].kind, GrpcMethodKind::ServerStreaming);
        assert_eq!(descriptors[2].kind, GrpcMethodKind::ClientStreaming);
        assert_eq!(descriptors[3].kind, GrpcMethodKind::BidiStreaming);
        assert_eq!(
            GrpcMethodKind::BidiStreaming.label(),
            "bidirectional streaming"
        );
    }

    #[test]
    fn extracts_method_catalog_from_file_descriptor_set() {
        let descriptor_set = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("demo/users.proto".to_string()),
                package: Some("demo.v1".to_string()),
                service: vec![
                    ServiceDescriptorProto {
                        name: Some("Users".to_string()),
                        method: vec![
                            MethodDescriptorProto {
                                name: Some("GetUser".to_string()),
                                input_type: Some(".demo.v1.GetUserRequest".to_string()),
                                output_type: Some(".demo.v1.User".to_string()),
                                ..Default::default()
                            },
                            MethodDescriptorProto {
                                name: Some("WatchUsers".to_string()),
                                input_type: Some(".demo.v1.WatchUsersRequest".to_string()),
                                output_type: Some(".demo.v1.User".to_string()),
                                server_streaming: Some(true),
                                ..Default::default()
                            },
                            MethodDescriptorProto {
                                name: Some("UploadUsers".to_string()),
                                input_type: Some(".demo.v1.UserChunk".to_string()),
                                output_type: Some(".demo.v1.UploadSummary".to_string()),
                                client_streaming: Some(true),
                                ..Default::default()
                            },
                            MethodDescriptorProto {
                                name: Some("Chat".to_string()),
                                input_type: Some(".demo.v1.ChatMessage".to_string()),
                                output_type: Some(".demo.v1.ChatMessage".to_string()),
                                client_streaming: Some(true),
                                server_streaming: Some(true),
                                ..Default::default()
                            },
                        ],
                        ..Default::default()
                    },
                    ServiceDescriptorProto {
                        name: Some("Health".to_string()),
                        method: vec![MethodDescriptorProto {
                            name: Some("Check".to_string()),
                            input_type: Some(".demo.v1.HealthCheckRequest".to_string()),
                            output_type: Some(".demo.v1.HealthCheckResponse".to_string()),
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                ],
                syntax: Some("proto3".to_string()),
                ..Default::default()
            }],
        };

        let encoded = descriptor_set.encode_to_vec();
        let descriptors = decode_grpc_file_descriptor_set(&encoded).expect("descriptor set");

        assert_eq!(descriptors.len(), 5);
        assert_eq!(descriptors[0].full_method_name(), "demo.v1.Users/GetUser");
        assert_eq!(descriptors[0].request_type, "demo.v1.GetUserRequest");
        assert_eq!(descriptors[0].response_type, "demo.v1.User");
        assert_eq!(descriptors[0].kind, GrpcMethodKind::Unary);
        assert_eq!(descriptors[1].kind, GrpcMethodKind::ServerStreaming);
        assert_eq!(descriptors[2].kind, GrpcMethodKind::ClientStreaming);
        assert_eq!(descriptors[3].kind, GrpcMethodKind::BidiStreaming);
        assert_eq!(
            format_grpc_method_catalog(&descriptors),
            "\
unary demo.v1.Users/GetUser demo.v1.GetUserRequest demo.v1.User
server-streaming demo.v1.Users/WatchUsers demo.v1.WatchUsersRequest demo.v1.User
client-streaming demo.v1.Users/UploadUsers demo.v1.UserChunk demo.v1.UploadSummary
bidi demo.v1.Users/Chat demo.v1.ChatMessage demo.v1.ChatMessage
unary demo.v1.Health/Check demo.v1.HealthCheckRequest demo.v1.HealthCheckResponse"
        );
    }

    #[test]
    fn loads_method_catalog_from_descriptor_set_file() {
        let descriptor_set = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                package: Some("admin".to_string()),
                service: vec![ServiceDescriptorProto {
                    name: Some("Audit".to_string()),
                    method: vec![MethodDescriptorProto {
                        name: Some("ListEvents".to_string()),
                        input_type: Some(".admin.ListEventsRequest".to_string()),
                        output_type: Some(".admin.ListEventsResponse".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };

        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("audit.protoset");
        std::fs::write(&path, descriptor_set.encode_to_vec()).expect("write descriptor set");

        let descriptors = load_grpc_file_descriptor_set(&path).expect("descriptor set file");

        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].full_method_name(), "admin.Audit/ListEvents");
        assert_eq!(
            format_grpc_method_catalog(&descriptors),
            "unary admin.Audit/ListEvents admin.ListEventsRequest admin.ListEventsResponse"
        );
    }
}

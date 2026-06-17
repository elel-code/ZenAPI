use anyhow::{Result, anyhow, bail};
use futures_util::StreamExt;
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor};
use prost_types::{FileDescriptorProto, FileDescriptorSet};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use tonic::{
    Request, Status,
    codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder},
    metadata::{AsciiMetadataKey, AsciiMetadataValue, BinaryMetadataKey, BinaryMetadataValue},
    transport::Endpoint,
};
use tonic_reflection::pb::v1::{
    ServerReflectionRequest, server_reflection_client::ServerReflectionClient,
    server_reflection_request::MessageRequest, server_reflection_response::MessageResponse,
};

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GrpcUnaryResponse {
    pub method: String,
    pub metadata: Vec<GrpcMetadata>,
    pub message: Value,
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

pub fn load_grpc_file_descriptor_set_proto(path: impl AsRef<Path>) -> Result<FileDescriptorSet> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|error| {
        anyhow!(
            "failed to read gRPC descriptor set {}: {error}",
            path.display()
        )
    })?;
    FileDescriptorSet::decode(bytes.as_slice()).map_err(|error| {
        anyhow!(
            "failed to decode gRPC descriptor set {}: {error}",
            path.display()
        )
    })
}

pub fn load_grpc_file_descriptor_set(path: impl AsRef<Path>) -> Result<Vec<GrpcMethodDescriptor>> {
    let descriptor_set = load_grpc_file_descriptor_set_proto(path)?;
    grpc_method_descriptors_from_file_descriptor_set(&descriptor_set)
}

pub fn load_grpc_proto_file(
    proto_path: impl AsRef<Path>,
    include_paths: &[PathBuf],
    protoc_path: impl AsRef<Path>,
) -> Result<Vec<GrpcMethodDescriptor>> {
    let descriptor_set =
        load_grpc_proto_file_descriptor_set(proto_path, include_paths, protoc_path)?;
    grpc_method_descriptors_from_file_descriptor_set(&descriptor_set)
}

pub fn load_grpc_proto_file_descriptor_set(
    proto_path: impl AsRef<Path>,
    include_paths: &[PathBuf],
    protoc_path: impl AsRef<Path>,
) -> Result<FileDescriptorSet> {
    let proto_path = proto_path.as_ref();
    let protoc_path = protoc_path.as_ref();
    if !proto_path.exists() {
        bail!("gRPC proto file does not exist: {}", proto_path.display());
    }

    let descriptor_path = temporary_grpc_descriptor_path();
    let mut command = Command::new(protoc_path);
    command.arg("--include_imports").arg(format!(
        "--descriptor_set_out={}",
        descriptor_path.to_string_lossy()
    ));

    if let Some(parent) = proto_path.parent() {
        command.arg(format!("-I{}", parent.to_string_lossy()));
    }
    for include_path in include_paths {
        command.arg(format!("-I{}", include_path.to_string_lossy()));
    }
    command.arg(proto_path);

    let output = command.output().map_err(|error| {
        anyhow!(
            "failed to run protoc {}: {error}",
            protoc_path.to_string_lossy()
        )
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            output.status.to_string()
        } else {
            stderr
        };
        let _ = fs::remove_file(&descriptor_path);
        bail!("protoc failed for {}: {detail}", proto_path.display());
    }

    let result = load_grpc_file_descriptor_set_proto(&descriptor_path);
    let _ = fs::remove_file(&descriptor_path);
    result
}

pub async fn load_grpc_reflection_descriptors(endpoint: &str) -> Result<Vec<GrpcMethodDescriptor>> {
    let descriptor_set = load_grpc_reflection_descriptor_set(endpoint).await?;
    grpc_method_descriptors_from_file_descriptor_set(&descriptor_set)
}

pub async fn load_grpc_reflection_descriptor_set(endpoint: &str) -> Result<FileDescriptorSet> {
    let endpoint = parse_grpc_endpoint(endpoint)?;
    let channel = Endpoint::from_shared(endpoint.clone())
        .map_err(|error| anyhow!("invalid gRPC reflection endpoint {endpoint}: {error}"))?
        .connect()
        .await
        .map_err(|error| {
            anyhow!("failed to connect to gRPC reflection endpoint {endpoint}: {error}")
        })?;
    let mut client = ServerReflectionClient::new(channel);

    let services = request_grpc_reflection_services(&mut client).await?;
    if services.is_empty() {
        bail!("gRPC reflection endpoint did not report any services");
    }

    let mut files = BTreeMap::new();
    for service in services {
        let descriptors =
            request_grpc_reflection_file_containing_symbol(&mut client, &service).await?;
        for descriptor in descriptors {
            let name = descriptor.name.clone().unwrap_or_default();
            files.entry(name).or_insert(descriptor);
        }
    }

    Ok(FileDescriptorSet {
        file: files.into_values().collect(),
    })
}

pub async fn invoke_grpc_unary(
    endpoint: &str,
    full_method: &str,
    metadata_lines: &str,
    message_json: &str,
    descriptor_set: FileDescriptorSet,
) -> Result<GrpcUnaryResponse> {
    let endpoint = parse_grpc_endpoint(endpoint)?;
    let (service, method_name) = parse_grpc_method_path(full_method)?;
    let metadata = parse_grpc_metadata_lines(metadata_lines)?;
    let message = parse_grpc_message_json(message_json)?;
    let pool = DescriptorPool::from_file_descriptor_set(descriptor_set)
        .map_err(|error| anyhow!("failed to build gRPC descriptor pool: {error}"))?;
    let method = resolve_dynamic_grpc_method(&pool, &service, &method_name)?;
    if method.is_client_streaming() || method.is_server_streaming() {
        bail!("gRPC method {service}/{method_name} is not unary");
    }

    let request_message = dynamic_message_from_json(method.input(), &message)?;
    let codec = DynamicMessageCodec::new(method.input(), method.output());
    let channel = Endpoint::from_shared(endpoint.clone())
        .map_err(|error| anyhow!("invalid gRPC endpoint {endpoint}: {error}"))?
        .connect()
        .await
        .map_err(|error| anyhow!("failed to connect to gRPC endpoint {endpoint}: {error}"))?;
    let mut client = tonic::client::Grpc::new(channel);
    let path = tonic::codegen::http::uri::PathAndQuery::from_maybe_shared(format!(
        "/{service}/{method_name}"
    ))
    .map_err(|error| anyhow!("invalid gRPC method path {service}/{method_name}: {error}"))?;
    let mut request = Request::new(request_message);
    apply_grpc_metadata(request.metadata_mut(), &metadata)?;

    client
        .ready()
        .await
        .map_err(|error| anyhow!("gRPC endpoint {endpoint} was not ready: {error}"))?;
    let response = client
        .unary(request, path, codec)
        .await
        .map_err(format_grpc_status_error)?;
    let response_metadata = grpc_metadata_from_tonic(response.metadata())?;
    let response_message = serde_json::to_value(response.into_inner())
        .map_err(|error| anyhow!("failed to serialize gRPC response message: {error}"))?;

    Ok(GrpcUnaryResponse {
        method: format!("{service}/{method_name}"),
        metadata: response_metadata,
        message: response_message,
    })
}

pub async fn invoke_grpc_unary_from_descriptor_bytes(
    endpoint: &str,
    full_method: &str,
    metadata_lines: &str,
    message_json: &str,
    descriptor_set_bytes: &[u8],
) -> Result<GrpcUnaryResponse> {
    let descriptor_set = FileDescriptorSet::decode(descriptor_set_bytes)
        .map_err(|error| anyhow!("failed to decode gRPC descriptor set: {error}"))?;
    invoke_grpc_unary(
        endpoint,
        full_method,
        metadata_lines,
        message_json,
        descriptor_set,
    )
    .await
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

fn resolve_dynamic_grpc_method(
    pool: &DescriptorPool,
    service: &str,
    method_name: &str,
) -> Result<prost_reflect::MethodDescriptor> {
    let service_descriptor = pool
        .get_service_by_name(service)
        .ok_or_else(|| anyhow!("gRPC service {service} was not found in the descriptor set"))?;
    service_descriptor
        .methods()
        .find(|method| method.name() == method_name)
        .ok_or_else(|| {
            anyhow!("gRPC method {service}/{method_name} was not found in the descriptor set")
        })
}

fn dynamic_message_from_json(
    descriptor: MessageDescriptor,
    value: &Value,
) -> Result<DynamicMessage> {
    let json = value.to_string();
    let mut deserializer = serde_json::Deserializer::from_str(&json);
    let message = DynamicMessage::deserialize(descriptor, &mut deserializer)
        .map_err(|error| anyhow!("failed to encode gRPC request JSON: {error}"))?;
    deserializer
        .end()
        .map_err(|error| anyhow!("gRPC request JSON had trailing data: {error}"))?;
    Ok(message)
}

fn apply_grpc_metadata(
    target: &mut tonic::metadata::MetadataMap,
    metadata: &[GrpcMetadata],
) -> Result<()> {
    for item in metadata {
        if item.name.ends_with("-bin") {
            let key = BinaryMetadataKey::from_bytes(item.name.as_bytes()).map_err(|error| {
                anyhow!("invalid binary gRPC metadata key {}: {error}", item.name)
            })?;
            let value = BinaryMetadataValue::from_bytes(item.value.as_bytes());
            target.insert_bin(key, value);
        } else {
            let key = AsciiMetadataKey::from_bytes(item.name.as_bytes())
                .map_err(|error| anyhow!("invalid gRPC metadata key {}: {error}", item.name))?;
            let value = AsciiMetadataValue::try_from(item.value.as_str()).map_err(|error| {
                anyhow!("invalid gRPC metadata value for {}: {error}", item.name)
            })?;
            target.insert(key, value);
        }
    }
    Ok(())
}

fn grpc_metadata_from_tonic(map: &tonic::metadata::MetadataMap) -> Result<Vec<GrpcMetadata>> {
    map.iter()
        .map(|entry| match entry {
            tonic::metadata::KeyAndValueRef::Ascii(key, value) => Ok(GrpcMetadata {
                name: key.to_string(),
                value: value
                    .to_str()
                    .map_err(|error| anyhow!("invalid ASCII gRPC metadata value: {error}"))?
                    .to_string(),
            }),
            tonic::metadata::KeyAndValueRef::Binary(key, value) => Ok(GrpcMetadata {
                name: key.to_string(),
                value: String::from_utf8_lossy(&value.to_bytes().map_err(|error| {
                    anyhow!("invalid binary gRPC metadata value for {}: {error}", key)
                })?)
                .to_string(),
            }),
        })
        .collect()
}

fn format_grpc_status_error(status: Status) -> anyhow::Error {
    let details = if status.details().is_empty() {
        String::new()
    } else {
        format!("; details={} bytes", status.details().len())
    };
    anyhow!(
        "gRPC unary request failed: {:?}: {}{}",
        status.code(),
        status.message(),
        details
    )
}

#[derive(Clone)]
struct DynamicMessageCodec {
    encode_descriptor: MessageDescriptor,
    decode_descriptor: MessageDescriptor,
}

impl DynamicMessageCodec {
    fn new(encode_descriptor: MessageDescriptor, decode_descriptor: MessageDescriptor) -> Self {
        Self {
            encode_descriptor,
            decode_descriptor,
        }
    }
}

impl Codec for DynamicMessageCodec {
    type Encode = DynamicMessage;
    type Decode = DynamicMessage;
    type Encoder = DynamicMessageEncoder;
    type Decoder = DynamicMessageDecoder;

    fn encoder(&mut self) -> Self::Encoder {
        DynamicMessageEncoder {
            descriptor_name: self.encode_descriptor.full_name().to_string(),
        }
    }

    fn decoder(&mut self) -> Self::Decoder {
        DynamicMessageDecoder {
            descriptor: self.decode_descriptor.clone(),
        }
    }
}

#[derive(Clone)]
struct DynamicMessageEncoder {
    descriptor_name: String,
}

impl Encoder for DynamicMessageEncoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn encode(
        &mut self,
        item: Self::Item,
        buf: &mut EncodeBuf<'_>,
    ) -> std::result::Result<(), Self::Error> {
        item.encode(buf).map_err(|error| {
            Status::internal(format!(
                "failed to encode dynamic gRPC message {}: {error}",
                self.descriptor_name
            ))
        })
    }
}

#[derive(Clone)]
struct DynamicMessageDecoder {
    descriptor: MessageDescriptor,
}

impl Decoder for DynamicMessageDecoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn decode(
        &mut self,
        buf: &mut DecodeBuf<'_>,
    ) -> std::result::Result<Option<Self::Item>, Self::Error> {
        DynamicMessage::decode(self.descriptor.clone(), buf)
            .map(Some)
            .map_err(|error| {
                Status::internal(format!("failed to decode dynamic gRPC message: {error}"))
            })
    }
}

async fn request_grpc_reflection_services<T>(
    client: &mut ServerReflectionClient<T>,
) -> Result<Vec<String>>
where
    T: tonic::client::GrpcService<tonic::body::Body>,
    T::Error: Into<tonic::codegen::StdError>,
    T::ResponseBody: tonic::codegen::Body<Data = tonic::codegen::Bytes> + Send + 'static,
    <T::ResponseBody as tonic::codegen::Body>::Error: Into<tonic::codegen::StdError> + Send,
{
    let response = send_grpc_reflection_request(
        client,
        ServerReflectionRequest {
            host: String::new(),
            message_request: Some(MessageRequest::ListServices(String::new())),
        },
    )
    .await?;

    match response {
        MessageResponse::ListServicesResponse(services) => Ok(services
            .service
            .into_iter()
            .map(|service| service.name)
            .filter(|name| !name.is_empty() && !name.starts_with("grpc.reflection."))
            .collect()),
        MessageResponse::ErrorResponse(error) => bail!(
            "gRPC reflection list services failed: {} ({})",
            error.error_message,
            error.error_code
        ),
        _ => bail!("gRPC reflection returned an unexpected list services response"),
    }
}

async fn request_grpc_reflection_file_containing_symbol<T>(
    client: &mut ServerReflectionClient<T>,
    symbol: &str,
) -> Result<Vec<FileDescriptorProto>>
where
    T: tonic::client::GrpcService<tonic::body::Body>,
    T::Error: Into<tonic::codegen::StdError>,
    T::ResponseBody: tonic::codegen::Body<Data = tonic::codegen::Bytes> + Send + 'static,
    <T::ResponseBody as tonic::codegen::Body>::Error: Into<tonic::codegen::StdError> + Send,
{
    let response = send_grpc_reflection_request(
        client,
        ServerReflectionRequest {
            host: String::new(),
            message_request: Some(MessageRequest::FileContainingSymbol(symbol.to_string())),
        },
    )
    .await?;

    match response {
        MessageResponse::FileDescriptorResponse(file_response) => file_response
            .file_descriptor_proto
            .into_iter()
            .map(|bytes| {
                FileDescriptorProto::decode(bytes.as_slice()).map_err(|error| {
                    anyhow!("failed to decode gRPC reflection descriptor for {symbol}: {error}")
                })
            })
            .collect(),
        MessageResponse::ErrorResponse(error) => bail!(
            "gRPC reflection file lookup failed for {symbol}: {} ({})",
            error.error_message,
            error.error_code
        ),
        _ => bail!("gRPC reflection returned an unexpected file descriptor response for {symbol}"),
    }
}

async fn send_grpc_reflection_request<T>(
    client: &mut ServerReflectionClient<T>,
    request: ServerReflectionRequest,
) -> Result<MessageResponse>
where
    T: tonic::client::GrpcService<tonic::body::Body>,
    T::Error: Into<tonic::codegen::StdError>,
    T::ResponseBody: tonic::codegen::Body<Data = tonic::codegen::Bytes> + Send + 'static,
    <T::ResponseBody as tonic::codegen::Body>::Error: Into<tonic::codegen::StdError> + Send,
{
    let mut inbound = client
        .server_reflection_info(Request::new(futures_util::stream::iter([request])))
        .await
        .map_err(|error| anyhow!("gRPC reflection request failed: {error}"))?
        .into_inner();

    let response = inbound
        .next()
        .await
        .ok_or_else(|| anyhow!("gRPC reflection response stream ended without a response"))?
        .map_err(|error| anyhow!("gRPC reflection response failed: {error}"))?;

    response
        .message_response
        .ok_or_else(|| anyhow!("gRPC reflection response did not include a message"))
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

fn temporary_grpc_descriptor_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "zenapi-grpc-{}-{nanos}.protoset",
        std::process::id()
    ))
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
    use prost_types::{
        DescriptorProto, FieldDescriptorProto, MethodDescriptorProto, ServiceDescriptorProto,
        field_descriptor_proto::{Label, Type},
    };
    use serde_json::json;
    use std::convert::Infallible;
    use std::net::SocketAddr;
    use temp_dir::TempDir;
    use tokio::sync::oneshot;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::transport::Server;
    use tonic_reflection::server::Builder as ReflectionBuilder;

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

    #[test]
    fn loads_method_catalog_from_proto_source_file() {
        let temp_dir = TempDir::new().expect("temp dir");
        let proto_path = temp_dir.path().join("users.proto");
        std::fs::write(
            &proto_path,
            r#"
syntax = "proto3";
package demo.v1;

service Users {
  rpc GetUser (GetUserRequest) returns (User);
  rpc WatchUsers (WatchUsersRequest) returns (stream User);
}

message GetUserRequest {
  string id = 1;
}

message WatchUsersRequest {
  string group = 1;
}

message User {
  string id = 1;
}
"#,
        )
        .expect("write proto");

        let descriptors =
            load_grpc_proto_file(&proto_path, &[], "protoc").expect("proto descriptor");

        assert_eq!(descriptors.len(), 2);
        assert_eq!(descriptors[0].full_method_name(), "demo.v1.Users/GetUser");
        assert_eq!(descriptors[0].kind, GrpcMethodKind::Unary);
        assert_eq!(
            descriptors[1].full_method_name(),
            "demo.v1.Users/WatchUsers"
        );
        assert_eq!(descriptors[1].kind, GrpcMethodKind::ServerStreaming);
        assert_eq!(
            format_grpc_method_catalog(&descriptors),
            "\
unary demo.v1.Users/GetUser demo.v1.GetUserRequest demo.v1.User
server-streaming demo.v1.Users/WatchUsers demo.v1.WatchUsersRequest demo.v1.User"
        );
    }

    #[tokio::test]
    async fn loads_method_catalog_from_reflection_service() {
        let descriptor_set = FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("demo/users.proto".to_string()),
                package: Some("demo.v1".to_string()),
                service: vec![ServiceDescriptorProto {
                    name: Some("Users".to_string()),
                    method: vec![
                        MethodDescriptorProto {
                            name: Some("GetUser".to_string()),
                            input_type: Some(".demo.v1.GetUserRequest".to_string()),
                            output_type: Some(".demo.v1.User".to_string()),
                            ..Default::default()
                        },
                        MethodDescriptorProto {
                            name: Some("UploadUsers".to_string()),
                            input_type: Some(".demo.v1.UserChunk".to_string()),
                            output_type: Some(".demo.v1.UploadSummary".to_string()),
                            client_streaming: Some(true),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }],
                syntax: Some("proto3".to_string()),
                ..Default::default()
            }],
        };
        let encoded = descriptor_set.encode_to_vec();

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr");
        let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local addr"));
        let server = tokio::spawn(async move {
            let service = ReflectionBuilder::configure()
                .register_encoded_file_descriptor_set(&encoded)
                .build_v1()
                .expect("reflection service");

            Server::builder()
                .add_service(service)
                .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("reflection server");
        });

        let descriptors = load_grpc_reflection_descriptors(&endpoint)
            .await
            .expect("reflection descriptors");
        shutdown_tx.send(()).expect("shutdown");
        server.await.expect("server join");

        assert_eq!(descriptors.len(), 2);
        assert_eq!(descriptors[0].full_method_name(), "demo.v1.Users/GetUser");
        assert_eq!(descriptors[0].kind, GrpcMethodKind::Unary);
        assert_eq!(
            descriptors[1].full_method_name(),
            "demo.v1.Users/UploadUsers"
        );
        assert_eq!(descriptors[1].kind, GrpcMethodKind::ClientStreaming);
        assert_eq!(
            format_grpc_method_catalog(&descriptors),
            "\
unary demo.v1.Users/GetUser demo.v1.GetUserRequest demo.v1.User
client-streaming demo.v1.Users/UploadUsers demo.v1.UserChunk demo.v1.UploadSummary"
        );
    }

    #[tokio::test]
    async fn invokes_unary_grpc_service_with_dynamic_messages() {
        let descriptor_set = unary_test_descriptor_set();
        let pool = DescriptorPool::from_file_descriptor_set(descriptor_set.clone()).expect("pool");
        let service = TestUsersServer {
            input: pool
                .get_message_by_name("demo.v1.GetUserRequest")
                .expect("request descriptor"),
            output: pool
                .get_message_by_name("demo.v1.User")
                .expect("response descriptor"),
        };

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr");
        let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local addr"));
        let server = tokio::spawn(async move {
            Server::builder()
                .add_service(service)
                .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("unary test server");
        });

        let response = invoke_grpc_unary(
            &endpoint,
            "/demo.v1.Users/GetUser",
            "x-trace-id=abc-123",
            r#"{"id":"u_123"}"#,
            descriptor_set,
        )
        .await
        .expect("unary response");
        shutdown_tx.send(()).expect("shutdown");
        server.await.expect("server join");

        assert_eq!(response.method, "demo.v1.Users/GetUser");
        assert_eq!(
            response.message,
            json!({
                "id": "u_123",
                "name": "Ada abc-123",
            })
        );
        assert!(
            response
                .metadata
                .iter()
                .any(|item| item.name == "x-served-by" && item.value == "zenapi-test")
        );
    }

    #[tokio::test]
    async fn rejects_non_unary_grpc_invocation() {
        let mut descriptor_set = unary_test_descriptor_set();
        descriptor_set.file[0].service[0].method[0].server_streaming = Some(true);

        let error = invoke_grpc_unary(
            "http://localhost:1",
            "/demo.v1.Users/GetUser",
            "",
            "{}",
            descriptor_set,
        )
        .await
        .expect_err("streaming method");

        assert!(error.to_string().contains("is not unary"));
    }

    fn unary_test_descriptor_set() -> FileDescriptorSet {
        FileDescriptorSet {
            file: vec![FileDescriptorProto {
                name: Some("demo/users.proto".to_string()),
                package: Some("demo.v1".to_string()),
                message_type: vec![
                    DescriptorProto {
                        name: Some("GetUserRequest".to_string()),
                        field: vec![string_field("id", 1)],
                        ..Default::default()
                    },
                    DescriptorProto {
                        name: Some("User".to_string()),
                        field: vec![string_field("id", 1), string_field("name", 2)],
                        ..Default::default()
                    },
                ],
                service: vec![ServiceDescriptorProto {
                    name: Some("Users".to_string()),
                    method: vec![MethodDescriptorProto {
                        name: Some("GetUser".to_string()),
                        input_type: Some(".demo.v1.GetUserRequest".to_string()),
                        output_type: Some(".demo.v1.User".to_string()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                syntax: Some("proto3".to_string()),
                ..Default::default()
            }],
        }
    }

    fn string_field(name: &str, number: i32) -> FieldDescriptorProto {
        FieldDescriptorProto {
            name: Some(name.to_string()),
            number: Some(number),
            label: Some(Label::Optional as i32),
            r#type: Some(Type::String as i32),
            json_name: Some(name.to_string()),
            ..Default::default()
        }
    }

    #[derive(Clone)]
    struct TestUsersServer {
        input: MessageDescriptor,
        output: MessageDescriptor,
    }

    impl<B> tonic::codegen::Service<tonic::codegen::http::Request<B>> for TestUsersServer
    where
        B: tonic::codegen::Body + Send + 'static,
        B::Error: Into<tonic::codegen::StdError> + Send + 'static,
    {
        type Response = tonic::codegen::http::Response<tonic::body::Body>;
        type Error = Infallible;
        type Future = tonic::codegen::BoxFuture<Self::Response, Self::Error>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::result::Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: tonic::codegen::http::Request<B>) -> Self::Future {
            match req.uri().path() {
                "/demo.v1.Users/GetUser" => {
                    let method = TestGetUserSvc {
                        output: self.output.clone(),
                    };
                    let codec = DynamicMessageCodec::new(self.output.clone(), self.input.clone());
                    let fut = async move {
                        let mut grpc = tonic::server::Grpc::new(codec);
                        Ok(grpc.unary(method, req).await)
                    };
                    Box::pin(fut)
                }
                _ => Box::pin(async move {
                    let mut response =
                        tonic::codegen::http::Response::new(tonic::body::Body::default());
                    let headers = response.headers_mut();
                    headers.insert(
                        tonic::Status::GRPC_STATUS,
                        (tonic::Code::Unimplemented as i32).into(),
                    );
                    headers.insert(
                        tonic::codegen::http::header::CONTENT_TYPE,
                        tonic::metadata::GRPC_CONTENT_TYPE,
                    );
                    Ok(response)
                }),
            }
        }
    }

    impl tonic::server::NamedService for TestUsersServer {
        const NAME: &'static str = "demo.v1.Users";
    }

    #[derive(Clone)]
    struct TestGetUserSvc {
        output: MessageDescriptor,
    }

    impl tonic::server::UnaryService<DynamicMessage> for TestGetUserSvc {
        type Response = DynamicMessage;
        type Future = tonic::codegen::BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

        fn call(&mut self, request: tonic::Request<DynamicMessage>) -> Self::Future {
            let output = self.output.clone();
            Box::pin(async move {
                let trace_id = request
                    .metadata()
                    .get("x-trace-id")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("missing")
                    .to_string();
                let request_message = serde_json::to_value(request.into_inner())
                    .map_err(|error| tonic::Status::internal(error.to_string()))?;
                let id = request_message
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let response_json = json!({
                    "id": id,
                    "name": format!("Ada {trace_id}"),
                });
                let message = dynamic_message_from_json(output, &response_json)
                    .map_err(|error| tonic::Status::internal(error.to_string()))?;
                let mut response = tonic::Response::new(message);
                response
                    .metadata_mut()
                    .insert("x-served-by", "zenapi-test".parse().expect("metadata"));
                Ok(response)
            })
        }
    }
}

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrpcMethodKind {
    Unary,
    ServerStreaming,
    ClientStreaming,
    BidiStreaming,
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
) -> Result<GrpcRequestDraft> {
    let endpoint = parse_grpc_endpoint(endpoint)?;
    let (service, method) = parse_grpc_method_path(full_method)?;
    let metadata = parse_grpc_metadata_lines(metadata_lines)?;
    let message = parse_grpc_message_json(message_json)?;

    Ok(GrpcRequestDraft {
        endpoint,
        service,
        method,
        metadata,
        message,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_grpc_request_draft_from_editor_fields() {
        let draft = build_grpc_request_draft(
            " http://localhost:50051 ",
            "/demo.Users/GetUser",
            "authorization=Bearer token\nx-trace-id: abc",
            r#"{"id":"u_123"}"#,
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
}

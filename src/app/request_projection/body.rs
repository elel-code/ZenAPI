use anyhow::{Result, bail};
use std::path::Path;
use zenapi::client::RequestBody;

use super::super::graphql::build_graphql_request_body;
use super::key_value::parse_key_value_lines;

pub(in crate::app) fn build_request_body(
    mode: &str,
    input: &str,
    graphql_variables: &str,
    raw_body_subtype: &str,
) -> Result<RequestBody> {
    match mode {
        "none" => Ok(RequestBody::None),
        "form" => Ok(RequestBody::Multipart(parse_key_value_lines(
            input,
            "form field",
        )?)),
        "urlenc" => Ok(RequestBody::FormUrlEncoded(parse_key_value_lines(
            input,
            "urlencoded field",
        )?)),
        "binary" => {
            let path = input.trim();
            if path.is_empty() {
                bail!("binary body path is empty");
            }
            Ok(RequestBody::BinaryFile {
                path: path.to_string(),
                content_type: Some(binary_body_content_type(path).to_string()),
            })
        }
        "graphql" => build_graphql_request_body(input, graphql_variables),
        _ => Ok(RequestBody::Raw {
            content_type: Some(raw_body_content_type(raw_body_subtype).to_string()),
            body: input.to_string(),
        }),
    }
}

pub(in crate::app) fn normalize_raw_body_subtype(subtype: &str) -> &'static str {
    match subtype.trim().to_lowercase().as_str() {
        "text" | "plain" | "txt" => "text",
        "xml" => "xml",
        _ => "json",
    }
}

fn raw_body_content_type(subtype: &str) -> &'static str {
    match normalize_raw_body_subtype(subtype) {
        "text" => "text/plain",
        "xml" => "application/xml",
        _ => "application/json",
    }
}

pub(in crate::app) fn binary_body_content_type(path: &str) -> &'static str {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_lowercase();
    match extension.as_str() {
        "json" | "map" => "application/json",
        "txt" | "log" => "text/plain",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        "wasm" => "application/wasm",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

pub(in crate::app) fn raw_body_subtype_from_content_type(content_type: &str) -> String {
    let normalized = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    match normalized.as_str() {
        "text/plain" => "text".to_string(),
        "application/xml" | "text/xml" => "xml".to_string(),
        _ => "json".to_string(),
    }
}

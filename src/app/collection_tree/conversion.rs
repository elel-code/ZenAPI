use zenapi::{
    client::RequestBody,
    codegen::CodegenRequest,
    collections::{CollectionBody, CollectionRequest, NameValue},
};

pub(in crate::app) fn collection_request_from_codegen(
    request: &CodegenRequest,
) -> CollectionRequest {
    CollectionRequest {
        name: format!("{} {}", request.method, request.url),
        method: request.method.clone(),
        url: request.url.clone(),
        headers: name_values_from_pairs(&request.headers),
        query_params: name_values_from_pairs(&request.query_params),
        body: collection_body_from_request_body(&request.body),
        pre_request_script: String::new(),
        tests: Vec::new(),
    }
}

fn name_values_from_pairs(pairs: &[(String, String)]) -> Vec<NameValue> {
    pairs
        .iter()
        .map(|(name, value)| NameValue {
            name: name.clone(),
            value: value.clone(),
        })
        .collect()
}

fn collection_body_from_request_body(body: &RequestBody) -> CollectionBody {
    match body {
        RequestBody::None => CollectionBody::None,
        RequestBody::Raw { content_type, body } => CollectionBody::Raw {
            content_type: content_type
                .clone()
                .unwrap_or_else(|| "application/json".to_string()),
            body: body.clone(),
        },
        RequestBody::FormUrlEncoded(fields) => CollectionBody::UrlEncoded {
            fields: name_values_from_pairs(fields),
        },
        RequestBody::Multipart(fields) => CollectionBody::FormData {
            fields: name_values_from_pairs(fields),
        },
        RequestBody::BinaryFile { path, content_type } => CollectionBody::Binary {
            path: path.clone(),
            content_type: content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_string()),
        },
    }
}

pub(in crate::app) fn format_name_values(values: &[NameValue]) -> String {
    values
        .iter()
        .map(|value| format!("{}={}", value.name, value.value))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(in crate::app) fn format_header_lines(headers: &[(String, String)]) -> String {
    headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

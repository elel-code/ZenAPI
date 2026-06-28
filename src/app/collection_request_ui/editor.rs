use anyhow::{Result, bail};
use zenapi::{
    assertions::ResponseAssertion,
    codegen::CodegenRequest,
    collections::{CollectionBody, CollectionRequest},
};

use crate::{auth::build_auth_entries, ui::AppWindow};

use super::super::{
    collection_tree::{collection_request_from_codegen, format_name_values},
    request_editor_ui::{
        refresh_auth_key_rows, refresh_basic_auth_fields, refresh_body_field_rows,
        refresh_header_rows, refresh_query_param_rows, refresh_test_assertion_rows,
    },
    request_projection::{
        RequestProjectionInput, build_request_body, parse_key_value_lines,
        raw_body_subtype_from_content_type, upsert_pair,
    },
    response_assertion_parser::format_response_assertions,
    set_response,
};

pub(in crate::app) fn collection_request_from_editor(
    input: &RequestProjectionInput,
    tests: Vec<ResponseAssertion>,
) -> Result<CollectionRequest> {
    let mut headers = parse_key_value_lines(&input.headers, "header")?;
    let mut query_params = parse_key_value_lines(&input.query_params, "query param")?;
    let (auth_headers, auth_query_params) =
        build_auth_entries(&input.auth_mode, &input.auth_config)?;
    for (name, value) in auth_headers {
        upsert_pair(&mut headers, name, value, true);
    }
    for (name, value) in auth_query_params {
        upsert_pair(&mut query_params, name, value, false);
    }
    let request = CodegenRequest {
        method: input.method.clone(),
        url: input.url.trim().to_string(),
        headers,
        query_params,
        body: build_request_body(
            &input.body_mode,
            &input.body,
            &input.graphql_variables,
            &input.raw_body_subtype,
        )?,
    };
    if request.url.trim().is_empty() {
        bail!("request URL is empty");
    }

    let mut collection_request = collection_request_from_codegen(&request);
    collection_request.pre_request_script = input.pre_request_script.trim().to_string();
    collection_request.tests = tests;
    Ok(collection_request)
}

pub(in crate::app) fn collection_body_to_slint(body: &CollectionBody) -> (String, String, String) {
    match body {
        CollectionBody::None => ("none".to_string(), String::new(), "json".to_string()),
        CollectionBody::Raw { body, content_type } => (
            "raw".to_string(),
            body.clone(),
            raw_body_subtype_from_content_type(content_type),
        ),
        CollectionBody::FormData { fields } => (
            "form".to_string(),
            format_name_values(fields),
            "json".to_string(),
        ),
        CollectionBody::UrlEncoded { fields } => (
            "urlenc".to_string(),
            format_name_values(fields),
            "json".to_string(),
        ),
        CollectionBody::Binary { path, .. } => {
            ("binary".to_string(), path.clone(), "json".to_string())
        }
    }
}

pub(in crate::app) fn restore_collection_request(app: &AppWindow, request: &CollectionRequest) {
    let (body_mode, request_body, raw_body_subtype) = collection_body_to_slint(&request.body);
    app.set_method(request.method.clone().into());
    app.set_url(request.url.clone().into());
    app.set_query_params(format_name_values(&request.query_params).into());
    refresh_query_param_rows(app);
    app.set_request_headers(format_name_values(&request.headers).into());
    refresh_header_rows(app);
    app.set_auth_mode("none".into());
    app.set_auth_config("".into());
    refresh_auth_key_rows(app);
    refresh_basic_auth_fields(app);
    app.set_body_mode(body_mode.into());
    app.set_raw_body_subtype(raw_body_subtype.into());
    app.set_request_body(request_body.into());
    refresh_body_field_rows(app);
    app.set_graphql_variables("{}".into());
    app.set_pre_request_script(request.pre_request_script.clone().into());
    app.set_request_tests(format_response_assertions(&request.tests).into());
    refresh_test_assertion_rows(app);
    app.set_collection_status(format!("Selected {}", request.name).into());
    set_response(
        app,
        "Collection request loaded",
        &request.name,
        "neutral",
        &request.url,
    );
}

use anyhow::{Result, bail};
use zenapi::{
    codegen::CodegenRequest,
    pre_request::{execute_pre_request_actions, resolve_codegen_request_templates},
    variables::{Variable, VariableStore, replace_variables},
};

use crate::auth::build_auth_entries;

mod body;
mod key_value;

#[cfg(test)]
pub(super) use self::body::binary_body_content_type;
pub(super) use self::body::{
    build_request_body, normalize_raw_body_subtype, raw_body_subtype_from_content_type,
};
pub(super) use self::key_value::{
    apply_header_preset, parse_key_value_lines, split_key_value_line, upsert_pair,
};

pub(super) struct RequestProjectionInput {
    pub(super) method: String,
    pub(super) url: String,
    pub(super) query_params: String,
    pub(super) headers: String,
    pub(super) auth_mode: String,
    pub(super) auth_config: String,
    pub(super) body_mode: String,
    pub(super) raw_body_subtype: String,
    pub(super) body: String,
    pub(super) graphql_variables: String,
    pub(super) pre_request_script: String,
    pub(super) global_variables: String,
    pub(super) environment_name: String,
    pub(super) environment_variables: String,
}

pub(super) fn build_codegen_request_projection(
    input: &RequestProjectionInput,
) -> Result<CodegenRequest> {
    let (variables, active_environment) = build_variable_store(
        &input.global_variables,
        &input.environment_name,
        &input.environment_variables,
    )?;
    let active_environment = active_environment.as_deref();

    let mut request = build_unresolved_editor_request(input, &variables, active_environment)?;
    let execution = execute_pre_request_actions(
        &input.pre_request_script,
        request,
        variables,
        active_environment,
    )?;
    request = resolve_codegen_request_templates(
        execution.request,
        &execution.variables,
        active_environment,
    )?;
    request.url = request.url.trim().to_string();
    if request.url.is_empty() {
        bail!("request URL is empty");
    }

    Ok(request)
}

fn build_unresolved_editor_request(
    input: &RequestProjectionInput,
    variables: &VariableStore,
    active_environment: Option<&str>,
) -> Result<CodegenRequest> {
    let mut headers = parse_key_value_lines(&input.headers, "header")?;
    let mut query_params = parse_key_value_lines(&input.query_params, "query param")?;
    let auth_config = resolve_text(&input.auth_config, variables, active_environment)?;
    let (auth_headers, auth_query_params) = build_auth_entries(&input.auth_mode, &auth_config)?;
    for (name, value) in auth_headers {
        upsert_pair(&mut headers, name, value, true);
    }
    for (name, value) in auth_query_params {
        upsert_pair(&mut query_params, name, value, false);
    }

    Ok(CodegenRequest {
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
    })
}

pub(super) fn build_variable_store(
    global_input: &str,
    environment_name: &str,
    environment_input: &str,
) -> Result<(VariableStore, Option<String>)> {
    let environment_name = environment_name.trim();
    let environment_pairs = parse_key_value_lines(environment_input, "environment variable")?;
    if !environment_pairs.is_empty() && environment_name.is_empty() {
        bail!("environment name is empty");
    }

    let mut store = VariableStore::new();
    for (name, value) in parse_key_value_lines(global_input, "global variable")? {
        store.upsert(Variable::global(name, value));
    }

    for (name, value) in environment_pairs {
        store.upsert(Variable::environment(environment_name, name, value));
    }

    let active_environment = (!environment_name.is_empty()).then(|| environment_name.to_string());
    Ok((store, active_environment))
}

pub(super) fn resolve_text(
    input: &str,
    variables: &VariableStore,
    active_environment: Option<&str>,
) -> Result<String> {
    replace_variables(input, variables, active_environment)
}

#[cfg(test)]
pub(super) fn resolve_pairs(
    pairs: Vec<(String, String)>,
    variables: &VariableStore,
    active_environment: Option<&str>,
) -> Result<Vec<(String, String)>> {
    pairs
        .into_iter()
        .map(|(name, value)| {
            Ok((
                replace_variables(&name, variables, active_environment)?,
                replace_variables(&value, variables, active_environment)?,
            ))
        })
        .collect()
}

use anyhow::{Result, anyhow};

use crate::{
    client::RequestBody,
    codegen::CodegenRequest,
    variables::{Variable, VariableScope, VariableStore, replace_variables},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreRequestExecution {
    pub request: CodegenRequest,
    pub variables: VariableStore,
    pub actions_applied: usize,
    pub actions: Vec<PreRequestActionRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreRequestActionRecord {
    pub action: String,
    pub target: String,
}

pub fn execute_pre_request_actions(
    script: &str,
    mut request: CodegenRequest,
    mut variables: VariableStore,
    active_environment: Option<&str>,
) -> Result<PreRequestExecution> {
    let mut actions = Vec::new();

    for action in script_actions(script) {
        let (name, argument) = parse_action(action)?;
        let target;
        match name {
            "method" | "set_method" => {
                request.method = require_argument(argument, name)?;
                target = "method".to_string();
            }
            "url" | "set_url" => {
                request.url = require_argument(argument, name)?;
                target = "url".to_string();
            }
            "header" | "set_header" => {
                let (key, value) = parse_assignment(argument, name)?;
                target = key.clone();
                upsert_pair_case_insensitive(&mut request.headers, key, value);
            }
            "query" | "set_query" => {
                let (key, value) = parse_assignment(argument, name)?;
                target = key.clone();
                upsert_pair(&mut request.query_params, key, value);
            }
            "body" | "set_body" => {
                set_raw_body(&mut request.body, require_argument(argument, name)?);
                target = "body".to_string();
            }
            "var" | "set_var" => {
                let (key, value) = parse_assignment(argument, name)?;
                target = key.clone();
                upsert_variable(&mut variables, active_environment, key, value);
            }
            "global" | "set_global" => {
                let (key, value) = parse_assignment(argument, name)?;
                target = key.clone();
                upsert_global_variable(&mut variables, key, value);
            }
            "env" | "set_env" => {
                let environment = active_environment
                    .ok_or_else(|| anyhow!("{name} requires an active environment"))?;
                let (key, value) = parse_assignment(argument, name)?;
                target = key.clone();
                upsert_environment_variable(&mut variables, environment, key, value);
            }
            _ => return Err(anyhow!("unknown pre-request action: {name}")),
        }
        actions.push(PreRequestActionRecord {
            action: name.to_string(),
            target,
        });
    }

    Ok(PreRequestExecution {
        request,
        variables,
        actions_applied: actions.len(),
        actions,
    })
}

pub fn resolve_codegen_request_templates(
    request: CodegenRequest,
    variables: &VariableStore,
    active_environment: Option<&str>,
) -> Result<CodegenRequest> {
    Ok(CodegenRequest {
        method: replace_variables(&request.method, variables, active_environment)?,
        url: replace_variables(&request.url, variables, active_environment)?
            .trim()
            .to_string(),
        headers: resolve_pairs(request.headers, variables, active_environment)?,
        query_params: resolve_pairs(request.query_params, variables, active_environment)?,
        body: resolve_request_body(request.body, variables, active_environment)?,
    })
}

fn script_actions(script: &str) -> impl Iterator<Item = &str> {
    script.split([';', '\n']).map(str::trim).filter(|action| {
        !action.is_empty() && !action.starts_with('#') && !action.starts_with("//")
    })
}

fn parse_action(action: &str) -> Result<(&str, &str)> {
    let Some((name, argument)) = action.split_once(char::is_whitespace) else {
        return Err(anyhow!("pre-request action needs arguments: {action}"));
    };
    Ok((name.trim(), argument.trim()))
}

fn require_argument(argument: &str, action: &str) -> Result<String> {
    let argument = argument.trim();
    if argument.is_empty() {
        return Err(anyhow!("{action} requires a value"));
    }
    Ok(argument.to_string())
}

fn parse_assignment(argument: &str, action: &str) -> Result<(String, String)> {
    let Some((key, value)) = argument.split_once('=') else {
        return Err(anyhow!("{action} expects name=value"));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(anyhow!("{action} expects a non-empty name"));
    }
    Ok((key.to_string(), value.trim().to_string()))
}

fn upsert_pair(pairs: &mut Vec<(String, String)>, key: String, value: String) {
    if let Some((_, existing_value)) = pairs.iter_mut().find(|(name, _)| name == &key) {
        *existing_value = value;
    } else {
        pairs.push((key, value));
    }
}

fn upsert_pair_case_insensitive(pairs: &mut Vec<(String, String)>, key: String, value: String) {
    if let Some((existing_key, existing_value)) = pairs
        .iter_mut()
        .find(|(name, _)| name.eq_ignore_ascii_case(&key))
    {
        *existing_key = key;
        *existing_value = value;
    } else {
        pairs.push((key, value));
    }
}

fn upsert_variable(
    variables: &mut VariableStore,
    active_environment: Option<&str>,
    key: String,
    value: String,
) {
    if let Some(environment) = active_environment {
        upsert_environment_variable(variables, environment, key, value);
    } else {
        upsert_global_variable(variables, key, value);
    }
}

fn upsert_global_variable(variables: &mut VariableStore, key: String, value: String) {
    variables.upsert(Variable {
        name: key,
        initial_value: String::new(),
        current_value: value,
        scope: VariableScope::Global,
    });
}

fn upsert_environment_variable(
    variables: &mut VariableStore,
    environment: &str,
    key: String,
    value: String,
) {
    variables.upsert(Variable {
        name: key,
        initial_value: String::new(),
        current_value: value,
        scope: VariableScope::Environment {
            name: environment.to_string(),
        },
    });
}

fn set_raw_body(body: &mut RequestBody, value: String) {
    match body {
        RequestBody::Raw { body, .. } => *body = value,
        _ => {
            *body = RequestBody::Raw {
                content_type: Some("text/plain".to_string()),
                body: value,
            };
        }
    }
}

fn resolve_pairs(
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
        .filter(|pair| {
            pair.as_ref()
                .map(|(name, _)| !name.trim().is_empty())
                .unwrap_or(true)
        })
        .collect()
}

fn resolve_request_body(
    body: RequestBody,
    variables: &VariableStore,
    active_environment: Option<&str>,
) -> Result<RequestBody> {
    match body {
        RequestBody::None => Ok(RequestBody::None),
        RequestBody::Raw { content_type, body } => Ok(RequestBody::Raw {
            content_type: content_type
                .map(|content_type| replace_variables(&content_type, variables, active_environment))
                .transpose()?,
            body: replace_variables(&body, variables, active_environment)?,
        }),
        RequestBody::FormUrlEncoded(fields) => Ok(RequestBody::FormUrlEncoded(resolve_pairs(
            fields,
            variables,
            active_environment,
        )?)),
        RequestBody::Multipart(fields) => Ok(RequestBody::Multipart(resolve_pairs(
            fields,
            variables,
            active_environment,
        )?)),
        RequestBody::BinaryFile { path, content_type } => Ok(RequestBody::BinaryFile {
            path: replace_variables(&path, variables, active_environment)?,
            content_type: content_type
                .map(|content_type| replace_variables(&content_type, variables, active_environment))
                .transpose()?,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> CodegenRequest {
        CodegenRequest {
            method: "GET".to_string(),
            url: "{{baseUrl}}/users".to_string(),
            headers: vec![("Accept".to_string(), "application/json".to_string())],
            query_params: Vec::new(),
            body: RequestBody::None,
        }
    }

    #[test]
    fn applies_pre_request_actions_and_resolves_templates() {
        let mut variables = VariableStore::new();
        variables.upsert(Variable::environment(
            "dev",
            "baseUrl",
            "http://localhost:8080",
        ));

        let execution = execute_pre_request_actions(
            "set_var token=abc; set_header Authorization=Bearer {{token}}; set_query debug=true; set_method POST",
            request(),
            variables,
            Some("dev"),
        )
        .expect("execute");
        let resolved =
            resolve_codegen_request_templates(execution.request, &execution.variables, Some("dev"))
                .expect("resolve");

        assert_eq!(execution.actions_applied, 4);
        assert_eq!(
            execution.actions,
            vec![
                PreRequestActionRecord {
                    action: "set_var".to_string(),
                    target: "token".to_string(),
                },
                PreRequestActionRecord {
                    action: "set_header".to_string(),
                    target: "Authorization".to_string(),
                },
                PreRequestActionRecord {
                    action: "set_query".to_string(),
                    target: "debug".to_string(),
                },
                PreRequestActionRecord {
                    action: "set_method".to_string(),
                    target: "method".to_string(),
                },
            ]
        );
        assert_eq!(resolved.method, "POST");
        assert_eq!(resolved.url, "http://localhost:8080/users");
        assert_eq!(
            resolved.headers,
            vec![
                ("Accept".to_string(), "application/json".to_string()),
                ("Authorization".to_string(), "Bearer abc".to_string()),
            ]
        );
        assert_eq!(
            resolved.query_params,
            vec![("debug".to_string(), "true".to_string())]
        );
    }

    #[test]
    fn rejects_invalid_pre_request_actions() {
        let error =
            execute_pre_request_actions("set_header X-Test", request(), VariableStore::new(), None)
                .expect_err("assignment error");

        assert!(error.to_string().contains("name=value"));
    }

    #[test]
    fn set_body_promotes_empty_body_to_raw_text() {
        let execution =
            execute_pre_request_actions("set_body hello", request(), VariableStore::new(), None)
                .expect("execute");

        assert_eq!(
            execution.request.body,
            RequestBody::Raw {
                content_type: Some("text/plain".to_string()),
                body: "hello".to_string(),
            }
        );
    }
}

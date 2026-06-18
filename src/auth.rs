use anyhow::{Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde::Deserialize;
use std::time::Duration;

pub(crate) fn build_auth_entries(
    mode: &str,
    input: &str,
) -> Result<(Vec<(String, String)>, Vec<(String, String)>)> {
    let input = input.trim();
    match mode {
        "none" => Ok((Vec::new(), Vec::new())),
        "oauth2" => {
            let token = oauth2_bearer_token(input)?;
            Ok((
                vec![("Authorization".to_string(), format!("Bearer {token}"))],
                Vec::new(),
            ))
        }
        "bearer" | "jwt" => {
            if input.is_empty() {
                bail!("access token is empty");
            }
            Ok((
                vec![("Authorization".to_string(), format!("Bearer {input}"))],
                Vec::new(),
            ))
        }
        "basic" => {
            let Some((username, password)) = input.split_once(':') else {
                bail!("basic auth must use username:password");
            };
            if username.trim().is_empty() {
                bail!("basic auth username is empty");
            }
            let encoded = BASE64_STANDARD.encode(format!("{}:{}", username.trim(), password));
            Ok((
                vec![("Authorization".to_string(), format!("Basic {encoded}"))],
                Vec::new(),
            ))
        }
        "api-header" => {
            let values = parse_key_value_lines(input, "api key")?;
            if values.is_empty() {
                bail!("api key header is empty");
            }
            Ok((values, Vec::new()))
        }
        "api-query" => {
            let values = parse_key_value_lines(input, "api key")?;
            if values.is_empty() {
                bail!("api key query is empty");
            }
            Ok((Vec::new(), values))
        }
        _ => Ok((Vec::new(), Vec::new())),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct OAuth2TokenConfig {
    token_endpoint: String,
    client_id: String,
    client_secret: String,
    scope: String,
    audience: String,
    grant_type: String,
    refresh_token: String,
    entries: Vec<(String, String)>,
}

#[derive(Debug, Deserialize)]
struct OAuth2TokenResponse {
    access_token: String,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
}

fn oauth2_bearer_token(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() {
        bail!("oauth2 access token is empty");
    }
    if !looks_like_oauth2_config(input) {
        return Ok(input.to_string());
    }

    let entries = parse_key_value_lines(input, "oauth2 config")?;
    oauth2_config_value(&entries, "access_token")
        .or_else(|| oauth2_config_value(&entries, "token"))
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| anyhow!("oauth2 access_token is empty; fetch a token first"))
}

fn looks_like_oauth2_config(input: &str) -> bool {
    let mut non_empty_lines = 0usize;
    for line in input.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        non_empty_lines += 1;
        if let Some((key, _)) = split_key_value_line(line) {
            if is_oauth2_config_key(key.trim()) {
                return true;
            }
        }
    }
    non_empty_lines > 1
}

fn is_oauth2_config_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "token_endpoint"
            | "token_url"
            | "client_id"
            | "client_secret"
            | "scope"
            | "audience"
            | "grant_type"
            | "refresh_token"
            | "access_token"
            | "token"
            | "token_type"
            | "expires_in"
    )
}

fn parse_oauth2_token_config(input: &str) -> Result<OAuth2TokenConfig> {
    let entries = parse_key_value_lines(input, "oauth2 config")?;
    let token_endpoint = oauth2_config_value(&entries, "token_endpoint")
        .or_else(|| oauth2_config_value(&entries, "token_url"))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("oauth2 token_endpoint is required"))?;
    let client_id = oauth2_config_value(&entries, "client_id")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("oauth2 client_id is required"))?;
    let refresh_token = oauth2_config_value(&entries, "refresh_token").unwrap_or_default();
    let grant_type = oauth2_config_value(&entries, "grant_type")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if refresh_token.trim().is_empty() {
                "client_credentials".to_string()
            } else {
                "refresh_token".to_string()
            }
        });

    if grant_type == "refresh_token" && refresh_token.trim().is_empty() {
        bail!("oauth2 refresh_token is required for refresh_token grant");
    }

    Ok(OAuth2TokenConfig {
        token_endpoint,
        client_id,
        client_secret: oauth2_config_value(&entries, "client_secret").unwrap_or_default(),
        scope: oauth2_config_value(&entries, "scope").unwrap_or_default(),
        audience: oauth2_config_value(&entries, "audience").unwrap_or_default(),
        grant_type,
        refresh_token,
        entries,
    })
}

fn oauth2_config_value(entries: &[(String, String)], key: &str) -> Option<String> {
    entries
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.trim().to_string())
}

pub(crate) async fn fetch_oauth2_token_config(input: &str) -> Result<String> {
    let config = parse_oauth2_token_config(input)?;
    let mut form = vec![
        ("grant_type".to_string(), config.grant_type.clone()),
        ("client_id".to_string(), config.client_id.clone()),
    ];
    if !config.client_secret.trim().is_empty() {
        form.push(("client_secret".to_string(), config.client_secret.clone()));
    }
    if !config.scope.trim().is_empty() {
        form.push(("scope".to_string(), config.scope.clone()));
    }
    if !config.audience.trim().is_empty() {
        form.push(("audience".to_string(), config.audience.clone()));
    }
    if config.grant_type == "refresh_token" {
        form.push(("refresh_token".to_string(), config.refresh_token.clone()));
    }

    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| anyhow!("oauth2 token client build failed: {error}"))?
        .post(&config.token_endpoint)
        .header("Accept", "application/json")
        .form(&form)
        .send()
        .await
        .map_err(|error| anyhow!("oauth2 token request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| anyhow!("oauth2 token response read failed: {error}"))?;
    if !status.is_success() {
        bail!("oauth2 token endpoint returned {status}: {body}");
    }

    let token = serde_json::from_str::<OAuth2TokenResponse>(&body)
        .map_err(|error| anyhow!("oauth2 token response is not valid JSON: {error}"))?;
    if token.access_token.trim().is_empty() {
        bail!("oauth2 token response access_token is empty");
    }

    Ok(format_oauth2_token_config(&config.entries, &token))
}

fn format_oauth2_token_config(entries: &[(String, String)], token: &OAuth2TokenResponse) -> String {
    let mut updated = entries.to_vec();
    upsert_pair(
        &mut updated,
        "access_token".to_string(),
        token.access_token.clone(),
        true,
    );
    if let Some(token_type) = token
        .token_type
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        upsert_pair(
            &mut updated,
            "token_type".to_string(),
            token_type.to_string(),
            true,
        );
    }
    if let Some(expires_in) = token.expires_in {
        upsert_pair(
            &mut updated,
            "expires_in".to_string(),
            expires_in.to_string(),
            true,
        );
    }
    if let Some(refresh_token) = token
        .refresh_token
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        upsert_pair(
            &mut updated,
            "refresh_token".to_string(),
            refresh_token.to_string(),
            true,
        );
    }
    format_key_value_preview(&updated)
}

pub(crate) fn split_basic_auth_config(input: &str) -> (String, String) {
    let input = input.trim();
    if input.is_empty() {
        return (String::new(), String::new());
    }

    input
        .split_once(':')
        .map(|(username, password)| (username.to_string(), password.to_string()))
        .unwrap_or_else(|| (input.to_string(), String::new()))
}

pub(crate) fn format_basic_auth_config(username: &str, password: &str) -> String {
    if username.is_empty() && password.is_empty() {
        String::new()
    } else {
        format!("{username}:{password}")
    }
}

fn parse_key_value_lines(input: &str, field_name: &str) -> Result<Vec<(String, String)>> {
    input
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                None
            } else {
                Some((index + 1, line))
            }
        })
        .map(|(line_number, line)| {
            let Some((key, value)) = split_key_value_line(line) else {
                bail!("{field_name} line {line_number} must use key=value or key: value");
            };
            let key = key.trim();
            if key.is_empty() {
                bail!("{field_name} line {line_number} has an empty name");
            }
            Ok((key.to_string(), value.trim().to_string()))
        })
        .collect()
}

fn split_key_value_line(line: &str) -> Option<(&str, &str)> {
    let separator = match (line.find('='), line.find(':')) {
        (Some(eq), Some(colon)) => eq.min(colon),
        (Some(eq), None) => eq,
        (None, Some(colon)) => colon,
        (None, None) => return None,
    };

    Some((&line[..separator], &line[separator + 1..]))
}

fn format_key_value_preview(fields: &[(String, String)]) -> String {
    fields
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn upsert_pair(
    pairs: &mut Vec<(String, String)>,
    name: String,
    value: String,
    case_insensitive: bool,
) {
    if let Some((_, existing_value)) = pairs.iter_mut().find(|(existing_name, _)| {
        if case_insensitive {
            existing_name.eq_ignore_ascii_case(&name)
        } else {
            existing_name == &name
        }
    }) {
        *existing_value = value;
    } else {
        pairs.push((name, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_auth_headers_and_query_params() {
        assert_eq!(
            build_auth_entries("bearer", "secret").unwrap(),
            (
                vec![("Authorization".to_string(), "Bearer secret".to_string())],
                Vec::new()
            )
        );
        assert_eq!(
            build_auth_entries("oauth2", "access-token").unwrap(),
            (
                vec![(
                    "Authorization".to_string(),
                    "Bearer access-token".to_string()
                )],
                Vec::new()
            )
        );
        assert_eq!(
            build_auth_entries("oauth2", "access_token=config-token\nclient_id=app").unwrap(),
            (
                vec![(
                    "Authorization".to_string(),
                    "Bearer config-token".to_string()
                )],
                Vec::new()
            )
        );
        assert_eq!(
            build_auth_entries("basic", "user:pass").unwrap(),
            (
                vec![(
                    "Authorization".to_string(),
                    "Basic dXNlcjpwYXNz".to_string()
                )],
                Vec::new()
            )
        );
        assert_eq!(
            build_auth_entries("api-query", "api_key=secret").unwrap(),
            (
                Vec::new(),
                vec![("api_key".to_string(), "secret".to_string())]
            )
        );
    }

    #[test]
    fn parses_oauth2_token_config() {
        let config = parse_oauth2_token_config(
            "token_endpoint=https://auth.example/token\nclient_id=client\nclient_secret=secret\nscope=read",
        )
        .expect("oauth2 config");

        assert_eq!(config.token_endpoint, "https://auth.example/token");
        assert_eq!(config.client_id, "client");
        assert_eq!(config.client_secret, "secret");
        assert_eq!(config.scope, "read");
        assert_eq!(config.grant_type, "client_credentials");
        assert_eq!(config.refresh_token, "");
    }

    #[test]
    fn formats_oauth2_token_config_response_fields() {
        let entries = vec![
            (
                "token_endpoint".to_string(),
                "https://auth.example/token".to_string(),
            ),
            ("client_id".to_string(), "client".to_string()),
            ("scope".to_string(), "read".to_string()),
            ("access_token".to_string(), "old-token".to_string()),
        ];
        let token = OAuth2TokenResponse {
            access_token: "new-token".to_string(),
            token_type: Some("Bearer".to_string()),
            expires_in: Some(3600),
            refresh_token: Some("next-refresh".to_string()),
        };

        assert_eq!(
            format_oauth2_token_config(&entries, &token),
            "token_endpoint=https://auth.example/token\nclient_id=client\nscope=read\naccess_token=new-token\ntoken_type=Bearer\nexpires_in=3600\nrefresh_token=next-refresh"
        );
    }

    #[test]
    fn fetches_oauth2_token_config_from_local_endpoint() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async {
            use axum::{Json, Router, extract::Form, routing::post};
            use std::{
                collections::BTreeMap,
                sync::{Arc, Mutex},
            };
            use tokio::net::TcpListener;

            let received = Arc::new(Mutex::new(None::<BTreeMap<String, String>>));
            let endpoint_received = received.clone();
            let app = Router::new().route(
                "/token",
                post(move |Form(form): Form<BTreeMap<String, String>>| {
                    let endpoint_received = endpoint_received.clone();
                    async move {
                        *endpoint_received.lock().expect("received form") = Some(form);
                        Json(json!({
                            "access_token": "server-token",
                            "token_type": "Bearer",
                            "expires_in": 7200,
                            "refresh_token": "server-refresh"
                        }))
                    }
                }),
            );
            let listener = TcpListener::bind(("127.0.0.1", 0))
                .await
                .expect("listener");
            let address = listener.local_addr().expect("address");
            let server = tokio::spawn(async move {
                axum::serve(listener, app).await.expect("token server");
            });

            let updated = fetch_oauth2_token_config(&format!(
                "token_endpoint=http://{address}/token\nclient_id=client\nclient_secret=secret\nscope=read write\naudience=api"
            ))
            .await
            .expect("token fetch");
            server.abort();

            assert_eq!(
                updated,
                "token_endpoint=http://".to_string()
                    + &address.to_string()
                    + "/token\nclient_id=client\nclient_secret=secret\nscope=read write\naudience=api\naccess_token=server-token\ntoken_type=Bearer\nexpires_in=7200\nrefresh_token=server-refresh"
            );
            assert_eq!(
                received.lock().expect("received form").as_ref().expect("form"),
                &BTreeMap::from([
                    ("audience".to_string(), "api".to_string()),
                    ("client_id".to_string(), "client".to_string()),
                    ("client_secret".to_string(), "secret".to_string()),
                    ("grant_type".to_string(), "client_credentials".to_string()),
                    ("scope".to_string(), "read write".to_string()),
                ])
            );
        });
    }

    #[test]
    fn splits_and_formats_basic_auth_config_for_ui_fields() {
        assert_eq!(
            split_basic_auth_config("user:pass"),
            ("user".to_string(), "pass".to_string())
        );
        assert_eq!(
            split_basic_auth_config("user:p:a:s:s"),
            ("user".to_string(), "p:a:s:s".to_string())
        );
        assert_eq!(
            split_basic_auth_config("legacy-user"),
            ("legacy-user".to_string(), String::new())
        );

        assert_eq!(format_basic_auth_config("user", "pass"), "user:pass");
        assert_eq!(format_basic_auth_config("", ""), "");
    }
}

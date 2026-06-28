use anyhow::{Result, anyhow, bail};
use serde_json::Value;

pub(in crate::app) fn summarize_graphql_schema_response(text: &str) -> Result<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("response is empty");
    }

    let value: Value = serde_json::from_str(trimmed)
        .map_err(|error| anyhow!("response is not valid JSON: {error}"))?;
    let schema = graphql_schema_value(&value).ok_or_else(|| {
        let details = graphql_error_messages(&value)
            .map(|message| format!("; GraphQL errors: {message}"))
            .unwrap_or_default();
        anyhow!("GraphQL introspection schema not found{details}")
    })?;
    let types = schema
        .get("types")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("GraphQL introspection schema has no types array"))?;

    let visible_types: Vec<&Value> = types.iter().filter(|ty| graphql_type_visible(ty)).collect();
    let mut lines = Vec::new();
    lines.push("GraphQL Schema".to_string());
    lines.push(format!(
        "Query: {}",
        graphql_named_type(schema.get("queryType")).unwrap_or_else(|| "none".to_string())
    ));
    lines.push(format!(
        "Mutation: {}",
        graphql_named_type(schema.get("mutationType")).unwrap_or_else(|| "none".to_string())
    ));
    lines.push(format!(
        "Subscription: {}",
        graphql_named_type(schema.get("subscriptionType")).unwrap_or_else(|| "none".to_string())
    ));
    lines.push(format!(
        "Types: {} shown / {} total",
        visible_types.len(),
        types.len()
    ));

    for ty in visible_types {
        render_graphql_schema_type(ty, &mut lines);
    }

    Ok(lines.join("\n"))
}

fn graphql_schema_value(value: &Value) -> Option<&Value> {
    value.pointer("/data/__schema").or_else(|| {
        value.get("__schema").or_else(|| {
            if value.get("types").is_some() && value.get("queryType").is_some() {
                Some(value)
            } else {
                None
            }
        })
    })
}

fn graphql_error_messages(value: &Value) -> Option<String> {
    let messages: Vec<String> = value
        .get("errors")?
        .as_array()?
        .iter()
        .filter_map(|error| error.get("message").and_then(Value::as_str))
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .take(3)
        .map(ToOwned::to_owned)
        .collect();
    if messages.is_empty() {
        None
    } else {
        Some(messages.join("; "))
    }
}

fn graphql_named_type(value: Option<&Value>) -> Option<String> {
    value?
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn graphql_type_visible(value: &Value) -> bool {
    graphql_type_name(value)
        .map(|name| !name.starts_with("__"))
        .unwrap_or(false)
}

fn graphql_type_name(value: &Value) -> Option<&str> {
    value
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

fn render_graphql_schema_type(value: &Value, lines: &mut Vec<String>) {
    let Some(name) = graphql_type_name(value) else {
        return;
    };
    let kind = value.get("kind").and_then(Value::as_str).unwrap_or("TYPE");
    lines.push(String::new());
    lines.push(format!("{} {name}", graphql_type_heading(kind)));

    match kind {
        "OBJECT" | "INTERFACE" => render_graphql_fields(value.get("fields"), lines),
        "INPUT_OBJECT" => render_graphql_input_fields(value.get("inputFields"), lines),
        "ENUM" => render_graphql_enum_values(value.get("enumValues"), lines),
        "UNION" => render_graphql_union_members(value.get("possibleTypes"), lines),
        _ => {}
    }
}

fn graphql_type_heading(kind: &str) -> &'static str {
    match kind {
        "OBJECT" => "type",
        "INTERFACE" => "interface",
        "INPUT_OBJECT" => "input",
        "ENUM" => "enum",
        "UNION" => "union",
        "SCALAR" => "scalar",
        _ => "type",
    }
}

fn render_graphql_fields(value: Option<&Value>, lines: &mut Vec<String>) {
    let Some(fields) = value.and_then(Value::as_array) else {
        lines.push("  (fields not included)".to_string());
        return;
    };
    if fields.is_empty() {
        lines.push("  (no fields)".to_string());
        return;
    }

    for field in fields {
        let Some(name) = field.get("name").and_then(Value::as_str) else {
            continue;
        };
        let args = field
            .get("args")
            .and_then(Value::as_array)
            .map(|args| {
                args.iter()
                    .filter_map(render_graphql_arg)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let field_type = graphql_type_ref(field.get("type"));
        let signature = if args.is_empty() {
            format!("{name}: {field_type}")
        } else {
            format!("{name}({args}): {field_type}")
        };
        lines.push(format!(
            "  {signature}{}",
            graphql_deprecation_suffix(field)
        ));
    }
}

fn render_graphql_arg(value: &Value) -> Option<String> {
    let name = value.get("name")?.as_str()?;
    Some(format!("{name}: {}", graphql_type_ref(value.get("type"))))
}

fn render_graphql_input_fields(value: Option<&Value>, lines: &mut Vec<String>) {
    let Some(fields) = value.and_then(Value::as_array) else {
        lines.push("  (input fields not included)".to_string());
        return;
    };
    if fields.is_empty() {
        lines.push("  (no input fields)".to_string());
        return;
    }

    for field in fields {
        let Some(name) = field.get("name").and_then(Value::as_str) else {
            continue;
        };
        lines.push(format!(
            "  {name}: {}{}",
            graphql_type_ref(field.get("type")),
            graphql_deprecation_suffix(field)
        ));
    }
}

fn render_graphql_enum_values(value: Option<&Value>, lines: &mut Vec<String>) {
    let Some(values) = value.and_then(Value::as_array) else {
        lines.push("  (enum values not included)".to_string());
        return;
    };
    if values.is_empty() {
        lines.push("  (no enum values)".to_string());
        return;
    }

    for enum_value in values {
        let Some(name) = enum_value.get("name").and_then(Value::as_str) else {
            continue;
        };
        lines.push(format!(
            "  {name}{}",
            graphql_deprecation_suffix(enum_value)
        ));
    }
}

fn render_graphql_union_members(value: Option<&Value>, lines: &mut Vec<String>) {
    let Some(members) = value.and_then(Value::as_array) else {
        lines.push("  (possible types not included)".to_string());
        return;
    };
    let names: Vec<&str> = members.iter().filter_map(graphql_type_name).collect();
    if names.is_empty() {
        lines.push("  (no possible types)".to_string());
    } else {
        lines.push(format!("  = {}", names.join(" | ")));
    }
}

fn graphql_deprecation_suffix(value: &Value) -> String {
    if !value
        .get("isDeprecated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return String::new();
    }

    match value
        .get("deprecationReason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
    {
        Some(reason) => format!(" @deprecated(reason: {})", json_string(reason)),
        None => " @deprecated".to_string(),
    }
}

fn graphql_type_ref(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return "Unknown".to_string();
    };
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match kind {
        "NON_NULL" => format!("{}!", graphql_type_ref(value.get("ofType"))),
        "LIST" => format!("[{}]", graphql_type_ref(value.get("ofType"))),
        _ => graphql_named_type(Some(value))
            .or_else(|| {
                value
                    .get("ofType")
                    .map(|nested| graphql_type_ref(Some(nested)))
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| {
                if kind.is_empty() {
                    "Unknown".to_string()
                } else {
                    kind.to_string()
                }
            }),
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

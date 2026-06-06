use serde_json::{Map, Value, json};

pub(crate) fn operation_mock_body(operation: &Map<String, Value>) -> Value {
    if let Some(example) = first_response_example(operation) {
        return example;
    }

    if let Some(schema) = first_json_response_schema(operation) {
        return mock_from_schema(schema);
    }

    json!({
        "ok": true,
        "source": "ZenAPI mock",
    })
}

fn first_response_example(operation: &Map<String, Value>) -> Option<Value> {
    let responses = operation.get("responses")?.as_object()?;
    let response = responses
        .get("200")
        .or_else(|| responses.get("201"))
        .or_else(|| responses.get("default"))
        .or_else(|| responses.values().next())?;
    let content = response.get("content")?.as_object()?;
    let media = content
        .get("application/json")
        .or_else(|| content.values().next())?;

    if let Some(example) = media.get("example") {
        return Some(example.clone());
    }

    let examples = media.get("examples")?.as_object()?;
    let first = examples.values().next()?;
    first.get("value").cloned().or_else(|| Some(first.clone()))
}

fn first_json_response_schema(operation: &Map<String, Value>) -> Option<&Value> {
    let responses = operation.get("responses")?.as_object()?;
    let response = responses
        .get("200")
        .or_else(|| responses.get("201"))
        .or_else(|| responses.get("default"))
        .or_else(|| responses.values().next())?;

    if let Some(schema) = response.get("schema") {
        return Some(schema);
    }

    let content = response.get("content")?.as_object()?;
    let media = content
        .get("application/json")
        .or_else(|| content.values().next())?;
    media.get("schema")
}

pub(crate) fn mock_from_schema(schema: &Value) -> Value {
    if let Some(example) = schema.get("example") {
        return example.clone();
    }

    if let Some(default) = schema.get("default") {
        return default.clone();
    }

    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
        if let Some(first) = enum_values.first() {
            return first.clone();
        }
    }

    if let Some(one_of) = schema.get("oneOf").and_then(Value::as_array) {
        if let Some(first) = one_of.first() {
            return mock_from_schema(first);
        }
    }

    if let Some(any_of) = schema.get("anyOf").and_then(Value::as_array) {
        if let Some(first) = any_of.first() {
            return mock_from_schema(first);
        }
    }

    let schema_type = schema.get("type").and_then(Value::as_str);

    match schema_type {
        Some("object") | None if schema.get("properties").is_some() => {
            let mut body = Map::new();
            if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                for (name, property_schema) in properties {
                    body.insert(name.clone(), mock_value_for_property(name, property_schema));
                }
            }
            Value::Object(body)
        }
        Some("array") => {
            let item = schema
                .get("items")
                .map(mock_from_schema)
                .unwrap_or_else(|| json!({}));
            Value::Array(vec![item])
        }
        Some("integer") => json!(1),
        Some("number") => json!(1.0),
        Some("boolean") => json!(true),
        Some("string") => {
            let format = schema.get("format").and_then(Value::as_str).unwrap_or("");
            json!(sample_string_for("", format))
        }
        _ => json!({ "ok": true }),
    }
}

fn mock_value_for_property(name: &str, schema: &Value) -> Value {
    if let Some(example) = schema.get("example") {
        return example.clone();
    }

    if let Some(default) = schema.get("default") {
        return default.clone();
    }

    let schema_type = schema.get("type").and_then(Value::as_str);
    let format = schema.get("format").and_then(Value::as_str).unwrap_or("");

    match schema_type {
        Some("string") | None => json!(sample_string_for(name, format)),
        Some("integer") => json!(1),
        Some("number") => json!(1.0),
        Some("boolean") => json!(true),
        Some("array") => {
            let item = schema
                .get("items")
                .map(mock_from_schema)
                .unwrap_or_else(|| json!("item"));
            Value::Array(vec![item])
        }
        Some("object") => mock_from_schema(schema),
        _ => mock_from_schema(schema),
    }
}

fn sample_string_for(name: &str, format: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();

    if format == "email" || lower.contains("email") {
        "dev@zenapi.local"
    } else if format == "date-time" || lower.contains("time") || lower.ends_with("_at") {
        "2026-06-02T00:00:00Z"
    } else if format == "date" {
        "2026-06-02"
    } else if format == "uri" || format == "url" || lower.contains("url") {
        "https://zenapi.local"
    } else if lower.contains("phone") {
        "+1-555-0100"
    } else if lower.contains("name") {
        "Zen API"
    } else if lower == "id" || lower.ends_with("_id") || lower.ends_with("id") {
        "id_001"
    } else {
        "string"
    }
}

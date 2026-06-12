use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, path::Path};

const POSTMAN_SCHEMA_V21: &str =
    "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiCollection {
    pub name: String,
    pub description: String,
    pub items: Vec<CollectionItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CollectionItem {
    Folder(CollectionFolder),
    Request(CollectionRequest),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionFolder {
    pub name: String,
    pub description: String,
    pub items: Vec<CollectionItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionRequest {
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<NameValue>,
    pub query_params: Vec<NameValue>,
    pub body: CollectionBody,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameValue {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CollectionBody {
    None,
    Raw { content_type: String, body: String },
    FormData { fields: Vec<NameValue> },
    UrlEncoded { fields: Vec<NameValue> },
    Binary { path: String, content_type: String },
}

impl ApiCollection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            items: Vec::new(),
        }
    }

    pub fn from_json(input: &str) -> Result<Self> {
        serde_json::from_str(input).context("failed to parse ZenAPI collection JSON")
    }

    pub fn load_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read collection {}", path.display()))?;
        Self::from_json(&content)
            .or_else(|native_error| {
                Self::from_postman_json(&content)
                    .with_context(|| format!("failed native parse: {native_error}"))
            })
            .with_context(|| format!("failed to parse collection {}", path.display()))
    }

    pub fn save_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let content = self.to_json_pretty()?;
        fs::write(path, content)
            .with_context(|| format!("failed to write collection {}", path.display()))
    }

    pub fn save_postman_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let content = self.to_postman_json_pretty()?;
        fs::write(path, content)
            .with_context(|| format!("failed to write Postman collection {}", path.display()))
    }

    pub fn to_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("failed to serialize ZenAPI collection JSON")
    }

    pub fn from_postman_json(input: &str) -> Result<Self> {
        let value = serde_json::from_str::<Value>(input).context("failed to parse Postman JSON")?;
        let info = value
            .get("info")
            .and_then(Value::as_object)
            .context("Postman collection is missing info")?;
        let name = info
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Imported Collection")
            .to_string();
        let description = stringish(info.get("description")).unwrap_or_default();
        let items = value
            .get("item")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(postman_item).collect::<Result<Vec<_>>>())
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            name,
            description,
            items,
        })
    }

    pub fn to_postman_json_pretty(&self) -> Result<String> {
        let value = json!({
            "info": {
                "name": self.name,
                "description": self.description,
                "schema": POSTMAN_SCHEMA_V21,
            },
            "item": self.items.iter().map(postman_item_json).collect::<Vec<_>>(),
        });

        serde_json::to_string_pretty(&value).context("failed to serialize Postman collection JSON")
    }
}

fn postman_item(value: &Value) -> Result<CollectionItem> {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Untitled")
        .to_string();
    let description = stringish(value.get("description")).unwrap_or_default();

    if let Some(items) = value.get("item").and_then(Value::as_array) {
        return Ok(CollectionItem::Folder(CollectionFolder {
            name,
            description,
            items: items.iter().map(postman_item).collect::<Result<Vec<_>>>()?,
        }));
    }

    let request = value
        .get("request")
        .context("Postman item is missing request")?;
    Ok(CollectionItem::Request(CollectionRequest {
        name,
        method: request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("GET")
            .to_string(),
        url: postman_url(request.get("url")),
        headers: postman_headers(request.get("header")),
        query_params: postman_query_params(request.get("url")),
        body: postman_body(request.get("body")),
    }))
}

fn postman_url(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(url)) => url.clone(),
        Some(Value::Object(url)) => url
            .get("raw")
            .and_then(Value::as_str)
            .or_else(|| {
                url.get("host")
                    .and_then(Value::as_array)
                    .and_then(|values| first_string(values))
            })
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

fn postman_headers(value: Option<&Value>) -> Vec<NameValue> {
    value
        .and_then(Value::as_array)
        .map(|headers| {
            headers
                .iter()
                .filter_map(|header| {
                    Some(NameValue {
                        name: header.get("key")?.as_str()?.to_string(),
                        value: header
                            .get("value")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn postman_query_params(value: Option<&Value>) -> Vec<NameValue> {
    value
        .and_then(Value::as_object)
        .and_then(|url| url.get("query"))
        .and_then(Value::as_array)
        .map(|params| {
            params
                .iter()
                .filter_map(|param| {
                    Some(NameValue {
                        name: param.get("key")?.as_str()?.to_string(),
                        value: param
                            .get("value")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn postman_body(value: Option<&Value>) -> CollectionBody {
    let Some(body) = value.and_then(Value::as_object) else {
        return CollectionBody::None;
    };

    match body.get("mode").and_then(Value::as_str).unwrap_or("none") {
        "raw" => CollectionBody::Raw {
            content_type: "application/json".to_string(),
            body: body
                .get("raw")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        },
        "urlencoded" => CollectionBody::UrlEncoded {
            fields: postman_key_value_array(body.get("urlencoded")),
        },
        "formdata" => CollectionBody::FormData {
            fields: postman_key_value_array(body.get("formdata")),
        },
        "file" => CollectionBody::Binary {
            path: body
                .get("file")
                .and_then(|file| file.get("src"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            content_type: "application/octet-stream".to_string(),
        },
        _ => CollectionBody::None,
    }
}

fn postman_key_value_array(value: Option<&Value>) -> Vec<NameValue> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(NameValue {
                        name: item.get("key")?.as_str()?.to_string(),
                        value: item
                            .get("value")
                            .or_else(|| item.get("src"))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn postman_item_json(item: &CollectionItem) -> Value {
    match item {
        CollectionItem::Folder(folder) => json!({
            "name": folder.name,
            "description": folder.description,
            "item": folder.items.iter().map(postman_item_json).collect::<Vec<_>>(),
        }),
        CollectionItem::Request(request) => json!({
            "name": request.name,
            "request": {
                "method": request.method,
                "url": {
                    "raw": request.url,
                    "query": request.query_params.iter().map(postman_name_value_json).collect::<Vec<_>>(),
                },
                "header": request.headers.iter().map(postman_name_value_json).collect::<Vec<_>>(),
                "body": postman_body_json(&request.body),
            },
        }),
    }
}

fn postman_name_value_json(pair: &NameValue) -> Value {
    json!({
        "key": pair.name,
        "value": pair.value,
    })
}

fn postman_body_json(body: &CollectionBody) -> Value {
    match body {
        CollectionBody::None => json!({ "mode": "none" }),
        CollectionBody::Raw { body, .. } => json!({
            "mode": "raw",
            "raw": body,
        }),
        CollectionBody::FormData { fields } => json!({
            "mode": "formdata",
            "formdata": fields.iter().map(postman_name_value_json).collect::<Vec<_>>(),
        }),
        CollectionBody::UrlEncoded { fields } => json!({
            "mode": "urlencoded",
            "urlencoded": fields.iter().map(postman_name_value_json).collect::<Vec<_>>(),
        }),
        CollectionBody::Binary { path, .. } => json!({
            "mode": "file",
            "file": { "src": path },
        }),
    }
}

fn stringish(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) => Some(value.clone()),
        Value::Object(object) => object
            .get("content")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        _ => None,
    }
}

fn first_string(values: &[Value]) -> Option<&str> {
    values.iter().find_map(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> CollectionRequest {
        CollectionRequest {
            name: "List users".to_string(),
            method: "GET".to_string(),
            url: "https://api.example.com/users".to_string(),
            headers: vec![NameValue {
                name: "Accept".to_string(),
                value: "application/json".to_string(),
            }],
            query_params: vec![NameValue {
                name: "limit".to_string(),
                value: "20".to_string(),
            }],
            body: CollectionBody::None,
        }
    }

    #[test]
    fn serializes_and_deserializes_native_collection_json() {
        let collection = ApiCollection {
            name: "Demo".to_string(),
            description: "Local collection".to_string(),
            items: vec![CollectionItem::Folder(CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: vec![CollectionItem::Request(sample_request())],
            })],
        };

        let json = collection.to_json_pretty().expect("serialize");
        let parsed = ApiCollection::from_json(&json).expect("parse");

        assert_eq!(parsed, collection);
    }

    #[test]
    fn imports_postman_v21_collection() {
        let input = r#"
{
  "info": {
    "name": "Postman Demo",
    "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
  },
  "item": [
    {
      "name": "Users",
      "item": [
        {
          "name": "Create user",
          "request": {
            "method": "POST",
            "url": {
              "raw": "https://api.example.com/users?debug=true",
              "query": [
                { "key": "debug", "value": "true" }
              ]
            },
            "header": [
              { "key": "Content-Type", "value": "application/json" }
            ],
            "body": {
              "mode": "raw",
              "raw": "{\"name\":\"Zen\"}"
            }
          }
        }
      ]
    }
  ]
}
"#;

        let collection = ApiCollection::from_postman_json(input).expect("import");

        assert_eq!(collection.name, "Postman Demo");
        let CollectionItem::Folder(folder) = &collection.items[0] else {
            panic!("expected folder");
        };
        let CollectionItem::Request(request) = &folder.items[0] else {
            panic!("expected request");
        };
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "https://api.example.com/users?debug=true");
        assert_eq!(request.query_params[0].name, "debug");
        assert!(matches!(request.body, CollectionBody::Raw { .. }));
    }

    #[test]
    fn exports_postman_v21_collection() {
        let collection = ApiCollection {
            name: "Demo".to_string(),
            description: String::new(),
            items: vec![CollectionItem::Request(sample_request())],
        };

        let exported = collection.to_postman_json_pretty().expect("export");
        let value = serde_json::from_str::<Value>(&exported).expect("json");

        assert_eq!(value["info"]["schema"], POSTMAN_SCHEMA_V21);
        assert_eq!(value["item"][0]["name"], "List users");
        assert_eq!(value["item"][0]["request"]["method"], "GET");
        assert_eq!(value["item"][0]["request"]["header"][0]["key"], "Accept");
    }
}

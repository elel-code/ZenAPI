mod json;
mod model;
mod parser;
mod schema;
mod yaml;

pub use model::{ApiRoute, ApiSpec};
pub use parser::{load_openapi_file, parse_openapi};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openapi_json_routes_and_schema_mock() {
        let spec = parse_openapi(
            r#"
            {
              "openapi": "3.0.0",
              "info": { "title": "Demo", "version": "1.0.0" },
              "paths": {
                "/users": {
                  "get": {
                    "summary": "List users",
                    "responses": {
                      "200": {
                        "content": {
                          "application/json": {
                            "schema": {
                              "type": "array",
                              "items": {
                                "type": "object",
                                "properties": {
                                  "id": { "type": "string" },
                                  "email": { "type": "string", "format": "email" }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
            "#,
        )
        .expect("valid OpenAPI JSON");

        assert_eq!(spec.title, "Demo");
        assert_eq!(spec.routes.len(), 1);
        assert_eq!(spec.routes[0].method, "GET");
        assert_eq!(spec.routes[0].path, "/users");
        assert_eq!(spec.routes[0].mock_body[0]["email"], "dev@zenapi.local");
    }

    #[test]
    fn parses_common_yaml_paths() {
        let spec = parse_openapi(
            r#"
openapi: 3.0.0
info:
  title: Demo YAML
  version: 1.0.0
paths:
  /users:
    get:
      summary: List users
    post:
      summary: Create user
"#,
        )
        .expect("valid minimal YAML");

        assert_eq!(spec.title, "Demo YAML");
        assert_eq!(spec.routes.len(), 2);
        assert!(spec.routes.iter().any(|route| route.method == "GET"));
        assert!(
            spec.routes
                .iter()
                .any(|route| route.summary == "Create user")
        );
    }

    #[test]
    fn loads_openapi_file_from_disk() {
        let dir = temp_dir::TempDir::new().expect("temp dir");
        let path = dir.path().join("demo.yaml");
        std::fs::write(
            &path,
            r#"
openapi: 3.0.0
info:
  title: Disk Demo
  version: 1.0.0
paths:
  /health:
    get:
      summary: Health check
"#,
        )
        .expect("write fixture");

        let spec = load_openapi_file(&path).expect("load file");

        assert_eq!(spec.title, "Disk Demo");
        assert_eq!(spec.routes.len(), 1);
        assert_eq!(spec.routes[0].method, "GET");
        assert_eq!(spec.routes[0].path, "/health");
    }

    #[test]
    fn generates_schema_aware_mock_values() {
        let spec = parse_openapi(
            r#"
{
  "openapi": "3.0.0",
  "info": { "title": "Demo", "version": "1.0.0" },
  "paths": {
    "/profile": {
      "get": {
        "responses": {
          "200": {
            "content": {
              "application/json": {
                "schema": {
                  "type": "object",
                  "properties": {
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "email": { "type": "string" },
                    "callback_url": { "type": "string" },
                    "created_at": { "type": "string" }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
"#,
        )
        .expect("valid OpenAPI JSON");

        let body = &spec.routes[0].mock_body;
        assert_eq!(body["id"], "id_001");
        assert_eq!(body["name"], "Zen API");
        assert_eq!(body["email"], "dev@zenapi.local");
        assert_eq!(body["callback_url"], "https://zenapi.local");
        assert_eq!(body["created_at"], "2026-06-02T00:00:00Z");
    }

    #[test]
    fn response_examples_take_priority_over_schema_mocks() {
        let spec = parse_openapi(
            r#"
{
  "openapi": "3.0.0",
  "info": { "title": "Demo", "version": "1.0.0" },
  "paths": {
    "/profile": {
      "get": {
        "responses": {
          "200": {
            "content": {
              "application/json": {
                "examples": {
                  "success": {
                    "value": {
                      "source": "example",
                      "name": "Explicit Example"
                    }
                  }
                },
                "schema": {
                  "type": "object",
                  "properties": {
                    "name": { "type": "string" }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
"#,
        )
        .expect("valid OpenAPI JSON");

        assert_eq!(spec.routes[0].mock_body["source"], "example");
        assert_eq!(spec.routes[0].mock_body["name"], "Explicit Example");
    }
}

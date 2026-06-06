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
}

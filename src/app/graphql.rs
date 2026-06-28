use anyhow::{Result, anyhow, bail};
use serde_json::Value;
use slint::ComponentHandle;
use zenapi::client::RequestBody;

use crate::ui::AppWindow;

use super::{request_editor_ui::refresh_body_field_rows, set_response};

mod schema;

pub(super) use self::schema::summarize_graphql_schema_response;

pub(super) struct GraphqlTemplate {
    pub name: &'static str,
    pub query: &'static str,
    pub variables: &'static str,
}

const GRAPHQL_INTROSPECTION_QUERY: &str = r#"query IntrospectionQuery {
  __schema {
    queryType {
      name
    }
    mutationType {
      name
    }
    subscriptionType {
      name
    }
    types {
      kind
      name
      fields(includeDeprecated: true) {
        name
        args {
          name
          type {
            ...TypeRef
          }
        }
        type {
          ...TypeRef
        }
        isDeprecated
        deprecationReason
      }
    }
  }
}

fragment TypeRef on __Type {
  kind
  name
  ofType {
    kind
    name
    ofType {
      kind
      name
      ofType {
        kind
        name
        ofType {
          kind
          name
        }
      }
    }
  }
}"#;

const GRAPHQL_QUERY_TEMPLATE: &str = r#"query Example($id: ID!) {
  node(id: $id) {
    id
    __typename
  }
}"#;

const GRAPHQL_MUTATION_TEMPLATE: &str = r#"mutation Example($input: ExampleInput!) {
  example(input: $input) {
    id
  }
}"#;

pub(super) fn graphql_template(template: &str) -> Option<GraphqlTemplate> {
    match template {
        "introspection" | "intro" => Some(GraphqlTemplate {
            name: "Introspection",
            query: GRAPHQL_INTROSPECTION_QUERY,
            variables: "{}",
        }),
        "query" => Some(GraphqlTemplate {
            name: "Query",
            query: GRAPHQL_QUERY_TEMPLATE,
            variables: r#"{
  "id": "example-id"
}"#,
        }),
        "mutation" => Some(GraphqlTemplate {
            name: "Mutation",
            query: GRAPHQL_MUTATION_TEMPLATE,
            variables: r#"{
  "input": {}
}"#,
        }),
        _ => None,
    }
}

pub(super) fn wire_graphql_helpers(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_apply_graphql_template(move |template| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let Some(template) = graphql_template(template.as_str()) else {
            set_response(
                &app,
                "GraphQL template failed",
                "",
                "error",
                "Unknown GraphQL template.",
            );
            return;
        };

        app.set_body_mode("graphql".into());
        app.set_request_body(template.query.into());
        refresh_body_field_rows(&app);
        app.set_graphql_variables(template.variables.into());
        set_response(
            &app,
            "GraphQL template ready",
            template.name,
            "success",
            "Template applied to the request body.",
        );
    });
}

pub(super) fn build_graphql_request_body(query: &str, variables: &str) -> Result<RequestBody> {
    let query = query.trim();
    if query.is_empty() {
        bail!("GraphQL query is empty");
    }
    let variables = parse_graphql_variables(variables)?;
    let payload = serde_json::json!({
        "query": query,
        "variables": variables,
    });

    Ok(RequestBody::Raw {
        content_type: Some("application/json".to_string()),
        body: serde_json::to_string(&payload)?,
    })
}

fn parse_graphql_variables(input: &str) -> Result<Value> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Value::Object(Default::default()));
    }

    let value: Value = serde_json::from_str(input)
        .map_err(|error| anyhow!("GraphQL variables JSON is invalid: {error}"))?;
    if !value.is_object() {
        bail!("GraphQL variables must be a JSON object");
    }
    Ok(value)
}

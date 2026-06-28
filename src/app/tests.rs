use super::*;
use serde_json::json;

fn route(method: &str, path: &str, summary: &str) -> ApiRoute {
    ApiRoute {
        method: method.to_string(),
        path: path.to_string(),
        summary: summary.to_string(),
        mock_body: json!({}),
        mock_rules: Vec::new(),
    }
}

fn saved_request(name: &str, method: &str, url: &str) -> CollectionRequest {
    CollectionRequest {
        name: name.to_string(),
        method: method.to_string(),
        url: url.to_string(),
        headers: Vec::new(),
        query_params: Vec::new(),
        body: CollectionBody::None,
        pre_request_script: String::new(),
        tests: Vec::new(),
    }
}

#[test]
fn filters_routes_by_method_path_or_summary() {
    let routes = vec![
        route("GET", "/users", "List accounts"),
        route("POST", "/sessions", "Create login session"),
        route("DELETE", "/users/{id}", "Remove account"),
    ];

    assert_eq!(filter_routes(&routes, "post"), vec![routes[1].clone()]);
    assert_eq!(filter_routes(&routes, "sessions"), vec![routes[1].clone()]);
    assert_eq!(filter_routes(&routes, "remove"), vec![routes[2].clone()]);
}

#[test]
fn updates_selected_mock_response_body() {
    let routes = vec![
        route("GET", "/users", "List accounts"),
        route("POST", "/sessions", "Create login session"),
    ];
    let mut state = AppState {
        routes: routes.clone(),
        visible_routes: vec![routes[1].clone()],
        ..AppState::default()
    };

    let updated = update_selected_mock_response(&mut state, 0, r#"{ "token": "abc", "ok": true }"#)
        .expect("mock body update");

    assert_eq!(updated.method, "POST");
    assert_eq!(updated.path, "/sessions");
    assert_eq!(updated.mock_body, json!({ "token": "abc", "ok": true }));
    assert_eq!(state.routes[0].mock_body, json!({}));
    assert_eq!(
        state.routes[1].mock_body,
        json!({ "token": "abc", "ok": true })
    );
    assert_eq!(
        state.visible_routes[0].mock_body,
        json!({ "token": "abc", "ok": true })
    );

    assert!(update_selected_mock_response(&mut state, -1, "{}").is_err());
    assert!(update_selected_mock_response(&mut state, 0, "{ nope").is_err());
    assert_eq!(
        state.routes[1].mock_body,
        json!({ "token": "abc", "ok": true })
    );
}

#[test]
fn adds_saves_and_deletes_selected_mock_rules() {
    let routes = vec![route("GET", "/profile", "Profile")];
    let mut state = AppState {
        routes: routes.clone(),
        visible_routes: routes,
        ..AppState::default()
    };

    let (route, row_id) = add_selected_mock_rule(&mut state, 0, "header").expect("add mock rule");
    assert_eq!(row_id, 0);
    assert_eq!(route.mock_rules[0].source, MockRuleSource::Header);
    assert_eq!(route.mock_rules[0].name, "x-mock-scenario");
    assert_eq!(state.visible_routes[0].mock_rules.len(), 1);

    let route = save_selected_mock_rule(
        &mut state,
        0,
        row_id,
        "query",
        "scenario",
        "admin",
        r#"{ "role": "admin" }"#,
    )
    .expect("save mock rule");
    assert_eq!(route.mock_rules[0].source, MockRuleSource::Query);
    assert_eq!(route.mock_rules[0].name, "scenario");
    assert_eq!(route.mock_rules[0].value, "admin");
    assert_eq!(route.mock_rules[0].mock_body, json!({ "role": "admin" }));
    assert_eq!(
        state.routes[0].mock_rules,
        state.visible_routes[0].mock_rules
    );

    assert!(save_selected_mock_rule(&mut state, 0, row_id, "query", "", "admin", "{}").is_err());
    assert!(
        save_selected_mock_rule(&mut state, 0, row_id, "header", "x-mode", "admin", "{ nope")
            .is_err()
    );

    let (route, next_row) =
        delete_selected_mock_rule(&mut state, 0, row_id).expect("delete mock rule");
    assert!(route.mock_rules.is_empty());
    assert_eq!(next_row, None);
    assert!(state.visible_routes[0].mock_rules.is_empty());
}

#[test]
fn maps_http_status_to_response_tone() {
    assert_eq!(response_tone(200), "success");
    assert_eq!(response_tone(302), "success");
    assert_eq!(response_tone(100), "neutral");
    assert_eq!(response_tone(404), "error");
}

#[test]
fn splits_response_meta_for_header_columns() {
    assert_eq!(
        split_response_meta("142 ms / 842 B"),
        ("142 ms".to_string(), "842 B".to_string())
    );
    assert_eq!(
        split_response_meta("17 ms"),
        ("17 ms".to_string(), String::new())
    );
    assert_eq!(split_response_meta(""), (String::new(), String::new()));
}

#[test]
fn formats_json_response_text_for_viewer() {
    assert_eq!(
        format_json_response_text("{\"ok\":true,\"items\":[1,2]}").expect("format"),
        "{\n  \"items\": [\n    1,\n    2\n  ],\n  \"ok\": true\n}"
    );
    assert!(format_json_response_text("not json").is_err());
    assert!(format_json_response_text("").is_err());
}

#[test]
fn folds_json_response_text_for_viewer() {
    assert_eq!(
        fold_json_response_text(
            r#"{"user":{"id":1,"name":"Ada"},"roles":["admin","dev"],"ok":true}"#
        )
        .expect("fold object"),
        "{\n  \"ok\": true,\n  \"roles\": [ ... ],\n  \"user\": { ... }\n}"
    );
    assert_eq!(
        fold_json_response_text(r#"[{"id":1},2,[]]"#).expect("fold array"),
        "[\n  { ... },\n  2,\n  []\n]"
    );
    assert!(fold_json_response_text("not json").is_err());
}

#[test]
fn summarizes_non_json_folded_response_text() {
    assert_eq!(
        folded_response_view_text("headers", "Content-Type: application/json\nX-Trace: 42"),
        "Headers folded\n2 line(s), 42 byte(s)\n\nContent-Type: application/json"
    );
    assert_eq!(
        folded_response_view_text("cookies", ""),
        "Cookies folded\n0 lines"
    );
    assert!(folded_response_view_text("pretty", "not json").starts_with("Pretty folded\n"));
}

#[test]
fn selects_response_copy_text_from_byte_offsets() {
    assert_eq!(response_copy_text("abcdef", 1, 4), ("bcd", true));
    assert_eq!(response_copy_text("abcdef", 4, 1), ("bcd", true));
    assert_eq!(response_copy_text("h\u{e9}llo", 0, 3), ("h\u{e9}", true));
    assert_eq!(response_copy_text("abcdef", 2, 2), ("abcdef", false));
    assert_eq!(response_copy_text("abcdef", -1, 3), ("abcdef", false));
    assert_eq!(response_copy_text("abcdef", 0, 20), ("abcdef", false));
    assert_eq!(
        response_copy_text("h\u{e9}llo", 0, 2),
        ("h\u{e9}llo", false)
    );
}

#[test]
fn parses_query_and_header_text_lines() {
    assert_eq!(
        parse_key_value_lines(
            "Accept: application/json\nsearch = rust slint\nbaseUrl=https://api.example.com\n# ignored\n\nlimit=20",
            "header"
        )
        .expect("parse"),
        vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("search".to_string(), "rust slint".to_string()),
            (
                "baseUrl".to_string(),
                "https://api.example.com".to_string()
            ),
            ("limit".to_string(), "20".to_string()),
        ]
    );
}

#[test]
fn rejects_malformed_key_value_lines() {
    let error = parse_key_value_lines("Accept: application/json\nmissing-separator", "header")
        .expect_err("invalid line");

    assert!(error.to_string().contains("line 2"));
}

#[test]
fn applies_header_presets_to_text_headers() {
    assert_eq!(
        apply_header_preset("accept: text/plain\nX-Trace=abc", "accept-json")
            .expect("accept preset"),
        "accept: application/json\nX-Trace: abc"
    );
    assert_eq!(
        apply_header_preset("Accept: application/json", "content-json").expect("content preset"),
        "Accept: application/json\nContent-Type: application/json"
    );
    assert_eq!(
        apply_header_preset("", "bearer-token").expect("bearer preset"),
        "Authorization: Bearer {{token}}"
    );
    assert!(
        apply_header_preset("Accept: application/json", "unknown")
            .expect_err("unknown preset")
            .to_string()
            .contains("unknown header preset")
    );
}

#[test]
fn formats_response_headers_for_display() {
    assert_eq!(
        format_headers(&[
            ("content-type".to_string(), "application/json".to_string()),
            ("x-request-id".to_string(), "abc".to_string())
        ]),
        "content-type: application/json\nx-request-id: abc"
    );
    assert_eq!(format_headers(&[]), "No headers");
}

#[test]
fn formats_response_cookies_for_display() {
    assert_eq!(
        format_cookies(&[
            ("content-type".to_string(), "application/json".to_string()),
            (
                "Set-Cookie".to_string(),
                "session=abc; Path=/; HttpOnly".to_string()
            ),
            (
                "set-cookie".to_string(),
                "theme=dark; Max-Age=3600".to_string()
            )
        ]),
        "1 session=abc; Path=/; HttpOnly\n2 theme=dark; Max-Age=3600"
    );
    assert_eq!(format_cookies(&[]), "No cookies");
    assert_eq!(
        format_cookies(&[("content-type".to_string(), "application/json".to_string())]),
        "No cookies"
    );
}

#[test]
fn builds_request_body_from_slint_mode() {
    assert_eq!(
        build_request_body("none", "ignored", "", "").unwrap(),
        RequestBody::None
    );
    assert_eq!(
        build_request_body("raw", "{\"name\":\"Zen\"}", "", "json").unwrap(),
        RequestBody::Raw {
            content_type: Some("application/json".to_string()),
            body: "{\"name\":\"Zen\"}".to_string(),
        }
    );
    assert_eq!(
        build_request_body("raw", "hello", "", "text").unwrap(),
        RequestBody::Raw {
            content_type: Some("text/plain".to_string()),
            body: "hello".to_string(),
        }
    );
    assert_eq!(
        build_request_body("raw", "<ok/>", "", "xml").unwrap(),
        RequestBody::Raw {
            content_type: Some("application/xml".to_string()),
            body: "<ok/>".to_string(),
        }
    );
    assert_eq!(
        build_request_body("urlenc", "search=rust slint\nlimit: 20", "", "").unwrap(),
        RequestBody::FormUrlEncoded(vec![
            ("search".to_string(), "rust slint".to_string()),
            ("limit".to_string(), "20".to_string())
        ])
    );
    assert_eq!(
        build_request_body("form", "file=@/tmp/upload.txt", "", "").unwrap(),
        RequestBody::Multipart(vec![("file".to_string(), "@/tmp/upload.txt".to_string())])
    );
    assert_eq!(
        build_request_body("binary", "/tmp/body.bin", "", "").unwrap(),
        RequestBody::BinaryFile {
            path: "/tmp/body.bin".to_string(),
            content_type: Some("application/octet-stream".to_string()),
        }
    );
}

#[test]
fn infers_binary_body_content_types_from_extensions() {
    assert_eq!(
        binary_body_content_type("/tmp/payload.JSON"),
        "application/json"
    );
    assert_eq!(binary_body_content_type("/tmp/report.csv"), "text/csv");
    assert_eq!(binary_body_content_type("/tmp/image.jpeg"), "image/jpeg");
    assert_eq!(
        binary_body_content_type("/tmp/archive.tar"),
        "application/x-tar"
    );
    assert_eq!(
        binary_body_content_type("/tmp/unknown.custom"),
        "application/octet-stream"
    );
}

#[test]
fn rejects_empty_binary_body_path() {
    let error = build_request_body("binary", "  ", "", "").expect_err("empty path");

    assert!(error.to_string().contains("path is empty"));
}

#[test]
fn builds_graphql_payload_body() {
    assert_eq!(
        build_request_body(
            "graphql",
            "query User($id: ID!) { user(id: $id) { name } }",
            r#"{"id":"u_123"}"#,
            "",
        )
        .unwrap(),
        RequestBody::Raw {
            content_type: Some("application/json".to_string()),
            body: r#"{"query":"query User($id: ID!) { user(id: $id) { name } }","variables":{"id":"u_123"}}"#.to_string(),
        }
    );

    let error = build_request_body("graphql", "{ viewer { id } }", "[]", "")
        .expect_err("invalid variables");
    assert!(error.to_string().contains("JSON object"));
}

#[test]
fn returns_graphql_helper_templates() {
    let introspection = graphql_template("introspection").expect("introspection template");
    assert_eq!(introspection.name, "Introspection");
    assert!(introspection.query.contains("__schema"));
    assert!(introspection.query.contains("fragment TypeRef"));
    assert_eq!(introspection.variables, "{}");

    let query = graphql_template("query").expect("query template");
    assert!(query.query.contains("query Example"));
    assert!(query.variables.contains("example-id"));

    let mutation = graphql_template("mutation").expect("mutation template");
    assert!(mutation.query.contains("mutation Example"));
    assert!(mutation.variables.contains("\"input\""));
    assert!(graphql_template("unknown").is_none());
}

#[test]
fn summarizes_graphql_introspection_response() {
    let response = r#"{
  "data": {
"__schema": {
  "queryType": { "name": "Query" },
  "mutationType": { "name": "Mutation" },
  "subscriptionType": null,
  "types": [
    {
      "kind": "OBJECT",
      "name": "Query",
      "fields": [
        {
          "name": "user",
          "args": [
            {
              "name": "id",
              "type": {
                "kind": "NON_NULL",
                "name": null,
                "ofType": { "kind": "SCALAR", "name": "ID" }
              }
            }
          ],
          "type": { "kind": "OBJECT", "name": "User" },
          "isDeprecated": false,
          "deprecationReason": null
        },
        {
          "name": "friends",
          "args": [],
          "type": {
            "kind": "LIST",
            "name": null,
            "ofType": {
              "kind": "NON_NULL",
              "name": null,
              "ofType": { "kind": "OBJECT", "name": "User" }
            }
          },
          "isDeprecated": true,
          "deprecationReason": "Use connections"
        }
      ]
    },
    {
      "kind": "OBJECT",
      "name": "User",
      "fields": [
        {
          "name": "id",
          "args": [],
          "type": { "kind": "SCALAR", "name": "ID" },
          "isDeprecated": false
        }
      ]
    },
    {
      "kind": "OBJECT",
      "name": "__Type",
      "fields": []
    }
  ]
}
  }
}"#;

    let summary = summarize_graphql_schema_response(response).expect("schema summary");

    assert!(summary.contains("GraphQL Schema"));
    assert!(summary.contains("Query: Query"));
    assert!(summary.contains("Mutation: Mutation"));
    assert!(summary.contains("Subscription: none"));
    assert!(summary.contains("Types: 2 shown / 3 total"));
    assert!(summary.contains("type Query"));
    assert!(summary.contains("user(id: ID!): User"));
    assert!(summary.contains("friends: [User!] @deprecated(reason: \"Use connections\")"));
    assert!(summary.contains("type User"));
    assert!(summary.contains("id: ID"));
    assert!(!summary.contains("type __Type"));
}

#[test]
fn reports_graphql_introspection_errors() {
    let error = summarize_graphql_schema_response(
        r#"{"errors":[{"message":"Cannot query field __schema"}]}"#,
    )
    .expect_err("missing schema");

    assert!(
        error
            .to_string()
            .contains("GraphQL introspection schema not found")
    );
    assert!(error.to_string().contains("Cannot query field __schema"));
}

#[test]
fn auth_entries_override_existing_headers() {
    let mut headers = vec![
        ("accept".to_string(), "application/json".to_string()),
        ("authorization".to_string(), "old".to_string()),
    ];
    for (name, value) in build_auth_entries("jwt", "new-token").unwrap().0 {
        upsert_pair(&mut headers, name, value, true);
    }

    assert_eq!(
        headers,
        vec![
            ("accept".to_string(), "application/json".to_string()),
            ("authorization".to_string(), "Bearer new-token".to_string()),
        ]
    );
}

#[test]
fn builds_variable_store_and_resolves_environment_overrides() {
    let (variables, active_environment) = build_variable_store(
        "baseUrl=https://api.example.com\ntoken=global",
        "dev",
        "baseUrl=http://localhost:8080",
    )
    .expect("variables");

    assert_eq!(active_environment.as_deref(), Some("dev"));
    assert_eq!(
        resolve_text(
            "{{baseUrl}}/users",
            &variables,
            active_environment.as_deref()
        )
        .expect("resolve"),
        "http://localhost:8080/users"
    );
    assert_eq!(
        resolve_text(
            "Bearer {{token}}",
            &variables,
            active_environment.as_deref()
        )
        .expect("resolve"),
        "Bearer global"
    );
}

#[test]
fn preserves_environment_profile_values_by_name() {
    let mut profiles = EnvironmentProfiles::new("dev", "baseUrl=http://localhost:8080");

    assert_eq!(
        profiles.switch_to("prod", "baseUrl=http://localhost:8080"),
        ""
    );
    profiles.save_active("baseUrl=https://api.example.com");
    assert_eq!(
        profiles.switch_to("dev", "baseUrl=https://api.example.com"),
        "baseUrl=http://localhost:8080"
    );
    assert_eq!(
        profiles.switch_to("prod", "baseUrl=http://localhost:8080"),
        "baseUrl=https://api.example.com"
    );
    assert_eq!(
        profiles.switch_to("local", "baseUrl=https://api.example.com"),
        ""
    );
}

#[test]
fn builds_environment_rows_from_saved_profiles() {
    use slint::Model;

    let mut values_by_name = BTreeMap::new();
    values_by_name.insert(
        "dev".to_string(),
        "baseUrl=http://127.0.0.1:8080".to_string(),
    );
    values_by_name.insert(
        "prod".to_string(),
        "baseUrl=https://api.example.com\ntoken=secret".to_string(),
    );
    let profiles = EnvironmentProfiles {
        active_name: "prod".to_string(),
        values_by_name,
        order: vec!["prod".to_string(), "dev".to_string()],
    };

    let rows = environment_rows_model(&profiles);
    assert_eq!(rows.row_count(), 2);

    let prod = rows.row_data(0).expect("prod row");
    assert_eq!(prod.name.as_str(), "prod");
    assert_eq!(prod.label.as_str(), "Production");
    assert_eq!(prod.detail.as_str(), "2 env variable(s)");
    assert_eq!(prod.tone.as_str(), "error");

    let dev = rows.row_data(1).expect("dev row");
    assert_eq!(dev.name.as_str(), "dev");
    assert_eq!(dev.label.as_str(), "Development");
    assert_eq!(dev.detail.as_str(), "1 env variable(s)");
    assert_eq!(dev.tone.as_str(), "inactive");
}

#[test]
fn deletes_environment_profiles_and_keeps_workspace_values() {
    let mut values_by_name = BTreeMap::new();
    values_by_name.insert(
        "dev".to_string(),
        "baseUrl=http://127.0.0.1:8080".to_string(),
    );
    values_by_name.insert(
        "prod".to_string(),
        "baseUrl=https://api.example.com".to_string(),
    );
    let workspace = EnvironmentWorkspace {
        active_name: "prod".to_string(),
        global_variables: "token=secret".to_string(),
        values_by_name,
        order: vec!["dev".to_string(), "prod".to_string()],
    };

    let mut profiles = EnvironmentProfiles::from_workspace(Some(workspace), "dev", "");
    assert_eq!(
        profiles.switch_to("dev", "baseUrl=https://api.example.com"),
        "baseUrl=http://127.0.0.1:8080"
    );
    assert_eq!(
        profiles.delete("dev", "baseUrl=http://127.0.0.1:8081"),
        Some((
            "prod".to_string(),
            "baseUrl=https://api.example.com".to_string()
        ))
    );
    assert!(!profiles.values_by_name.contains_key("dev"));
    assert_eq!(profiles.active_name, "prod");
    assert_eq!(profiles.order, vec!["prod".to_string()]);
    assert!(profiles.delete("missing", "").is_none());
}

#[test]
fn renames_environment_profiles_without_overwriting_existing_values() {
    let mut values_by_name = BTreeMap::new();
    values_by_name.insert(
        "dev".to_string(),
        "baseUrl=http://127.0.0.1:8080".to_string(),
    );
    values_by_name.insert(
        "prod".to_string(),
        "baseUrl=https://api.example.com".to_string(),
    );
    let mut profiles = EnvironmentProfiles {
        active_name: "dev".to_string(),
        values_by_name,
        order: vec!["prod".to_string(), "dev".to_string()],
    };

    assert_eq!(
        profiles.rename("dev", "local", "baseUrl=http://127.0.0.1:8081"),
        Some((
            "local".to_string(),
            "baseUrl=http://127.0.0.1:8081".to_string()
        ))
    );
    assert_eq!(profiles.active_name, "local");
    assert_eq!(
        profiles.order,
        vec!["prod".to_string(), "local".to_string()]
    );
    assert!(!profiles.values_by_name.contains_key("dev"));
    assert_eq!(
        profiles.values_by_name.get("local").map(String::as_str),
        Some("baseUrl=http://127.0.0.1:8081")
    );
    assert!(
        profiles
            .rename("local", "prod", "baseUrl=http://127.0.0.1:8082")
            .is_none()
    );
    assert_eq!(
        profiles.values_by_name.get("local").map(String::as_str),
        Some("baseUrl=http://127.0.0.1:8081")
    );
}

#[test]
fn reorders_environment_profiles_using_saved_order() {
    let workspace: EnvironmentWorkspace = serde_json::from_str(
        r#"{
            "active_name": "prod",
            "global_variables": "token=secret",
            "values_by_name": {
                "dev": "baseUrl=http://127.0.0.1:8080",
                "prod": "baseUrl=https://api.example.com",
                "test": "baseUrl=https://staging.example.com"
            },
            "order": ["dev", "prod", "test"]
        }"#,
    )
    .expect("workspace");
    let mut profiles = EnvironmentProfiles::from_workspace(Some(workspace), "dev", "");

    assert_eq!(
        profiles.order,
        vec!["dev".to_string(), "prod".to_string(), "test".to_string()]
    );
    assert!(profiles.move_profile("test", -1, "baseUrl=https://api.example.com"));
    assert_eq!(
        profiles.order,
        vec!["dev".to_string(), "test".to_string(), "prod".to_string()]
    );
    assert_eq!(
        profiles.values_by_name.get("prod").map(String::as_str),
        Some("baseUrl=https://api.example.com")
    );
    assert!(profiles.move_profile("dev", 1, "baseUrl=https://api.example.com"));
    assert_eq!(
        profiles.order,
        vec!["test".to_string(), "dev".to_string(), "prod".to_string()]
    );
    assert!(!profiles.move_profile("test", -1, ""));
    assert!(!profiles.move_profile("missing", 1, ""));
}

#[test]
fn saves_and_loads_environment_workspace_to_disk() {
    let path = std::env::temp_dir().join(format!(
        "zenapi-environment-workspace-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    assert!(
        load_environment_workspace(&path)
            .expect("missing env")
            .is_none()
    );

    let mut values_by_name = BTreeMap::new();
    values_by_name.insert(
        "dev".to_string(),
        "baseUrl=http://127.0.0.1:8080".to_string(),
    );
    values_by_name.insert(
        "prod".to_string(),
        "baseUrl=https://api.example.com".to_string(),
    );
    let workspace = EnvironmentWorkspace {
        active_name: "prod".to_string(),
        global_variables: "token=secret".to_string(),
        values_by_name,
        order: vec!["prod".to_string(), "dev".to_string()],
    };

    save_environment_workspace(&path, &workspace).expect("save env workspace");
    let loaded = load_environment_workspace(&path)
        .expect("load env workspace")
        .expect("workspace");

    assert_eq!(loaded, workspace);
    let _ = fs::remove_file(path);
}

#[test]
fn resolves_variables_in_pairs() {
    let (variables, active_environment) =
        build_variable_store("token=secret", "", "").expect("variables");

    assert_eq!(
        resolve_pairs(
            vec![("Authorization".to_string(), "Bearer {{token}}".to_string())],
            &variables,
            active_environment.as_deref()
        )
        .expect("resolve"),
        vec![("Authorization".to_string(), "Bearer secret".to_string())]
    );
}

#[test]
fn rejects_environment_variables_without_environment_name() {
    let error = build_variable_store("", "", "baseUrl=http://localhost:8080")
        .expect_err("missing env name");

    assert!(error.to_string().contains("environment name is empty"));
}

#[test]
fn builds_variable_rows_for_environment_page() {
    use slint::Model;

    let rows = variable_table_model(
        "baseUrl=https://api.example.com\ntoken=global",
        "baseUrl=http://localhost:8080",
    );

    assert_eq!(rows.row_count(), 3);
    let global = rows.row_data(0).expect("global row");
    assert_eq!(global.row_id, 0);
    assert_eq!(global.scope.as_str(), "global");
    assert_eq!(global.name.as_str(), "baseUrl");
    assert_eq!(global.current_value.as_str(), "https://api.example.com");

    let env = rows.row_data(2).expect("env row");
    assert_eq!(env.row_id, 0);
    assert_eq!(env.scope.as_str(), "environment");
    assert_eq!(env.name.as_str(), "baseUrl");
    assert_eq!(env.current_value.as_str(), "http://localhost:8080");
}

#[test]
fn updates_adds_and_deletes_variable_text_rows() {
    let input = "# keep\nbaseUrl=https://api.example.com\ntoken=global";

    assert_eq!(
        update_variable_text(input, 1, "token", "changed"),
        "# keep\nbaseUrl=https://api.example.com\ntoken=changed"
    );

    assert_eq!(
        add_variable_text("baseUrl=https://api.example.com", "GLOBAL_VAR"),
        "baseUrl=https://api.example.com\nGLOBAL_VAR="
    );

    assert_eq!(
        add_variable_text("GLOBAL_VAR=one", "GLOBAL_VAR"),
        "GLOBAL_VAR=one\nGLOBAL_VAR_2="
    );

    assert_eq!(delete_variable_text(input, 0), "# keep\ntoken=global");
}

#[test]
fn builds_key_value_rows_for_query_params() {
    use slint::Model;

    let rows = key_value_table_model("# keep\nsearch=slint\nlimit: 20");

    assert_eq!(rows.row_count(), 2);
    let search = rows.row_data(0).expect("search row");
    assert_eq!(search.row_id, 0);
    assert_eq!(search.key.as_str(), "search");
    assert_eq!(search.value.as_str(), "slint");

    let limit = rows.row_data(1).expect("limit row");
    assert_eq!(limit.row_id, 1);
    assert_eq!(limit.key.as_str(), "limit");
    assert_eq!(limit.value.as_str(), "20");
}

#[test]
fn updates_adds_and_deletes_key_value_text_rows() {
    let input = "# keep\nsearch=slint\nlimit=20";

    assert_eq!(
        update_key_value_text(input, 1, "limit", "50"),
        "# keep\nsearch=slint\nlimit=50"
    );
    assert_eq!(
        add_key_value_text("search=slint", "param"),
        "search=slint\nparam="
    );
    assert_eq!(
        add_key_value_text("param=one", "param"),
        "param=one\nparam_2="
    );
    assert_eq!(
        merge_key_value_text(
            "search=slint\nlimit=20",
            "limit=50\nsort: desc",
            "query param",
            false,
            false,
        )
        .expect("merge params"),
        ("search=slint\nlimit=50\nsort=desc".to_string(), 2)
    );
    assert_eq!(
        merge_key_value_text(
            "Accept: application/json\nX-Trace=one",
            "accept: text/plain\nAuthorization=Bearer token",
            "header",
            true,
            true,
        )
        .expect("merge headers"),
        (
            "Accept: text/plain\nX-Trace: one\nAuthorization: Bearer token".to_string(),
            2
        )
    );
    assert!(
        merge_key_value_text("search=slint", "   \n# none", "query param", false, false)
            .expect_err("empty clipboard")
            .to_string()
            .contains("does not contain any query param rows")
    );
    let import_path =
        std::env::temp_dir().join(format!("zenapi-query-import-{}.txt", std::process::id()));
    fs::write(&import_path, "limit=50\nsort: desc").expect("write import fixture");
    assert_eq!(
        merge_key_value_file(
            "search=slint\nlimit=20",
            import_path.to_str().expect("utf-8 import path"),
            "query param",
            false,
            false,
        )
        .expect("merge params from file"),
        ("search=slint\nlimit=50\nsort=desc".to_string(), 2)
    );
    assert!(
        merge_key_value_file("search=slint", " ", "query param", false, false)
            .expect_err("empty import path")
            .to_string()
            .contains("query param import path is required")
    );
    assert_eq!(
        add_form_file_field_text(
            "search=slint",
            "upload",
            import_path.to_str().expect("utf-8 import path"),
        )
        .expect("add form file"),
        format!("search=slint\nupload=@{}", import_path.display())
    );
    assert!(
        add_form_file_field_text("search=slint", "", import_path.to_str().unwrap())
            .expect_err("empty field")
            .to_string()
            .contains("field name is required")
    );
    assert!(
        add_form_file_field_text("search=slint", "upload", "/tmp/zenapi-missing-upload")
            .expect_err("missing file")
            .to_string()
            .contains("does not exist")
    );
    let _ = fs::remove_file(import_path);
    assert_eq!(delete_key_value_text(input, 0), "# keep\nlimit=20");
}

#[test]
fn updates_adds_and_deletes_test_assertion_rows() {
    use slint::Model;

    let input =
        "# keep\nstatus_equals 200\nheader_equals content-type application/json\nbody_contains ok";
    let rows = test_assertion_table_model(input);
    assert_eq!(rows.row_count(), 3);

    let status = rows.row_data(0).expect("status row");
    assert_eq!(status.row_id, 0);
    assert_eq!(status.kind.as_str(), "status_equals");
    assert_eq!(status.target.as_str(), "200");
    assert_eq!(status.expected.as_str(), "");

    let header = rows.row_data(1).expect("header row");
    assert_eq!(header.kind.as_str(), "header_equals");
    assert_eq!(header.target.as_str(), "content-type");
    assert_eq!(header.expected.as_str(), "application/json");

    let pm_rows = test_assertion_table_model(
        r#"pm.test("status", () => { pm.response.to.have.status(204); })"#,
    );
    let pm_status = pm_rows.row_data(0).expect("pm status row");
    assert_eq!(pm_status.kind.as_str(), "status_equals");
    assert_eq!(pm_status.target.as_str(), "204");
    assert_eq!(pm_status.expected.as_str(), "");

    assert_eq!(
        update_test_assertion_text(input, 1, "json_path_equals", "data.id", "1"),
        "# keep\nstatus_equals 200\njson_path_equals data.id 1\nbody_contains ok"
    );
    assert_eq!(
        next_test_assertion_template("status_equals"),
        ("status_in_range", "200", "299")
    );
    assert_eq!(
        next_test_assertion_template("status_in_range"),
        ("response_time_below", "500", "")
    );
    assert_eq!(
        next_test_assertion_template("response_time_below"),
        ("response_size_below", "65536", "")
    );
    assert_eq!(
        next_test_assertion_template("response_size_below"),
        ("header_exists", "content-type", "")
    );
    let (kind, target, expected) = next_test_assertion_template("header_exists");
    assert_eq!(
        update_test_assertion_text(input, 1, kind, target, expected),
        "# keep\nstatus_equals 200\nheader_equals content-type application/json\nbody_contains ok"
    );
    assert_eq!(
        next_test_assertion_template("header_equals"),
        ("header_not_exists", "x-debug", "")
    );
    assert_eq!(
        next_test_assertion_template("header_not_exists"),
        ("body_equals", r#"{"ok":true}"#, "")
    );
    assert_eq!(
        next_test_assertion_template("body_equals"),
        ("body_contains", "ok", "")
    );
    assert_eq!(
        next_test_assertion_template("json_path_equals"),
        ("json_path_not_equals", "data.id", "0")
    );
    assert_eq!(
        next_test_assertion_template("json_path_not_equals"),
        ("status_equals", "200", "")
    );
    assert_eq!(
        next_test_assertion_template("body_contains"),
        ("body_not_contains", "error", "")
    );
    assert_eq!(
        next_test_assertion_template("body_not_contains"),
        ("json_path_exists", "data.id", "")
    );
    assert_eq!(
        next_test_assertion_template("json_path_exists"),
        ("json_path_not_exists", "error", "")
    );
    assert_eq!(
        next_test_assertion_template("json_path_not_exists"),
        ("json_path_type", "data.items", "array")
    );
    assert_eq!(
        next_test_assertion_template("json_path_type"),
        ("json_path_length", "data.items", "2")
    );
    assert_eq!(
        next_test_assertion_template("json_path_length"),
        ("json_path_contains", "data.items", r#"{"id":1}"#)
    );
    assert_eq!(
        next_test_assertion_template("json_path_contains"),
        ("json_path_not_contains", "data.items", r#"{"id":999}"#)
    );
    assert_eq!(
        next_test_assertion_template("json_path_not_contains"),
        ("json_path_equals", "data.id", "1")
    );
    assert_eq!(
        add_test_assertion_text("status_equals 200"),
        "status_equals 200\nstatus_equals 200"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "header").unwrap(),
        "status_equals 200\nheader_equals content-type application/json"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "header_absent").unwrap(),
        "status_equals 200\nheader_not_exists x-debug"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "body").unwrap(),
        "status_equals 200\nbody_contains ok"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "body_equals").unwrap(),
        "status_equals 200\nbody_equals {\"ok\":true}"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "body_not").unwrap(),
        "status_equals 200\nbody_not_contains error"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "json").unwrap(),
        "status_equals 200\njson_path_equals data.id 1"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "json_exists").unwrap(),
        "status_equals 200\njson_path_exists data.id"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "json_not_exists").unwrap(),
        "status_equals 200\njson_path_not_exists error"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "json_type").unwrap(),
        "status_equals 200\njson_path_type data.items array"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "json_length").unwrap(),
        "status_equals 200\njson_path_length data.items 2"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "json_contains").unwrap(),
        "status_equals 200\njson_path_contains data.items {\"id\":1}"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "json_not_contains").unwrap(),
        "status_equals 200\njson_path_not_contains data.items {\"id\":999}"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "json_not_equals").unwrap(),
        "status_equals 200\njson_path_not_equals data.id 0"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "time").unwrap(),
        "status_equals 200\nresponse_time_below 500"
    );
    assert_eq!(
        add_test_assertion_template_text("status_equals 200", "size").unwrap(),
        "status_equals 200\nresponse_size_below 65536"
    );
    assert_eq!(
        add_custom_test_assertion_text("status_equals 200", "status_in_range", "200", "299",)
            .unwrap(),
        "status_equals 200\nstatus_in_range 200 299"
    );
    assert_eq!(
        add_custom_test_assertion_text("", "body_contains", "ready", "").unwrap(),
        "body_contains ready"
    );
    assert_eq!(
        test_assertion_template("range"),
        Some(("status_in_range", "200", "299"))
    );
    assert!(add_test_assertion_template_text(input, "unknown").is_err());
    assert!(add_custom_test_assertion_text(input, "unknown", "x", "").is_err());
    assert!(add_custom_test_assertion_text(input, "status_in_range", "200", "").is_err());
    assert_eq!(
        delete_test_assertion_text(input, 0),
        "# keep\nheader_equals content-type application/json\nbody_contains ok"
    );
}

#[test]
fn masks_secret_values_in_variable_preview() {
    let preview = variables_json_preview(
        "token=global-secret\nbaseUrl=https://api.example.com",
        "dev",
        "API_KEY=local-secret",
    );

    assert!(preview.contains("\"activeEnvironment\": \"dev\""));
    assert!(preview.contains("\"token\": \"********\""));
    assert!(preview.contains("\"API_KEY\": \"********\""));
    assert!(preview.contains("\"baseUrl\": \"https://api.example.com\""));
    assert!(!preview.contains("global-secret"));
    assert!(!preview.contains("local-secret"));
}

#[test]
fn projects_current_slint_request_for_codegen() {
    let request = build_codegen_request_projection(&RequestProjectionInput {
        method: "POST".to_string(),
        url: "{{baseUrl}}/users".to_string(),
        query_params: "debug=true".to_string(),
        headers: "Accept: application/json".to_string(),
        auth_mode: "bearer".to_string(),
        auth_config: "{{token}}".to_string(),
        body_mode: "raw".to_string(),
        raw_body_subtype: "json".to_string(),
        body: "{\"name\":\"{{name}}\"}".to_string(),
        graphql_variables: String::new(),
        pre_request_script: String::new(),
        global_variables: "baseUrl=https://api.example.com\ntoken=secret\nname=Zen".to_string(),
        environment_name: String::new(),
        environment_variables: String::new(),
    })
    .expect("request");

    assert_eq!(request.method, "POST");
    assert_eq!(request.url, "https://api.example.com/users");
    assert_eq!(
        request.headers,
        vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("Authorization".to_string(), "Bearer secret".to_string())
        ]
    );
    assert_eq!(
        request.query_params,
        vec![("debug".to_string(), "true".to_string())]
    );
    assert_eq!(
        request.body,
        RequestBody::Raw {
            content_type: Some("application/json".to_string()),
            body: "{\"name\":\"Zen\"}".to_string(),
        }
    );
}

#[test]
fn applies_pre_request_actions_before_projection_resolution() {
    let request = build_codegen_request_projection(&RequestProjectionInput {
        method: "GET".to_string(),
        url: "{{baseUrl}}/users".to_string(),
        query_params: String::new(),
        headers: String::new(),
        auth_mode: "none".to_string(),
        auth_config: String::new(),
        body_mode: "none".to_string(),
        raw_body_subtype: "json".to_string(),
        body: String::new(),
        graphql_variables: String::new(),
        pre_request_script:
            "set_var baseUrl=http://127.0.0.1:8080\nset_header X-Mode=test\nset_query debug=true"
                .to_string(),
        global_variables: "baseUrl=https://api.example.com".to_string(),
        environment_name: String::new(),
        environment_variables: String::new(),
    })
    .expect("request");

    assert_eq!(request.url, "http://127.0.0.1:8080/users");
    assert_eq!(
        request.headers,
        vec![("X-Mode".to_string(), "test".to_string())]
    );
    assert_eq!(
        request.query_params,
        vec![("debug".to_string(), "true".to_string())]
    );
}

#[test]
fn parses_and_formats_native_test_assertions() {
    let assertions = parse_response_assertions(
        "status_equals 200\nresponse_time_below 500\nresponse_size_below 65536\nheader_equals Content-Type application/json\njson_path_exists data.id\njson_path_type data.items array\njson_path_length data.items 2\njson_path_contains data.items {\"id\":1}\njson_path_equals ok true",
    )
    .expect("assertions");

    assert_eq!(
        assertions,
        vec![
            ResponseAssertion {
                name: "status_equals 200".to_string(),
                kind: ResponseAssertionKind::StatusEquals { status: 200 },
            },
            ResponseAssertion {
                name: "response_time_below 500".to_string(),
                kind: ResponseAssertionKind::ResponseTimeBelow { max_ms: 500 },
            },
            ResponseAssertion {
                name: "response_size_below 65536".to_string(),
                kind: ResponseAssertionKind::ResponseSizeBelow { max_bytes: 65536 },
            },
            ResponseAssertion {
                name: "header_equals Content-Type application/json".to_string(),
                kind: ResponseAssertionKind::HeaderEquals {
                    name: "Content-Type".to_string(),
                    value: "application/json".to_string(),
                },
            },
            ResponseAssertion {
                name: "json_path_exists data.id".to_string(),
                kind: ResponseAssertionKind::JsonPathExists {
                    path: "data.id".to_string(),
                },
            },
            ResponseAssertion {
                name: "json_path_type data.items array".to_string(),
                kind: ResponseAssertionKind::JsonPathType {
                    path: "data.items".to_string(),
                    value_type: "array".to_string(),
                },
            },
            ResponseAssertion {
                name: "json_path_length data.items 2".to_string(),
                kind: ResponseAssertionKind::JsonPathLength {
                    path: "data.items".to_string(),
                    length: 2,
                },
            },
            ResponseAssertion {
                name: "json_path_contains data.items {\"id\":1}".to_string(),
                kind: ResponseAssertionKind::JsonPathContains {
                    path: "data.items".to_string(),
                    value: serde_json::json!({ "id": 1 }),
                },
            },
            ResponseAssertion {
                name: "json_path_equals ok true".to_string(),
                kind: ResponseAssertionKind::JsonPathEquals {
                    path: "ok".to_string(),
                    value: Value::Bool(true),
                },
            },
        ]
    );
    assert_eq!(
        format_response_assertions(&assertions),
        "status_equals 200\nresponse_time_below 500\nresponse_size_below 65536\nheader_equals Content-Type application/json\njson_path_exists data.id\njson_path_type data.items array\njson_path_length data.items 2\njson_path_contains data.items {\"id\":1}\njson_path_equals ok true"
    );
}

#[test]
fn parses_and_formats_negative_and_exact_test_assertions() {
    let assertions = parse_response_assertions(
        "header_not_exists x-debug\nbody_equals {\"ok\":true}\nbody_not_contains error\njson_path_not_exists error\njson_path_not_contains tags \"beta\"\njson_path_not_equals ok false",
    )
    .expect("negative assertions");

    assert_eq!(
        assertions,
        vec![
            ResponseAssertion {
                name: "header_not_exists x-debug".to_string(),
                kind: ResponseAssertionKind::HeaderNotExists {
                    name: "x-debug".to_string(),
                },
            },
            ResponseAssertion {
                name: "body_equals {\"ok\":true}".to_string(),
                kind: ResponseAssertionKind::BodyEquals {
                    text: "{\"ok\":true}".to_string(),
                },
            },
            ResponseAssertion {
                name: "body_not_contains error".to_string(),
                kind: ResponseAssertionKind::BodyNotContains {
                    text: "error".to_string(),
                },
            },
            ResponseAssertion {
                name: "json_path_not_exists error".to_string(),
                kind: ResponseAssertionKind::JsonPathNotExists {
                    path: "error".to_string(),
                },
            },
            ResponseAssertion {
                name: "json_path_not_contains tags \"beta\"".to_string(),
                kind: ResponseAssertionKind::JsonPathNotContains {
                    path: "tags".to_string(),
                    value: Value::from("beta"),
                },
            },
            ResponseAssertion {
                name: "json_path_not_equals ok false".to_string(),
                kind: ResponseAssertionKind::JsonPathNotEquals {
                    path: "ok".to_string(),
                    value: Value::Bool(false),
                },
            },
        ]
    );
    assert_eq!(
        format_response_assertions(&assertions),
        "header_not_exists x-debug\nbody_equals {\"ok\":true}\nbody_not_contains error\njson_path_not_exists error\njson_path_not_contains tags \"beta\"\njson_path_not_equals ok false"
    );
}

#[test]
fn parses_common_pm_test_assertions() {
    let assertions = parse_response_assertions(
        r#"pm.test("status is 201", () => { pm.response.to.have.status(201); })
pm.test('content type', function () { pm.expect(pm.response.headers.get("Content-Type")).to.eql("application/json"); })
pm.test("body has token", () => { pm.expect(pm.response.text()).to.include("token"); })
pm.test("json id", () => { pm.expect(pm.response.json().data.id).to.eql(42); })"#,
    )
    .expect("pm.test assertions");

    assert_eq!(
        assertions,
        vec![
            ResponseAssertion {
                name: "status is 201".to_string(),
                kind: ResponseAssertionKind::StatusEquals { status: 201 },
            },
            ResponseAssertion {
                name: "content type".to_string(),
                kind: ResponseAssertionKind::HeaderEquals {
                    name: "Content-Type".to_string(),
                    value: "application/json".to_string(),
                },
            },
            ResponseAssertion {
                name: "body has token".to_string(),
                kind: ResponseAssertionKind::BodyContains {
                    text: "token".to_string(),
                },
            },
            ResponseAssertion {
                name: "json id".to_string(),
                kind: ResponseAssertionKind::JsonPathEquals {
                    path: "data.id".to_string(),
                    value: Value::from(42),
                },
            },
        ]
    );

    assert!(
        parse_response_assertions(
            r#"pm.test("custom", () => { pm.expect(true).to.equal(true); })"#
        )
        .expect_err("unsupported pm.test")
        .to_string()
        .contains("unsupported pm.test assertion")
    );
}

#[test]
fn parses_additional_pm_assertions() {
    let assertions = parse_response_assertions(
        r#"pm.test("success range", () => { pm.response.to.be.success; })
pm.test("status within", () => { pm.expect(pm.response.code).to.be.within(200, 299); })
pm.test("response time", () => { pm.expect(pm.response.responseTime).to.be.below(500); })
pm.test("response size", () => { pm.expect(pm.response.responseSize).to.be.at.most(65536); })
pm.test("header exists", () => { pm.response.to.have.header("X-Request-Id"); })
pm.test("header has", () => { pm.expect(pm.response.headers.has("Content-Type")).to.be.true; })
pm.test("header absent", () => { pm.expect(pm.response.headers.has("X-Debug")).to.be.false; })
pm.test("body exact", () => { pm.expect(pm.response.text()).to.eql("ready"); })
pm.test("body string", () => { pm.expect(pm.response.text()).to.have.string("ready"); })
pm.test("body excludes", () => { pm.expect(pm.response.text()).to.not.include("error"); })
pm.test("json bracket", () => { pm.expect(pm.response.json().data[0]["id"]).to.eql(42); })
pm.test("json property", () => { pm.expect(pm.response.json().data).to.have.property("name", "Zen"); })
pm.test("json property exists", () => { pm.expect(pm.response.json().data).to.have.property("name"); })
pm.test("json root type", () => { pm.expect(pm.response.json()).to.be.an("object"); })
pm.test("json array type", () => { pm.expect(pm.response.json().data.items).to.be.an("array"); })
pm.test("json length", () => { pm.expect(pm.response.json().data.items).to.have.lengthOf(2); })
pm.test("json length property", () => { pm.expect(pm.response.json().data.items.length).to.eql(2); })
pm.test("json contains", () => { pm.expect(pm.response.json().data.items).to.deep.include({"id":1}); })
pm.test("json not contains", () => { pm.expect(pm.response.json().data.items).to.not.deep.include({"id":999}); })
pm.test("json string contains", () => { pm.expect(pm.response.json().message).to.contain("ready"); })
pm.test("json exists", () => { pm.expect(pm.response.json().data.id).to.exist; })
pm.test("json missing", () => { pm.expect(pm.response.json().error).to.not.exist; })
pm.test("json undefined", () => { pm.expect(pm.response.json().debug).to.be.undefined; })
pm.test("json not equal", () => { pm.expect(pm.response.json().ok).to.not.eql(false); })
pm.test("json bool", () => { pm.expect(pm.response.json().ok).to.be.true; })
pm.test("json null", () => { pm.expect(pm.response.json().error).to.be.null; })"#,
    )
    .expect("additional pm assertions");

    assert_eq!(
        assertions,
        vec![
            ResponseAssertion {
                name: "success range".to_string(),
                kind: ResponseAssertionKind::StatusInRange { min: 200, max: 299 },
            },
            ResponseAssertion {
                name: "status within".to_string(),
                kind: ResponseAssertionKind::StatusInRange { min: 200, max: 299 },
            },
            ResponseAssertion {
                name: "response time".to_string(),
                kind: ResponseAssertionKind::ResponseTimeBelow { max_ms: 500 },
            },
            ResponseAssertion {
                name: "response size".to_string(),
                kind: ResponseAssertionKind::ResponseSizeBelow { max_bytes: 65536 },
            },
            ResponseAssertion {
                name: "header exists".to_string(),
                kind: ResponseAssertionKind::HeaderExists {
                    name: "X-Request-Id".to_string(),
                },
            },
            ResponseAssertion {
                name: "header has".to_string(),
                kind: ResponseAssertionKind::HeaderExists {
                    name: "Content-Type".to_string(),
                },
            },
            ResponseAssertion {
                name: "header absent".to_string(),
                kind: ResponseAssertionKind::HeaderNotExists {
                    name: "X-Debug".to_string(),
                },
            },
            ResponseAssertion {
                name: "body exact".to_string(),
                kind: ResponseAssertionKind::BodyEquals {
                    text: "ready".to_string(),
                },
            },
            ResponseAssertion {
                name: "body string".to_string(),
                kind: ResponseAssertionKind::BodyContains {
                    text: "ready".to_string(),
                },
            },
            ResponseAssertion {
                name: "body excludes".to_string(),
                kind: ResponseAssertionKind::BodyNotContains {
                    text: "error".to_string(),
                },
            },
            ResponseAssertion {
                name: "json bracket".to_string(),
                kind: ResponseAssertionKind::JsonPathEquals {
                    path: "data.0.id".to_string(),
                    value: Value::from(42),
                },
            },
            ResponseAssertion {
                name: "json property".to_string(),
                kind: ResponseAssertionKind::JsonPathEquals {
                    path: "data.name".to_string(),
                    value: Value::from("Zen"),
                },
            },
            ResponseAssertion {
                name: "json property exists".to_string(),
                kind: ResponseAssertionKind::JsonPathExists {
                    path: "data.name".to_string(),
                },
            },
            ResponseAssertion {
                name: "json root type".to_string(),
                kind: ResponseAssertionKind::JsonPathType {
                    path: String::new(),
                    value_type: "object".to_string(),
                },
            },
            ResponseAssertion {
                name: "json array type".to_string(),
                kind: ResponseAssertionKind::JsonPathType {
                    path: "data.items".to_string(),
                    value_type: "array".to_string(),
                },
            },
            ResponseAssertion {
                name: "json length".to_string(),
                kind: ResponseAssertionKind::JsonPathLength {
                    path: "data.items".to_string(),
                    length: 2,
                },
            },
            ResponseAssertion {
                name: "json length property".to_string(),
                kind: ResponseAssertionKind::JsonPathLength {
                    path: "data.items".to_string(),
                    length: 2,
                },
            },
            ResponseAssertion {
                name: "json contains".to_string(),
                kind: ResponseAssertionKind::JsonPathContains {
                    path: "data.items".to_string(),
                    value: serde_json::json!({ "id": 1 }),
                },
            },
            ResponseAssertion {
                name: "json not contains".to_string(),
                kind: ResponseAssertionKind::JsonPathNotContains {
                    path: "data.items".to_string(),
                    value: serde_json::json!({ "id": 999 }),
                },
            },
            ResponseAssertion {
                name: "json string contains".to_string(),
                kind: ResponseAssertionKind::JsonPathContains {
                    path: "message".to_string(),
                    value: Value::from("ready"),
                },
            },
            ResponseAssertion {
                name: "json exists".to_string(),
                kind: ResponseAssertionKind::JsonPathExists {
                    path: "data.id".to_string(),
                },
            },
            ResponseAssertion {
                name: "json missing".to_string(),
                kind: ResponseAssertionKind::JsonPathNotExists {
                    path: "error".to_string(),
                },
            },
            ResponseAssertion {
                name: "json undefined".to_string(),
                kind: ResponseAssertionKind::JsonPathNotExists {
                    path: "debug".to_string(),
                },
            },
            ResponseAssertion {
                name: "json not equal".to_string(),
                kind: ResponseAssertionKind::JsonPathNotEquals {
                    path: "ok".to_string(),
                    value: Value::Bool(false),
                },
            },
            ResponseAssertion {
                name: "json bool".to_string(),
                kind: ResponseAssertionKind::JsonPathEquals {
                    path: "ok".to_string(),
                    value: Value::Bool(true),
                },
            },
            ResponseAssertion {
                name: "json null".to_string(),
                kind: ResponseAssertionKind::JsonPathEquals {
                    path: "error".to_string(),
                    value: Value::Null,
                },
            },
        ]
    );
}

#[test]
fn parses_pm_json_alias_assertions() {
    let assertions = parse_response_assertions(
        r#"pm.test("json alias equals", () => { const jsonData = pm.response.json(); pm.expect(jsonData.data.id).to.eql(42); })
pm.test("json alias property", () => { let payload = pm.response.json(); pm.expect(payload.data).to.have.property("name", "Zen"); })
pm.test("json alias bool", () => { var body = pm.response.json(); pm.expect(body.ok).to.be.true; })
pm.test("json alias root property", () => { const json = pm.response.json(); pm.expect(json).to.have.property("count", 3); })
pm.test("json alias exists", () => { const json = pm.response.json(); pm.expect(json.data.id).to.not.be.undefined; })
pm.test("json alias type", () => { const json = pm.response.json(); pm.expect(json.data.items).to.be.a("array"); })
pm.test("json alias length", () => { const json = pm.response.json(); pm.expect(json.data.items).to.have.length(2); })
pm.test("json alias contains", () => { const json = pm.response.json(); pm.expect(json.tags).to.include("stable"); })"#,
    )
    .expect("pm json alias assertions");

    assert_eq!(
        assertions,
        vec![
            ResponseAssertion {
                name: "json alias equals".to_string(),
                kind: ResponseAssertionKind::JsonPathEquals {
                    path: "data.id".to_string(),
                    value: Value::from(42),
                },
            },
            ResponseAssertion {
                name: "json alias property".to_string(),
                kind: ResponseAssertionKind::JsonPathEquals {
                    path: "data.name".to_string(),
                    value: Value::from("Zen"),
                },
            },
            ResponseAssertion {
                name: "json alias bool".to_string(),
                kind: ResponseAssertionKind::JsonPathEquals {
                    path: "ok".to_string(),
                    value: Value::Bool(true),
                },
            },
            ResponseAssertion {
                name: "json alias root property".to_string(),
                kind: ResponseAssertionKind::JsonPathEquals {
                    path: "count".to_string(),
                    value: Value::from(3),
                },
            },
            ResponseAssertion {
                name: "json alias exists".to_string(),
                kind: ResponseAssertionKind::JsonPathExists {
                    path: "data.id".to_string(),
                },
            },
            ResponseAssertion {
                name: "json alias type".to_string(),
                kind: ResponseAssertionKind::JsonPathType {
                    path: "data.items".to_string(),
                    value_type: "array".to_string(),
                },
            },
            ResponseAssertion {
                name: "json alias length".to_string(),
                kind: ResponseAssertionKind::JsonPathLength {
                    path: "data.items".to_string(),
                    length: 2,
                },
            },
            ResponseAssertion {
                name: "json alias contains".to_string(),
                kind: ResponseAssertionKind::JsonPathContains {
                    path: "tags".to_string(),
                    value: Value::from("stable"),
                },
            },
        ]
    );
}

#[test]
fn formats_single_response_assertion_results() {
    let results = vec![
        ResponseAssertionResult {
            name: "status_equals 200".to_string(),
            passed: true,
            error: None,
        },
        ResponseAssertionResult {
            name: "body_contains ok".to_string(),
            passed: false,
            error: Some("response body does not contain \"ok\"".to_string()),
        },
    ];

    assert_eq!(
        response_status_with_assertions(200, &results),
        "HTTP 200 / tests 1/2"
    );
    assert_eq!(response_tone_with_assertions(200, &results), "error");
    assert_eq!(
        response_body_with_assertions("{}", &results),
        "{}\n\nTests\n[PASS] status_equals 200\n[FAIL] body_contains ok - response body does not contain \"ok\""
    );
}

#[test]
fn saves_editor_request_with_pre_request_and_tests() {
    let tests = parse_response_assertions("status_equals 201").expect("assertions");
    let request = collection_request_from_editor(
        &RequestProjectionInput {
            method: "POST".to_string(),
            url: "{{baseUrl}}/users".to_string(),
            query_params: "debug=true".to_string(),
            headers: "Accept: application/json".to_string(),
            auth_mode: "none".to_string(),
            auth_config: String::new(),
            body_mode: "raw".to_string(),
            raw_body_subtype: "text".to_string(),
            body: "{\"name\":\"{{name}}\"}".to_string(),
            graphql_variables: String::new(),
            pre_request_script: "set_header X-Mode=test".to_string(),
            global_variables: String::new(),
            environment_name: String::new(),
            environment_variables: String::new(),
        },
        tests.clone(),
    )
    .expect("collection request");

    assert_eq!(request.url, "{{baseUrl}}/users");
    assert_eq!(request.pre_request_script, "set_header X-Mode=test");
    assert_eq!(request.tests, tests);
    assert_eq!(
        request.body,
        CollectionBody::Raw {
            content_type: "text/plain".to_string(),
            body: "{\"name\":\"{{name}}\"}".to_string(),
        }
    );
}

#[test]
fn maps_slint_codegen_language_names() {
    assert_eq!(snippet_language("curl"), SnippetLanguage::Curl);
    assert_eq!(snippet_language("python"), SnippetLanguage::PythonRequests);
    assert_eq!(snippet_language("js"), SnippetLanguage::JavaScriptFetch);
    assert_eq!(snippet_language("rust"), SnippetLanguage::RustReqwest);
    assert_eq!(snippet_language("go"), SnippetLanguage::GoNetHttp);
    assert_eq!(snippet_language("unknown"), SnippetLanguage::Curl);
}

#[test]
fn formats_codegen_metadata_for_generated_snippets() {
    let request = CodegenRequest {
        method: "POST".to_string(),
        url: "https://api.example.com/users".to_string(),
        headers: vec![("Accept".to_string(), "application/json".to_string())],
        query_params: vec![("debug".to_string(), "true".to_string())],
        body: RequestBody::Raw {
            content_type: Some("application/json".to_string()),
            body: "{\"ok\":true}".to_string(),
        },
    };

    let metadata = codegen_metadata(
        &request,
        codegen_language_label(SnippetLanguage::Curl),
        "a\nb",
    );

    assert!(metadata.contains("Language: cURL"));
    assert!(metadata.contains("Request: POST https://api.example.com/users"));
    assert!(metadata.contains("Lines: 2 / Bytes: 3"));
    assert!(metadata.contains("Headers: 1 / Query params: 1 / Body: raw"));
}

#[test]
fn saves_codegen_snippet_to_disk() {
    let path =
        std::env::temp_dir().join(format!("zenapi-codegen-snippet-{}.txt", std::process::id()));
    let _ = fs::remove_file(&path);

    save_codegen_snippet(path.to_str().expect("utf-8 temp path"), "curl example")
        .expect("save snippet");

    assert_eq!(
        fs::read_to_string(&path).expect("saved snippet"),
        "curl example"
    );
    assert!(save_codegen_snippet("   ", "curl example").is_err());

    let _ = fs::remove_file(path);
}

#[test]
fn saves_projected_request_as_collection_request() {
    let request = CodegenRequest {
        method: "POST".to_string(),
        url: "https://api.example.com/users".to_string(),
        headers: vec![("Authorization".to_string(), "Bearer secret".to_string())],
        query_params: vec![("debug".to_string(), "true".to_string())],
        body: RequestBody::Raw {
            content_type: Some("application/json".to_string()),
            body: "{\"name\":\"Zen\"}".to_string(),
        },
    };

    let saved = collection_request_from_codegen(&request);

    assert_eq!(saved.name, "POST https://api.example.com/users");
    assert_eq!(saved.method, "POST");
    assert_eq!(saved.url, "https://api.example.com/users");
    assert_eq!(
        saved.headers,
        vec![NameValue {
            name: "Authorization".to_string(),
            value: "Bearer secret".to_string(),
        }]
    );
    assert_eq!(
        saved.query_params,
        vec![NameValue {
            name: "debug".to_string(),
            value: "true".to_string(),
        }]
    );
    assert_eq!(
        saved.body,
        CollectionBody::Raw {
            content_type: "application/json".to_string(),
            body: "{\"name\":\"Zen\"}".to_string(),
        }
    );
}

#[test]
fn finds_nested_collection_requests_by_flattened_row_id() {
    let collection = ApiCollection {
        name: "Demo".to_string(),
        description: String::new(),
        items: vec![
            CollectionItem::Folder(zenapi::collections::CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: vec![
                    CollectionItem::Request(saved_request(
                        "List users",
                        "GET",
                        "https://api.example.com/users",
                    )),
                    CollectionItem::Request(saved_request(
                        "Create user",
                        "POST",
                        "https://api.example.com/users",
                    )),
                ],
            }),
            CollectionItem::Request(saved_request(
                "Health",
                "GET",
                "https://api.example.com/health",
            )),
        ],
    };

    assert_eq!(count_collection_requests(&collection.items), 3);
    assert_eq!(
        collection_request_at(&collection, 0).map(|request| request.name.as_str()),
        Some("List users")
    );
    assert_eq!(
        collection_request_at(&collection, 1).map(|request| request.name.as_str()),
        Some("Create user")
    );
    assert_eq!(
        collection_request_at(&collection, 2).map(|request| request.name.as_str()),
        Some("Health")
    );
    assert!(collection_request_at(&collection, 3).is_none());
}

#[test]
fn collection_model_includes_folder_rows_without_request_ids() {
    use slint::Model;

    let collection = ApiCollection {
        name: "Demo".to_string(),
        description: String::new(),
        items: vec![
            CollectionItem::Folder(zenapi::collections::CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: vec![
                    CollectionItem::Request(saved_request(
                        "List users",
                        "GET",
                        "https://api.example.com/users",
                    )),
                    CollectionItem::Folder(zenapi::collections::CollectionFolder {
                        name: "Admin".to_string(),
                        description: String::new(),
                        items: vec![CollectionItem::Request(saved_request(
                            "Suspend user",
                            "POST",
                            "https://api.example.com/users/suspend",
                        ))],
                    }),
                ],
            }),
            CollectionItem::Request(saved_request(
                "Health",
                "GET",
                "https://api.example.com/health",
            )),
        ],
    };

    let rows = collection_model(&collection);

    assert_eq!(rows.row_count(), 5);

    let users = rows.row_data(0).expect("users folder row");
    assert!(users.is_folder);
    assert_eq!(users.id, -1);
    assert_eq!(users.name.as_str(), "Users");
    assert_eq!(users.folder_path.as_str(), r#"["Users"]"#);

    let list = rows.row_data(1).expect("list request row");
    assert!(!list.is_folder);
    assert_eq!(list.id, 0);
    assert_eq!(list.name.as_str(), "  List users");

    let admin = rows.row_data(2).expect("admin folder row");
    assert!(admin.is_folder);
    assert_eq!(admin.id, -1);
    assert_eq!(admin.name.as_str(), "  Admin");
    assert_eq!(admin.folder_path.as_str(), r#"["Users","Admin"]"#);

    let suspend = rows.row_data(3).expect("nested request row");
    assert!(!suspend.is_folder);
    assert_eq!(suspend.id, 1);
    assert_eq!(suspend.name.as_str(), "    Suspend user");

    let health = rows.row_data(4).expect("root request row");
    assert!(!health.is_folder);
    assert_eq!(health.id, 2);
    assert_eq!(health.name.as_str(), "Health");
    assert_eq!(health.folder_path.as_str(), "");
}

#[test]
fn adds_root_collection_folder() {
    use slint::Model;

    let mut collection = ApiCollection::new("Demo");

    assert!(add_collection_folder_in(&mut collection, "", "   ").is_none());
    assert_eq!(
        add_collection_folder_in(&mut collection, "", " Admin "),
        Some("Admin".to_string())
    );
    assert_eq!(collection.items.len(), 1);

    let rows = collection_model(&collection);
    assert_eq!(rows.row_count(), 1);
    let folder = rows.row_data(0).expect("folder row");
    assert!(folder.is_folder);
    assert_eq!(folder.id, -1);
    assert_eq!(folder.name.as_str(), "Admin");
    assert_eq!(folder.folder_path.as_str(), r#"["Admin"]"#);
}

#[test]
fn adds_nested_collection_folder_under_selected_path() {
    use slint::Model;

    let mut collection = ApiCollection {
        name: "Demo".to_string(),
        description: String::new(),
        items: vec![CollectionItem::Folder(
            zenapi::collections::CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: vec![CollectionItem::Folder(
                    zenapi::collections::CollectionFolder {
                        name: "Admin".to_string(),
                        description: String::new(),
                        items: Vec::new(),
                    },
                )],
            },
        )],
    };

    assert_eq!(
        add_collection_folder_in(&mut collection, r#"["Users","Admin"]"#, "Audit"),
        Some("Audit".to_string())
    );
    assert!(add_collection_folder_in(&mut collection, r#"["Missing"]"#, "Nope").is_none());

    let rows = collection_model(&collection);
    assert_eq!(rows.row_count(), 3);

    let audit = rows.row_data(2).expect("audit folder row");
    assert!(audit.is_folder);
    assert_eq!(audit.name.as_str(), "    Audit");
    assert_eq!(audit.folder_path.as_str(), r#"["Users","Admin","Audit"]"#);
    assert_eq!(
        collection_folder_label(r#"["Users","Admin"]"#),
        "Users / Admin"
    );
}

#[test]
fn renames_collection_folders_by_path() {
    use slint::Model;

    let mut collection = ApiCollection {
        name: "Demo".to_string(),
        description: String::new(),
        items: vec![CollectionItem::Folder(CollectionFolder {
            name: "Users".to_string(),
            description: String::new(),
            items: vec![CollectionItem::Folder(CollectionFolder {
                name: "Admin".to_string(),
                description: String::new(),
                items: vec![CollectionItem::Request(saved_request(
                    "Suspend user",
                    "POST",
                    "https://api.example.com/users/suspend",
                ))],
            })],
        })],
    };

    assert_eq!(
        rename_collection_folder_at(&mut collection, r#"["Users","Admin"]"#, "Ops"),
        Some(("Ops".to_string(), r#"["Users","Ops"]"#.to_string()))
    );
    assert!(rename_collection_folder_at(&mut collection, "", "Root").is_none());
    assert!(rename_collection_folder_at(&mut collection, r#"["Missing"]"#, "Nope").is_none());
    assert!(rename_collection_folder_at(&mut collection, r#"["Users","Ops"]"#, "   ").is_none());

    let rows = collection_model(&collection);
    let folder = rows.row_data(1).expect("renamed folder row");
    assert!(folder.is_folder);
    assert_eq!(folder.name.as_str(), "  Ops");
    assert_eq!(folder.folder_path.as_str(), r#"["Users","Ops"]"#);
    assert_eq!(
        collection_request_at(&collection, 0).map(|request| request.name.as_str()),
        Some("Suspend user")
    );
}

#[test]
fn removes_collection_folders_by_path() {
    let mut collection = ApiCollection {
        name: "Demo".to_string(),
        description: String::new(),
        items: vec![
            CollectionItem::Folder(CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: vec![CollectionItem::Folder(CollectionFolder {
                    name: "Admin".to_string(),
                    description: String::new(),
                    items: vec![CollectionItem::Request(saved_request(
                        "Suspend user",
                        "POST",
                        "https://api.example.com/users/suspend",
                    ))],
                })],
            }),
            CollectionItem::Request(saved_request(
                "Health",
                "GET",
                "https://api.example.com/health",
            )),
        ],
    };

    let removed = remove_collection_folder_at(&mut collection, r#"["Users","Admin"]"#)
        .expect("removed folder");

    assert_eq!(removed.name, "Admin");
    assert_eq!(count_collection_requests(&removed.items), 1);
    assert_eq!(count_collection_requests(&collection.items), 1);
    assert!(remove_collection_folder_at(&mut collection, "").is_none());
    assert!(remove_collection_folder_at(&mut collection, r#"["Missing"]"#).is_none());
    assert_eq!(
        collection_request_at(&collection, 0).map(|request| request.name.as_str()),
        Some("Health")
    );
}

#[test]
fn reorders_collection_requests_with_flattened_ids() {
    use slint::Model;

    let mut collection = ApiCollection {
        name: "Demo".to_string(),
        description: String::new(),
        items: vec![
            CollectionItem::Folder(CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: vec![CollectionItem::Request(saved_request(
                    "List users",
                    "GET",
                    "https://api.example.com/users",
                ))],
            }),
            CollectionItem::Request(saved_request(
                "Health",
                "GET",
                "https://api.example.com/health",
            )),
        ],
    };

    let (moved, new_id) =
        reorder_collection_request_at(&mut collection, 1, -1).expect("move over folder");

    assert_eq!(moved.name, "Health");
    assert_eq!(new_id, 0);
    assert_eq!(
        collection_request_at(&collection, 0).map(|request| request.name.as_str()),
        Some("Health")
    );
    assert_eq!(
        collection_request_at(&collection, 1).map(|request| request.name.as_str()),
        Some("List users")
    );

    let rows = collection_model(&collection);
    let health = rows.row_data(0).expect("health request row");
    assert!(!health.is_folder);
    assert_eq!(health.id, 0);
    assert_eq!(health.name.as_str(), "Health");

    let (moved, new_id) =
        reorder_collection_request_at(&mut collection, 0, 1).expect("move below folder");
    assert_eq!(moved.name, "Health");
    assert_eq!(new_id, 1);
    assert!(reorder_collection_request_at(&mut collection, 1, 1).is_none());
}

#[test]
fn reorders_collection_folders_within_parent() {
    use slint::Model;

    let mut collection = ApiCollection {
        name: "Demo".to_string(),
        description: String::new(),
        items: vec![CollectionItem::Folder(CollectionFolder {
            name: "Users".to_string(),
            description: String::new(),
            items: vec![
                CollectionItem::Folder(CollectionFolder {
                    name: "Admin".to_string(),
                    description: String::new(),
                    items: Vec::new(),
                }),
                CollectionItem::Folder(CollectionFolder {
                    name: "Audit".to_string(),
                    description: String::new(),
                    items: Vec::new(),
                }),
            ],
        })],
    };

    assert_eq!(
        reorder_collection_folder_at(&mut collection, r#"["Users","Audit"]"#, -1),
        Some(("Audit".to_string(), r#"["Users","Audit"]"#.to_string()))
    );
    assert!(reorder_collection_folder_at(&mut collection, r#"["Users","Audit"]"#, -1).is_none());

    let rows = collection_model(&collection);
    let audit = rows.row_data(1).expect("audit folder row");
    assert!(audit.is_folder);
    assert_eq!(audit.name.as_str(), "  Audit");
    assert_eq!(audit.folder_path.as_str(), r#"["Users","Audit"]"#);

    let admin = rows.row_data(2).expect("admin folder row");
    assert!(admin.is_folder);
    assert_eq!(admin.name.as_str(), "  Admin");
    assert_eq!(admin.folder_path.as_str(), r#"["Users","Admin"]"#);
}

#[test]
fn saves_collection_requests_under_selected_folder() {
    let mut collection = ApiCollection {
        name: "Demo".to_string(),
        description: String::new(),
        items: vec![
            CollectionItem::Folder(zenapi::collections::CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: vec![CollectionItem::Request(saved_request(
                    "List users",
                    "GET",
                    "https://api.example.com/users",
                ))],
            }),
            CollectionItem::Request(saved_request(
                "Health",
                "GET",
                "https://api.example.com/health",
            )),
        ],
    };

    let nested_id = add_collection_request_in(
        &mut collection,
        r#"["Users"]"#,
        saved_request("Create user", "POST", "https://api.example.com/users"),
    )
    .expect("nested save");

    assert_eq!(nested_id, 1);
    assert_eq!(
        collection_request_at(&collection, 0).map(|request| request.name.as_str()),
        Some("List users")
    );
    assert_eq!(
        collection_request_at(&collection, 1).map(|request| request.name.as_str()),
        Some("Create user")
    );
    assert_eq!(
        collection_request_at(&collection, 2).map(|request| request.name.as_str()),
        Some("Health")
    );

    let root_id =
        add_collection_request_in(&mut collection, "", saved_request("Ping", "GET", "/ping"))
            .expect("root save");

    assert_eq!(root_id, 3);
    assert_eq!(
        collection_request_at(&collection, 3).map(|request| request.name.as_str()),
        Some("Ping")
    );
    assert!(
        add_collection_request_in(
            &mut collection,
            r#"["Missing"]"#,
            saved_request("Nope", "GET", "/nope"),
        )
        .is_none()
    );
    assert_eq!(count_collection_requests(&collection.items), 4);
}

#[test]
fn removes_nested_collection_requests_by_flattened_row_id() {
    let mut collection = ApiCollection {
        name: "Demo".to_string(),
        description: String::new(),
        items: vec![
            CollectionItem::Folder(zenapi::collections::CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: vec![
                    CollectionItem::Request(saved_request(
                        "List users",
                        "GET",
                        "https://api.example.com/users",
                    )),
                    CollectionItem::Request(saved_request(
                        "Create user",
                        "POST",
                        "https://api.example.com/users",
                    )),
                ],
            }),
            CollectionItem::Request(saved_request(
                "Health",
                "GET",
                "https://api.example.com/health",
            )),
        ],
    };

    let removed = remove_collection_request_at(&mut collection, 1).expect("removed request");

    assert_eq!(removed.name, "Create user");
    assert_eq!(count_collection_requests(&collection.items), 2);
    assert_eq!(
        collection_request_at(&collection, 1).map(|request| request.name.as_str()),
        Some("Health")
    );
    assert!(remove_collection_request_at(&mut collection, 5).is_none());
    assert_eq!(count_collection_requests(&collection.items), 2);
}

#[test]
fn moves_collection_requests_between_root_and_folders() {
    let mut collection = ApiCollection {
        name: "Demo".to_string(),
        description: String::new(),
        items: vec![
            CollectionItem::Folder(zenapi::collections::CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: vec![CollectionItem::Folder(
                    zenapi::collections::CollectionFolder {
                        name: "Admin".to_string(),
                        description: String::new(),
                        items: vec![CollectionItem::Request(saved_request(
                            "Suspend user",
                            "POST",
                            "https://api.example.com/users/suspend",
                        ))],
                    },
                )],
            }),
            CollectionItem::Request(saved_request(
                "Health",
                "GET",
                "https://api.example.com/health",
            )),
        ],
    };

    let moved = move_collection_request_to_folder(&mut collection, 1, r#"["Users","Admin"]"#)
        .expect("move root request into folder");
    assert_eq!(moved.name, "Health");
    assert_eq!(count_collection_requests(&collection.items), 2);
    assert_eq!(
        collection_request_at(&collection, 0).map(|request| request.name.as_str()),
        Some("Suspend user")
    );
    assert_eq!(
        collection_request_at(&collection, 1).map(|request| request.name.as_str()),
        Some("Health")
    );

    let moved = move_collection_request_to_folder(&mut collection, 0, "")
        .expect("move nested request to root");
    assert_eq!(moved.name, "Suspend user");
    assert_eq!(
        collection_request_at(&collection, 0).map(|request| request.name.as_str()),
        Some("Health")
    );
    assert_eq!(
        collection_request_at(&collection, 1).map(|request| request.name.as_str()),
        Some("Suspend user")
    );
    assert!(move_collection_request_to_folder(&mut collection, 1, r#"["Missing"]"#).is_none());
    assert_eq!(count_collection_requests(&collection.items), 2);
}

#[test]
fn duplicates_nested_collection_requests_by_flattened_row_id() {
    let mut collection = ApiCollection {
        name: "Demo".to_string(),
        description: String::new(),
        items: vec![
            CollectionItem::Folder(zenapi::collections::CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: vec![
                    CollectionItem::Request(saved_request(
                        "List users",
                        "GET",
                        "https://api.example.com/users",
                    )),
                    CollectionItem::Request(saved_request(
                        "Create user",
                        "POST",
                        "https://api.example.com/users",
                    )),
                ],
            }),
            CollectionItem::Request(saved_request(
                "Health",
                "GET",
                "https://api.example.com/health",
            )),
        ],
    };

    let duplicate =
        duplicate_collection_request_at(&mut collection, 1).expect("duplicated request");

    assert_eq!(duplicate.name, "Create user Copy");
    assert_eq!(count_collection_requests(&collection.items), 4);
    assert_eq!(
        collection_request_at(&collection, 2).map(|request| request.name.as_str()),
        Some("Create user Copy")
    );
    assert_eq!(
        collection_request_at(&collection, 3).map(|request| request.name.as_str()),
        Some("Health")
    );
    assert!(duplicate_collection_request_at(&mut collection, 9).is_none());
}

#[test]
fn renames_nested_collection_requests_by_flattened_row_id() {
    let mut collection = ApiCollection {
        name: "Demo".to_string(),
        description: String::new(),
        items: vec![
            CollectionItem::Folder(zenapi::collections::CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: vec![
                    CollectionItem::Request(saved_request(
                        "List users",
                        "GET",
                        "https://api.example.com/users",
                    )),
                    CollectionItem::Request(saved_request(
                        "Create user",
                        "POST",
                        "https://api.example.com/users",
                    )),
                ],
            }),
            CollectionItem::Request(saved_request(
                "Health",
                "GET",
                "https://api.example.com/health",
            )),
        ],
    };

    let renamed = rename_collection_request_at(&mut collection, 1, "Create team").expect("renamed");

    assert_eq!(renamed.name, "Create team");
    assert_eq!(
        collection_request_at(&collection, 1).map(|request| request.name.as_str()),
        Some("Create team")
    );
    assert_eq!(
        collection_request_at(&collection, 2).map(|request| request.name.as_str()),
        Some("Health")
    );
    assert!(rename_collection_request_at(&mut collection, 9, "Missing").is_none());
    assert!(rename_collection_request_at(&mut collection, 1, "   ").is_none());
}

#[test]
fn maps_collection_body_and_fields_back_to_slint_editors() {
    assert_eq!(
        format_name_values(&[
            NameValue {
                name: "Accept".to_string(),
                value: "application/json".to_string(),
            },
            NameValue {
                name: "X-Request-Id".to_string(),
                value: "abc".to_string(),
            },
        ]),
        "Accept=application/json\nX-Request-Id=abc"
    );
    assert_eq!(
        collection_body_to_slint(&CollectionBody::UrlEncoded {
            fields: vec![NameValue {
                name: "search".to_string(),
                value: "slint".to_string(),
            }],
        }),
        (
            "urlenc".to_string(),
            "search=slint".to_string(),
            "json".to_string()
        )
    );
    assert_eq!(
        collection_body_to_slint(&CollectionBody::Raw {
            content_type: "application/xml; charset=utf-8".to_string(),
            body: "<ok/>".to_string(),
        }),
        ("raw".to_string(), "<ok/>".to_string(), "xml".to_string())
    );
    assert_eq!(
        collection_body_to_slint(&CollectionBody::Binary {
            path: "/tmp/body.bin".to_string(),
            content_type: "application/octet-stream".to_string(),
        }),
        (
            "binary".to_string(),
            "/tmp/body.bin".to_string(),
            "json".to_string()
        )
    );
}

#[test]
fn parses_runner_options_from_slint_controls() {
    assert_eq!(
        runner_options("25", true).unwrap(),
        RunnerOptions {
            delay_ms: 25,
            failure_strategy: FailureStrategy::StopOnFailure,
        }
    );
    assert_eq!(
        runner_options(" ", false).unwrap(),
        RunnerOptions {
            delay_ms: 0,
            failure_strategy: FailureStrategy::Continue,
        }
    );

    let error = runner_options("soon", false).expect_err("invalid delay");
    assert!(error.to_string().contains("non-negative integer"));
}

#[test]
fn formats_runner_summary_for_response_panel() {
    let result = CollectionRunResult {
        index: 0,
        path: vec!["Demo".to_string(), "Health".to_string()],
        name: "Health".to_string(),
        method: "GET".to_string(),
        url: "https://api.example.com/health".to_string(),
        status: Some(200),
        success: true,
        elapsed_ms: 12,
        body_bytes: 42,
        pre_request_actions: Vec::new(),
        assertions: Vec::new(),
        error: None,
    };
    let summary = CollectionRunSummary {
        collection_name: "Demo".to_string(),
        total: 1,
        passed: 1,
        failed: 0,
        stopped_early: false,
        elapsed_ms: 15,
        results: vec![result],
    };

    assert_eq!(runner_response_tone(&summary), "success");
    assert_eq!(runner_response_status(&summary), "Runner passed");
    assert_eq!(
        runner_summary_line(&summary),
        "Demo: 1 passed, 0 failed, 1 total / 15 ms"
    );
    assert_eq!(
        format_runner_summary(&summary),
        "Demo: 1 passed, 0 failed, 1 total / 15 ms\n[PASS] HTTP 200 GET https://api.example.com/health (Demo / Health)"
    );
}

#[test]
fn saves_runner_report_formats_to_disk() {
    let summary = CollectionRunSummary {
        collection_name: "Demo".to_string(),
        total: 1,
        passed: 1,
        failed: 0,
        stopped_early: false,
        elapsed_ms: 15,
        results: vec![CollectionRunResult {
            index: 0,
            path: vec!["Demo".to_string(), "Health".to_string()],
            name: "Health".to_string(),
            method: "GET".to_string(),
            url: "https://api.example.com/health".to_string(),
            status: Some(200),
            success: true,
            elapsed_ms: 12,
            body_bytes: 42,
            pre_request_actions: Vec::new(),
            assertions: Vec::new(),
            error: None,
        }],
    };
    let text_path =
        std::env::temp_dir().join(format!("zenapi-runner-report-{}.txt", std::process::id()));
    let json_path =
        std::env::temp_dir().join(format!("zenapi-runner-report-{}.json", std::process::id()));
    let _ = fs::remove_file(&text_path);
    let _ = fs::remove_file(&json_path);

    save_runner_report(
        text_path.to_str().expect("utf-8 temp path"),
        &summary,
        "text",
    )
    .expect("save runner report");

    assert_eq!(
        fs::read_to_string(&text_path).expect("runner report"),
        "Demo: 1 passed, 0 failed, 1 total / 15 ms\n[PASS] HTTP 200 GET https://api.example.com/health (Demo / Health)"
    );

    save_runner_report(
        json_path.to_str().expect("utf-8 temp path"),
        &summary,
        "JSON",
    )
    .expect("save json runner report");
    let parsed: CollectionRunSummary =
        serde_json::from_str(&fs::read_to_string(&json_path).expect("json runner report"))
            .expect("parse runner json");
    assert_eq!(parsed, summary);

    assert_eq!(normalize_runner_report_format(" json "), "json");
    assert_eq!(normalize_runner_report_format("csv"), "text");
    assert!(save_runner_report("   ", &summary, "json").is_err());
    let _ = fs::remove_file(text_path);
    let _ = fs::remove_file(json_path);
}

#[test]
fn formats_failed_runner_result_details() {
    let result = CollectionRunResult {
        index: 0,
        path: vec!["Demo".to_string(), "Create".to_string()],
        name: "Create".to_string(),
        method: "POST".to_string(),
        url: "https://api.example.com/users".to_string(),
        status: None,
        success: false,
        elapsed_ms: 0,
        body_bytes: 0,
        pre_request_actions: vec!["set header X-Debug".to_string()],
        assertions: Vec::new(),
        error: Some("connection refused".to_string()),
    };

    assert_eq!(runner_result_status(&result), "FAIL");
    assert_eq!(
        runner_result_detail(&result),
        "ERR / 0 ms / 0 B / pre 1 / connection refused"
    );
    assert_eq!(
        format_runner_result(&result),
        "[FAIL] ERR POST https://api.example.com/users (Demo / Create) - connection refused - pre-request 1"
    );
}

#[test]
fn parses_realtime_event_limits() {
    assert_eq!(parse_positive_usize("3", "SSE events").unwrap(), 3);
    assert!(
        parse_positive_usize("0", "SSE events")
            .expect_err("zero")
            .to_string()
            .contains("greater than zero")
    );
    assert!(
        parse_positive_usize("many", "SSE events")
            .expect_err("invalid")
            .to_string()
            .contains("positive integer")
    );
}

#[test]
fn parses_websocket_protocols_and_binary_messages() {
    assert_eq!(
        parse_websocket_protocols("chat, superchat\ntrace"),
        vec![
            "chat".to_string(),
            "superchat".to_string(),
            "trace".to_string()
        ]
    );
    assert_eq!(
        parse_websocket_binary_message("00 01 ff,0x10").unwrap(),
        vec![0, 1, 255, 16]
    );
    assert!(parse_websocket_binary_message("").is_err());
    assert!(parse_websocket_binary_message("100").is_err());
    assert_eq!(
        websocket_session_command("text", "hello").unwrap(),
        client::WebSocketSessionCommand::SendText("hello".to_string())
    );
    assert_eq!(
        websocket_session_command("binary", "0a ff").unwrap(),
        client::WebSocketSessionCommand::SendBinary(vec![10, 255])
    );
}

#[test]
fn formats_websocket_and_sse_output() {
    let websocket = client::WebSocketExchange {
        url: "ws://localhost/socket".to_string(),
        sent: "hello".to_string(),
        received: vec![client::WebSocketMessage {
            kind: client::WebSocketMessageKind::Text,
            data: "echo:hello".to_string(),
        }],
        elapsed_ms: 12,
    };
    assert_eq!(
        format_websocket_exchange(&websocket),
        "URL: ws://localhost/socket\nSent: hello\nReceived 1 [text]: echo:hello"
    );
    let session_events = vec![
        client::WebSocketSessionEvent::Connected {
            url: "ws://localhost/socket".to_string(),
        },
        client::WebSocketSessionEvent::Sent(client::WebSocketMessage {
            kind: client::WebSocketMessageKind::Text,
            data: "hello".to_string(),
        }),
        client::WebSocketSessionEvent::Received(client::WebSocketMessage {
            kind: client::WebSocketMessageKind::Text,
            data: "echo:hello".to_string(),
        }),
    ];
    assert_eq!(
        format_websocket_session_events(&session_events),
        "1. connected ws://localhost/socket\n2. sent [text]: hello\n3. received [text]: echo:hello"
    );
    assert_eq!(
        websocket_session_status(session_events.last().expect("event")),
        "WebSocket received"
    );

    let sse = client::SseExchange {
        url: "http://localhost/events".to_string(),
        events: vec![client::SseEvent {
            event: Some("update".to_string()),
            data: "{\"ok\":true}".to_string(),
            id: Some("42".to_string()),
            retry: None,
        }],
        elapsed_ms: 14,
    };
    assert_eq!(
        format_sse_exchange(&sse),
        "URL: http://localhost/events\n1. update / id 42\n{\"ok\":true}"
    );
    assert_eq!(latest_sse_event_id(&sse.events), Some("42"));

    let stream_events = vec![
        client::SseStreamEvent::Connected {
            url: "http://localhost/events".to_string(),
        },
        client::SseStreamEvent::Event(client::SseEvent {
            event: Some("update".to_string()),
            data: "{\"ok\":true}".to_string(),
            id: Some("42".to_string()),
            retry: None,
        }),
        client::SseStreamEvent::Reconnecting {
            attempt: 1,
            delay_ms: 500,
            reason: "stream ended".to_string(),
        },
        client::SseStreamEvent::Closed("stream ended".to_string()),
    ];
    assert_eq!(
        format_sse_stream_events(&stream_events),
        "1. connected http://localhost/events\n2. update / id 42\n{\"ok\":true}\n3. reconnecting attempt 1 in 500 ms\nstream ended\n4. closed\nstream ended"
    );
    assert_eq!(sse_stream_event_last_id(&stream_events[1]), Some("42"));
    assert_eq!(sse_stream_status(&stream_events[2]), "SSE reconnecting");
    assert_eq!(sse_stream_tone(&stream_events[2]), "busy");
    assert!(sse_stream_event_done(&stream_events[3]));
}

#[test]
fn bounds_sse_stream_event_history() {
    let mut events = Vec::new();
    for index in 0..(MAX_SSE_STREAM_EVENTS + 3) {
        push_bounded_sse_stream_event(
            &mut events,
            client::SseStreamEvent::Event(client::SseEvent {
                event: None,
                data: format!("event {index}"),
                id: Some(index.to_string()),
                retry: None,
            }),
        );
    }

    assert_eq!(events.len(), MAX_SSE_STREAM_EVENTS);
    assert_eq!(sse_stream_event_last_id(&events[0]), Some("3"));
    assert_eq!(sse_stream_meta(events.len()), "latest 200 events");
}

#[test]
fn formats_grpc_draft_for_response_panel() {
    let draft = build_grpc_request_draft(
        "http://localhost:50051",
        "demo.Users/GetUser",
        "authorization=Bearer token",
        r#"{"id":"u_123"}"#,
        "unary demo.Users/GetUser demo.GetUserRequest demo.GetUserResponse",
    )
    .expect("draft");

    assert_eq!(
        format_grpc_draft(&draft),
        "Endpoint: http://localhost:50051\nMethod: /demo.Users/GetUser\n\nDescriptor\nKind: unary\nRequest: demo.GetUserRequest\nResponse: demo.GetUserResponse\n\nMetadata\nauthorization: Bearer token\n\nMessage\n{\n  \"id\": \"u_123\"\n}\n\nCommand\ngrpcurl \\\n  -plaintext \\\n  -H 'authorization: Bearer token' \\\n  -d '{\n  \"id\": \"u_123\"\n}' \\\n  'localhost:50051' \\\n  'demo.Users/GetUser'"
    );
}

#[test]
fn formats_grpcurl_command_for_secure_endpoint_and_quoted_metadata() {
    let draft = build_grpc_request_draft(
        "https://grpc.example.com",
        "demo.Users/GetUser",
        "x-note=owner's token",
        r#"{"name":"Ada"}"#,
        "",
    )
    .expect("draft");
    let message = serde_json::to_string_pretty(&draft.message).expect("message");

    assert_eq!(
        format_grpcurl_command(&draft, &message),
        "grpcurl \\\n  -H 'x-note: owner'\\''s token' \\\n  -d '{\n  \"name\": \"Ada\"\n}' \\\n  'grpc.example.com' \\\n  'demo.Users/GetUser'"
    );
}

#[test]
fn keeps_recent_mock_logs_first_and_bounded() {
    let mut logs = Vec::new();
    push_mock_log(
        &mut logs,
        MockRequestLog {
            method: "GET".to_string(),
            path: "/old".to_string(),
            status: 200,
        },
        2,
    );
    push_mock_log(
        &mut logs,
        MockRequestLog {
            method: "POST".to_string(),
            path: "/new".to_string(),
            status: 404,
        },
        2,
    );
    push_mock_log(
        &mut logs,
        MockRequestLog {
            method: "PUT".to_string(),
            path: "/latest".to_string(),
            status: 204,
        },
        2,
    );

    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].path, "/latest");
    assert_eq!(logs[1].path, "/new");

    assert_eq!(clear_mock_logs(&mut logs), 2);
    assert!(logs.is_empty());
}

#[test]
fn filters_mock_logs_for_sidebar_model() {
    use slint::Model;

    let logs = vec![
        MockRequestLog {
            method: "GET".to_string(),
            path: "/users".to_string(),
            status: 200,
        },
        MockRequestLog {
            method: "POST".to_string(),
            path: "/sessions".to_string(),
            status: 404,
        },
    ];

    let by_path = filtered_mock_log_model(&logs, "sessions");
    assert_eq!(by_path.row_count(), 1);
    assert_eq!(
        by_path.row_data(0).expect("mock log").method.as_str(),
        "POST"
    );

    let by_status = filtered_mock_log_model(&logs, "200");
    assert_eq!(by_status.row_count(), 1);
    assert_eq!(
        by_status.row_data(0).expect("mock log").path.as_str(),
        "/users"
    );
}

#[test]
fn saves_filtered_mock_logs_to_disk() {
    let logs = vec![
        MockRequestLog {
            method: "GET".to_string(),
            path: "/users".to_string(),
            status: 200,
        },
        MockRequestLog {
            method: "POST".to_string(),
            path: "/sessions".to_string(),
            status: 404,
        },
    ];
    let path = std::env::temp_dir().join(format!(
        "zenapi-mock-log-export-{}.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&path);

    let count = save_mock_logs(path.to_str().expect("utf-8 temp path"), &logs, "sessions")
        .expect("save mock logs");
    let exported: Vec<MockRequestLog> =
        serde_json::from_str(&fs::read_to_string(&path).expect("mock log export"))
            .expect("mock log json");

    assert_eq!(count, 1);
    assert_eq!(exported[0].path, "/sessions");
    assert!(save_mock_logs("   ", &logs, "").is_err());

    let _ = fs::remove_file(path);
}

#[test]
fn builds_history_request_from_projected_request() {
    let request = CodegenRequest {
        method: "POST".to_string(),
        url: "https://api.example.com/users".to_string(),
        headers: vec![("Authorization".to_string(), "Bearer token".to_string())],
        query_params: vec![("debug".to_string(), "true".to_string())],
        body: RequestBody::FormUrlEncoded(vec![
            ("name".to_string(), "Zen".to_string()),
            ("role".to_string(), "admin".to_string()),
        ]),
    };
    let input = RequestProjectionInput {
        method: "POST".to_string(),
        url: "{{baseUrl}}/users".to_string(),
        query_params: "debug=true".to_string(),
        headers: String::new(),
        auth_mode: "bearer".to_string(),
        auth_config: "{{token}}".to_string(),
        body_mode: "urlenc".to_string(),
        raw_body_subtype: "json".to_string(),
        body: "name=Zen\nrole=admin".to_string(),
        graphql_variables: String::new(),
        pre_request_script: "set_header X-Trace=yes".to_string(),
        global_variables: "baseUrl=https://api.example.com\ntoken=secret".to_string(),
        environment_name: String::new(),
        environment_variables: String::new(),
    };

    assert_eq!(
        history_request(&request, &input, "status_equals 201"),
        HistoryRequest {
            method: "POST".to_string(),
            url: "https://api.example.com/users".to_string(),
            query_params: vec![("debug".to_string(), "true".to_string())],
            headers: vec![("Authorization".to_string(), "Bearer token".to_string())],
            auth_mode: "bearer".to_string(),
            auth_config: "{{token}}".to_string(),
            body_kind: "urlenc".to_string(),
            raw_body_subtype: "json".to_string(),
            body_preview: "name=Zen\nrole=admin".to_string(),
            pre_request_script: "set_header X-Trace=yes".to_string(),
            request_tests: "status_equals 201".to_string(),
        }
    );
}

#[test]
fn filters_history_rows_for_sidebar_model() {
    use slint::Model;

    let mut history = RequestHistory::new();
    history.record_at(
        1,
        HistoryRequest {
            method: "GET".to_string(),
            url: "https://api.example.com/users".to_string(),
            query_params: Vec::new(),
            headers: Vec::new(),
            auth_mode: "none".to_string(),
            auth_config: String::new(),
            body_kind: "none".to_string(),
            raw_body_subtype: "json".to_string(),
            body_preview: String::new(),
            pre_request_script: String::new(),
            request_tests: String::new(),
        },
        HistoryResponse {
            status: "HTTP 200".to_string(),
            meta: "12 ms".to_string(),
            body_preview: "{}".to_string(),
        },
    );
    history.record_at(
        2,
        HistoryRequest {
            method: "POST".to_string(),
            url: "https://api.example.com/sessions".to_string(),
            query_params: Vec::new(),
            headers: Vec::new(),
            auth_mode: "none".to_string(),
            auth_config: String::new(),
            body_kind: "raw".to_string(),
            raw_body_subtype: "json".to_string(),
            body_preview: "{}".to_string(),
            pre_request_script: String::new(),
            request_tests: String::new(),
        },
        HistoryResponse {
            status: "HTTP 401".to_string(),
            meta: "auth".to_string(),
            body_preview: "{}".to_string(),
        },
    );

    let model = filtered_history_model(&history, "sessions");

    assert_eq!(model.row_count(), 1);
    let row = model.row_data(0).expect("history row");
    assert_eq!(row.method.as_str(), "POST");
    assert_eq!(row.status.as_str(), "HTTP 401");
}

#[test]
fn deletes_history_entry_and_refreshes_filtered_rows() {
    use slint::Model;

    let mut history = RequestHistory::new();
    let users_id = history.record_at(
        1,
        HistoryRequest {
            method: "GET".to_string(),
            url: "https://api.example.com/users".to_string(),
            query_params: Vec::new(),
            headers: Vec::new(),
            auth_mode: "none".to_string(),
            auth_config: String::new(),
            body_kind: "none".to_string(),
            raw_body_subtype: "json".to_string(),
            body_preview: String::new(),
            pre_request_script: String::new(),
            request_tests: String::new(),
        },
        HistoryResponse {
            status: "HTTP 200".to_string(),
            meta: "12 ms".to_string(),
            body_preview: "{}".to_string(),
        },
    );
    history.record_at(
        2,
        HistoryRequest {
            method: "POST".to_string(),
            url: "https://api.example.com/sessions".to_string(),
            query_params: Vec::new(),
            headers: Vec::new(),
            auth_mode: "none".to_string(),
            auth_config: String::new(),
            body_kind: "raw".to_string(),
            raw_body_subtype: "json".to_string(),
            body_preview: "{}".to_string(),
            pre_request_script: String::new(),
            request_tests: String::new(),
        },
        HistoryResponse {
            status: "HTTP 401".to_string(),
            meta: "auth".to_string(),
            body_preview: "{}".to_string(),
        },
    );

    let model = delete_history_entry(&mut history, users_id, "users").expect("delete model");

    assert_eq!(model.row_count(), 0);
    assert!(history.find(users_id).is_none());
    assert!(delete_history_entry(&mut history, users_id, "").is_none());
}

#[test]
fn previews_request_body_modes_for_history() {
    assert_eq!(
        request_body_preview(&RequestBody::None),
        ("none".to_string(), "json".to_string(), String::new())
    );
    assert_eq!(
        request_body_preview(&RequestBody::Raw {
            content_type: Some("text/plain".to_string()),
            body: "{\"ok\":true}".to_string(),
        }),
        (
            "raw".to_string(),
            "text".to_string(),
            "{\"ok\":true}".to_string()
        )
    );
    assert_eq!(
        request_body_preview(&RequestBody::Multipart(vec![(
            "file".to_string(),
            "@/tmp/upload.txt".to_string()
        )])),
        (
            "form".to_string(),
            "json".to_string(),
            "file=@/tmp/upload.txt".to_string()
        )
    );
    assert_eq!(
        request_body_preview(&RequestBody::BinaryFile {
            path: "/tmp/body.bin".to_string(),
            content_type: None,
        }),
        (
            "binary".to_string(),
            "json".to_string(),
            "/tmp/body.bin".to_string()
        )
    );
}

#[test]
fn truncates_long_history_response_previews() {
    assert_eq!(truncate_preview("abc", 3), "abc");
    assert_eq!(truncate_preview("abcdef", 3), "abc\n...");
}

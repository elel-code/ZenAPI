use crate::client::RequestBody;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodegenRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub query_params: Vec<(String, String)>,
    pub body: RequestBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnippetLanguage {
    Curl,
    PythonRequests,
    JavaScriptFetch,
    RustReqwest,
    GoNetHttp,
}

pub fn generate_snippet(request: &CodegenRequest, language: SnippetLanguage) -> String {
    match language {
        SnippetLanguage::Curl => curl_snippet(request),
        SnippetLanguage::PythonRequests => python_requests_snippet(request),
        SnippetLanguage::JavaScriptFetch => javascript_fetch_snippet(request),
        SnippetLanguage::RustReqwest => rust_reqwest_snippet(request),
        SnippetLanguage::GoNetHttp => go_net_http_snippet(request),
    }
}

fn curl_snippet(request: &CodegenRequest) -> String {
    let mut lines = vec![
        "curl".to_string(),
        "  -sS".to_string(),
        "  --noproxy '*'".to_string(),
        "  --max-time 5".to_string(),
        format!("  -X {}", shell_quote(&request.method)),
        format!("  {}", shell_quote(&url_with_query(request))),
    ];

    for (name, value) in &request.headers {
        lines.push(format!("  -H {}", shell_quote(&format!("{name}: {value}"))));
    }

    match &request.body {
        RequestBody::None => {}
        RequestBody::Raw { body, .. } => lines.push(format!("  --data {}", shell_quote(body))),
        RequestBody::FormUrlEncoded(fields) => {
            lines.push(format!(
                "  --data {}",
                shell_quote(&form_urlencoded(fields))
            ));
        }
        RequestBody::Multipart(fields) => {
            for (name, value) in fields {
                lines.push(format!("  -F {}", shell_quote(&format!("{name}={value}"))));
            }
        }
        RequestBody::BinaryFile { path, .. } => {
            lines.push(format!(
                "  --data-binary {}",
                shell_quote(&format!("@{path}"))
            ));
        }
    }

    lines.join(" \\\n")
}

fn python_requests_snippet(request: &CodegenRequest) -> String {
    let mut snippet = String::from("import requests\n\n");
    snippet.push_str(&format!(
        "url = {}\n",
        json_string(&url_with_query(request))
    ));
    snippet.push_str(&format!("headers = {}\n", python_dict(&request.headers)));

    match &request.body {
        RequestBody::None => snippet.push_str(&format!(
            "response = requests.request({}, url, headers=headers)\n",
            json_string(&request.method)
        )),
        RequestBody::Raw { body, .. } => snippet.push_str(&format!(
            "response = requests.request({}, url, headers=headers, data={})\n",
            json_string(&request.method),
            json_string(body)
        )),
        RequestBody::FormUrlEncoded(fields) => snippet.push_str(&format!(
            "response = requests.request({}, url, headers=headers, data={})\n",
            json_string(&request.method),
            python_dict(fields)
        )),
        RequestBody::Multipart(fields) => snippet.push_str(&format!(
            "response = requests.request({}, url, headers=headers, files={}, data={})\n",
            json_string(&request.method),
            python_files_dict(fields),
            python_text_fields_dict(fields)
        )),
        RequestBody::BinaryFile { path, .. } => snippet.push_str(&format!(
            "with open({}, \"rb\") as body:\n    response = requests.request({}, url, headers=headers, data=body)\n",
            json_string(path),
            json_string(&request.method)
        )),
    }

    snippet.push_str("print(response.text)\n");
    snippet
}

fn javascript_fetch_snippet(request: &CodegenRequest) -> String {
    let mut snippet = String::new();
    snippet.push_str(&format!(
        "const response = await fetch({}, {{\n",
        json_string(&url_with_query(request))
    ));
    snippet.push_str(&format!("  method: {},\n", json_string(&request.method)));
    snippet.push_str(&format!("  headers: {},\n", js_object(&request.headers)));

    if let Some(body) = body_as_text(&request.body) {
        snippet.push_str(&format!("  body: {},\n", json_string(&body)));
    }

    snippet.push_str("});\nconsole.log(await response.text());\n");
    snippet
}

fn rust_reqwest_snippet(request: &CodegenRequest) -> String {
    let mut snippet = String::from("let client = reqwest::Client::new();\nlet response = client\n");
    snippet.push_str(&format!(
        "    .request(reqwest::Method::{}, {})\n",
        request.method.to_ascii_uppercase(),
        json_string(&url_with_query(request))
    ));

    for (name, value) in &request.headers {
        snippet.push_str(&format!(
            "    .header({}, {})\n",
            json_string(name),
            json_string(value)
        ));
    }

    if let Some(body) = body_as_text(&request.body) {
        snippet.push_str(&format!("    .body({}.to_string())\n", json_string(&body)));
    }

    snippet.push_str("    .send()\n    .await?;\nprintln!(\"{}\", response.text().await?);\n");
    snippet
}

fn go_net_http_snippet(request: &CodegenRequest) -> String {
    let body = body_as_text(&request.body).unwrap_or_default();
    let mut snippet = String::from(
        "package main\n\nimport (\n  \"fmt\"\n  \"io\"\n  \"net/http\"\n  \"strings\"\n)\n\nfunc main() {\n",
    );
    snippet.push_str(&format!(
        "  req, err := http.NewRequest({}, {}, strings.NewReader({}))\n",
        json_string(&request.method),
        json_string(&url_with_query(request)),
        json_string(&body)
    ));
    snippet.push_str("  if err != nil { panic(err) }\n");

    for (name, value) in &request.headers {
        snippet.push_str(&format!(
            "  req.Header.Set({}, {})\n",
            json_string(name),
            json_string(value)
        ));
    }

    snippet.push_str(
        "  res, err := http.DefaultClient.Do(req)\n  if err != nil { panic(err) }\n  defer res.Body.Close()\n  body, _ := io.ReadAll(res.Body)\n  fmt.Println(string(body))\n}\n",
    );
    snippet
}

fn url_with_query(request: &CodegenRequest) -> String {
    if request.query_params.is_empty() {
        return request.url.clone();
    }

    let separator = if request.url.contains('?') { "&" } else { "?" };
    format!(
        "{}{}{}",
        request.url,
        separator,
        form_urlencoded(&request.query_params)
    )
}

fn form_urlencoded(fields: &[(String, String)]) -> String {
    fields
        .iter()
        .map(|(name, value)| format!("{}={}", percent_encode(name), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            b' ' => vec!['+'],
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn body_as_text(body: &RequestBody) -> Option<String> {
    match body {
        RequestBody::None => None,
        RequestBody::Raw { body, .. } => Some(body.clone()),
        RequestBody::FormUrlEncoded(fields) => Some(form_urlencoded(fields)),
        RequestBody::Multipart(fields) => Some(
            fields
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("&"),
        ),
        RequestBody::BinaryFile { path, .. } => Some(format!("@{path}")),
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization cannot fail")
}

fn python_dict(fields: &[(String, String)]) -> String {
    if fields.is_empty() {
        return "{}".to_string();
    }

    format!(
        "{{{}}}",
        fields
            .iter()
            .map(|(name, value)| format!("{}: {}", json_string(name), json_string(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn python_files_dict(fields: &[(String, String)]) -> String {
    let files = fields
        .iter()
        .filter_map(|(name, value)| value.strip_prefix('@').map(|path| (name, path)))
        .map(|(name, path)| format!("{}: open({}, \"rb\")", json_string(name), json_string(path)))
        .collect::<Vec<_>>();

    if files.is_empty() {
        "{}".to_string()
    } else {
        format!("{{{}}}", files.join(", "))
    }
}

fn python_text_fields_dict(fields: &[(String, String)]) -> String {
    python_dict(
        &fields
            .iter()
            .filter(|(_, value)| !value.starts_with('@'))
            .cloned()
            .collect::<Vec<_>>(),
    )
}

fn js_object(fields: &[(String, String)]) -> String {
    if fields.is_empty() {
        return "{}".to_string();
    }

    format!(
        "{{ {} }}",
        fields
            .iter()
            .map(|(name, value)| format!("{}: {}", json_string(name), json_string(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Bytes, routing::post};
    use std::process::Command;
    use tokio::net::TcpListener;

    fn request() -> CodegenRequest {
        CodegenRequest {
            method: "POST".to_string(),
            url: "https://api.example.com/users".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            query_params: vec![("search".to_string(), "rust slint".to_string())],
            body: RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"name\":\"Zen\"}".to_string(),
            },
        }
    }

    #[test]
    fn generates_curl_snippet() {
        let snippet = generate_snippet(&request(), SnippetLanguage::Curl);

        assert!(snippet.contains("curl"));
        assert!(snippet.contains("-X 'POST'"));
        assert!(snippet.contains("search=rust+slint"));
        assert!(snippet.contains("-H 'Content-Type: application/json'"));
        assert!(snippet.contains("--data '{\"name\":\"Zen\"}'"));
    }

    #[test]
    fn generates_python_javascript_rust_and_go_snippets() {
        let request = request();

        assert!(
            generate_snippet(&request, SnippetLanguage::PythonRequests)
                .contains("requests.request")
        );
        assert!(generate_snippet(&request, SnippetLanguage::JavaScriptFetch).contains("fetch"));
        assert!(generate_snippet(&request, SnippetLanguage::RustReqwest).contains("reqwest"));
        assert!(generate_snippet(&request, SnippetLanguage::GoNetHttp).contains("http.NewRequest"));
    }

    #[test]
    fn appends_query_params_to_existing_urls() {
        let mut request = request();
        request.url = "https://api.example.com/users?debug=true".to_string();

        assert_eq!(
            url_with_query(&request),
            "https://api.example.com/users?debug=true&search=rust+slint"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn generated_curl_executes_against_local_server() {
        if Command::new("curl").arg("--version").output().is_err() {
            return;
        }

        async fn echo(body: Bytes) -> String {
            String::from_utf8_lossy(&body).to_string()
        }

        let app = Router::new().route("/echo", post(echo));
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("test server addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test server");
        });
        let request = CodegenRequest {
            method: "POST".to_string(),
            url: format!("http://{addr}/echo"),
            headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
            query_params: Vec::new(),
            body: RequestBody::Raw {
                content_type: Some("text/plain".to_string()),
                body: "curl-ok".to_string(),
            },
        };
        let snippet = generate_snippet(&request, SnippetLanguage::Curl);

        let output = Command::new("sh")
            .arg("-c")
            .arg(snippet)
            .output()
            .expect("run curl snippet");
        server.abort();

        assert!(
            output.status.success(),
            "curl failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "curl-ok");
    }
}

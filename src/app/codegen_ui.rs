use anyhow::Result;
use slint::ComponentHandle;
use zenapi::{
    client::RequestBody,
    codegen::{CodegenRequest, SnippetLanguage, generate_snippet},
};

use crate::ui::AppWindow;

use super::{
    clipboard_ui::copy_text_to_clipboard, file_io::write_text_file,
    request_editor_ui::request_projection_input,
    request_projection::build_codegen_request_projection, set_response,
};

pub(super) fn snippet_language(language: &str) -> SnippetLanguage {
    match language {
        "python" => SnippetLanguage::PythonRequests,
        "js" => SnippetLanguage::JavaScriptFetch,
        "rust" => SnippetLanguage::RustReqwest,
        "go" => SnippetLanguage::GoNetHttp,
        _ => SnippetLanguage::Curl,
    }
}

pub(super) fn codegen_language_label(language: SnippetLanguage) -> &'static str {
    match language {
        SnippetLanguage::Curl => "cURL",
        SnippetLanguage::PythonRequests => "Python requests",
        SnippetLanguage::JavaScriptFetch => "JavaScript fetch",
        SnippetLanguage::RustReqwest => "Rust reqwest",
        SnippetLanguage::GoNetHttp => "Go net/http",
    }
}

fn codegen_body_label(body: &RequestBody) -> &'static str {
    match body {
        RequestBody::None => "none",
        RequestBody::Raw { .. } => "raw",
        RequestBody::FormUrlEncoded(_) => "urlencoded",
        RequestBody::Multipart(_) => "multipart",
        RequestBody::BinaryFile { .. } => "binary",
    }
}

pub(super) fn codegen_metadata(
    request: &CodegenRequest,
    language_label: &str,
    snippet: &str,
) -> String {
    let line_count = snippet.lines().count();
    let byte_count = snippet.len();
    format!(
        "Language: {language_label}\nRequest: {} {}\nLines: {line_count} / Bytes: {byte_count}\nHeaders: {} / Query params: {} / Body: {}",
        request.method,
        request.url,
        request.headers.len(),
        request.query_params.len(),
        codegen_body_label(&request.body)
    )
}

pub(super) fn save_codegen_snippet(path: &str, snippet: &str) -> Result<()> {
    write_text_file(path, snippet, "snippet export")
}

pub(super) fn wire_codegen(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_generate_code(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let request = match build_codegen_request_projection(&request_projection_input(&app)) {
            Ok(request) => request,
            Err(error) => {
                set_response(&app, "Codegen failed", "", "error", &error.to_string());
                return;
            }
        };
        let language = snippet_language(&app.get_codegen_language());
        let snippet = generate_snippet(&request, language);
        app.set_codegen_metadata(
            codegen_metadata(&request, codegen_language_label(language), &snippet).into(),
        );
        app.set_codegen_output(snippet.into());
        set_response(
            &app,
            "Codegen ready",
            &app.get_codegen_language(),
            "success",
            "Snippet generated.",
        );
    });

    let weak_app = app.as_weak();
    app.on_copy_codegen(move || {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let snippet = app.get_codegen_output().to_string();
        if snippet.is_empty() {
            app.set_activity("Generate a snippet before copying".into());
            return;
        }

        match copy_text_to_clipboard(&snippet) {
            Ok(()) => app.set_activity("Copied code snippet".into()),
            Err(error) => app.set_activity(format!("Copy failed: {error}").into()),
        }
    });

    let weak_app = app.as_weak();
    app.on_save_codegen(move |path| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let request = match build_codegen_request_projection(&request_projection_input(&app)) {
            Ok(request) => request,
            Err(error) => {
                set_response(&app, "Codegen save failed", "", "error", &error.to_string());
                return;
            }
        };
        let language = snippet_language(&app.get_codegen_language());
        let snippet = generate_snippet(&request, language);
        let metadata = codegen_metadata(&request, codegen_language_label(language), &snippet);

        match save_codegen_snippet(path.as_str(), &snippet) {
            Ok(()) => {
                app.set_codegen_output(snippet.into());
                app.set_codegen_metadata(metadata.into());
                set_response(
                    &app,
                    "Codegen saved",
                    path.as_str(),
                    "success",
                    "Snippet exported to disk.",
                );
            }
            Err(error) => {
                set_response(
                    &app,
                    "Codegen save failed",
                    path.as_str(),
                    "error",
                    &error.to_string(),
                );
            }
        }
    });
}

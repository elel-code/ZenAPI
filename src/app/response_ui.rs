use slint::ComponentHandle;

use crate::ui::AppWindow;

use super::{
    clipboard_ui::copy_text_to_clipboard,
    graphql::summarize_graphql_schema_response,
    response_format::{
        folded_response_view_text, format_json_response_text, response_copy_text,
        response_tab_label, split_response_meta,
    },
};

pub(super) fn set_response(app: &AppWindow, status: &str, meta: &str, tone: &str, body: &str) {
    let (response_time, response_size) = split_response_meta(meta);
    app.set_response_status(status.into());
    app.set_response_meta(meta.into());
    app.set_response_time(response_time.into());
    app.set_response_size(response_size.into());
    app.set_response_tone(tone.into());
    set_response_payload(app, body, body, "", "No cookies");
}

pub(super) fn set_response_payload(
    app: &AppWindow,
    body: &str,
    raw_body: &str,
    headers: &str,
    cookies: &str,
) {
    app.set_response_body(body.into());
    app.set_response_raw_body(raw_body.into());
    app.set_response_headers(headers.into());
    app.set_response_cookies(cookies.into());
    app.set_folded_response_body(folded_response_view_text("pretty", body).into());
    app.set_folded_response_raw_body(folded_response_view_text("raw", raw_body).into());
    app.set_folded_response_headers(folded_response_view_text("headers", headers).into());
    app.set_folded_response_cookies(folded_response_view_text("cookies", cookies).into());
}

pub(super) fn wire_response_actions(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_copy_response(move |tab, text, anchor, cursor| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        let (copy_text, copied_selection) = response_copy_text(text.as_str(), anchor, cursor);
        match copy_text_to_clipboard(copy_text) {
            Ok(()) => {
                let selection_label = if copied_selection { " selected" } else { "" };
                app.set_activity(
                    format!(
                        "Copied{selection_label} {} response",
                        response_tab_label(&tab)
                    )
                    .into(),
                )
            }
            Err(error) => app.set_activity(format!("Copy failed: {error}").into()),
        }
    });

    let weak_app = app.as_weak();
    app.on_format_response(move |tab| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let current = match tab.as_str() {
            "pretty" => app.get_response_body().to_string(),
            "raw" => app.get_response_raw_body().to_string(),
            _ => {
                app.set_activity("Format is available for Pretty and Raw responses".into());
                return;
            }
        };

        match format_json_response_text(&current) {
            Ok(formatted) => {
                match tab.as_str() {
                    "pretty" => {
                        app.set_response_body(formatted.clone().into());
                        app.set_folded_response_body(
                            folded_response_view_text("pretty", &formatted).into(),
                        );
                    }
                    "raw" => {
                        app.set_response_raw_body(formatted.clone().into());
                        app.set_folded_response_raw_body(
                            folded_response_view_text("raw", &formatted).into(),
                        );
                    }
                    _ => {}
                }
                app.set_activity(format!("Formatted {} response", response_tab_label(&tab)).into());
            }
            Err(error) => app.set_activity(format!("Format failed: {error}").into()),
        }
    });

    let weak_app = app.as_weak();
    app.on_summarize_graphql_schema(move |tab, text| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        if tab.as_str() != "pretty" && tab.as_str() != "raw" {
            app.set_activity("Schema summary is available for Pretty and Raw responses".into());
            return;
        }

        match summarize_graphql_schema_response(text.as_str()) {
            Ok(summary) => {
                app.set_response_body(summary.clone().into());
                app.set_folded_response_body(folded_response_view_text("pretty", &summary).into());
                app.set_activity("Summarized GraphQL schema".into());
            }
            Err(error) => app.set_activity(format!("Schema summary failed: {error}").into()),
        }
    });
}

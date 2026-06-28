use slint::ComponentHandle;

use crate::ui::AppWindow;

use super::clipboard_ui::copy_text_to_clipboard;

pub(super) fn wire_api_key_page_actions(app: &AppWindow) {
    let weak_app = app.as_weak();
    app.on_copy_api_key_integration(move |kind, text| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };

        let text = text.trim();
        if text.is_empty() {
            app.set_activity("No API key integration text to copy".into());
            return;
        }

        let label = match kind.as_str() {
            "curl" => "API Keys cURL",
            "auth" => "API Keys auth mapping",
            _ => "API Keys integration",
        };
        match copy_text_to_clipboard(text) {
            Ok(()) => app.set_activity(format!("Copied {label}").into()),
            Err(error) => app.set_activity(format!("Copy failed: {error}").into()),
        }
    });
}

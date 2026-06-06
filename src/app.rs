mod bindings;
mod fonts;
mod state;

use anyhow::{Result, anyhow};
use slint::ComponentHandle;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

use self::state::AppState;
use crate::ui::AppWindow;

pub fn run() -> Result<()> {
    let runtime = Arc::new(Runtime::new()?);
    let state = Arc::new(Mutex::new(AppState::default()));
    let app = AppWindow::new().map_err(|err| anyhow!(err.to_string()))?;
    fonts::register_app_fonts();

    app.set_response_body(
        "Import an OpenAPI file, select a route, then send a request or start the mock server."
            .into(),
    );

    bindings::wire_import(&app, state.clone());
    bindings::wire_route_filter(&app, state.clone());
    bindings::wire_route_selection(&app, state.clone());
    bindings::wire_request_sender(&app, runtime.clone());
    bindings::wire_mock_server(&app, runtime, state);

    app.run().map_err(|err| anyhow!(err.to_string()))
}

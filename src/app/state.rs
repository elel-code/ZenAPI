use zenapi::{mock_server::MockServer, openapi::ApiRoute};

#[derive(Default)]
pub(super) struct AppState {
    pub routes: Vec<ApiRoute>,
    pub visible_routes: Vec<ApiRoute>,
    pub server: Option<MockServer>,
}

pub(super) enum ServerAction {
    Start(Vec<ApiRoute>),
    Stop(MockServer),
}

impl AppState {
    pub fn next_server_action(&mut self) -> ServerAction {
        if let Some(server) = self.server.take() {
            ServerAction::Stop(server)
        } else {
            ServerAction::Start(self.routes.clone())
        }
    }
}

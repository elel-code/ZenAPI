use crate::ui::AppWindow;

use super::super::request_projection::RequestProjectionInput;

pub(in crate::app) fn request_projection_input(app: &AppWindow) -> RequestProjectionInput {
    RequestProjectionInput {
        method: app.get_method().to_string(),
        url: app.get_url().to_string(),
        query_params: app.get_query_params().to_string(),
        headers: app.get_request_headers().to_string(),
        auth_mode: app.get_auth_mode().to_string(),
        auth_config: app.get_auth_config().to_string(),
        body_mode: app.get_body_mode().to_string(),
        raw_body_subtype: app.get_raw_body_subtype().to_string(),
        body: app.get_request_body().to_string(),
        graphql_variables: app.get_graphql_variables().to_string(),
        pre_request_script: app.get_pre_request_script().to_string(),
        global_variables: app.get_global_variables().to_string(),
        environment_name: app.get_environment_name().to_string(),
        environment_variables: app.get_environment_variables().to_string(),
    }
}

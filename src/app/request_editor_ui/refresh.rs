use crate::{auth::split_basic_auth_config, ui::AppWindow};

use super::super::{
    key_value_ui::key_value_table_model,
    test_assertion_ui::test_assertion_table_model,
    variable_ui::{variable_table_model, variables_json_preview},
};

pub(in crate::app) fn refresh_query_param_rows(app: &AppWindow) {
    app.set_query_param_rows(key_value_table_model(app.get_query_params().as_str()));
}

pub(in crate::app) fn refresh_header_rows(app: &AppWindow) {
    app.set_header_rows(key_value_table_model(app.get_request_headers().as_str()));
}

pub(in crate::app) fn refresh_auth_key_rows(app: &AppWindow) {
    app.set_auth_key_rows(key_value_table_model(app.get_auth_config().as_str()));
}

pub(in crate::app) fn refresh_basic_auth_fields(app: &AppWindow) {
    let (username, password) = split_basic_auth_config(app.get_auth_config().as_str());
    app.set_auth_basic_username(username.into());
    app.set_auth_basic_password(password.into());
}

pub(in crate::app) fn refresh_body_field_rows(app: &AppWindow) {
    app.set_body_field_rows(key_value_table_model(app.get_request_body().as_str()));
}

pub(in crate::app) fn refresh_test_assertion_rows(app: &AppWindow) {
    app.set_test_assertion_rows(test_assertion_table_model(app.get_request_tests().as_str()));
}

pub(in crate::app) fn refresh_variable_table(app: &AppWindow) {
    let global_variables = app.get_global_variables();
    let environment_name = app.get_environment_name();
    let environment_variables = app.get_environment_variables();

    app.set_variable_rows(variable_table_model(
        global_variables.as_str(),
        environment_variables.as_str(),
    ));
    app.set_variables_json_preview(
        variables_json_preview(
            global_variables.as_str(),
            environment_name.as_str(),
            environment_variables.as_str(),
        )
        .into(),
    );
}

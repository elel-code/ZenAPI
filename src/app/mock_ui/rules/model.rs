use anyhow::{Result, bail};
use slint::{ModelRc, VecModel};
use zenapi::openapi::{ApiRoute, MockRule, MockRuleSource};

use crate::ui::{AppWindow, MockRuleRow, RouteRow};

use super::super::super::response_format::{json_value, pretty_json, truncate_summary_line};

pub(in crate::app) fn parse_mock_rule_source(source: &str) -> Result<MockRuleSource> {
    match source.trim().to_lowercase().as_str() {
        "header" => Ok(MockRuleSource::Header),
        "query" => Ok(MockRuleSource::Query),
        _ => bail!("mock rule source must be header or query"),
    }
}

pub(in crate::app) fn default_mock_rule_name(source: MockRuleSource) -> &'static str {
    match source {
        MockRuleSource::Header => "x-mock-scenario",
        MockRuleSource::Query => "scenario",
    }
}

pub(in crate::app) fn mock_rule_source_key(source: MockRuleSource) -> &'static str {
    match source {
        MockRuleSource::Header => "header",
        MockRuleSource::Query => "query",
    }
}

fn mock_rule_source_label(source: MockRuleSource) -> &'static str {
    match source {
        MockRuleSource::Header => "Header",
        MockRuleSource::Query => "Query",
    }
}

pub(in crate::app) fn empty_mock_rule_model() -> ModelRc<MockRuleRow> {
    mock_rule_model(&[])
}

pub(in crate::app) fn mock_rule_model(rules: &[MockRule]) -> ModelRc<MockRuleRow> {
    ModelRc::new(VecModel::from_iter(rules.iter().enumerate().map(
        |(index, rule)| MockRuleRow {
            row_id: index as i32,
            source: mock_rule_source_label(rule.source).into(),
            name: rule.name.clone().into(),
            value: rule.value.clone().into(),
            body_preview: truncate_summary_line(&json_value(&rule.mock_body), 96).into(),
        },
    )))
}

pub(in crate::app) fn route_model(routes: &[ApiRoute]) -> ModelRc<RouteRow> {
    ModelRc::new(VecModel::from_iter(routes.iter().map(|route| RouteRow {
        method: route.method.clone().into(),
        path: route.path.clone().into(),
        summary: route.summary.clone().into(),
    })))
}

pub(in crate::app) fn set_selected_mock_route(app: &AppWindow, route: &ApiRoute) {
    app.set_selected_mock_method(route.method.clone().into());
    app.set_selected_mock_path(route.path.clone().into());
    app.set_selected_mock_summary(route.summary.clone().into());
    app.set_selected_mock_body(pretty_json(&route.mock_body).into());
    refresh_mock_rule_rows(app, route);
    if let Some(rule) = route.mock_rules.first() {
        set_selected_mock_rule(app, 0, rule);
    } else {
        clear_selected_mock_rule(app);
    }
}

pub(in crate::app) fn clear_selected_mock_route(app: &AppWindow) {
    app.set_selected_mock_method("".into());
    app.set_selected_mock_path("".into());
    app.set_selected_mock_summary("".into());
    app.set_selected_mock_body("".into());
    app.set_mock_rule_rows(empty_mock_rule_model());
    clear_selected_mock_rule(app);
}

pub(in crate::app) fn refresh_mock_rule_rows(app: &AppWindow, route: &ApiRoute) {
    app.set_mock_rule_rows(mock_rule_model(&route.mock_rules));
}

pub(in crate::app) fn set_selected_mock_rule(app: &AppWindow, row_id: i32, rule: &MockRule) {
    app.set_selected_mock_rule(row_id);
    app.set_selected_mock_rule_source(mock_rule_source_key(rule.source).into());
    app.set_selected_mock_rule_name(rule.name.clone().into());
    app.set_selected_mock_rule_value(rule.value.clone().into());
    app.set_selected_mock_rule_body(pretty_json(&rule.mock_body).into());
}

pub(in crate::app) fn clear_selected_mock_rule(app: &AppWindow) {
    app.set_selected_mock_rule(-1);
    app.set_selected_mock_rule_source("header".into());
    app.set_selected_mock_rule_name("".into());
    app.set_selected_mock_rule_value("".into());
    app.set_selected_mock_rule_body("{\n  \n}".into());
}

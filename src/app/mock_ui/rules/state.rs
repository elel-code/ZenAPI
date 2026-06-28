use anyhow::{Result, anyhow, bail};
use serde_json::Value;
use zenapi::openapi::{ApiRoute, MockRule};

use super::super::super::AppState;
use super::model::{default_mock_rule_name, parse_mock_rule_source};

pub(in crate::app) fn update_selected_mock_response(
    state: &mut AppState,
    selected_route: i32,
    body: &str,
) -> Result<ApiRoute> {
    let selected_route: usize = selected_route
        .try_into()
        .map_err(|_| anyhow!("select a mock route before saving a response"))?;
    let selected = state
        .visible_routes
        .get(selected_route)
        .cloned()
        .ok_or_else(|| anyhow!("select a mock route before saving a response"))?;
    let mock_body = serde_json::from_str::<Value>(body.trim())
        .map_err(|err| anyhow!("mock response body must be valid JSON: {err}"))?;

    let route = state
        .routes
        .iter_mut()
        .find(|route| route.method == selected.method && route.path == selected.path)
        .ok_or_else(|| anyhow!("selected mock route is no longer available"))?;
    route.mock_body = mock_body.clone();
    let updated = route.clone();

    if let Some(visible_route) = state.visible_routes.get_mut(selected_route) {
        visible_route.mock_body = mock_body;
    }

    Ok(updated)
}

pub(in crate::app) fn selected_route(state: &AppState, selected_route: i32) -> Result<ApiRoute> {
    let selected_route: usize = selected_route
        .try_into()
        .map_err(|_| anyhow!("select a mock route first"))?;
    state
        .visible_routes
        .get(selected_route)
        .cloned()
        .ok_or_else(|| anyhow!("select a mock route first"))
}

fn selected_route_indices(state: &AppState, selected_route: i32) -> Result<(usize, usize)> {
    let visible_index: usize = selected_route
        .try_into()
        .map_err(|_| anyhow!("select a mock route first"))?;
    let selected = state
        .visible_routes
        .get(visible_index)
        .ok_or_else(|| anyhow!("select a mock route first"))?;
    let route_index = state
        .routes
        .iter()
        .position(|route| route.method == selected.method && route.path == selected.path)
        .ok_or_else(|| anyhow!("selected mock route is no longer available"))?;
    Ok((route_index, visible_index))
}

pub(in crate::app) fn add_selected_mock_rule(
    state: &mut AppState,
    selected_route: i32,
    source: &str,
) -> Result<(ApiRoute, i32)> {
    let (route_index, visible_index) = selected_route_indices(state, selected_route)?;
    let source = parse_mock_rule_source(source)?;
    let mock_body = state.routes[route_index].mock_body.clone();
    let rule = MockRule {
        source,
        name: default_mock_rule_name(source).to_string(),
        value: "success".to_string(),
        mock_body,
    };

    state.routes[route_index].mock_rules.push(rule);
    let row_id = (state.routes[route_index].mock_rules.len() - 1) as i32;
    let updated = state.routes[route_index].clone();
    state.visible_routes[visible_index] = updated.clone();
    Ok((updated, row_id))
}

pub(in crate::app) fn save_selected_mock_rule(
    state: &mut AppState,
    selected_route: i32,
    row_id: i32,
    source: &str,
    name: &str,
    value: &str,
    body: &str,
) -> Result<ApiRoute> {
    let (route_index, visible_index) = selected_route_indices(state, selected_route)?;
    let row_index: usize = row_id
        .try_into()
        .map_err(|_| anyhow!("select a mock rule before saving"))?;
    if row_index >= state.routes[route_index].mock_rules.len() {
        bail!("select a mock rule before saving");
    }

    let name = name.trim();
    let value = value.trim();
    if name.is_empty() {
        bail!("mock rule name is required");
    }
    if value.is_empty() {
        bail!("mock rule match value is required");
    }
    let mock_body = serde_json::from_str::<Value>(body.trim())
        .map_err(|err| anyhow!("mock rule response body must be valid JSON: {err}"))?;

    state.routes[route_index].mock_rules[row_index] = MockRule {
        source: parse_mock_rule_source(source)?,
        name: name.to_string(),
        value: value.to_string(),
        mock_body,
    };
    let updated = state.routes[route_index].clone();
    state.visible_routes[visible_index] = updated.clone();
    Ok(updated)
}

pub(in crate::app) fn delete_selected_mock_rule(
    state: &mut AppState,
    selected_route: i32,
    row_id: i32,
) -> Result<(ApiRoute, Option<i32>)> {
    let (route_index, visible_index) = selected_route_indices(state, selected_route)?;
    let row_index: usize = row_id
        .try_into()
        .map_err(|_| anyhow!("select a mock rule before deleting"))?;
    if row_index >= state.routes[route_index].mock_rules.len() {
        bail!("select a mock rule before deleting");
    }

    state.routes[route_index].mock_rules.remove(row_index);
    let next_row = if state.routes[route_index].mock_rules.is_empty() {
        None
    } else {
        Some(row_index.min(state.routes[route_index].mock_rules.len() - 1) as i32)
    };
    let updated = state.routes[route_index].clone();
    state.visible_routes[visible_index] = updated.clone();
    Ok((updated, next_row))
}

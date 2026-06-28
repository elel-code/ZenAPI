use serde_json::Value;
use slint::{ModelRc, VecModel};

use crate::ui::VariableTableRow;

use super::request_projection::split_key_value_line;

pub(super) fn variable_table_model(
    global_input: &str,
    environment_input: &str,
) -> ModelRc<VariableTableRow> {
    let global_rows = variable_ui_entries(global_input)
        .into_iter()
        .enumerate()
        .map(|(row_id, (_, name, value))| variable_table_row(row_id, "global", name, value));
    let environment_rows = variable_ui_entries(environment_input)
        .into_iter()
        .enumerate()
        .map(|(row_id, (_, name, value))| variable_table_row(row_id, "environment", name, value));

    ModelRc::new(VecModel::from_iter(global_rows.chain(environment_rows)))
}

fn variable_table_row(
    row_id: usize,
    scope: &'static str,
    name: String,
    value: String,
) -> VariableTableRow {
    VariableTableRow {
        row_id: row_id as i32,
        scope: scope.into(),
        name: name.into(),
        initial_value: value.clone().into(),
        current_value: value.into(),
    }
}

pub(super) fn variable_ui_entries(input: &str) -> Vec<(usize, String, String)> {
    input
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            split_key_value_line(line).map(|(name, value)| {
                (
                    line_index,
                    name.trim().to_string(),
                    value.trim().to_string(),
                )
            })
        })
        .collect()
}

pub(super) fn update_variable_text(input: &str, row_id: i32, name: &str, value: &str) -> String {
    let mut lines = input.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = variable_ui_entries(input);
    let new_line = format!("{}={}", name.trim(), value.trim());

    if let Some((line_index, _, _)) = row_id
        .try_into()
        .ok()
        .and_then(|row_id: usize| entries.get(row_id))
    {
        lines[*line_index] = new_line;
    } else {
        lines.push(new_line);
    }

    lines.join("\n")
}

pub(super) fn add_variable_text(input: &str, base_name: &str) -> String {
    let name = unique_variable_name(input, base_name);
    append_variable_line(input, &format!("{name}="))
}

pub(super) fn delete_variable_text(input: &str, row_id: i32) -> String {
    let mut lines = input.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = variable_ui_entries(input);

    if let Some((line_index, _, _)) = row_id
        .try_into()
        .ok()
        .and_then(|row_id: usize| entries.get(row_id))
    {
        lines.remove(*line_index);
    }

    lines.join("\n")
}

fn append_variable_line(input: &str, line: &str) -> String {
    if input.trim().is_empty() {
        line.to_string()
    } else if input.ends_with('\n') {
        format!("{input}{line}")
    } else {
        format!("{input}\n{line}")
    }
}

fn unique_variable_name(input: &str, base_name: &str) -> String {
    let existing = variable_ui_entries(input)
        .into_iter()
        .map(|(_, name, _)| name)
        .collect::<Vec<_>>();
    if !existing.iter().any(|name| name == base_name) {
        return base_name.to_string();
    }

    (2..)
        .map(|index| format!("{base_name}_{index}"))
        .find(|candidate| !existing.iter().any(|name| name == candidate))
        .unwrap_or_else(|| base_name.to_string())
}

pub(super) fn variables_json_preview(
    global_input: &str,
    environment_name: &str,
    environment_input: &str,
) -> String {
    let global = variable_scope_json(global_input);
    let environment = variable_scope_json(environment_input);
    let active_environment = environment_name.trim();
    let preview = serde_json::json!({
        "activeEnvironment": if active_environment.is_empty() {
            Value::Null
        } else {
            Value::String(active_environment.to_string())
        },
        "globals": global,
        "environment": environment,
    });

    serde_json::to_string_pretty(&preview).unwrap_or_else(|_| "{}".to_string())
}

fn variable_scope_json(input: &str) -> Value {
    let mut values = serde_json::Map::new();
    for (_, name, value) in variable_ui_entries(input) {
        values.insert(
            name.clone(),
            Value::String(mask_variable_preview_value(&name, &value)),
        );
    }
    Value::Object(values)
}

fn mask_variable_preview_value(name: &str, value: &str) -> String {
    let name = name.to_ascii_lowercase();
    if name.contains("key") || name.contains("token") || name.contains("secret") {
        "********".to_string()
    } else {
        value.to_string()
    }
}

pub(super) fn is_global_scope(scope: &str) -> bool {
    scope == "global"
}

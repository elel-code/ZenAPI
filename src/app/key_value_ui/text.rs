use anyhow::{Result, bail};
use slint::{ModelRc, VecModel};
use std::path::Path;

use crate::ui::KeyValueTableRow;

use super::super::{
    collection_tree::format_header_lines,
    file_io::read_text_file,
    request_projection::{parse_key_value_lines, split_key_value_line, upsert_pair},
};

pub(in crate::app) fn key_value_table_model(input: &str) -> ModelRc<KeyValueTableRow> {
    ModelRc::new(VecModel::from_iter(
        key_value_ui_entries(input)
            .into_iter()
            .enumerate()
            .map(|(row_id, (_, key, value))| KeyValueTableRow {
                row_id: row_id as i32,
                key: key.into(),
                value: value.into(),
            }),
    ))
}

fn key_value_ui_entries(input: &str) -> Vec<(usize, String, String)> {
    input
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            split_key_value_line(line)
                .map(|(key, value)| (line_index, key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

pub(in crate::app) fn update_key_value_text(
    input: &str,
    row_id: i32,
    key: &str,
    value: &str,
) -> String {
    let mut lines = input.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = key_value_ui_entries(input);
    let new_line = format!("{}={}", key.trim(), value.trim());

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

pub(in crate::app) fn add_key_value_text(input: &str, base_key: &str) -> String {
    let key = unique_key_value_name(input, base_key);
    append_key_value_line(input, &format!("{key}="))
}

pub(in crate::app) fn add_form_file_field_text(
    input: &str,
    field: &str,
    path: &str,
) -> Result<String> {
    let field = field.trim();
    if field.is_empty() {
        bail!("form file field name is required");
    }

    let path = path.trim();
    if path.is_empty() {
        bail!("form file path is required");
    }

    let file_path = Path::new(path);
    if !file_path.is_file() {
        bail!("form file path does not exist or is not a file: {path}");
    }

    Ok(append_key_value_line(input, &format!("{field}=@{path}")))
}

pub(in crate::app) fn merge_key_value_text(
    input: &str,
    imported: &str,
    field_name: &str,
    case_insensitive: bool,
    colon_output: bool,
) -> Result<(String, usize)> {
    let mut values = parse_key_value_lines(input, field_name)?;
    let imported_values = parse_key_value_lines(imported, field_name)?;
    if imported_values.is_empty() {
        bail!("clipboard does not contain any {field_name} rows");
    }

    let count = imported_values.len();
    for (name, value) in imported_values {
        upsert_pair(&mut values, name, value, case_insensitive);
    }

    let output = if colon_output {
        format_header_lines(&values)
    } else {
        format_key_value_preview(&values)
    };
    Ok((output, count))
}

pub(in crate::app) fn merge_key_value_file(
    input: &str,
    path: &str,
    field_name: &str,
    case_insensitive: bool,
    colon_output: bool,
) -> Result<(String, usize)> {
    let contents = read_text_file(path, &format!("{field_name} import"))?;
    merge_key_value_text(
        input,
        contents.as_str(),
        field_name,
        case_insensitive,
        colon_output,
    )
}

pub(in crate::app) fn delete_key_value_text(input: &str, row_id: i32) -> String {
    let mut lines = input.lines().map(str::to_string).collect::<Vec<_>>();
    let entries = key_value_ui_entries(input);

    if let Some((line_index, _, _)) = row_id
        .try_into()
        .ok()
        .and_then(|row_id: usize| entries.get(row_id))
    {
        lines.remove(*line_index);
    }

    lines.join("\n")
}

fn append_key_value_line(input: &str, line: &str) -> String {
    if input.trim().is_empty() {
        line.to_string()
    } else if input.ends_with('\n') {
        format!("{input}{line}")
    } else {
        format!("{input}\n{line}")
    }
}

pub(in crate::app) fn unique_key_value_name(input: &str, base_key: &str) -> String {
    let existing = key_value_ui_entries(input)
        .into_iter()
        .map(|(_, key, _)| key)
        .collect::<Vec<_>>();
    if !existing.iter().any(|key| key == base_key) {
        return base_key.to_string();
    }

    (2..)
        .map(|index| format!("{base_key}_{index}"))
        .find(|candidate| !existing.iter().any(|key| key == candidate))
        .unwrap_or_else(|| base_key.to_string())
}

pub(in crate::app) fn format_key_value_preview(fields: &[(String, String)]) -> String {
    fields
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
}
